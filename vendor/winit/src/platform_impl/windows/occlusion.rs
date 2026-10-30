//! zgx patch: `WindowEvent::Occluded`, which Windows doesn't send.
//!
//! A window is occluded when it is on another virtual desktop, or when
//! windows known to be opaque above it leave none of it on screen. Stopping
//! a window someone can see is far worse than drawing one nobody can, so
//! whatever might show what's behind it covers nothing (`covers`), and a
//! layout with any doubt in it, more than one monitor's worth of window, or
//! monitors of different scales, keeps drawing (`Coverage`).
//!
//! A window drawing checks a couple of times a second from its frames
//! (`Window::pre_present_notify`), and an idle one not at all: the system
//! isn't asked to tell a visible window anything. An occluded one, which
//! has no frames, hears from a hook on other windows' changes, and looks
//! again each second for those no event reports (a style or z-order change).
use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows_sys::Win32::Graphics::Gdi::{GetWindowRgnBox, ERROR};
use windows_sys::Win32::UI::Accessibility::{
    SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongW, IsIconic, IsWindowVisible, IsZoomed,
    KillTimer, SetTimer, CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
    OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WS_CAPTION, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_THICKFRAME,
};

use super::dpi::{get_monitor_dpi, hwnd_dpi};
use super::monitor::{available_monitors, get_monitor_info, MonitorHandle};
use crate::occlusion_area::{Area, Coverage, Screen};

/// The window's timer (`WM_TIMER`): settles a check, away from the frame or
/// system event that asked for it.
pub(super) const TIMER: usize = 0x7a67;
/// A change reported by a hook, looked at once this settles.
const AFTER_EVENT_MS: u32 = 30;
/// Between looks at an occluded window nothing reported on.
const WHILE_OCCLUDED_MS: u32 = 1000;
/// `DWMWA_SYSTEMBACKDROP_TYPE`, and the highest value known: Mica (2) and
/// tabbed Mica (4), Explorer's, show the wallpaper and no window behind;
/// acrylic (`DWMSBT_TRANSIENTWINDOW`, 3) shows what's behind it, blurred.
const SYSTEM_BACKDROP_TYPE: u32 = 38;
const BACKDROP_ACRYLIC: i32 = 3;
const BACKDROP_LAST_KNOWN: i32 = 4;
/// The radius of Windows 11's window corners at 96 DPI.
const CORNER_RADIUS: u32 = 8;

