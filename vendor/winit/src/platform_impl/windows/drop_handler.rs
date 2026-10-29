use std::ffi::{c_void, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::core::{IUnknown, GUID, HRESULT};
use windows_sys::Win32::Foundation::{DV_E_FORMATETC, HWND, POINTL, S_OK};
use windows_sys::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::{
    ReleaseStgMedium, CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};

use tracing::debug;

use crate::platform_impl::platform::definitions::{
    IDataObjectVtbl, IDropTarget, IDropTargetVtbl, IUnknownVtbl,
};
use crate::platform_impl::platform::WindowId;

use crate::event::Event;
use crate::window::WindowId as RootWindowId;

#[repr(C)]
pub struct FileDropHandlerData {
    pub interface: IDropTarget,
    refcount: AtomicUsize,
    window: HWND,
    send_event: Box<dyn Fn(Event<()>)>,
    cursor_effect: u32,
    hovered_is_valid: bool, /* If the currently hovered item is not valid there must not be any
                             * `HoveredFileCancelled` emitted */
    /// zgx patch: the drag carries text, not files (`platform::text_drop`).
    text: bool,
    /// zgx patch: where a drag carrying files was last said to be
    /// (`CursorMoved`), in the content's physical pixels.
    file_point: Option<(f64, f64)>,
}

pub struct FileDropHandler {
    pub data: *mut FileDropHandlerData,
}

#[allow(non_snake_case)]
impl FileDropHandler {
    pub fn new(window: HWND, send_event: Box<dyn Fn(Event<()>)>) -> FileDropHandler {
        let data = Box::new(FileDropHandlerData {
            interface: IDropTarget { lpVtbl: &DROP_TARGET_VTBL as *const IDropTargetVtbl },
            refcount: AtomicUsize::new(1),
            window,
            send_event,
            cursor_effect: DROPEFFECT_NONE,
            hovered_is_valid: false,
            text: false,
            file_point: None,
        });
        FileDropHandler { data: Box::into_raw(data) }
    }

    // Implement IUnknown
    pub unsafe extern "system" fn QueryInterface(
        _this: *mut IUnknown,
        _riid: *const GUID,
        _ppvObject: *mut *mut c_void,
    ) -> HRESULT {
        // This function doesn't appear to be required for an `IDropTarget`.
        // An implementation would be nice however.
        unimplemented!();
    }

    pub unsafe extern "system" fn AddRef(this: *mut IUnknown) -> u32 {
        let drop_handler_data = unsafe { Self::from_interface(this) };
        let count = drop_handler_data.refcount.fetch_add(1, Ordering::Release) + 1;
        count as u32
    }

    pub unsafe extern "system" fn Release(this: *mut IUnknown) -> u32 {
        let drop_handler = unsafe { Self::from_interface(this) };
        let count = drop_handler.refcount.fetch_sub(1, Ordering::Release) - 1;
        if count == 0 {
            // Destroy the underlying data
            drop(unsafe { Box::from_raw(drop_handler as *mut FileDropHandlerData) });
        }
        count as u32
    }

    pub unsafe extern "system" fn DragEnter(
        this: *mut IDropTarget,
        pDataObj: *const IDataObject,
        _grfKeyState: u32,
        pt: POINTL,
        pdwEffect: *mut u32,
    ) -> HRESULT {
        use crate::event::WindowEvent::HoveredFile;
        let drop_handler = unsafe { Self::from_interface(this) };
        drop_handler.file_point = None;
        let mut files = Vec::new();
        let hdrop = unsafe { Self::iterate_filenames(pDataObj, |filename| files.push(filename)) };
        // zgx patch: the files are where the drag is (ZGX_PATCH 12).
        if !files.is_empty() {
            unsafe { drop_handler.files_at(pt) };
        }
        for filename in files {
            drop_handler.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(drop_handler.window)),
                event: HoveredFile(filename),
            });
        }
        drop_handler.hovered_is_valid = hdrop.is_some();
        // zgx patch: text, not files, goes where the window says.
        drop_handler.text = hdrop.is_none()
            && crate::platform::text_drop::text_drops().is_some()
            && unsafe { Self::has_text(pDataObj) };
        if drop_handler.text {
            unsafe { *pdwEffect = drop_handler.text_over(pt) };
            return S_OK;
        }
        drop_handler.cursor_effect =
            if drop_handler.hovered_is_valid { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
        unsafe {
            *pdwEffect = drop_handler.cursor_effect;
        }

        S_OK
    }

    pub unsafe extern "system" fn DragOver(
        this: *mut IDropTarget,
        _grfKeyState: u32,
        pt: POINTL,
        pdwEffect: *mut u32,
    ) -> HRESULT {
        let drop_handler = unsafe { Self::from_interface(this) };
        if drop_handler.text {
            unsafe { *pdwEffect = drop_handler.text_over(pt) };
            return S_OK;
        }
        if drop_handler.hovered_is_valid {
            unsafe { drop_handler.files_at(pt) };
        }
        unsafe {
            *pdwEffect = drop_handler.cursor_effect;
        }

        S_OK
    }

    pub unsafe extern "system" fn DragLeave(this: *mut IDropTarget) -> HRESULT {
        use crate::event::WindowEvent::HoveredFileCancelled;
        let drop_handler = unsafe { Self::from_interface(this) };
        if std::mem::take(&mut drop_handler.text) {
            if let Some(drops) = crate::platform::text_drop::text_drops() {
                drops.out(drop_handler.id());
            }
            return S_OK;
        }
        if drop_handler.hovered_is_valid {
            drop_handler.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(drop_handler.window)),
                event: HoveredFileCancelled,
            });
            // zgx patch: the drag, and the pointer with it, left.
            if drop_handler.file_point.take().is_some() {
                drop_handler.send_event(Event::WindowEvent {
                    window_id: RootWindowId(WindowId(drop_handler.window)),
                    event: crate::event::WindowEvent::CursorLeft { device_id: super::DEVICE_ID },
                });
            }
        }

        S_OK
    }

    pub unsafe extern "system" fn Drop(
        this: *mut IDropTarget,
        pDataObj: *const IDataObject,
        _grfKeyState: u32,
        pt: POINTL,
        _pdwEffect: *mut u32,
    ) -> HRESULT {
        use crate::event::WindowEvent::DroppedFile;
        let drop_handler = unsafe { Self::from_interface(this) };
        if std::mem::take(&mut drop_handler.text) {
            let drops = crate::platform::text_drop::text_drops();
            let (x, y) = unsafe { drop_handler.client_point(pt) };
            let taken = match (unsafe { Self::text(pDataObj) }, &drops) {
                (Some(text), Some(drops)) => drops.dropped(drop_handler.id(), x, y, &text),
                _ => false,
            };
            if !taken {
                if let Some(drops) = drops {
                    drops.out(drop_handler.id());
                }
            }
            unsafe { *_pdwEffect = if taken { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
            return S_OK;
        }
        let mut files = Vec::new();
        let hdrop = unsafe { Self::iterate_filenames(pDataObj, |filename| files.push(filename)) };
        // zgx patch: dropped where the drag was let go of, not where the
        // pointer last moved (an OLE drag sends the window no mouse moves).
        if !files.is_empty() {
            unsafe { drop_handler.files_at(pt) };
        }
        for filename in files {
            drop_handler.send_event(Event::WindowEvent {
                window_id: RootWindowId(WindowId(drop_handler.window)),
                event: DroppedFile(filename),
            });
        }
        drop_handler.file_point = None;
        if let Some(hdrop) = hdrop {
            unsafe { DragFinish(hdrop) };
        }

        S_OK
    }

    /// zgx patch: `CF_UNICODETEXT` in an `HGLOBAL`.
    fn text_format() -> FORMATETC {
        FORMATETC {
            cfFormat: CF_UNICODETEXT,
            ptd: ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL as u32,
        }
    }

    /// zgx patch: whether the data object has text.
    unsafe fn has_text(data_obj: *const IDataObject) -> bool {
        let query = unsafe { (*(*data_obj).cast::<IDataObjectVtbl>()).QueryGetData };
        unsafe { query(data_obj as *mut _, &Self::text_format()) == S_OK }
    }

    /// zgx patch: the data object's text.
    unsafe fn text(data_obj: *const IDataObject) -> Option<String> {
        let mut medium = unsafe { std::mem::zeroed() };
        let get_data_fn = unsafe { (*(*data_obj).cast::<IDataObjectVtbl>()).GetData };
        if unsafe { get_data_fn(data_obj as *mut _, &Self::text_format(), &mut medium) } < 0 {
            return None;
        }
        let global = unsafe { medium.u.hGlobal };
        let units = unsafe { GlobalLock(global) } as *const u16;
        let text = (!units.is_null()).then(|| {
            // NUL-terminated UTF-16.
            let length = (0..).take_while(|&i| unsafe { *units.add(i) } != 0).count();
            let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(units, length) });
            unsafe { GlobalUnlock(global) };
            text
        });
        unsafe { ReleaseStgMedium(&mut medium) };
        text
    }

    unsafe fn from_interface<'a, InterfaceT>(this: *mut InterfaceT) -> &'a mut FileDropHandlerData {
        unsafe { &mut *(this as *mut _) }
    }

    unsafe fn iterate_filenames<F>(data_obj: *const IDataObject, mut callback: F) -> Option<HDROP>
    where
        F: FnMut(PathBuf),
    {
        let drop_format = FORMATETC {
            cfFormat: CF_HDROP,
            ptd: ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL as u32,
        };

        let mut medium = unsafe { std::mem::zeroed() };
        let get_data_fn = unsafe { (*(*data_obj).cast::<IDataObjectVtbl>()).GetData };
        let get_data_result = unsafe { get_data_fn(data_obj as *mut _, &drop_format, &mut medium) };
        if get_data_result >= 0 {
            let hdrop = unsafe { medium.u.hGlobal as HDROP };

            // The second parameter (0xFFFFFFFF) instructs the function to return the item count
            let item_count = unsafe { DragQueryFileW(hdrop, 0xffffffff, ptr::null_mut(), 0) };

            for i in 0..item_count {
                // Get the length of the path string NOT including the terminating null character.
                // Previously, this was using a fixed size array of MAX_PATH length, but the
                // Windows API allows longer paths under certain circumstances.
                let character_count =
                    unsafe { DragQueryFileW(hdrop, i, ptr::null_mut(), 0) as usize };
                let str_len = character_count + 1;

                // Fill path_buf with the null-terminated file name
                let mut path_buf = Vec::with_capacity(str_len);
                unsafe {
                    DragQueryFileW(hdrop, i, path_buf.as_mut_ptr(), str_len as u32);
                    path_buf.set_len(str_len);
                }

                callback(OsString::from_wide(&path_buf[0..character_count]).into());
            }

            Some(hdrop)
        } else if get_data_result == DV_E_FORMATETC {
            // If the dropped item is not a file this error will occur.
            // In this case it is OK to return without taking further action.
            debug!("Error occurred while processing dropped/hovered item: item is not a file.");
            None
        } else {
            debug!("Unexpected error occurred while processing dropped/hovered item.");
            None
        }
    }
}

