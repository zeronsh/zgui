//! Windows' Text Services Framework for zgx's editors: a thread manager, a
//! document manager per window, and a context over an `ITextStoreACP2` of
//! zgx's own (`TextStore`, `text_store.rs`), so input methods and text
//! services read the focused editor's text around the caret, place their
//! candidates by the clause they convert, and insert where it is: the emoji
//! panel, dictation and the touch keyboard type into it, and an input
//! method's composition is its preedit.
//!
//! The default; IMM32's path (winit's) instead with
//! `Application::text_services(false)`, or `ZGX_TSF=0` (which, like
//! `ZGX_TSF=1`, overrides what the app chose). It is IMM32's too where the
//! framework does not start.
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
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemAlloc, FORMATETC, IDataObject},
            Variant::{VARIANT, VT_I4, VT_UNKNOWN},
        },
        UI::{
            TextServices::{
                CLSID_TF_CategoryMgr, CLSID_TF_DisplayAttributeMgr, CLSID_TF_ThreadMgr,
                GUID_PROP_ATTRIBUTE, GUID_PROP_INPUTSCOPE, ITextStoreACP2, ITextStoreACP2_Impl,
                ITextStoreACPSink, ITfCategoryMgr, ITfCompositionView, ITfContext,
                ITfContextOwnerCompositionServices, ITfContextOwnerCompositionSink,
                ITfContextOwnerCompositionSink_Impl, ITfDisplayAttributeInfo,
                ITfDisplayAttributeMgr, ITfDocumentMgr, ITfEditRecord, ITfInputScope,
                ITfInputScope_Impl, ITfRange, ITfRangeACP, ITfSource, ITfTextEditSink,
                ITfTextEditSink_Impl, ITfThreadMgr, InputScope, TEXT_STORE_LOCK_FLAGS,
                TEXT_STORE_TEXT_CHANGE_FLAGS, TF_ATTR_CONVERTED, TF_ATTR_FIXEDCONVERTED,
                TF_ATTR_TARGET_CONVERTED, TF_ATTR_TARGET_NOTCONVERTED, TF_DISPLAYATTRIBUTE,
                TF_IAS_QUERYONLY, TS_AE_END, TS_AE_START, TS_ATTRVAL, TS_DEFAULT_SELECTION,
                TS_E_NOLOCK, TS_E_SYNCHRONOUS, TS_LC_CHANGE, TS_LF_READ, TS_LF_READWRITE,
                TS_LF_SYNC, TS_RT_PLAIN, TS_RUNINFO, TS_S_ASYNC, TS_SELECTION_ACP,
                TS_SELECTIONSTYLE, TS_SS_NOHIDDENTEXT, TS_STATUS, TS_TEXTCHANGE,
            },
            WindowsAndMessaging::GetClientRect,
        },
    },
    core::{BOOL, BSTR, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result, implement},
};
use zgx::{scene::Rect, text_edit::ClauseStyle, widgets::EditorHandle};

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
    /// What reads an input method's display attributes (how it marks its
    /// composition's clauses), made when one first composes.
    attributes: Option<(ITfCategoryMgr, ITfDisplayAttributeMgr)>,
    /// The framework asked for the input scope (`RequestSupportedAttrs`),
    /// to fetch with `RetrieveRequestedAttrs`.
    scope_asked: bool,
    /// The input scope it was last told of (`retell_scope`).
    told_scope: i32,
}

/// The Text Services Framework for one window.
pub(crate) struct Tsf {
    thread: ITfThreadMgr,
    document: ITfDocumentMgr,
    _context: ITfContext,
    shared: Rc<RefCell<Shared>>,
    focused: bool,
    /// The context's text-edit sink (`OnEndEdit`), advised while it lives.
    edits: Option<u32>,
}

/// Whether windows use the framework (`Application::text_services`).
pub(crate) static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether a window uses the framework.
fn enabled() -> bool {
    choose(
        zgx::env::var_os("ZGX_TSF").as_deref(),
        ENABLED.load(std::sync::atomic::Ordering::Relaxed),
    )
}
/// `ZGX_TSF=0` or `1` if set (the user's), else what the app chose.
fn choose(env: Option<&std::ffi::OsStr>, app: bool) -> bool {
    match env {
        Some(value) => value != "0",
        None => app,
    }
}