const HOOKED: [(u32, u32); 6] = [
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
    (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
    (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
];

/// This thread's occluded windows, and whether each has a look on the way.
#[derive(Default)]
struct Watched {
    windows: Vec<(HWND, bool)>,
    hooks: Vec<HWINEVENTHOOK>,
}

thread_local! {
    static WATCHED: RefCell<Watched> = RefCell::default();
}

/// Have a window's timer settle a check soon.
pub(super) fn settle_soon(window: HWND) {
    unsafe { SetTimer(window, TIMER, 1, None) };
}

/// A window has gone occluded (or looked again and still is).
pub(super) fn watch(window: HWND) {
    WATCHED.with_borrow_mut(|watched| {
        match watched.windows.iter_mut().find(|(hwnd, _)| *hwnd == window) {
            Some((_, looking)) => *looking = false,
            None => watched.windows.push((window, false)),
        }
        if watched.hooks.is_empty() {
            for (min, max) in HOOKED {
                let hook = unsafe {
                    SetWinEventHook(min, max, 0, Some(changed), 0, 0, WINEVENT_OUTOFCONTEXT)
                };
                if hook != 0 {
                    watched.hooks.push(hook);
                }
            }
        }
    });
    unsafe { SetTimer(window, TIMER, WHILE_OCCLUDED_MS, None) };
}

/// A window is no longer occluded, or has gone.
pub(super) fn unwatch(window: HWND) {
    unsafe { KillTimer(window, TIMER) };
    WATCHED.with_borrow_mut(|watched| {
        watched.windows.retain(|(hwnd, _)| *hwnd != window);
        if watched.windows.is_empty() {
            for hook in watched.hooks.drain(..) {
                unsafe { UnhookWinEvent(hook) };
            }
        }
    });
}

/// Something changed on the desktop: occluded windows look again. Timers
/// already set stay, or a drag's run of events would put the look off to
/// its end.
unsafe extern "system" fn changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW || child != CHILDID_SELF as i32 {
        return;
    }
    WATCHED.with_borrow_mut(|watched| {
        for (window, looking) in &mut watched.windows {
            if !std::mem::replace(looking, true) {
                unsafe { SetTimer(*window, TIMER, AFTER_EVENT_MS, None) };
            }
        }
    });
}

/// Whether nothing of `window` can be seen. A hidden or minimized window is
/// its owner's to know about: not this question.
pub(super) fn hidden(window: HWND) -> bool {
    if unsafe { IsWindowVisible(window) == 0 || IsIconic(window) != 0 } {
        return false;
    }
    if cloaked(window) {
        return true;
    }
    let Some(coverage) = frame_bounds(window).and_then(|bounds| Coverage::new(bounds, &screens()))
    else {
        return false;
    };
    let mut search = Search {
        window,
        radius: (CORNER_RADIUS * unsafe { hwnd_dpi(window) }).div_ceil(96) as i32,
        coverage,
    };
    unsafe { EnumWindows(Some(next_above), &mut search as *mut Search as LPARAM) };
    search.coverage.hidden()
}

struct Search {
    window: HWND,
    radius: i32,
    coverage: Coverage,
}

/// Windows top first; those before `search.window` are above it.
unsafe extern "system" fn next_above(window: HWND, search: LPARAM) -> BOOL {
    let search = unsafe { &mut *(search as *mut Search) };
    if window == search.window {
        return 0;
    }
    if covers(window, search.window) {
        match frame_bounds(window) {
            Some(bounds) => {
                search.coverage.add(bounds, rounded(window).then_some(search.radius))
            },
            None => search.coverage.distrust(),
        }
    }
    (!search.coverage.settled()).into()
}

/// Whether nothing shows through `window`, as far as can be told. Layered,
/// click-through and tool windows (overlays, the taskbar), those composed by
/// their own swap chain (`WS_EX_NOREDIRECTIONBITMAP`: Zeron, WinUI apps), an
/// acrylic backdrop and a shaped region can all show what's behind them; and
/// `target`'s own popups come and go with it.
fn covers(window: HWND, target: HWND) -> bool {
    unsafe {
        if IsWindowVisible(window) == 0 || IsIconic(window) != 0 || cloaked(window) {
            return false;
        }
        let ex_style = GetWindowLongW(window, GWL_EXSTYLE) as u32;
        let see_through =
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW;
        if ex_style & see_through != 0 {
            return false;
        }
        let mut backdrop = 0i32;
        let read = DwmGetWindowAttribute(
            window,
            SYSTEM_BACKDROP_TYPE,
            &mut backdrop as *mut i32 as *mut c_void,
            size_of::<i32>() as u32,
        );
        if read >= 0 && (backdrop == BACKDROP_ACRYLIC || backdrop > BACKDROP_LAST_KNOWN) {
            return false;
        }
        let mut region = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if GetWindowRgnBox(window, &mut region) != ERROR as i32 {
            return false;
        }
        let mut owner = GetWindow(window, GW_OWNER);
        while owner != 0 {
            if owner == target {
                return false;
            }
            owner = GetWindow(owner, GW_OWNER);
        }
    }
    true
}

/// Whether the system rounds this window's corners. Maximized and borderless
/// (full-screen) ones keep them square.
fn rounded(window: HWND) -> bool {
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) } as u32;
    style & (WS_CAPTION | WS_THICKFRAME) != 0 && unsafe { IsZoomed(window) } == 0
}

fn cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    let read = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        )
    };
    read >= 0 && cloaked != 0
}

/// The rectangle a window shows, without the invisible resize borders that
/// `GetWindowRect` counts.
fn frame_bounds(window: HWND) -> Option<Area> {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let read = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
    };
    (read >= 0).then_some(Area {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

/// Every monitor, or none if any can't be read (`Coverage` then says nothing).
fn screens() -> Vec<Screen> {
    let read = |monitor: &MonitorHandle| {
        let info = get_monitor_info(monitor.hmonitor()).ok()?;
        let rect = info.monitorInfo.rcMonitor;
        Some(Screen {
            area: Area {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
            dpi: get_monitor_dpi(monitor.hmonitor())?,
        })
    };
    available_monitors().iter().map(read).collect::<Option<_>>().unwrap_or_default()
}
