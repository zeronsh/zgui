//! Windows' Text Services Framework for zgx's editors: a thread manager, a
//! document manager per window, and a context over an `ITextStoreACP2` of
//! zgx's own (`TextStore`, `text_store.rs`), so input methods and text
//! services read the focused editor's text around the caret, place their
//! candidates by the clause they convert, and insert where it is: the emoji
//! panel, dictation and the touch keyboard type into it, and an input
//! method's composition is its preedit.
//!
//! Off unless `ZGX_TSF=1`: winit's IMM32 path stays the default until the
//! staged rollout makes this one the default (docs/windows.md).
//!
//! Locks: Windows asks for one (`RequestLock`) to read or edit, and hears of
//! edits it didn't make only when no lock is held. A request while another
//! lock is held is granted after it (`after_dispatch`), as the asynchronous
//! grant `TS_S_ASYNC` says; a synchronous one then is refused
//! (`TS_E_SYNCHRONOUS`), as the interface allows. zgx calls into the
//! framework only after its dispatch, so a lock never meets a borrowed
//! editor.
use crate::text_store::{Change, TextStore};
use std::{cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{E_INVALIDARG, E_NOTIMPL, HWND, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, FORMATETC, IDataObject},
        UI::{
            TextServices::{
                CLSID_TF_ThreadMgr, ITextStoreACP2, ITextStoreACP2_Impl, ITextStoreACPSink,
                ITfCompositionView, ITfContext, ITfContextOwnerCompositionServices,
                ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSink_Impl,
                ITfDocumentMgr, ITfRange, ITfRangeACP, ITfThreadMgr, TEXT_STORE_LOCK_FLAGS,
                TEXT_STORE_TEXT_CHANGE_FLAGS, TF_IAS_QUERYONLY, TS_AE_END, TS_AE_START, TS_ATTRVAL,
                TS_DEFAULT_SELECTION, TS_E_NOLOCK, TS_E_SYNCHRONOUS, TS_LC_CHANGE, TS_LF_READ,
                TS_LF_READWRITE, TS_LF_SYNC, TS_RT_PLAIN, TS_RUNINFO, TS_S_ASYNC, TS_SELECTION_ACP,
                TS_SELECTIONSTYLE, TS_SS_NOHIDDENTEXT, TS_STATUS, TS_TEXTCHANGE,
            },
            WindowsAndMessaging::GetClientRect,
        },
    },
    core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result, implement},
};
use zgx::{scene::Rect, widgets::EditorHandle};

/// DIAGNOSTIC BUILD ONLY (branch tsf-idle-diag, never merged): calls into
/// the store by method, and the UI thread's cycles in `after_dispatch`.
pub mod diag {
    use std::{collections::BTreeMap, sync::Mutex};
    static CALLS: Mutex<BTreeMap<&'static str, (u64, u64)>> = Mutex::new(BTreeMap::new());
    pub(crate) fn cycles() -> u64 {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
            fn GetCurrentThread() -> *mut std::ffi::c_void;
        }
        let mut c = 0;
        // SAFETY: the calling thread's own counter.
        unsafe {
            QueryThreadCycleTime(GetCurrentThread(), &mut c);
        }
        c
    }
    pub(crate) fn note(name: &'static str, cycles: u64) {
        let mut calls = CALLS.lock().unwrap();
        let e = calls.entry(name).or_default();
        e.0 += 1;
        e.1 += cycles;
    }
    /// Every count since the last take: name, calls, cycles.
    pub fn take() -> Vec<(&'static str, u64, u64)> {
        std::mem::take(&mut *CALLS.lock().unwrap())
            .into_iter()
            .map(|(k, (n, c))| (k, n, c))
            .collect()
    }
}

/// The view cookie: an editor shows its text in one.
const VIEW: u32 = 1;