impl Tsf {
    /// The framework for window `hwnd` (`scale`: its physical pixels per
    /// point), when it is on and starts; `None` keeps IMM32's path.
    pub(crate) fn new(hwnd: HWND, scale: f64) -> Option<Self> {
        if !enabled() {
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
                attributes: None,
                scope_asked: false,
                told_scope: 0,
            }));
            let store: IUnknown = Store(shared.clone()).into();
            let (mut context, mut cookie) = (None, 0);
            document
                .CreateContext(client, 0, &store, &mut context, &mut cookie)
                .ok()?;
            let context = context?;
            document.Push(&context).ok()?;
            // Told after each of an input method's edits, to read how it
            // marked its composition then.
            let edits = context
                .cast::<ITfSource>()
                .and_then(|source| source.AdviseSink(&ITfTextEditSink::IID, &store))
                .ok();
            Some(Self {
                thread,
                document,
                _context: context,
                shared,
                focused: false,
                edits,
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
        retell_scope(&self.shared);
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
        if self.shared.borrow().lock != 0 {
            return;
        }
        // The focused editor's input mode set while it has focus.
        retell_scope(&self.shared);
        let (sink, changes, layout) = {
            let mut shared = self.shared.borrow_mut();
            if shared.lock != 0 {
                return;
            }
            let told = shared.told_len;
            let changes = shared.store.changes(told);
            if !changes.is_empty() {
                shared.told_len = shared.store.len();
            }
            let layout = std::mem::take(&mut shared.layout_changed);
            // Nothing to tell or grant: most dispatches.
            if changes.is_empty() && !layout && shared.pending.is_none() {
                return;
            }
            (shared.sink.clone(), changes, layout)
        };
        let mut cancelled = false;
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
                        Change::CompositionCancelled => cancelled = true,
                    }
                }
                if layout {
                    let _ = sink.OnLayoutChange(TS_LC_CHANGE, VIEW);
                }
            }
        }
        // The editor dropped the composition (the app set its text or its
        // selection): the input method ends its own, with nothing of it left
        // to commit (its OnEndComposition finds none). No borrow is held
        // across the call: it calls back into the store.
        if cancelled
            && let Ok(services) = self._context.cast::<ITfContextOwnerCompositionServices>()
        {
            // SAFETY: the context's own services; no lock is held.
            let _ = unsafe { services.TerminateComposition(None) };
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
            if let (Some(cookie), Ok(source)) = (self.edits, self._context.cast::<ITfSource>()) {
                let _ = source.UnadviseSink(cookie);
            }
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

/// Tell the input method the input scope changed: focus moved to an editor
/// of another mode, or the focused one's mode changed. One context serves
/// every editor in the window, and an input method reads the scope again
/// only when told. (Nothing is borrowed across the call: the framework asks
/// for a lock in it.)
fn retell_scope(shared: &Rc<RefCell<Shared>>) {
    let (sink, len) = {
        let mut s = shared.borrow_mut();
        let scope = s.store.input_scope();
        if scope == s.told_scope {
            return;
        }
        s.told_scope = scope;
        (s.sink.clone(), s.store.len())
    };
    if let Some(sink) = sink {
        // SAFETY: the sink is alive while advised; no lock is held.
        let _ = unsafe { sink.OnAttrsChange(0, acp(len), &[GUID_PROP_INPUTSCOPE]) };
    }
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
    // What the input method edited under the lock it knows of: the
    // document's length from here.
    let mut s = shared.borrow_mut();
    s.lock = 0;
    s.told_len = s.store.len();
    result
}

#[implement(ITextStoreACP2, ITfContextOwnerCompositionSink, ITfTextEditSink)]
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
        // SAFETY: `riid` is a valid GUID from the caller.
        if unsafe { *riid } != ITextStoreACPSink::IID {
            return Err(E_INVALIDARG.into());
        }
        let sink: ITextStoreACPSink = punk.ok()?.cast()?;
        self.0.borrow_mut().sink = Some(sink);
        Ok(())
    }
    fn UnadviseSink(&self, _punk: Ref<IUnknown>) -> Result<()> {
        self.0.borrow_mut().sink = None;
        Ok(())
    }
    fn RequestLock(&self, flags: u32) -> Result<HRESULT> {
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
        Err(E_NOTIMPL.into())
    }
    fn GetEmbedded(&self, _at: i32, _service: *const GUID, _riid: *const GUID) -> Result<IUnknown> {
        Err(E_NOTIMPL.into())
    }
    fn QueryInsertEmbedded(
        &self,
        _service: *const GUID,
        _format: *const FORMATETC,
    ) -> Result<BOOL> {
        Ok(false.into())
    }
    fn InsertEmbedded(
        &self,
        _flags: u32,
        _start: i32,
        _end: i32,
        _data: Ref<IDataObject>,
    ) -> Result<TS_TEXTCHANGE> {
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
        Err(E_NOTIMPL.into())
    }
    fn RequestSupportedAttrs(&self, _flags: u32, count: u32, attrs: *const GUID) -> Result<()> {
        // The one attribute the store has: the editor's input scope.
        self.0.borrow_mut().scope_asked = asks_scope(count, attrs);
        Ok(())
    }
    fn RequestAttrsAtPosition(
        &self,
        _at: i32,
        count: u32,
        attrs: *const GUID,
        _flags: u32,
    ) -> Result<()> {
        // The scope is the whole document's, at every position.
        self.0.borrow_mut().scope_asked = asks_scope(count, attrs);
        Ok(())
    }
    fn RequestAttrsTransitioningAtPosition(
        &self,
        _at: i32,
        _count: u32,
        _attrs: *const GUID,
        _flags: u32,
    ) -> Result<()> {
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
        count: u32,
        values: *mut TS_ATTRVAL,
        fetched: *mut u32,
    ) -> Result<()> {
        let scope = {
            let mut shared = self.0.borrow_mut();
            std::mem::take(&mut shared.scope_asked).then(|| shared.store.input_scope())
        };
        let (Some(scope), true) = (scope, count > 0 && !values.is_null()) else {
            // SAFETY: the caller's out pointer.
            unsafe { *fetched = 0 };
            return Ok(());
        };
        // The scope as the framework takes it: an ITfInputScope in a
        // VT_UNKNOWN, which the caller releases.
        let mut value = VARIANT::default();
        let scopes: ITfInputScope = Scopes(InputScope(scope)).into();
        // SAFETY: a zeroed VARIANT, its type and pointer set as one; the
        // caller's buffer, `count` long, and out pointer.
        unsafe {
            let inner = &mut *value.Anonymous.Anonymous;
            inner.vt = VT_UNKNOWN;
            inner.Anonymous.punkVal = std::mem::ManuallyDrop::new(Some(scopes.into()));
            values.write(TS_ATTRVAL {
                idAttr: GUID_PROP_INPUTSCOPE,
                dwOverlapId: 0,
                varValue: value,
            });
            *fetched = 1;
        }
        Ok(())
    }
    fn GetEndACP(&self) -> Result<i32> {
        Ok(acp(self.read()?.store.len()))
    }
    fn GetActiveView(&self) -> Result<u32> {
        Ok(VIEW)
    }
    fn GetACPFromPoint(&self, _view: u32, _point: *const POINT, _flags: u32) -> Result<i32> {
        Err(E_NOTIMPL.into())
    }
    fn GetTextExt(
        &self,
        _view: u32,
        start: i32,
        end: i32,
        rect: *mut RECT,
        clipped: *mut BOOL,
    ) -> Result<()> {
        let shared = self.read()?;
        // Where the range shows; the caret's place where its text has no
        // lines to ask (a password's).
        let end = if end < 0 { shared.store.len() } else { at(end) };
        let area = shared
            .store
            .area(at(start), end)
            .or_else(|| shared.store.editor().map(EditorHandle::input_method_area))
            .unwrap_or_default();
        // SAFETY: the caller's out pointers.
        unsafe {
            *rect = Store::screen(&shared, area);
            *clipped = false.into();
        }
        Ok(())
    }
    fn GetScreenExt(&self, _view: u32) -> Result<RECT> {
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
        // Over the range it opens on: empty to compose at the caret, over
        // committed text to reconvert it.
        let (start, len) = extent(view.ok()?).unwrap_or((0, 0));
        self.0
            .borrow_mut()
            .store
            .start_composition(start..start + len);
        Ok(true.into())
    }
    fn OnUpdateComposition(
        &self,
        _view: Ref<ITfCompositionView>,
        _range: Ref<ITfRange>,
    ) -> Result<()> {
        // The composition's text came through SetText; its clauses (display
        // attributes) are read once the input method's edit ends (`OnEndEdit`).
        Ok(())
    }
    fn OnEndComposition(&self, _view: Ref<ITfCompositionView>) -> Result<()> {
        let mut shared = self.0.borrow_mut();
        shared.store.end_composition();
        shared.told_len = shared.store.len();
        Ok(())
    }
}

