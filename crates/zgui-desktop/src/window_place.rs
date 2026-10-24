//! Where a new window opens (macOS): with a given point of its content under
//! the pointer (`WindowOptions::under_pointer`: a tab torn off its window
//! opens where it was dropped), where it was last left
//! (`WindowOptions::frame_autosave`), or, placed nowhere in particular,
//! cascaded from the app's front window, as AppKit's document windows are,
//! rather than over it. It also joins AppKit's window tabs as `Tabbing`
//! says.
//!
//! Windows (`win32`): placed nowhere in particular, a window opens where
//! Windows puts it (`CW_USEDEFAULT`: each new window a step on from the last,
//! as Notepad's). A saved frame is its placement (`GetWindowPlacement`: the
//! restored frame, and whether it was maximized), kept in the registry under
//! `HKCU\Software\zgui\<executable>\Frames`, and reopened only where a monitor
//! still shows its title bar. AppKit's window tabs have no Windows
//! counterpart: `Tabbing` does nothing there.
//!
//! Linux (`linux`): on X11 a window goes under the pointer: the pointer is
//! the server's (`XQueryPointer`), and the window's frame the window
//! manager's (`_NET_REQUEST_FRAME_EXTENTS`); the position is the user's
//! (`USPosition`), which window managers keep rather than place the window
//! themselves. A saved frame is kept in `$XDG_STATE_HOME/zgui/<executable>/
//! frames` as the window moves and resizes (the host saves it once they
//! settle, `save`), and reopened at its size, maximized if it was left so,
//! and on X11 where it was if a monitor still shows its title bar. Placed
//! nowhere in particular, an X11 window opens a step on from the app's
//! front window (the program's position, `PPosition`); with none, where the
//! window manager puts it. On Wayland the compositor places every window:
//! a saved frame keeps its size and maximized state alone. `Tabbing` does
//! nothing there.

/// AppKit's window tabs (macOS; elsewhere none): an app's windows gather as
/// tabs of one window, as Finder's do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Tabbing {
    /// As the user's setting says (System Settings ▸ Desktop & Dock ▸
    /// Prefer tabs when opening documents), and with Window ▸ Merge All
    /// Windows.
    #[default]
    Automatic,
    /// New windows open as tabs of the front window.
    Preferred,
    /// Never: for windows with tabs of their own (zgui's), as Safari's.
    Disallowed,
}

/// How `place` left a window, on every platform that saves frames
/// (`WindowOptions::frame_autosave`: macOS, Windows).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Its saved frame was restored: its size and place are that, not its
    /// options'. The host then leaves its size alone (an inset or hidden
    /// title bar's resize to the options' size is skipped). On macOS the
    /// frame is already what it will be once content runs under the title
    /// bar; on Windows, as the frame was when it was saved.
    pub restored: bool,
    /// It was left maximized (Windows; macOS saves no zoomed state): it
    /// opens so.
    pub maximized: bool,
}