impl FileDropHandlerData {
    fn send_event(&self, event: Event<()>) {
        (self.send_event)(event);
    }

    fn id(&self) -> RootWindowId {
        RootWindowId(WindowId(self.window))
    }

    /// zgx patch: screen point `pt` (COM passes it by value) in the
    /// window's client pixels.
    /// zgx patch: a drag carrying files holds the pointer, so the window
    /// hears no mouse moves while it is over it: say where it is, as
    /// `CursorMoved` (in the content's physical pixels), whenever it moves,
    /// as macOS's window delegate does (ZGX_PATCH 12).
    unsafe fn files_at(&mut self, pt: POINTL) {
        let point = unsafe { self.client_point(pt) };
        if self.file_point == Some(point) {
            return;
        }
        self.file_point = Some(point);
        self.send_event(Event::WindowEvent {
            window_id: RootWindowId(WindowId(self.window)),
            event: crate::event::WindowEvent::CursorMoved {
                device_id: super::DEVICE_ID,
                position: crate::dpi::PhysicalPosition::new(point.0, point.1),
            },
        });
    }

    unsafe fn client_point(&self, pt: POINTL) -> (f64, f64) {
        let mut point = windows_sys::Win32::Foundation::POINT { x: pt.x, y: pt.y };
        unsafe { ScreenToClient(self.window, &mut point) };
        (point.x as f64, point.y as f64)
    }

    /// zgx patch: the text drag moved to screen point `pt`: a copy where
    /// the window would take it, else nothing.
    unsafe fn text_over(&self, pt: POINTL) -> u32 {
        let (x, y) = unsafe { self.client_point(pt) };
        let over = crate::platform::text_drop::text_drops()
            .is_some_and(|drops| drops.over(self.id(), x, y));
        if over {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        }
    }
}

impl Drop for FileDropHandler {
    fn drop(&mut self) {
        unsafe {
            FileDropHandler::Release(self.data as *mut IUnknown);
        }
    }
}

static DROP_TARGET_VTBL: IDropTargetVtbl = IDropTargetVtbl {
    parent: IUnknownVtbl {
        QueryInterface: FileDropHandler::QueryInterface,
        AddRef: FileDropHandler::AddRef,
        Release: FileDropHandler::Release,
    },
    DragEnter: FileDropHandler::DragEnter,
    DragOver: FileDropHandler::DragOver,
    DragLeave: FileDropHandler::DragLeave,
    Drop: FileDropHandler::Drop,
};