impl ITfTextEditSink_Impl for Store_Impl {
    fn OnEndEdit(
        &self,
        context: Ref<ITfContext>,
        read: u32,
        _record: Ref<ITfEditRecord>,
    ) -> Result<()> {
        // (No borrow is held across the framework's calls.)
        let attributes = {
            let mut shared = self.0.borrow_mut();
            if !shared.store.is_composing() {
                return Ok(());
            }
            if shared.attributes.is_none() {
                // SAFETY: COM is initialized on this thread (`Tsf::new`).
                shared.attributes = unsafe {
                    CoCreateInstance::<_, ITfCategoryMgr>(
                        &CLSID_TF_CategoryMgr,
                        None,
                        CLSCTX_INPROC_SERVER,
                    )
                    .and_then(|category| {
                        CoCreateInstance::<_, ITfDisplayAttributeMgr>(
                            &CLSID_TF_DisplayAttributeMgr,
                            None,
                            CLSCTX_INPROC_SERVER,
                        )
                        .map(|display| (category, display))
                    })
                    .ok()
                };
            }
            shared.attributes.clone()
        };
        let (Some(context), Some((category, display))) = (context.ok().ok(), attributes) else {
            return Ok(());
        };
        let clauses = clauses(context, read, &category, &display).unwrap_or_default();
        self.0.borrow_mut().store.mark_clauses(&clauses);
        Ok(())
    }
}

