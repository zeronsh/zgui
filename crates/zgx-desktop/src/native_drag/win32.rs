//! `DoDragDrop` with Shell's data object (`SHCreateDataObject`) carrying the
//! items as Windows apps read them (text, `CF_HDROP` files, URLs, a PNG), the
//! preview's last frame as its image (`IDragSourceHelper`: Windows draws and
//! moves it), and a drop source reporting the pointer back to the host.
//!
//! `DoDragDrop` runs a modal loop of its own. It is begun from a message to
//! a window of this module's, so winit's loop is not inside a callback of the
//! app's then, and its windows go on painting and animating from within it,
//! as through a live resize's.
use super::{DragImage, DragItem, NativeDrag};
use std::{
    cell::{Cell, RefCell},
    mem::ManuallyDrop,
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{
            COLORREF, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, GlobalFree,
            HGLOBAL, HWND, LPARAM, LRESULT, POINT, S_OK, SIZE, WPARAM,
        },
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
            HBITMAP,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, CoCreateInstance, DVASPECT_CONTENT, FORMATETC, IDataObject,
                STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
            },
            DataExchange::RegisterClipboardFormatW,
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::{
                CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
                DROPEFFECT_NONE, DoDragDrop, IDropSource, IDropSource_Impl,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
            Threading::GetCurrentProcessId,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::{
                CLSID_DragDropHelper, DROPFILES, DSH_ALLOWDROPDESCRIPTIONTEXT, IDragSourceHelper,
                IDragSourceHelper2, SHCreateDataObject, SHDRAGIMAGE,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, GetCursorPos, GetWindowThreadProcessId,
                HWND_MESSAGE, PostMessageW, RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_APP, WNDCLASSEXW, WindowFromPoint,
            },
        },
    },
    core::{BOOL, HRESULT, Interface, PCWSTR, implement, w},
};

/// The message that begins the drag waiting in `PENDING`.
const BEGIN: u32 = WM_APP + 0x7a;

/// A drag handed to Windows: where its pointer is, and what the host hears
/// of it.
struct Session {
    send: Box<dyn Fn(NativeDrag)>,
    /// The source window's scale: global points are its.
    scale: f64,
    at: Cell<POINT>,
    /// Let go over one of this app's windows: its own destinations take it
    /// there, not Windows' (winit's file drop target).
    own: Cell<bool>,
}
impl Session {
    /// The pointer, reported as it moves.
    fn track(&self) -> POINT {
        let mut point = POINT::default();
        // SAFETY: writes the pointer's position into a local.
        if unsafe { GetCursorPos(&mut point) }.is_ok() && point != self.at.get() {
            self.at.set(point);
            let (x, y) = self.global(point);
            (self.send)(NativeDrag::Moved { x, y });
        }
        self.at.get()
    }
    fn global(&self, point: POINT) -> (f64, f64) {
        (point.x as f64 / self.scale, point.y as f64 / self.scale)
    }
}

#[implement(IDropSource)]
struct Source(Rc<Session>);
impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        let at = self.0.track();
        if escape.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        if keys.0 & MK_LBUTTON.0 != 0 {
            return S_OK;
        }
        if over_own_window(at) {
            self.0.own.set(true);
            return DRAGDROP_S_CANCEL;
        }
        DRAGDROP_S_DROP
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Whether a window of this process is at screen point `at`.
fn over_own_window(at: POINT) -> bool {
    let mut process = 0;
    // SAFETY: reads the window at a point, and its process, into a local.
    unsafe {
        let window = WindowFromPoint(at);
        if window.is_invalid() {
            return false;
        }
        GetWindowThreadProcessId(window, Some(&mut process));
        process == GetCurrentProcessId()
    }
}

thread_local! {
    /// The drag `begin` handed over, until its message begins it.
    static PENDING: RefCell<Option<(IDataObject, Rc<Session>)>> = const { RefCell::new(None) };
    /// The message-only window beginning drags.
    static STARTER: Cell<Option<isize>> = const { Cell::new(None) };
}