/// What the COM object and the host share.
struct Shared {
    store: TextStore,
    sink: Option<ITextStoreACPSink>,
    /// The lock held now (`TS_LF_READ` or `TS_LF_READWRITE` bits), if any.
    lock: u32,
    /// A lock asked for while one couldn't be granted, granted after.
    pending: Option<u32>,
    hwnd: HWND,
    scale: f64,
    /// The document's length when the input method last heard of it.
    told_len: usize,
    layout_changed: bool,
}

/// The Text Services Framework for one window.
pub(crate) struct Tsf {
    thread: ITfThreadMgr,
    document: ITfDocumentMgr,
    _context: ITfContext,
    shared: Rc<RefCell<Shared>>,
    focused: bool,
}

impl Tsf {
    /// The framework for window `hwnd` (`scale`: its physical pixels per
    /// point), when `ZGX_TSF=1` and it starts; `None` keeps IMM32's path.
    pub(crate) fn new(hwnd: HWND, scale: f64) -> Option<Self> {
        if std::env::var_os("ZGX_TSF").is_none_or(|v| v != "1") {
            return None;
        }
        // SAFETY: COM was initialized on this thread by winit (OLE, for drag
        // and drop); every pointer passed is valid for the call.
        unsafe {
            let thread: ITfThreadMgr =
                CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER).ok()?;
            let client = thread.Activate().ok()?;
            let document = thread.CreateDocumentMgr().ok()?;
            let shared = Rc::new(RefCell::new(Shared {
                store: TextStore::new(),
                sink: None,
                lock: 0,
                pending: None,
                hwnd,
                scale,
                told_len: 0,
                layout_changed: false,
            }));
            let store: IUnknown = Store(shared.clone()).into();
            let (mut context, mut cookie) = (None, 0);
            document
                .CreateContext(client, 0, &store, &mut context, &mut cookie)
                .ok()?;
            let context = context?;
            document.Push(&context).ok()?;
            Some(Self {
                thread,
                document,
                _context: context,
                shared,
                focused: false,
            })
        }
    }
    /// The framework for winit's `window` (as `new`).
    pub(crate) fn for_window(window: &winit::window::Window, scale: f64) -> Option<Self> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        Self::new(HWND(handle.hwnd.get() as *mut _), scale)
    }
    /// The editor taking the input method now, or none.
    pub(crate) fn focus(&mut self, editor: Option<EditorHandle>) {
        let on = editor.is_some();
        // A composition open in the editor losing focus ends there: the
        // input method stops composing (its OnEndComposition commits it
        // into that editor), and the store commits it if it didn't.
        // (No borrow is held across the call: it calls back into the store.)
        let composing = self.shared.borrow().store.is_composing();
        if composing
            && let Ok(services) = self._context.cast::<ITfContextOwnerCompositionServices>()
        {
            // SAFETY: the context's own services; no lock is held.
            let _ = unsafe { services.TerminateComposition(None) };
        }
        {
            let mut shared = self.shared.borrow_mut();
            shared.store.focus(editor);
            shared.told_len = shared.store.len();
        }
        if on != self.focused {
            let hwnd = self.shared.borrow().hwnd;
            // SAFETY: the document manager outlives the association.
            let _ = unsafe {
                if on {
                    self.thread.AssociateFocus(hwnd, &self.document)
                } else {
                    self.thread.AssociateFocus(hwnd, None)
                }
            };
            self.focused = on;
        }
    }
    pub(crate) fn set_scale(&self, scale: f64) {
        self.shared.borrow_mut().scale = scale;
    }
    /// Where the input method's candidates go moved.
    pub(crate) fn layout_changed(&self) {
        self.shared.borrow_mut().layout_changed = true;
    }
    /// After zgx's dispatch: tell the input method what changed that it
    /// didn't do, then grant a lock it asked for meanwhile.
    pub(crate) fn after_dispatch(&self) {
        let start = diag::cycles();
        self.after_dispatch_inner();
        diag::note("after_dispatch", diag::cycles() - start);
    }
    fn after_dispatch_inner(&self) {
        let (sink, changes, layout) = {
            let mut shared = self.shared.borrow_mut();
            if shared.lock != 0 {
                return;
            }
            let told = shared.told_len;
            let changes = shared.store.changes(told);
            shared.told_len = shared.store.len();
            let layout = std::mem::take(&mut shared.layout_changed);
            (shared.sink.clone(), changes, layout)
        };
        if let Some(sink) = &sink {
            // SAFETY: the sink is alive while advised; the change is valid.
            unsafe {
                for change in changes {
                    match change {
                        Change::Text {
                            start,
                            old_end,
                            new_end,
                        } => {
                            let change = TS_TEXTCHANGE {
                                acpStart: acp(start),
                                acpOldEnd: acp(old_end),
                                acpNewEnd: acp(new_end),
                            };
                            let _ = sink.OnTextChange(TEXT_STORE_TEXT_CHANGE_FLAGS(0), &change);
                        }
                        Change::Selection => {
                            let _ = sink.OnSelectionChange();
                        }
                    }
                }
                if layout {
                    let _ = sink.OnLayoutChange(TS_LC_CHANGE, VIEW);
                }
            }
        }
        let pending = self.shared.borrow_mut().pending.take();
        if let Some(flags) = pending {
            let _ = grant(&self.shared, flags);
        }
    }
}