/// How the input method marks the document's text (its display attributes'
/// ranges), read with edit cookie `read`: each range and the clause it is.
fn clauses(
    context: &ITfContext,
    read: u32,
    category: &ITfCategoryMgr,
    display: &ITfDisplayAttributeMgr,
) -> Result<Vec<(std::ops::Range<usize>, ClauseStyle)>> {
    let mut clauses = Vec::new();
    // SAFETY: the context's own property and ranges, read within the edit
    // session `read` is for; every out pointer is valid for its call.
    unsafe {
        let property = context.GetProperty(&GUID_PROP_ATTRIBUTE)?;
        let mut ranges = None;
        property.EnumRanges(read, &mut ranges, None::<&ITfRange>)?;
        let Some(ranges) = ranges else {
            return Ok(clauses);
        };
        loop {
            let (mut range, mut fetched) = ([None], 0);
            if ranges.Next(&mut range, &mut fetched).is_err() || fetched == 0 {
                break;
            }
            let Some(range) = range[0].take() else {
                break;
            };
            let value = property.GetValue(read, &range)?;
            let value = &value.Anonymous.Anonymous;
            if value.vt != VT_I4 {
                continue;
            }
            let Ok(guid) = category.GetGUID(value.Anonymous.lVal as u32) else {
                continue;
            };
            let mut info: Option<ITfDisplayAttributeInfo> = None;
            if display
                .GetDisplayAttributeInfo(&guid, &mut info, std::ptr::null_mut())
                .is_err()
            {
                continue;
            }
            let mut attribute = TF_DISPLAYATTRIBUTE::default();
            let Some(info) = info else { continue };
            if info.GetAttributeInfo(&mut attribute).is_err() {
                continue;
            }
            let style = match attribute.bAttr {
                TF_ATTR_TARGET_CONVERTED | TF_ATTR_TARGET_NOTCONVERTED => ClauseStyle::Target,
                TF_ATTR_CONVERTED | TF_ATTR_FIXEDCONVERTED => ClauseStyle::Converted,
                // Input, an input error, another mark: typed, not converted.
                _ => ClauseStyle::Input,
            };
            let range: ITfRangeACP = range.cast()?;
            let (mut start, mut len) = (0, 0);
            range.GetExtent(&mut start, &mut len)?;
            clauses.push((at(start)..at(start) + at(len), style));
        }
    }
    Ok(clauses)
}