/// # Safety
///
/// Called by Windows alone, as the `ZgxDragStarter` class's procedure.
unsafe extern "system" fn starter_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == BEGIN {
        if let Some((data, session)) = PENDING.with(|pending| pending.borrow_mut().take()) {
            run(&data, session);
        }
        return LRESULT(0);
    }
    // SAFETY: the default procedure, for our own window.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn starter() -> Option<HWND> {
    if let Some(hwnd) = STARTER.get() {
        return Some(HWND(hwnd as *mut _));
    }
    let name = w!("ZgxDragStarter");
    // SAFETY: a class of this module's procedure, and a message-only window
    // of it on this thread.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(starter_proc),
            hInstance: instance.into(),
            lpszClassName: name,
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            name,
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
        .ok()?
    };
    STARTER.set(Some(hwnd.0 as isize));
    Some(hwnd)
}

/// `DoDragDrop`, then the drag's end to the host.
fn run(data: &IDataObject, session: Rc<Session>) {
    let source: IDropSource = Source(session.clone()).into();
    let mut effect = DROPEFFECT_NONE;
    // SAFETY: our data object and source, alive across the call, on the
    // thread that made them: winit's, which winit initialized OLE on (a
    // single-threaded apartment) as it made its windows, as DoDragDrop
    // needs. DoDragDrop calls the source on this thread alone.
    let result = unsafe {
        DoDragDrop(
            data,
            &source,
            DROPEFFECT_COPY | DROPEFFECT_LINK,
            &mut effect,
        )
    };
    let at = session.track();
    let (x, y) = session.global(at);
    let accepted = result == DRAGDROP_S_DROP && effect != DROPEFFECT_NONE && !session.own.get();
    (session.send)(NativeDrag::Ended { x, y, accepted });
}

