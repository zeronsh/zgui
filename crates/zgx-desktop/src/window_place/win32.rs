//! Where a new window opens on Windows (see the module's notes): a saved
//! frame put back while it is hidden, kept as it changes, and a point of its
//! content put under the pointer.
use super::Placed;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{ClientToScreen, MONITOR_DEFAULTTONULL, MonitorFromRect},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS,
            REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetCursorPos, GetWindowPlacement, GetWindowRect, SW_HIDE, SW_SHOWMAXIMIZED,
                SW_SHOWMINIMIZED, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPlacement,
                SetWindowPos, WINDOWPLACEMENT, WM_DESTROY, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE,
                WM_NCDESTROY, WM_WINDOWPOSCHANGED, WPF_RESTORETOMAXIMIZED,
            },
        },
    },
    core::{HSTRING, PCWSTR},
};

const SUBCLASS_ID: usize = 0x7a67_7066; // "zgpf"
/// A title bar's height, in pixels at 100%: what of a saved frame a monitor
/// must still show for it to reopen there.
const TITLE: i32 = 32;

/// A saved frame: the window's restored frame (workspace coordinates, as
/// `WINDOWPLACEMENT` keeps them) and whether it was maximized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frame {
    rect: (i32, i32, i32, i32),
    maximized: bool,
}
impl Frame {
    fn encode(self) -> String {
        let (left, top, right, bottom) = self.rect;
        format!("{left} {top} {right} {bottom} {}", u8::from(self.maximized))
    }
    fn decode(text: &str) -> Option<Self> {
        let mut numbers = text.split_whitespace().map(str::parse::<i32>);
        let mut next = || numbers.next()?.ok();
        let rect = (next()?, next()?, next()?, next()?);
        let maximized = next()? != 0;
        (rect.2 > rect.0 && rect.3 > rect.1).then_some(Self { rect, maximized })
    }
    /// Its title bar, which a monitor must still show.
    fn title(self) -> RECT {
        let (left, top, right, _) = self.rect;
        RECT {
            left,
            top,
            right,
            bottom: top + TITLE,
        }
    }
}

fn hwnd(window: &winit::window::Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// The registry key frames are kept under: one per executable.
fn key_path() -> HSTRING {
    let app = std::env::current_exe()
        .ok()
        .and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "zgx".into());
    HSTRING::from(format!("Software\\zgx\\{app}\\Frames"))
}
fn key(access: REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    // SAFETY: opens (or makes) a key of the user's, into a local.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &key_path(),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
    }
    .ok()
    .ok()?;
    Some(key)
}
fn load(name: &str) -> Option<Frame> {
    let key = key(KEY_READ)?;
    let mut buffer = [0u16; 128];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: reads a string value into a local buffer, given its size in
    // bytes; `RRF_RT_REG_SZ` has it NUL-terminated, or fails if it does not fit.
    let read = unsafe {
        RegGetValueW(
            key,
            PCWSTR::null(),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    // SAFETY: our key.
    let _ = unsafe { RegCloseKey(key) };
    read.ok().ok()?;
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Frame::decode(&String::from_utf16_lossy(&buffer[..length]))
}
fn store(name: &str, frame: Frame) {
    let Some(key) = key(KEY_WRITE) else {
        return;
    };
    let text: Vec<u8> = frame
        .encode()
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    // SAFETY: writes a string value of our key, then closes it.
    unsafe {
        let _ = RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(&text));
        let _ = RegCloseKey(key);
    }
}
/// The window's frame as it is now.
fn frame(hwnd: HWND) -> Option<Frame> {
    let mut placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: reads our own window's placement into a local.
    unsafe { GetWindowPlacement(hwnd, &mut placement) }.ok()?;
    let show = placement.showCmd as i32;
    let maximized = show == SW_SHOWMAXIMIZED.0
        || (show == SW_SHOWMINIMIZED.0 && placement.flags.contains(WPF_RESTORETOMAXIMIZED));
    let rect = placement.rcNormalPosition;
    Some(Frame {
        rect: (rect.left, rect.top, rect.right, rect.bottom),
        maximized,
    })
}
/// Put the hidden window back at `frame`, if a monitor still shows its
/// title bar: whether it did.
fn restore(hwnd: HWND, frame: Frame) -> bool {
    // SAFETY: asks which monitor shows a rectangle.
    if unsafe { MonitorFromRect(&frame.title(), MONITOR_DEFAULTTONULL) }.is_invalid() {
        return false;
    }
    let (left, top, right, bottom) = frame.rect;
    let placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        // Still hidden: it shows (maximized, if it was) as it would.
        showCmd: SW_HIDE.0 as u32,
        rcNormalPosition: RECT {
            left,
            top,
            right,
            bottom,
        },
        ..Default::default()
    };
    // SAFETY: places our own (hidden) window.
    unsafe { SetWindowPlacement(hwnd, &placement) }.is_ok()
}