/// Place `window` (not shown yet): `under_pointer` is the content point
/// (logical) to put under the pointer; `placed`, whether its options gave it
/// a position already.
#[cfg(target_os = "macos")]
pub(crate) fn place(
    window: &winit::window::Window,
    under_pointer: Option<(f64, f64)>,
    placed: bool,
    options: &crate::WindowOptions,
) -> Placed {
    use objc2::{MainThreadOnly, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSView, NSWindowTabbingMode};
    use objc2_foundation::{NSPoint, NSString};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return Placed::default();
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Placed::default();
    };
    // SAFETY: winit's content view, alive while its window is (borrowed
    // here); winit hands out an AppKit handle only on the main thread.
    let Some(view) = (unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }) else {
        return Placed::default();
    };
    let Some(window) = view.window() else {
        return Placed::default();
    };
    window.setTabbingMode(match options.tabbing {
        Tabbing::Automatic => NSWindowTabbingMode::Automatic,
        Tabbing::Preferred => NSWindowTabbingMode::Preferred,
        Tabbing::Disallowed => NSWindowTabbingMode::Disallowed,
    });
    if let Some(identifier) = &options.tabbing_identifier {
        window.setTabbingIdentifier(&NSString::from_str(identifier));
    }
    // Where it was last left, if it was: kept from now on, its size too.
    let mut restored = false;
    if let Some(name) = &options.frame_autosave {
        let name = NSString::from_str(name);
        restored = under_pointer.is_none() && window.setFrameUsingName(&name);
        window.setFrameAutosaveName(&name);
    }
    // The window takes content under its title bar after this
    // (`window_chrome::apply`), and its frame becomes its content rect
    // then: a saved frame (that content, as it was saved) is restored a
    // title bar taller, its content being the saved frame, so the window
    // ends as it was saved.
    if restored && options.titlebar != crate::Titlebar::Standard {
        let frame = window.frame();
        let content = window.contentRectForFrameRect(frame);
        let title = frame.size.height - content.size.height;
        window.setFrame_display(
            objc2_foundation::NSRect::new(
                frame.origin,
                objc2_foundation::NSSize::new(frame.size.width, frame.size.height + title),
            ),
            false,
        );
    }
    let restored = Placed {
        restored,
        maximized: false,
    };
    let frame = window.frame();
    let content = window.contentRectForFrameRect(frame);
    if let Some((x, y)) = under_pointer {
        // Screen space runs bottom-up: the content's top is above the
        // pointer by `y`, the frame's by the title bar's height more.
        let pointer = NSEvent::mouseLocation();
        let title = (frame.origin.y + frame.size.height) - (content.origin.y + content.size.height);
        window.setFrameTopLeftPoint(NSPoint::new(
            pointer.x - x - (content.origin.x - frame.origin.x),
            pointer.y + y + title,
        ));
        return Placed::default();
    }
    if placed || restored.restored {
        return restored;
    }
    // Cascaded from the app's front window: placed at its top-left, then
    // one step on (AppKit's offset, kept on its screen).
    // Not yet main or key as the app starts: then another window that can
    // be (not a popup's panel).
    let app = NSApplication::sharedApplication(window.mtm());
    let front = app
        .mainWindow()
        .or_else(|| app.keyWindow())
        .filter(|other| *other != window)
        .or_else(|| {
            app.windows()
                .iter()
                .find(|other| *other != window && other.canBecomeMainWindow())
        });
    if let Some(front) = front {
        let frame = front.frame();
        let top_left = NSPoint::new(frame.origin.x, frame.origin.y + frame.size.height);
        let next = window.cascadeTopLeftFromPoint(top_left);
        window.cascadeTopLeftFromPoint(next);
    }
    Placed::default()
}

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub(crate) use win32::place;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{maximize_hidden, place, save};

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub(crate) fn place(
    _: &winit::window::Window,
    _: Option<(f64, f64)>,
    _: bool,
    _: &crate::WindowOptions,
) -> Placed {
    Placed::default()
}

/// The `NSWindow` of winit's `window`.
#[cfg(target_os = "macos")]
fn ns_window(
    window: &winit::window::Window,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit's content view, alive while its window is (borrowed
    // here); winit hands out an AppKit handle only on the main thread.
    let view = unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }?;
    view.window()
}

/// Put `window`'s content top-left at global point `to` (top-down from the
/// primary screen's top), as a drag of its only tab does
/// (`View::drag_moves_window`): through the window server alone where it
/// can (`CGSMoveWindow`), as the drag preview's window moves, AppKit hearing
/// at `settle` (its frame's top-left kept in `pending`); else through
/// AppKit.
#[cfg(target_os = "macos")]
pub(crate) fn move_to(
    window: &winit::window::Window,
    to: (f64, f64),
    pending: &mut Option<(f64, f64)>,
) {
    use objc2_foundation::NSPoint;
    let Some(window) = ns_window(window) else {
        return;
    };
    let frame = window.frame();
    let content = window.contentRectForFrameRect(frame);
    // The frame's top-left: the title bar above the content.
    let title = (frame.origin.y + frame.size.height) - (content.origin.y + content.size.height);
    let top_left = (to.0 - (content.origin.x - frame.origin.x), to.1 - title);
    if crate::popup_window::server_move(&window, NSPoint::new(top_left.0, top_left.1)) {
        *pending = Some(top_left);
    } else {
        set_top_left(&window, top_left);
    }
}