impl Drop for Tsf {
    fn drop(&mut self) {
        // SAFETY: deactivating the thread manager this activated.
        unsafe {
            let _ = self.document.Pop(0);
            let _ = self.thread.Deactivate();
        }
    }
}

fn acp(at: usize) -> i32 {
    i32::try_from(at).unwrap_or(i32::MAX)
}
fn at(acp: i32) -> usize {
    usize::try_from(acp).unwrap_or(0)
}

/// Grant lock `flags` now: hold it while the sink reads or edits.
fn grant(shared: &Rc<RefCell<Shared>>, flags: u32) -> HRESULT {
    let sink = {
        let mut s = shared.borrow_mut();
        s.lock = flags & TS_LF_READWRITE.0;
        s.sink.clone()
    };
    // SAFETY: the sink is alive while advised.
    let result = sink.map_or(HRESULT(0), |sink| unsafe {
        sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags & TS_LF_READWRITE.0))
            .map_or_else(|e| e.code(), |()| HRESULT(0))
    });
    shared.borrow_mut().lock = 0;
    result
}

#[implement(ITextStoreACP2, ITfContextOwnerCompositionSink)]
struct Store(Rc<RefCell<Shared>>);

impl Store {
    fn read(&self) -> Result<std::cell::RefMut<'_, Shared>> {
        let shared = self.0.borrow_mut();
        if shared.lock & TS_LF_READ.0 == 0 {
            return Err(TS_E_NOLOCK.into());
        }
        Ok(shared)
    }
    fn write(&self) -> Result<std::cell::RefMut<'_, Shared>> {
        let shared = self.0.borrow_mut();
        if shared.lock & TS_LF_READWRITE.0 != TS_LF_READWRITE.0 {
            return Err(TS_E_NOLOCK.into());
        }
        Ok(shared)
    }
    /// A rect in the window's points as the screen's pixels.
    fn screen(shared: &Shared, r: Rect) -> RECT {
        let s = shared.scale;
        let mut origin = POINT::default();
        // SAFETY: a valid window and point.
        unsafe {
            let _ = ClientToScreen(shared.hwnd, &mut origin);
        }
        RECT {
            left: origin.x + (f64::from(r.x) * s) as i32,
            top: origin.y + (f64::from(r.y) * s) as i32,
            right: origin.x + (f64::from(r.x + r.width.max(1.)) * s) as i32,
            bottom: origin.y + (f64::from(r.y + r.height) * s) as i32,
        }
    }
}