/// `bytes` in memory a data object can own.
fn global(bytes: &[u8]) -> Option<HGLOBAL> {
    // SAFETY: a movable block of at least `bytes`' length, ours until handed
    // over; filled while locked (`at` points at its start then).
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).ok()?;
        let at = GlobalLock(handle);
        if at.is_null() {
            let _ = GlobalFree(Some(handle));
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(handle);
        Some(handle)
    }
}
/// Put `bytes` on `data` as clipboard format `format`.
fn set(data: &IDataObject, format: u16, bytes: &[u8]) -> bool {
    let Some(handle) = global(bytes) else {
        return false;
    };
    let format = FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        pUnkForRelease: ManuallyDrop::new(None),
    };
    // SAFETY: `format` and `medium` are locals read during the call. With
    // `fRelease` the data object owns the block once the call succeeds;
    // when it fails the block is still ours, and freed.
    if unsafe { data.SetData(&format, &medium, true) }.is_ok() {
        return true;
    }
    // SAFETY: our block, not taken.
    let _ = unsafe { GlobalFree(Some(handle)) };
    false
}
fn registered(name: PCWSTR) -> u16 {
    // SAFETY: registers (or finds) a format by a name the caller keeps
    // alive for the call (a `w!` literal).
    unsafe { RegisterClipboardFormatW(name) as u16 }
}
fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}
/// The items as Windows apps read them: text (`CF_UNICODETEXT`), files
/// (`CF_HDROP`), a URL (`UniformResourceLocatorW` and its ANSI twin, and as
/// text where there is none), an image (`PNG`). Whether any was put.
fn fill(data: &IDataObject, items: &[DragItem]) -> bool {
    let mut texts = Vec::new();
    let mut files = Vec::new();
    let mut put = false;
    for item in items {
        match item {
            DragItem::Text(text) => texts.push(text.clone()),
            DragItem::File(path) => files.push(path.clone()),
            DragItem::Url(url) => {
                put |= set(data, registered(w!("UniformResourceLocatorW")), &wide(url));
                let mut ansi = url.as_bytes().to_vec();
                ansi.push(0);
                put |= set(data, registered(w!("UniformResourceLocator")), &ansi);
                if !items.iter().any(|item| matches!(item, DragItem::Text(_))) {
                    texts.push(url.clone());
                }
            }
            DragItem::Image(bytes) => put |= set(data, registered(w!("PNG")), bytes),
        }
    }
    if !texts.is_empty() {
        put |= set(data, CF_UNICODETEXT.0, &wide(&texts.join("\r\n")));
    }
    if !files.is_empty() {
        put |= set(data, CF_HDROP.0, &drop_files(&files));
    }
    put
}
/// `CF_HDROP`'s block: a DROPFILES header, then each path NUL-terminated,
/// then a NUL.
fn drop_files(files: &[std::path::PathBuf]) -> Vec<u8> {
    let header = DROPFILES {
        pFiles: size_of::<DROPFILES>() as u32,
        fWide: true.into(),
        ..Default::default()
    };
    // SAFETY: the bytes of a `repr(C)` struct of five 4-byte fields (a u32,
    // a POINT's two i32s, two BOOLs): no padding, so all are initialized.
    let mut bytes = unsafe {
        std::slice::from_raw_parts(
            (&header as *const DROPFILES).cast::<u8>(),
            size_of::<DROPFILES>(),
        )
    }
    .to_vec();
    for path in files {
        let path = path.as_os_str().to_string_lossy();
        bytes.extend(path.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
    }
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// `image` as a 32-bit top-down DIB of premultiplied BGRA, as the drag image
/// helper takes it.
fn bitmap(image: &DragImage) -> Option<HBITMAP> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width as i32,
            biHeight: -(image.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: a section of `width * height * 4` bytes, filled below.
    unsafe {
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(bitmap.into());
            return None;
        }
        let length = (image.width * image.height * 4) as usize;
        std::ptr::copy_nonoverlapping(
            image.bgra.as_ptr(),
            bits.cast::<u8>(),
            length.min(image.bgra.len()),
        );
        Some(bitmap)
    }
}
/// Show `image` as the drag's, the pointer at its grab point.
fn set_image(data: &IDataObject, image: &DragImage) {
    let Some(bitmap) = bitmap(image) else {
        return;
    };
    let drag_image = SHDRAGIMAGE {
        sizeDragImage: SIZE {
            cx: image.width as i32,
            cy: image.height as i32,
        },
        ptOffset: POINT {
            x: (image.grab.0 * image.scale).round() as i32,
            y: (image.grab.1 * image.scale).round() as i32,
        },
        hbmpDragImage: bitmap,
        // No colour key: its alpha says what shows.
        crColorKey: COLORREF(0xffff_ffff),
    };
    // SAFETY: Shell's drag image helper; it owns the bitmap once it has
    // taken it, else it is ours to delete.
    let taken = unsafe {
        CoCreateInstance::<_, IDragSourceHelper>(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER)
            .and_then(|helper| {
                // A target that describes the drop ("Copy to Documents",
                // Explorer's) has it shown under the image.
                if let Ok(helper) = helper.cast::<IDragSourceHelper2>() {
                    let _ = helper.SetFlags(DSH_ALLOWDROPDESCRIPTIONTEXT.0 as u32);
                }
                helper.InitializeFromBitmap(&drag_image, data)
            })
            .is_ok()
    };
    if !taken {
        // SAFETY: our bitmap, not taken.
        let _ = unsafe { DeleteObject(bitmap.into()) };
    }
}

/// Hand the drag under way in `window` (its button down) to Windows,
/// carrying `items`: it begins as winit's loop next turns. False when it
/// cannot (nothing to carry).
pub(crate) fn begin(
    window: &winit::window::Window,
    items: &[DragItem],
    image: Option<DragImage>,
    send: impl Fn(NativeDrag) + 'static,
) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return false;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    // SAFETY: Shell's own data object, empty.
    let Ok(data) = (unsafe { SHCreateDataObject::<_, IDataObject>(None, None, None) }) else {
        return false;
    };
    if !fill(&data, items) {
        return false;
    }
    if let Some(image) = &image {
        set_image(&data, image);
    }
    let Some(starter) = starter() else {
        return false;
    };
    // SAFETY: reads our own window's DPI.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let session = Rc::new(Session {
        send: Box::new(send),
        scale: f64::from(dpi.max(96)) / 96.,
        at: Cell::new(POINT::default()),
        own: Cell::new(false),
    });
    PENDING.with(|pending| *pending.borrow_mut() = Some((data, session)));
    // SAFETY: a message to our own window.
    unsafe { PostMessageW(Some(starter), BEGIN, WPARAM(0), LPARAM(0)) }.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_go_as_a_wide_drop_files_list() {
        let bytes = drop_files(&["C:\\a.txt".into(), "C:\\b".into()]);
        let header = size_of::<DROPFILES>();
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            header as u32
        );
        let units: Vec<u16> = bytes[header..]
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let expected: Vec<u16> = "C:\\a.txt\0C:\\b\0\0".encode_utf16().collect();
        assert_eq!(units, expected);
    }
}
