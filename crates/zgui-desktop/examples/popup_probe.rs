//! Measures popup windows (macOS, Windows; on Linux it reports its menu for
//! `scripts/popup_windows_smoke.py` and `wayland_popups_smoke.py` to drive):
//! a menu hanging past the window's bottom edge, opened, hovered and chosen
//! from by synthetic AppKit events (on Windows, mouse messages) posted to
//! this process alone (the user's pointer is never moved).
//!
//! Prints the time from the posted click to the popup window showing, then
//! idles twice (before any popup, after one opened and closed) for an
//! outside sampler: `READY` marks launch, `IDLE_A`/`IDLE_B` each idle span.
//! Then it presses the trigger, drags to an item and releases there (a menu
//! opens on press, as AppKit's), times a choice to its popup's window gone
//! (the item flashes, the menu fades), and hides the app with a menu open
//! (it must close). `--spaces` also opens one in a full-screen window and
//! reports whether it shows on that Space.
//!
//! winit hears moves outside a window only while a mouse button is down,
//! which it asks the hardware (`NSEvent.pressedMouseButtons`): the probe
//! answers that for its synthetic presses, in this process only.
//!
//! Each click follows the pointer moving onto the trigger, as a real one
//! does, which readies a popup window (`Ui::on_popup_soon`); `PROBE_COLD=1`
//! clicks the first time without it. `ZGUI_POPUP_POOL=n` sets how many
//! closed popup windows are kept.
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Popup probe".into(),
            width: 420.,
            height: 220.,
            bounds: Some(zgui_desktop::WindowBounds::new(840, 440).at(400, 300)),
            ..Default::default()
        })
        .run(|cx| {
            let open = cx.ui.signal(false);
            let chosen = cx.ui.signal(String::new());
            let anchor = button()
                .id("file")
                .h(28.)
                .w(120.)
                .child(text("File"))
                .on_click({
                    let open = open.clone();
                    move || {
                        open.set(true);
                    }
                });
            // `POPUP_PROBE_ITEMS`: a menu taller than the screen, for
            // `wayland_popups_smoke.py`.
            let count = std::env::var("POPUP_PROBE_ITEMS")
                .ok()
                .and_then(|count| count.parse().ok())
                .unwrap_or(8);
            let items = (1..=count).map(|n| {
                let chosen = chosen.clone();
                menu_item(format!("Item {n}"))
                    .id(format!("item-{n}"))
                    .h(24.)
                    .on_click(move || {
                        chosen.set(format!("Item {n}"));
                    })
            });
            // `POPUP_PROBE_POPOVER`: a popover with an arrow in the menu's
            // place, for `wayland_popups_smoke.py` to see it flipped.
            let popup = if std::env::var_os("POPUP_PROBE_POPOVER").is_some() {
                popover("Details", open.clone(), anchor)
                    .w(200.)
                    .h(120.)
                    .bg(rgb(0x4a3768))
                    .arrow(8.)
            } else {
                menu("File", open.clone(), anchor)
                    .w(200.)
                    .p(5.)
                    .children(items)
            };
            let view = cx.render(column().w_full().h_full().p(16.).justify_end().child(popup));
            #[cfg(target_os = "macos")]
            {
                let window = cx.window.clone();
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    macos::run(ui, view, open, chosen).await;
                    window.close();
                });
            }
            #[cfg(windows)]
            {
                let window = cx.window.clone();
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    win32::run(ui, view, open, chosen).await;
                    window.close();
                });
            }
            // Elsewhere (X11) a script drives it from outside
            // (`scripts/popup_windows_smoke.py`): it reports the menu.
            #[cfg(not(any(target_os = "macos", windows)))]
            {
                let _ = view;
                // `POPUP_PROBE_OPEN`: the menu opens by itself, for a
                // compositor no script can click in (Weston).
                if std::env::var_os("POPUP_PROBE_OPEN").is_some() {
                    let open = open.clone();
                    cx.tasks.spawn(async move {
                        zgui::timer::sleep(std::time::Duration::from_millis(500)).await;
                        open.set(true);
                    });
                }
                let report = cx.ui.runtime.effect(move || {
                    println!("OPEN {} CHOSEN {:?}", open.get(), chosen.get());
                });
                cx.on_closed(move || drop(report));
                println!("READY");
            }
        })
}