impl ITextStoreACP2_Impl for Store_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<IUnknown>, _mask: u32) -> Result<()> {
        diag::note("AdviseSink", 0);
        // SAFETY: `riid` is a valid GUID from the caller.
        if unsafe { *riid } != ITextStoreACPSink::IID {
            return Err(E_INVALIDARG.into());
        }
        let sink: ITextStoreACPSink = punk.ok()?.cast()?;
        self.0.borrow_mut().sink = Some(sink);
        Ok(())
    }
    fn UnadviseSink(&self, _punk: Ref<IUnknown>) -> Result<()> {
        diag::note("UnadviseSink", 0);
        self.0.borrow_mut().sink = None;
        Ok(())
    }
    fn RequestLock(&self, flags: u32) -> Result<HRESULT> {
        diag::note("RequestLock", 0);
        let busy = {
            let shared = self.0.borrow();
            shared.lock != 0
        };
        if busy {
            if flags & TS_LF_SYNC != 0 {
                return Ok(TS_E_SYNCHRONOUS);
            }
            let mut shared = self.0.borrow_mut();
            // A write asked for wins over a read.
            let wanted = shared.pending.unwrap_or(0) | (flags & TS_LF_READWRITE.0);
            shared.pending = Some(wanted);
            return Ok(TS_S_ASYNC);
        }
        Ok(grant(&self.0, flags))
    }
    fn GetStatus(&self) -> Result<TS_STATUS> {
        diag::note("GetStatus", 0);
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: TS_SS_NOHIDDENTEXT,
        })
    }
    fn QueryInsert(
        &self,
        start: i32,
        end: i32,
        _cch: u32,
        result_start: *mut i32,
        result_end: *mut i32,
    ) -> Result<()> {
        diag::note("QueryInsert", 0);
        // SAFETY: the caller's out pointers.
        unsafe {
            *result_start = start;
            *result_end = end;
        }
        Ok(())
    }
    fn GetSelection(
        &self,
        index: u32,
        count: u32,
        selection: *mut TS_SELECTION_ACP,
        fetched: *mut u32,
    ) -> Result<()> {
        diag::note("GetSelection", 0);
        let shared = self.read()?;
        if count == 0 || (index != 0 && index != TS_DEFAULT_SELECTION) {
            // SAFETY: the caller's out pointer.
            unsafe { *fetched = 0 };
            return Ok(());
        }
        let (start, end, at_start) = shared.store.selection();
        // SAFETY: the caller's out pointers, for `count` (at least one).
        unsafe {
            *selection = TS_SELECTION_ACP {
                acpStart: acp(start),
                acpEnd: acp(end),
                style: TS_SELECTIONSTYLE {
                    ase: if at_start { TS_AE_START } else { TS_AE_END },
                    fInterimChar: false.into(),
                },
            };
            *fetched = 1;
        }
        Ok(())
    }
    fn SetSelection(&self, count: u32, selection: *const TS_SELECTION_ACP) -> Result<()> {
        diag::note("SetSelection", 0);
        let mut shared = self.write()?;
        if count == 0 {
            return Ok(());
        }
        // SAFETY: at least one selection from the caller.
        let s = unsafe { *selection };
        shared
            .store
            .set_selection(at(s.acpStart), at(s.acpEnd), s.style.ase == TS_AE_START);
        Ok(())
    }
    fn GetText(
        &self,
        start: i32,
        end: i32,
        text: PWSTR,
        text_wanted: u32,
        text_given: *mut u32,
        runs: *mut TS_RUNINFO,
        runs_wanted: u32,
        runs_given: *mut u32,
        next: *mut i32,
    ) -> Result<()> {
        diag::note("GetText", 0);
        let shared = self.read()?;
        let len = shared.store.len();
        let end = if end < 0 { len } else { at(end).min(len) };
        let units = shared.store.text(at(start), end);
        let n = units.len().min(text_wanted as usize);
        // SAFETY: the caller's buffers, `text_wanted` and `runs_wanted` long.
        unsafe {
            if !text.is_null() && n > 0 {
                std::ptr::copy_nonoverlapping(units.as_ptr(), text.0, n);
            }
            *text_given = n as u32;
            if runs_wanted > 0 && !runs.is_null() && n > 0 {
                *runs = TS_RUNINFO {
                    uCount: n as u32,
                    r#type: TS_RT_PLAIN,
                };
                *runs_given = 1;
            } else if !runs_given.is_null() {
                *runs_given = 0;
            }
            *next = acp(at(start) + n);
        }
        Ok(())
    }
    fn SetText(
        &self,
        _flags: u32,
        start: i32,
        end: i32,
        text: &PCWSTR,
        cch: u32,
    ) -> Result<TS_TEXTCHANGE> {
        diag::note("SetText", 0);
        let mut shared = self.write()?;
        // SAFETY: `cch` units at `text`, from the caller.
        let units = unsafe { std::slice::from_raw_parts(text.0, cch as usize) };
        let range = shared
            .store
            .replace(at(start), at(end), units)
            .unwrap_or(at(start)..at(start));
        shared.told_len = shared.store.len();
        Ok(TS_TEXTCHANGE {
            acpStart: start,
            acpOldEnd: end,
            acpNewEnd: acp(range.end),
        })
    }
    fn GetFormattedText(&self, _start: i32, _end: i32) -> Result<IDataObject> {
        diag::note("GetFormattedText", 0);
        Err(E_NOTIMPL.into())
    }
    fn GetEmbedded(&self, _at: i32, _service: *const GUID, _riid: *const GUID) -> Result<IUnknown> {
        diag::note("GetEmbedded", 0);
        Err(E_NOTIMPL.into())
    }
    fn QueryInsertEmbedded(
        &self,
        _service: *const GUID,
        _format: *const FORMATETC,
    ) -> Result<BOOL> {
        diag::note("QueryInsertEmbedded", 0);
        Ok(false.into())
    }
    fn InsertEmbedded(
        &self,
        _flags: u32,
        _start: i32,
        _end: i32,
        _data: Ref<IDataObject>,
    ) -> Result<TS_TEXTCHANGE> {
        diag::note("InsertEmbedded", 0);
        Err(E_NOTIMPL.into())
    }
    fn InsertTextAtSelection(
        &self,
        flags: u32,
        text: &PCWSTR,
        cch: u32,
        start_out: *mut i32,
        end_out: *mut i32,
        change: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        diag::note("InsertTextAtSelection", 0);
        let mut shared = self.write()?;
        let (start, end, _) = shared.store.selection();
        let range = if flags & TF_IAS_QUERYONLY.0 != 0 {
            start..end
        } else {
            // SAFETY: `cch` units at `text`, from the caller.
            let units = unsafe { std::slice::from_raw_parts(text.0, cch as usize) };
            let range = shared
                .store
                .replace(start, end, units)
                .unwrap_or(start..start);
            shared.told_len = shared.store.len();
            range
        };
        // SAFETY: the caller's out pointers, each optional.
        unsafe {
            if !start_out.is_null() {
                *start_out = acp(range.start);
            }
            if !end_out.is_null() {
                *end_out = acp(range.end);
            }
            if !change.is_null() {
                *change = TS_TEXTCHANGE {
                    acpStart: acp(start),
                    acpOldEnd: acp(end),
                    acpNewEnd: acp(range.end),
                };
            }
        }
        Ok(())
    }
    fn InsertEmbeddedAtSelection(
        &self,
        _flags: u32,
        _data: Ref<IDataObject>,
        _start: *mut i32,
        _end: *mut i32,
        _change: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        diag::note("InsertEmbeddedAtSelection", 0);
        Err(E_NOTIMPL.into())
    }
    fn RequestSupportedAttrs(&self, _flags: u32, _count: u32, _attrs: *const GUID) -> Result<()> {
        diag::note("RequestSupportedAttrs", 0);
        Ok(())
    }
    fn RequestAttrsAtPosition(
        &self,
        _at: i32,
        _count: u32,
        _attrs: *const GUID,
        _flags: u32,
    ) -> Result<()> {
        diag::note("RequestAttrsAtPosition", 0);
        Ok(())
    }
    fn RequestAttrsTransitioningAtPosition(
        &self,
        _at: i32,
        _count: u32,
        _attrs: *const GUID,
        _flags: u32,
    ) -> Result<()> {
        diag::note("RequestAttrsTransitioningAtPosition", 0);
        Ok(())
    }
    fn FindNextAttrTransition(
        &self,
        _start: i32,
        halt: i32,
        _count: u32,
        _attrs: *const GUID,
        _flags: u32,
        next: *mut i32,
        found: *mut BOOL,
        offset: *mut i32,
    ) -> Result<()> {
        diag::note("FindNextAttrTransition", 0);
        // SAFETY: the caller's out pointers.
        unsafe {
            *next = halt;
            *found = false.into();
            *offset = 0;
        }
        Ok(())
    }
    fn RetrieveRequestedAttrs(
        &self,
        _count: u32,
        _values: *mut TS_ATTRVAL,
        fetched: *mut u32,
    ) -> Result<()> {
        diag::note("RetrieveRequestedAttrs", 0);
        // SAFETY: the caller's out pointer.
        unsafe { *fetched = 0 };
        Ok(())
    }
    fn GetEndACP(&self) -> Result<i32> {
        diag::note("GetEndACP", 0);
        Ok(acp(self.read()?.store.len()))
    }
    fn GetActiveView(&self) -> Result<u32> {
        diag::note("GetActiveView", 0);
        Ok(VIEW)
    }
    fn GetACPFromPoint(&self, _view: u32, _point: *const POINT, _flags: u32) -> Result<i32> {
        diag::note("GetACPFromPoint", 0);
        Err(E_NOTIMPL.into())
    }
    fn GetTextExt(
        &self,
        _view: u32,
        _start: i32,
        _end: i32,
        rect: *mut RECT,
        clipped: *mut BOOL,
    ) -> Result<()> {
        diag::note("GetTextExt", 0);
        let shared = self.read()?;
        let area = shared
            .store
            .editor()
            .map(EditorHandle::input_method_area)
            .unwrap_or_default();
        // SAFETY: the caller's out pointers.
        unsafe {
            *rect = Store::screen(&shared, area);
            *clipped = false.into();
        }
        Ok(())
    }
    fn GetScreenExt(&self, _view: u32) -> Result<RECT> {
        diag::note("GetScreenExt", 0);
        let shared = self.0.borrow();
        let mut client = RECT::default();
        // SAFETY: a valid window and rect.
        unsafe {
            let _ = GetClientRect(shared.hwnd, &mut client);
            let mut origin = POINT::default();
            let _ = ClientToScreen(shared.hwnd, &mut origin);
            client.left += origin.x;
            client.right += origin.x;
            client.top += origin.y;
            client.bottom += origin.y;
        }
        Ok(client)
    }
}