/// Whether `count` attributes at `attrs` ask for the input scope.
fn asks_scope(count: u32, attrs: *const GUID) -> bool {
    if attrs.is_null() {
        return false;
    }
    // SAFETY: the caller's `count` attributes.
    unsafe { std::slice::from_raw_parts(attrs, count as usize) }.contains(&GUID_PROP_INPUTSCOPE)
}

/// An editor's input scope (`text_store::input_scope`), as the framework
/// reads it: one scope, no phrases or grammar.
#[implement(ITfInputScope)]
struct Scopes(InputScope);

impl ITfInputScope_Impl for Scopes_Impl {
    fn GetInputScopes(&self, scopes: *mut *mut InputScope, count: *mut u32) -> Result<()> {
        // SAFETY: memory the caller frees with CoTaskMemFree, one scope
        // long; the caller's out pointers.
        unsafe {
            let array = CoTaskMemAlloc(std::mem::size_of::<InputScope>()) as *mut InputScope;
            if array.is_null() {
                return Err(windows::Win32::Foundation::E_OUTOFMEMORY.into());
            }
            array.write(self.0);
            *scopes = array;
            *count = 1;
        }
        Ok(())
    }
    fn GetPhrase(&self, phrases: *mut *mut BSTR, count: *mut u32) -> Result<()> {
        // SAFETY: the caller's out pointers.
        unsafe {
            *phrases = std::ptr::null_mut();
            *count = 0;
        }
        Err(E_NOTIMPL.into())
    }
    fn GetRegularExpression(&self) -> Result<BSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetSRGS(&self) -> Result<BSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetXML(&self) -> Result<BSTR> {
        Err(E_NOTIMPL.into())
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

#[cfg(test)]
mod tests {
    //! The store as the framework drives it, through its COM interfaces,
    //! with no framework: what zgx hands Windows. (What the touch keyboard
    //! does with it is Windows'.)
    use super::*;
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::TextServices::{
            IS_DIGITS, IS_EMAIL_SMTPEMAILADDRESS, ITextStoreACPSink_Impl, TsLayoutCode,
        },
    };
    use zgx::{compose::prelude::*, text_edit::InputMode, widgets::Ui};

    /// The framework is the default; the app turns it off, and the user's
    /// `ZGX_TSF` overrides the app either way.
    #[test]
    fn the_framework_is_the_default_and_the_environment_overrides_the_app() {
        use std::ffi::OsStr;
        assert!(choose(None, true), "the default");
        assert!(!choose(None, false), "the app's IMM32");
        assert!(!choose(Some(OsStr::new("0")), true), "the user's IMM32");
        assert!(choose(Some(OsStr::new("1")), false), "the user's framework");
    }

    /// A sink that keeps the attributes it was told changed.
    #[implement(ITextStoreACPSink)]
    struct Told(Rc<RefCell<Vec<GUID>>>);
    impl ITextStoreACPSink_Impl for Told_Impl {
        fn OnTextChange(
            &self,
            _flags: TEXT_STORE_TEXT_CHANGE_FLAGS,
            _change: *const TS_TEXTCHANGE,
        ) -> Result<()> {
            Ok(())
        }
        fn OnSelectionChange(&self) -> Result<()> {
            Ok(())
        }
        fn OnLayoutChange(&self, _code: TsLayoutCode, _view: u32) -> Result<()> {
            Ok(())
        }
        fn OnStatusChange(&self, _flags: u32) -> Result<()> {
            Ok(())
        }
        fn OnAttrsChange(
            &self,
            _start: i32,
            _end: i32,
            count: u32,
            attrs: *const GUID,
        ) -> Result<()> {
            // SAFETY: the caller's `count` attributes.
            let attrs = unsafe { std::slice::from_raw_parts(attrs, count as usize) };
            self.0.borrow_mut().extend_from_slice(attrs);
            Ok(())
        }
        fn OnLockGranted(&self, _flags: TEXT_STORE_LOCK_FLAGS) -> Result<()> {
            Ok(())
        }
        fn OnStartEditTransaction(&self) -> Result<()> {
            Ok(())
        }
        fn OnEndEditTransaction(&self) -> Result<()> {
            Ok(())
        }
    }

    /// The scopes the store answers for: GUID_PROP_INPUTSCOPE asked for,
    /// then fetched, its ITfInputScope read, as the framework does.
    fn scopes(store: &ITextStoreACP2) -> Vec<i32> {
        // SAFETY: the store's own methods, with buffers of the sizes given;
        // the scope array freed with CoTaskMemFree, the value released.
        unsafe {
            store
                .RequestSupportedAttrs(0, &[GUID_PROP_INPUTSCOPE])
                .unwrap();
            let mut values = [std::mem::zeroed::<TS_ATTRVAL>()];
            let mut fetched = 0;
            store
                .RetrieveRequestedAttrs(&mut values, &mut fetched)
                .unwrap();
            assert_eq!(fetched, 1);
            assert_eq!(values[0].idAttr, GUID_PROP_INPUTSCOPE);
            let inner = &values[0].varValue.Anonymous.Anonymous;
            assert_eq!(inner.vt, VT_UNKNOWN);
            // A reference of its own: the VARIANT releases the one it holds
            // as it drops, as the framework's VariantClear does.
            let unknown = (*inner.Anonymous.punkVal).clone().unwrap();
            let scope: ITfInputScope = unknown.cast().unwrap();
            let (mut array, mut count) = (std::ptr::null_mut(), 0);
            scope.GetInputScopes(&mut array, &mut count).unwrap();
            let read = std::slice::from_raw_parts(array, count as usize)
                .iter()
                .map(|s| s.0)
                .collect();
            CoTaskMemFree(Some(array as *const _));
            read
        }
    }

    /// Each editor's input mode is the scope the framework reads, told
    /// again when focus moves to another mode's editor and when the
    /// focused editor's mode changes.
    #[test]
    fn the_framework_reads_each_editors_input_scope() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::new());
        let view = ui.mount(
            column()
                .child(
                    text_input("Email", value.clone())
                        .id("email")
                        .input_mode(InputMode::Email),
                )
                .child(
                    text_input("PIN", value.clone())
                        .id("pin")
                        .input_mode(InputMode::Numeric),
                )
                .child(text_input("Name", value).id("name")),
        );
        ui.prepare_frame();
        let told = Rc::new(RefCell::new(Vec::new()));
        let sink: ITextStoreACPSink = Told(told.clone()).into();
        let shared = Rc::new(RefCell::new(Shared {
            store: TextStore::new(),
            sink: Some(sink),
            lock: 0,
            pending: None,
            hwnd: HWND::default(),
            scale: 1.,
            told_len: 0,
            layout_changed: false,
            attributes: None,
            scope_asked: false,
            told_scope: 0,
        }));
        let store: ITextStoreACP2 = Store(shared.clone()).into();
        let focus = |id: &str| {
            ui.input.focus(&ui.scene, view.find(id));
            shared.borrow_mut().store.focus(ui.focused_editor());
            retell_scope(&shared);
        };
        focus("email");
        assert_eq!(scopes(&store), [IS_EMAIL_SMTPEMAILADDRESS.0]);
        assert_eq!(told.borrow().as_slice(), [GUID_PROP_INPUTSCOPE]);
        focus("pin");
        assert_eq!(scopes(&store), [IS_DIGITS.0]);
        assert_eq!(told.borrow().len(), 2, "told as focus moved");
        // Its mode changed while it has focus: told after the dispatch.
        ui.focused_editor()
            .unwrap()
            .set_input_mode(InputMode::Email);
        retell_scope(&shared);
        assert_eq!(scopes(&store), [IS_EMAIL_SMTPEMAILADDRESS.0]);
        assert_eq!(told.borrow().len(), 3);
        // Nothing changed: nothing told.
        retell_scope(&shared);
        assert_eq!(told.borrow().len(), 3);
        focus("name");
        assert_eq!(scopes(&store), [0], "IS_DEFAULT");
        // Asked for nothing, the store fetches nothing.
        // SAFETY: the store's own method, with a buffer of the size given.
        unsafe {
            store.RequestSupportedAttrs(0, &[]).unwrap();
            let mut values = [std::mem::zeroed::<TS_ATTRVAL>()];
            let mut fetched = 7;
            store
                .RetrieveRequestedAttrs(&mut values, &mut fetched)
                .unwrap();
            assert_eq!(fetched, 0);
        }
    }
}