/// Windows: mouse messages posted to the probe's own windows (the user's
/// pointer and other apps are never touched). Prints as on macOS, plus the
/// process's CPU time over each idle span and how many popup windows exist
/// (none before the first popup), and checks that popups follow their window
/// and close as the app deactivates or its title bar is pressed.
/// `PROBE_QUICK=1` skips the idle spans; `PROBE_SHOT=1` holds the first menu
/// open (`SHOT_OPEN`), then closed (`SHOT_CLOSED`), for a screenshot of each
/// (scripts/windows_popup_smoke.py).
#[cfg(windows)]
mod win32 {
    use std::time::Duration;
    use windows::{
        Win32::{
            Foundation::{FILETIME, HWND, LPARAM, POINT, RECT, WPARAM},
            Graphics::Gdi::ClientToScreen,
            System::Threading::{GetCurrentProcess, GetCurrentThreadId, GetProcessTimes},
            UI::{
                HiDpi::GetDpiForWindow,
                WindowsAndMessaging::{
                    EnumThreadWindows, GetClassNameW, GetWindowRect, GetWindowTextW, HTCAPTION,
                    IsWindowVisible, PostMessageW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
                    SetWindowPos, WM_ACTIVATEAPP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                    WM_NCRBUTTONDOWN,
                },
            },
        },
        core::BOOL,
    };
    use zgui::{compose::ViewHandle, reactive::Signal, scene::Rect, widgets::WeakUi};
    use zgui_gpu::present_log::now_ns;

    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn seconds() -> f64 {
        now_ns() as f64 / 1e9
    }
    /// The process's CPU time so far, in milliseconds.
    fn cpu_ms() -> f64 {
        let (mut created, mut exited, mut kernel, mut user) = Default::default();
        // SAFETY: reads this process's times into locals.
        let _ = unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        let ms = |t: FILETIME| {
            ((u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)) as f64 / 1e4
        };
        ms(kernel) + ms(user)
    }
    /// The probe's windows: its own, its shown popups, and how many popup
    /// windows exist, shown or not.
    fn windows() -> (HWND, Vec<HWND>, usize) {
        unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the vector `windows` passes, alive meanwhile.
            unsafe { (*(data.0 as *mut Vec<HWND>)).push(hwnd) };
            true.into()
        }
        let mut all: Vec<HWND> = Vec::new();
        // SAFETY: the callback writes only into `all`, during this call.
        let _ = unsafe {
            EnumThreadWindows(
                GetCurrentThreadId(),
                Some(each),
                LPARAM(&mut all as *mut _ as isize),
            )
        };
        let text = |hwnd: HWND, class: bool| {
            let mut buffer = [0u16; 64];
            // SAFETY: fills a local buffer from a window of this thread.
            let n = unsafe {
                if class {
                    GetClassNameW(hwnd, &mut buffer)
                } else {
                    GetWindowTextW(hwnd, &mut buffer)
                }
            };
            String::from_utf16_lossy(&buffer[..n.max(0) as usize])
        };
        let mut main = None;
        let (mut shown, mut popups) = (Vec::new(), 0);
        for hwnd in all {
            if text(hwnd, true) == "ZguiPopup" {
                popups += 1;
                // SAFETY: a window of this thread.
                if unsafe { IsWindowVisible(hwnd) }.as_bool() {
                    shown.push(hwnd);
                }
            } else if text(hwnd, false) == "Popup probe" {
                main = Some(hwnd);
            }
        }
        (main.expect("probe window"), shown, popups)
    }
    fn scale(hwnd: HWND) -> f64 {
        // SAFETY: reads a window of this thread's DPI.
        f64::from(unsafe { GetDpiForWindow(hwnd) }.max(96)) / 96.
    }
    fn client_origin(hwnd: HWND) -> (i32, i32) {
        let mut point = POINT::default();
        // SAFETY: converts into a local, for a window of this thread.
        let _ = unsafe { ClientToScreen(hwnd, &mut point) };
        (point.x, point.y)
    }
    fn screen_rect(hwnd: HWND) -> RECT {
        let mut rect = RECT::default();
        // SAFETY: reads a window of this thread's rectangle into a local.
        let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
        rect
    }
    /// Post mouse message `message` at client point `(x, y)` (physical).
    fn post(hwnd: HWND, message: u32, x: i32, y: i32, buttons: usize) {
        let lparam = ((y as i16 as u16 as isize) << 16) | (x as i16 as u16 as isize);
        // SAFETY: a message to a window of this thread.
        let _ = unsafe { PostMessageW(Some(hwnd), message, WPARAM(buttons), LPARAM(lparam)) };
    }
    /// A click at `at` (the probe window's coordinates) on `to`. The real
    /// pointer is elsewhere: a window tracking it hears it leave at once
    /// (`TrackMouseEvent`), so moves come with each press, queued ahead of
    /// that leave. winit reports no move to where it last saw the pointer
    /// in that window (a popup's input moved it since), so the first is a
    /// pixel away.
    fn click(main: HWND, to: HWND, at: (f64, f64)) {
        post_at(main, to, WM_MOUSEMOVE, (at.0 + 1., at.1), 0);
        post_at(main, to, WM_MOUSEMOVE, at, 0);
        post_at(main, to, WM_LBUTTONDOWN, at, 1);
        post_at(main, to, WM_LBUTTONUP, at, 0);
    }
    /// Post `message` at `point` in the probe window's coordinates (logical),
    /// to `to`: the probe window or one of its popups.
    fn post_at(main: HWND, to: HWND, message: u32, (x, y): (f64, f64), buttons: usize) {
        let scale_main = scale(main);
        let origin = client_origin(main);
        let (sx, sy) = (
            origin.0 as f64 + x * scale_main,
            origin.1 as f64 + y * scale_main,
        );
        let target = client_origin(to);
        post(
            to,
            message,
            (sx - target.0 as f64).round() as i32,
            (sy - target.1 as f64).round() as i32,
            buttons,
        );
    }
    fn center(rect: Rect) -> (f64, f64) {
        (
            (rect.x + rect.width / 2.) as f64,
            (rect.y + rect.height / 2.) as f64,
        )
    }
    pub async fn run(ui: WeakUi, view: ViewHandle, open: Signal<bool>, chosen: Signal<String>) {
        let quick = std::env::var_os("PROBE_QUICK").is_some();
        let shot = std::env::var_os("PROBE_SHOT").is_some();
        let start = seconds();
        sleep(3000).await;
        println!("READY {:.3}", seconds());
        let bounds = |id: &str| {
            let ui = ui.upgrade().unwrap();
            let node = view.find(id).unwrap();
            ui.scene.borrow().bounds(node)
        };
        let until = |at: f64| sleep(((start + at - seconds()) * 1000.).max(0.) as u64);
        let idle = |name: &'static str, from: f64, to: f64| {
            let until = &until;
            async move {
                if quick {
                    return;
                }
                until(from).await;
                let cpu = cpu_ms();
                println!("{name} {:.3}", seconds());
                until(to).await;
                println!(
                    "{name}_CPU_MS {:.1} over {:.0} s, popup windows {}",
                    cpu_ms() - cpu,
                    to - from,
                    windows().2
                );
            }
        };
        // Idle before any popup: no popup window exists.
        idle("IDLE_A", 18., 30.).await;
        println!("POPUP_WINDOWS_BEFORE {}", windows().2);
        let mut latencies = Vec::new();
        for round in 0..5 {
            let (main, ..) = windows();
            let at = center(bounds("file"));
            let cold = round == 0 && std::env::var_os("PROBE_COLD").is_some();
            if !cold {
                post_at(main, main, WM_MOUSEMOVE, at, 0);
                sleep(150).await;
            }
            let clicked = seconds();
            click(main, main, at);
            let mut shown = None;
            for _ in 0..500 {
                sleep(1).await;
                if !windows().1.is_empty() {
                    shown = Some(seconds());
                    break;
                }
            }
            let Some(shown) = shown else {
                println!("OPEN_FAILED round={round} open={}", open.get());
                return;
            };
            latencies.push((shown - clicked) * 1000.);
            let popup = windows().1[0];
            if round == 0 {
                let rect = screen_rect(popup);
                let window = screen_rect(main);
                println!(
                    "POPUP_RECT {} {} {} {} past_window_bottom={} scale={}",
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    rect.bottom > window.bottom,
                    scale(main)
                );
                sleep(300).await;
                if shot {
                    // A screenshot of it open, then of its place closed.
                    println!("SHOT_OPEN");
                    sleep(1500).await;
                }
            }
            // Hover down the items, then choose the sixth.
            for n in 1..=8 {
                post_at(
                    main,
                    popup,
                    WM_MOUSEMOVE,
                    center(bounds(&format!("item-{n}"))),
                    0,
                );
                sleep(30).await;
            }
            click(main, popup, center(bounds("item-6")));
            sleep(400).await;
            println!(
                "ROUND {round} chosen={:?} open={} popups={}",
                chosen.get(),
                open.get(),
                windows().1.len()
            );
            if shot && round == 0 {
                println!("SHOT_CLOSED");
                sleep(1500).await;
            }
            chosen.set(String::new());
            sleep(300).await;
        }
        println!(
            "OPEN_LATENCY_MS {}",
            latencies
                .iter()
                .map(|ms| format!("{ms:.2}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        // Pressed, dragged to the third item (past the window's edge) and
        // released there: it is chosen. The window holds the pointer while
        // pressed, so its moves come to it.
        let (main, ..) = windows();
        let from = center(bounds("file"));
        post_at(main, main, WM_MOUSEMOVE, from, 0);
        sleep(150).await;
        post_at(main, main, WM_MOUSEMOVE, (from.0 + 1., from.1), 0);
        post_at(main, main, WM_MOUSEMOVE, from, 0);
        post_at(main, main, WM_LBUTTONDOWN, from, 1);
        sleep(100).await;
        let to = center(bounds("item-3"));
        for step in 1..=10 {
            let t = step as f64 / 10.;
            let at = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            post_at(main, main, WM_MOUSEMOVE, at, 1);
            sleep(16).await;
        }
        let released = seconds();
        post_at(main, main, WM_LBUTTONUP, to, 0);
        let mut gone = None;
        for _ in 0..1000 {
            sleep(1).await;
            if windows().1.is_empty() {
                gone = Some((seconds() - released) * 1000.);
                break;
            }
        }
        println!(
            "PRESS_DRAG chosen={:?} open={} closed_after_ms={:?}",
            chosen.get(),
            open.get(),
            gone.map(|ms| ms.round())
        );
        // The window moved with a menu open: the menu moves with it.
        open.set(true);
        sleep(300).await;
        let (main, popups, _) = windows();
        let before = popups.first().map(|popup| screen_rect(*popup));
        let window = screen_rect(main);
        // SAFETY: moves the probe's own window, never activating it.
        let _ = unsafe {
            SetWindowPos(
                main,
                None,
                window.left + 40,
                window.top + 30,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        sleep(300).await;
        let after = windows().1.first().map(|popup| screen_rect(*popup));
        println!(
            "FOLLOW moved={:?}",
            before
                .zip(after)
                .map(|(before, after)| (after.left - before.left, after.top - before.top))
        );
        // The app deactivated with a menu open: it closes.
        let shown = !windows().1.is_empty();
        // SAFETY: a message to the probe's own window.
        let _ = unsafe { PostMessageW(Some(main), WM_ACTIVATEAPP, WPARAM(0), LPARAM(0)) };
        sleep(400).await;
        println!(
            "DEACTIVATE shown_before={shown} open_after={} popups={}",
            open.get(),
            windows().1.len()
        );
        // A press on the title bar with a menu open: it closes.
        open.set(true);
        sleep(300).await;
        let shown = !windows().1.is_empty();
        post(main, WM_NCRBUTTONDOWN, 0, 0, HTCAPTION as usize);
        sleep(400).await;
        println!(
            "TITLEBAR shown_before={shown} open_after={} popups={}",
            open.get(),
            windows().1.len()
        );
        idle("IDLE_B", 52., 63.).await;
        println!("POPUP_WINDOWS_AFTER {}", windows().2);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgui::{compose::ViewHandle, reactive::Signal, scene::Rect, widgets::WeakUi};
    use zgui_gpu::present_log::now_ns;

    thread_local! {
        static PRESSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    extern "C-unwind" fn pressed_buttons(
        _: *mut objc2::runtime::AnyObject,
        _: objc2::runtime::Sel,
    ) -> usize {
        PRESSED.with(|pressed| pressed.get())
    }
    /// `+[NSEvent pressedMouseButtons]` answers the probe's presses.
    fn hold_buttons() {
        let class = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        // SAFETY: a class method returning NSUInteger, replaced in this
        // process by one of the same signature.
        unsafe {
            let imp: objc2::runtime::Imp = std::mem::transmute(
                pressed_buttons
                    as extern "C-unwind" fn(
                        *mut objc2::runtime::AnyObject,
                        objc2::runtime::Sel,
                    ) -> usize,
            );
            objc2::ffi::class_replaceMethod(
                class.metaclass() as *const _ as *mut _,
                objc2::sel!(pressedMouseButtons),
                imp,
                c"Q@:".as_ptr(),
            );
        }
    }
    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn seconds() -> f64 {
        now_ns() as f64 / 1e9
    }
    /// The app's windows: its own (titled) and the shown popups.
    fn windows() -> (Retained<NSWindow>, Vec<Retained<NSWindow>>) {
        let mtm = MainThreadMarker::new().unwrap();
        let mut main = None;
        let mut popups = Vec::new();
        for window in NSApplication::sharedApplication(mtm).windows().iter() {
            if !window.isVisible() {
                continue;
            }
            if window.title().to_string() == "Popup probe" {
                main = Some(window);
            } else {
                popups.push(window);
            }
        }
        (main.expect("probe window"), popups)
    }
    /// Post a mouse event at `(x, y)` in `window`'s content (top-down).
    fn post(window: &NSWindow, kind: NSEventType, x: f64, y: f64) {
        let mtm = MainThreadMarker::new().unwrap();
        let height = window.contentView().unwrap().frame().size.height;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            NSPoint::new(x, height - y),
            NSEventModifierFlags::empty(),
            seconds(),
            window.windowNumber(),
            None,
            0,
            1,
            if kind == NSEventType::LeftMouseDown { 1. } else { 0. },
        )
        .unwrap();
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
    fn center(rect: Rect) -> (f64, f64) {
        (
            (rect.x + rect.width / 2.) as f64,
            (rect.y + rect.height / 2.) as f64,
        )
    }
    pub async fn run(ui: WeakUi, view: ViewHandle, open: Signal<bool>, chosen: Signal<String>) {
        let start = seconds();
        sleep(3000).await;
        println!("READY {:.3}", seconds());
        let bounds = |id: &str| {
            let ui = ui.upgrade().unwrap();
            let node = view.find(id).unwrap();
            ui.scene.borrow().bounds(node)
        };
        // Idle before any popup (from 18 s: the driver's startup memory goes
        // within 15 s, see docs).
        let until = |at: f64| sleep(((start + at - seconds()) * 1000.).max(0.) as u64);
        until(18.).await;
        println!("IDLE_A {:.3}", seconds());
        until(30.).await;
        let mut latencies = Vec::new();
        for round in 0..5 {
            let (main, _) = windows();
            let (x, y) = center(bounds("file"));
            let cold = round == 0 && std::env::var_os("PROBE_COLD").is_some();
            if !cold {
                post(&main, NSEventType::MouseMoved, x, y);
                sleep(150).await;
            }
            let clicked = seconds();
            post(&main, NSEventType::LeftMouseDown, x, y);
            post(&main, NSEventType::LeftMouseUp, x, y);
            let mut shown = None;
            for _ in 0..500 {
                sleep(1).await;
                if !windows().1.is_empty() {
                    shown = Some(seconds());
                    break;
                }
            }
            let Some(shown) = shown else {
                println!("OPEN_FAILED round={round} open={}", open.get());
                return;
            };
            latencies.push((shown - clicked) * 1000.);
            // Hover down the items, then choose the sixth.
            let popup = windows().1.remove(0);
            let frame = popup.frame();
            let origin = {
                let parent = main.convertRectToScreen(main.contentView().unwrap().frame());
                (
                    frame.origin.x - parent.origin.x,
                    (parent.origin.y + parent.size.height) - (frame.origin.y + frame.size.height),
                )
            };
            for n in 1..=8 {
                let (x, y) = center(bounds(&format!("item-{n}")));
                post(&popup, NSEventType::MouseMoved, x - origin.0, y - origin.1);
                sleep(30).await;
            }
            let (x, y) = center(bounds("item-6"));
            post(
                &popup,
                NSEventType::LeftMouseDown,
                x - origin.0,
                y - origin.1,
            );
            post(&popup, NSEventType::LeftMouseUp, x - origin.0, y - origin.1);
            sleep(300).await;
            println!(
                "ROUND {round} chosen={:?} open={} popups={}",
                chosen.get(),
                open.get(),
                windows().1.len()
            );
            chosen.set(String::new());
            sleep(300).await;
        }
        println!(
            "OPEN_LATENCY_MS {}",
            latencies
                .iter()
                .map(|ms| format!("{ms:.2}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let mtm = MainThreadMarker::new().unwrap();
        let (main, _) = windows();
        // Pressed, dragged to the third item (past the window's edge) and
        // released there: it is chosen.
        hold_buttons();
        let (x, y) = center(bounds("file"));
        post(&main, NSEventType::MouseMoved, x, y);
        sleep(150).await;
        PRESSED.with(|pressed| pressed.set(1));
        post(&main, NSEventType::LeftMouseDown, x, y);
        sleep(100).await;
        let (ix, iy) = center(bounds("item-3"));
        for step in 1..=10 {
            let t = step as f64 / 10.;
            post(
                &main,
                NSEventType::LeftMouseDragged,
                x + (ix - x) * t,
                y + (iy - y) * t,
            );
            sleep(16).await;
        }
        let released = seconds();
        post(&main, NSEventType::LeftMouseUp, ix, iy);
        PRESSED.with(|pressed| pressed.set(0));
        // The chosen item flashes, then the menu fades out.
        let mut gone = None;
        for _ in 0..1000 {
            sleep(1).await;
            if windows().1.is_empty() {
                gone = Some((seconds() - released) * 1000.);
                break;
            }
        }
        println!(
            "PRESS_DRAG chosen={:?} open={} closed_after_ms={:?}",
            chosen.get(),
            open.get(),
            gone.map(|ms| ms.round())
        );
        // The app hidden (deactivated) with a menu open: it closes.
        open.set(true);
        sleep(300).await;
        let shown = !windows().1.is_empty();
        NSApplication::sharedApplication(mtm).hide(None);
        sleep(400).await;
        println!("DEACTIVATE shown_before={shown} open_after={}", open.get());
        NSApplication::sharedApplication(mtm).unhide(None);
        sleep(400).await;
        if std::env::args().any(|arg| arg == "--spaces") {
            let (main, _) = windows();
            main.toggleFullScreen(None);
            sleep(2500).await;
            open.set(true);
            sleep(400).await;
            let popups = windows().1;
            println!(
                "SPACES fullscreen={} popup_shown={} on_active_space={}",
                main.styleMask()
                    .contains(objc2_app_kit::NSWindowStyleMask::FullScreen),
                !popups.is_empty(),
                popups.first().is_some_and(|popup| popup.isOnActiveSpace())
            );
            open.set(false);
            sleep(300).await;
            main.toggleFullScreen(None);
            sleep(2500).await;
        }
        until(52.).await;
        println!("IDLE_B {:.3}", seconds());
        until(63.).await;
    }
}