impl ITfContextOwnerCompositionSink_Impl for Store_Impl {
    fn OnStartComposition(&self, view: Ref<ITfCompositionView>) -> Result<BOOL> {
        diag::note("OnStartComposition", 0);
        let start = extent(view.ok()?).map_or(0, |(start, _)| start);
        self.0.borrow_mut().store.start_composition(start..start);
        Ok(true.into())
    }
    fn OnUpdateComposition(
        &self,
        _view: Ref<ITfCompositionView>,
        _range: Ref<ITfRange>,
    ) -> Result<()> {
        diag::note("OnUpdateComposition", 0);
        // The composition's text came through SetText; its clauses
        // (display attributes) are read from S1 on.
        Ok(())
    }
    fn OnEndComposition(&self, _view: Ref<ITfCompositionView>) -> Result<()> {
        diag::note("OnEndComposition", 0);
        let mut shared = self.0.borrow_mut();
        shared.store.end_composition();
        shared.told_len = shared.store.len();
        Ok(())
    }
}

/// A composition's range in the document: its start and length.
fn extent(view: &ITfCompositionView) -> Option<(usize, usize)> {
    // SAFETY: a range the composition gives, read through its ACP view.
    unsafe {
        let range: ITfRangeACP = view.GetRange().ok()?.cast().ok()?;
        let (mut start, mut len) = (0, 0);
        range.GetExtent(&mut start, &mut len).ok()?;
        Some((at(start), at(len)))
    }
}