/// `window`'s frame top-left to global `(x, y)`, through AppKit.
#[cfg(target_os = "macos")]
fn set_top_left(window: &objc2_app_kit::NSWindow, (x, y): (f64, f64)) {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::NSPoint;
    // Screen space runs bottom-up from the primary screen's bottom.
    let height = NSScreen::screens(objc2::MainThreadMarker::from(window))
        .firstObject()
        .map_or(0., |screen| screen.frame().size.height);
    window.setFrameTopLeftPoint(NSPoint::new(x, height - y));
}

/// The pointer in global coordinates (top-down from the primary screen's
/// top), as the window server has it: a window following the pointer is
/// placed from it, never from window-relative points that a move not yet
/// applied would skew.
#[cfg(target_os = "macos")]
pub(crate) fn pointer() -> Option<(f64, f64)> {
    use objc2_app_kit::{NSEvent, NSScreen};
    let mtm = objc2::MainThreadMarker::new()?;
    let height = NSScreen::screens(mtm).firstObject()?.frame().size.height;
    let point = NSEvent::mouseLocation();
    Some((point.x, height - point.y))
}

/// AppKit learns where `move_to` put the window.
#[cfg(target_os = "macos")]
pub(crate) fn settle(window: &winit::window::Window, pending: &mut Option<(f64, f64)>) {
    if let (Some(top_left), Some(window)) = (pending.take(), ns_window(window)) {
        set_top_left(&window, top_left);
    }
}

/// The front-most window at global point `(x, y)` (top-down from the primary
/// screen's top) below window `number`: what is under a window being moved.
#[cfg(target_os = "macos")]
pub(crate) fn window_below(x: f64, y: f64, number: isize) -> Option<isize> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWindow};
    use objc2_foundation::NSPoint;
    let mtm = MainThreadMarker::new()?;
    let height = NSScreen::screens(mtm)
        .firstObject()
        .map_or(0., |screen| screen.frame().size.height);
    let found = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(
        NSPoint::new(x, height - y),
        number,
        mtm,
    );
    (found > 0).then_some(found)
}

// X11: the window's frame moved by request (the window manager carries it
// out), and what is below it from the root's children in stacking order
// (`popup_window`'s X11 module); Wayland gives the app neither.
#[cfg(target_os = "linux")]
pub(crate) fn move_to(window: &winit::window::Window, to: (f64, f64), _: &mut Option<(f64, f64)>) {
    crate::popup_window::move_window(window, to);
}

#[cfg(target_os = "linux")]
pub(crate) fn pointer() -> Option<(f64, f64)> {
    crate::popup_window::global_pointer()
}

#[cfg(target_os = "linux")]
pub(crate) fn settle(_: &winit::window::Window, _: &mut Option<(f64, f64)>) {
    crate::popup_window::settle_window();
}

#[cfg(target_os = "linux")]
pub(crate) fn window_below(x: f64, y: f64, number: isize) -> Option<isize> {
    crate::popup_window::window_below(x, y, number)
}

// todo(windows): `SetWindowPos` and `WindowFromPoint` past the moved window
// (`GetWindow(GW_HWNDNEXT)`). Until then such a drag shows its preview, as
// any drag does.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn move_to(_: &winit::window::Window, _: (f64, f64), _: &mut Option<(f64, f64)>) {}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn pointer() -> Option<(f64, f64)> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn settle(_: &winit::window::Window, _: &mut Option<(f64, f64)>) {}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn window_below(_: f64, _: f64, _: isize) -> Option<isize> {
    None
}