/// The autosave subclass's state: the name, and whether a move or resize
/// loop runs (saved once, as it ends).
struct Autosave {
    name: String,
    looping: std::cell::Cell<bool>,
}
/// # Safety
///
/// Called by comctl32 alone, as the subclass `autosave` installs: `data` is
/// the `Autosave` it leaked for it.
unsafe extern "system" fn autosave_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: `data` is the `Autosave` `autosave` leaked for this subclass,
    // alive until `WM_NCDESTROY` takes it back.
    let state = unsafe { &*(data as *const Autosave) };
    let save = || {
        if let Some(frame) = frame(hwnd) {
            store(&state.name, frame);
        }
    };
    match message {
        WM_ENTERSIZEMOVE => state.looping.set(true),
        WM_EXITSIZEMOVE => {
            state.looping.set(false);
            save();
        }
        // Snapped, maximized, restored or moved by the app: outside a move
        // loop, each is a frame to keep.
        WM_WINDOWPOSCHANGED if !state.looping.get() => save(),
        WM_DESTROY => save(),
        WM_NCDESTROY => {
            // SAFETY: removes this subclass and takes back its state.
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(autosave_proc), SUBCLASS_ID);
                drop(Box::from_raw(data as *mut Autosave));
            }
            // SAFETY: the next procedure in the window's chain.
            return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    // SAFETY: the next procedure in the window's chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
/// Keep `hwnd`'s frame under `name` as it changes, and as it closes.
fn autosave(hwnd: HWND, name: &str) {
    let state = Box::into_raw(Box::new(Autosave {
        name: name.to_owned(),
        looping: std::cell::Cell::new(false),
    }));
    // SAFETY: a subclass of our own window on its thread; its state is taken
    // back as the window goes (or now, if not installed).
    unsafe {
        if !SetWindowSubclass(hwnd, Some(autosave_proc), SUBCLASS_ID, state as usize).as_bool() {
            drop(Box::from_raw(state));
        }
    }
}

/// Put content point `(x, y)` (logical) of the hidden window under the
/// pointer.
fn under_pointer(hwnd: HWND, (x, y): (f64, f64)) {
    let (mut pointer, mut client) = (POINT::default(), POINT::default());
    let mut frame = RECT::default();
    // SAFETY: reads the pointer and our own window's geometry into locals,
    // then moves the window.
    unsafe {
        if GetCursorPos(&mut pointer).is_err()
            || !ClientToScreen(hwnd, &mut client).as_bool()
            || GetWindowRect(hwnd, &mut frame).is_err()
        {
            return;
        }
        let scale = f64::from(GetDpiForWindow(hwnd).max(96)) / 96.;
        let left = pointer.x - (x * scale).round() as i32 - (client.x - frame.left);
        let top = pointer.y - (y * scale).round() as i32 - (client.y - frame.top);
        let _ = SetWindowPos(
            hwnd,
            None,
            left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Place `window` (not shown yet), as `super::place` says. Placed nowhere in
/// particular, it stays where Windows put it (`CW_USEDEFAULT` cascades);
/// `Tabbing` has no counterpart here.
pub(crate) fn place(
    window: &winit::window::Window,
    under_pointer_at: Option<(f64, f64)>,
    _placed: bool,
    options: &crate::WindowOptions,
) -> Placed {
    let Some(hwnd) = hwnd(window) else {
        return Placed::default();
    };
    let mut placed = Placed::default();
    if let Some(name) = &options.frame_autosave {
        if under_pointer_at.is_none()
            && let Some(frame) = load(name)
            && restore(hwnd, frame)
        {
            placed = Placed {
                restored: true,
                maximized: frame.maximized,
            };
        }
        autosave(hwnd, name);
    }
    if let Some(at) = under_pointer_at {
        under_pointer(hwnd, at);
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_frame_reads_back_as_it_was_written() {
        let frame = Frame {
            rect: (-1200, 40, -300, 640),
            maximized: true,
        };
        assert_eq!(Frame::decode(&frame.encode()), Some(frame));
        assert_eq!(Frame::decode("10 20 5 30 0"), None, "an empty frame");
        assert_eq!(Frame::decode("10 20"), None);
        assert_eq!(Frame::decode("garbage"), None);
    }
}
