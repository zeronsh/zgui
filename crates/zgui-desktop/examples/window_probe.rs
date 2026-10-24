//! Checks the app's windows as AppKit keeps them (macOS), with events posted
//! to this process alone: the Window menu and the windows it lists, a second
//! window cascaded from the first, a frame saved under its autosave name,
//! and what ⌘` cycles through. ⌘` itself is a system hotkey AppKit hears
//! from the window server, not a key event the app could post itself: the
//! probe sends one anyway and prints whether the key window changed. On
//! Windows it checks the cascade and saved frames (see `win32`). On Linux
//! `scripts/window_frames_smoke.py` checks them from outside: the second
//! window opens a moment after the first, as from File ▸ New Window, and
//! `window_probe saved` opens one window that keeps its frame.
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = |title: &str| WindowOptions {
        title: title.into(),
        width: 420.,
        height: 200.,
        ..Default::default()
    };
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("saved") {
        let saved = WindowOptions {
            frame_autosave: Some("zgui window_probe".into()),
            ..options("Saved")
        };
        return Application::new().window(saved).run(|cx| {
            cx.render(div().p(20.).child(text("Saved")));
            // Its size once shown: on Wayland nothing outside the app can
            // read it.
            let bounds = cx.window_info.bounds.clone();
            cx.tasks.spawn(async move {
                zgui::timer::sleep(std::time::Duration::from_millis(1500)).await;
                let (width, height) = bounds.get().size;
                println!("SIZE {width} {height}");
            });
        });
    }
    let second = options("Second");
    // A window with content under its title bar, reopening a frame saved
    // larger than its options ask: the saved size stands. (Not on Linux,
    // where the second window cascades from the first alone.)
    #[cfg(target_os = "macos")]
    macos::save_frame();
    #[cfg(not(target_os = "linux"))]
    let restored = WindowOptions {
        titlebar: zgui_desktop::Titlebar::Inset,
        frame_autosave: Some("zgui window_probe restored".into()),
        ..options("Restored")
    };
    #[cfg(not(target_os = "linux"))]
    let standard = WindowOptions {
        frame_autosave: Some("zgui window_probe standard".into()),
        ..options("Standard")
    };
    #[cfg(not(target_os = "linux"))]
    let hidden = WindowOptions {
        titlebar: zgui_desktop::Titlebar::Hidden,
        frame_autosave: Some("zgui window_probe hidden".into()),
        ..options("Hidden")
    };
    Application::new().window(options("First")).run(move |cx| {
        #[cfg(not(target_os = "linux"))]
        {
            cx.windows.open(restored, |cx| {
                cx.render(div().p(20.).child(text("Restored")));
            });
            cx.windows.open(standard, |cx| {
                cx.render(div().p(20.).child(text("Standard")));
            });
            cx.windows.open(hidden, |cx| {
                cx.render(div().p(20.).child(text("Hidden")));
            });
        }
        cx.render(div().p(20.).child(text("First")));
        let open_second = {
            let windows = cx.windows.clone();
            move || {
                windows.open(second, |cx| {
                    cx.render(div().p(20.).child(text("Second")));
                });
            }
        };
        // On X11 a window cascades from one the window manager shows.
        // `WINDOW_PROBE_SECOND_MS` waits longer, for a probe to act first.
        #[cfg(target_os = "linux")]
        cx.tasks.spawn(async move {
            let wait = std::env::var("WINDOW_PROBE_SECOND_MS")
                .ok()
                .and_then(|ms| ms.parse().ok())
                .unwrap_or(1000);
            zgui::timer::sleep(std::time::Duration::from_millis(wait)).await;
            open_second();
        });
        #[cfg(not(target_os = "linux"))]
        open_second();
        #[cfg(target_os = "macos")]
        cx.tasks.spawn(async move {
            macos::run().await;
            std::process::exit(0);
        });
        #[cfg(windows)]
        {
            let windows = cx.windows.clone();
            cx.tasks.spawn(async move {
                win32::run(windows, options).await;
                std::process::exit(0);
            });
        }
    })
}

/// Windows: where the app's windows open, moved by `SetWindowPos` on its own
/// windows. The second window opens where Windows cascades it; a window with
/// `frame_autosave` reopens where it was moved, maximized if it was left so,
/// and not at a saved frame no monitor shows. The probe removes the frame it
/// saved.
#[cfg(windows)]
mod win32 {
    use std::time::Duration;
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, POINT, RECT},
            Graphics::Gdi::{ClientToScreen, MONITOR_DEFAULTTONULL, MonitorFromWindow},
            System::{
                Registry::{
                    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegDeleteKeyW, RegSetKeyValueW,
                },
                Threading::GetCurrentThreadId,
            },
            UI::HiDpi::GetDpiForWindow,
            UI::WindowsAndMessaging::{
                EnumThreadWindows, GetCursorPos, GetWindowRect, GetWindowTextW, IsWindowVisible,
                IsZoomed, SW_MAXIMIZE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
                ShowWindow,
            },
        },
        core::{BOOL, w},
    };
    use zgui_desktop::{WindowFactory, WindowOptions};

    const KEY: windows::core::PCWSTR = w!("Software\\zgui\\window_probe\\Frames");
    const NAME: windows::core::PCWSTR = w!("zgui window_probe");

    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    /// The shown window of this thread titled `title`.
    fn window(title: &str) -> Option<HWND> {
        unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the vector `window` passes, alive meanwhile.
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
        all.into_iter().find(|hwnd| {
            let mut buffer = [0u16; 64];
            // SAFETY: windows of this thread, read into a local buffer.
            let n = unsafe { GetWindowTextW(*hwnd, &mut buffer) };
            unsafe { IsWindowVisible(*hwnd) }.as_bool()
                && String::from_utf16_lossy(&buffer[..n.max(0) as usize]) == title
        })
    }
    fn rect(hwnd: HWND) -> RECT {
        let mut rect = RECT::default();
        // SAFETY: a window of this thread's rectangle, into a local.
        let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
        rect
    }
    fn forget() {
        // SAFETY: removes the probe's own value, then its keys if empty (the
        // last, `Software\zgui`, only if no other app keeps frames there).
        unsafe {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, KEY, NAME);
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, KEY);
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, w!("Software\\zgui\\window_probe"));
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, w!("Software\\zgui"));
        }
    }

    pub async fn run(windows: WindowFactory, options: impl Fn(&str) -> WindowOptions) {
        sleep(2000).await;
        let (first, second) = (
            rect(window("First").unwrap()),
            rect(window("Second").unwrap()),
        );
        println!(
            "CASCADE dx={} dy={}",
            second.left - first.left,
            second.top - first.top
        );
        forget();
        let saved = || WindowOptions {
            frame_autosave: Some("zgui window_probe".into()),
            ..options("Saved")
        };
        let open = |windows: &WindowFactory| {
            windows.open(saved(), |cx| {
                cx.render(zgui::compose::prelude::div());
            })
        };

        // Moved, then closed: it reopens there.
        let handle = open(&windows);
        sleep(1200).await;
        let hwnd = window("Saved").unwrap();
        let at = rect(hwnd);
        let moved = (at.left + 60, at.top + 50);
        // SAFETY: moves the probe's own window.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                moved.0,
                moved.1,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        sleep(300).await;
        handle.close();
        sleep(800).await;
        let handle = open(&windows);
        sleep(1200).await;
        let again = rect(window("Saved").unwrap());
        println!(
            "AUTOSAVE moved_to={moved:?} reopened_at={:?} size_kept={}",
            (again.left, again.top),
            (again.right - again.left, again.bottom - again.top)
                == (at.right - at.left, at.bottom - at.top)
        );

        // Maximized, then closed: it reopens maximized.
        // SAFETY: maximizes the probe's own window.
        let _ = unsafe { ShowWindow(window("Saved").unwrap(), SW_MAXIMIZE) };
        sleep(500).await;
        handle.close();
        sleep(800).await;
        let handle = open(&windows);
        sleep(1200).await;
        // SAFETY: reads the probe's own window's state.
        let maximized = unsafe { IsZoomed(window("Saved").unwrap()) }.as_bool();
        println!("AUTOSAVE_MAXIMIZED {maximized}");
        handle.close();
        sleep(800).await;

        // A frame no monitor shows: it opens on one instead.
        let far: Vec<u8> = "60000 60000 60420 60240 0"
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        // SAFETY: writes the probe's own value.
        let _ = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                KEY,
                NAME,
                REG_SZ.0,
                Some(far.as_ptr().cast()),
                far.len() as u32,
            )
        };
        let handle = open(&windows);
        sleep(1200).await;
        // SAFETY: asks which monitor shows the probe's own window.
        let shown = !unsafe { MonitorFromWindow(window("Saved").unwrap(), MONITOR_DEFAULTTONULL) }
            .is_invalid();
        println!("AUTOSAVE_OFFSCREEN on_a_monitor={shown}");
        handle.close();
        sleep(500).await;
        forget();

        // A torn-off tab's window opens with its point under the pointer.
        let under = WindowOptions {
            under_pointer: Some((20., 10.)),
            ..options("Under")
        };
        let handle = windows.open(under, |cx| {
            cx.render(zgui::compose::prelude::div());
        });
        sleep(1200).await;
        let hwnd = window("Under").unwrap();
        let (mut pointer, mut client) = (POINT::default(), POINT::default());
        // SAFETY: reads the pointer and the probe's own window into locals.
        let scale = unsafe {
            let _ = GetCursorPos(&mut pointer);
            let _ = ClientToScreen(hwnd, &mut client);
            f64::from(GetDpiForWindow(hwnd)) / 96.
        };
        let point = (
            client.x + (20. * scale).round() as i32,
            client.y + (10. * scale).round() as i32,
        );
        println!(
            "UNDER_POINTER off_by={:?}",
            (point.0 - pointer.x, point.1 - pointer.y)
        );
        handle.close();
        sleep(300).await;
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::{NSPoint, NSString};
    use std::time::Duration;

    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn app() -> Retained<NSApplication> {
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap())
    }
    fn window(title: &str) -> Option<Retained<NSWindow>> {
        app()
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == title)
    }
    fn key_title() -> String {
        app()
            .keyWindow()
            .map_or_else(|| "none".into(), |window| window.title().to_string())
    }
    /// ⌘` as the keyboard would send it, to the key window.
    fn command_backtick() {
        let Some(key) = app().keyWindow() else {
            return;
        };
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::new(0., 0.),
            NSEventModifierFlags::Command,
            0.,
            key.windowNumber(),
            None,
            &NSString::from_str("`"),
            &NSString::from_str("`"),
            false,
            50,
        )
        .unwrap();
        app().sendEvent(&event);
    }
    /// The standard defaults (a method send: no NSUserDefaults binding).
    fn defaults() -> Retained<objc2::runtime::AnyObject> {
        // SAFETY: NSUserDefaults' class method.
        unsafe { objc2::msg_send![objc2::class!(NSUserDefaults), standardUserDefaults] }
    }
    const RESTORED: &str = "NSWindow Frame zgui window_probe restored";
    /// A frame saved for the restored window: 600 by 340 points, larger
    /// than its options' 420 by 200.
    pub fn save_frame() {
        for key in [RESTORED, STANDARD, HIDDEN] {
            // SAFETY: NSUserDefaults' string setter.
            unsafe {
                let _: () = objc2::msg_send![
                    &*defaults(),
                    setObject: &*NSString::from_str("300 300 600 340 0 0 3200 1770 "),
                    forKey: &*NSString::from_str(key)
                ];
            }
        }
    }
    const STANDARD: &str = "NSWindow Frame zgui window_probe standard";
    const HIDDEN: &str = "NSWindow Frame zgui window_probe hidden";
    pub async fn run() {
        sleep(2000).await;
        for title in ["Standard", "Hidden"] {
            if let Some(window) = window(title) {
                let frame = window.frame();
                println!(
                    "RESTORED {title} frame=({:.0}, {:.0}, {:.0}, {:.0})",
                    frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
                );
                window.close();
            }
        }
        for key in [STANDARD, HIDDEN] {
            // SAFETY: as `save_frame`.
            unsafe {
                let _: () = objc2::msg_send![
                    &*defaults(),
                    removeObjectForKey: &*NSString::from_str(key)
                ];
            }
        }
        if let Some(restored) = window("Restored") {
            let frame = restored.frame();
            println!(
                "RESTORED Inset frame=({:.0}, {:.0}, {:.0}, {:.0})",
                frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
            );
            restored.close();
        }
        // SAFETY: as `save_frame`; the probe leaves no default behind.
        unsafe {
            let _: () = objc2::msg_send![
                &*defaults(),
                removeObjectForKey: &*NSString::from_str(RESTORED)
            ];
        }
        let app = app();
        let menu = app.windowsMenu().map(|menu| {
            menu.itemArray()
                .iter()
                .map(|item| item.title().to_string())
                .collect::<Vec<_>>()
        });
        println!("WINDOW_MENU {menu:?}");
        let (first, second) = (window("First").unwrap(), window("Second").unwrap());
        let (a, b) = (first.frame(), second.frame());
        println!(
            "CASCADE dx={} dy={}",
            b.origin.x - a.origin.x,
            (a.origin.y + a.size.height) - (b.origin.y + b.size.height)
        );
        // Moved, its frame is saved under its name.
        first.setFrameAutosaveName(&NSString::from_str("zgui window_probe"));
        let _ = second;
        first.setFrameOrigin(NSPoint::new(a.origin.x + 40., a.origin.y - 40.));
        sleep(300).await;
        // SAFETY: NSUserDefaults' class method and string getter.
        let saved: Option<Retained<NSString>> = unsafe {
            let defaults: Retained<objc2::runtime::AnyObject> =
                objc2::msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
            objc2::msg_send![
                &*defaults,
                stringForKey: &*NSString::from_str("NSWindow Frame zgui window_probe")
            ]
        };
        let saved = saved.map(|frame| frame.to_string());
        println!("AUTOSAVE {saved:?}");
        // SAFETY: as above; the probe leaves no default behind.
        unsafe {
            let defaults: Retained<objc2::runtime::AnyObject> =
                objc2::msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
            let _: () = objc2::msg_send![
                &*defaults,
                removeObjectForKey: &*NSString::from_str("NSWindow Frame zgui window_probe")
            ];
        }
        // What ⌘` cycles through: windows AppKit lists that can take the
        // keyboard.
        println!(
            "CYCLABLE {:?}",
            [&first, &second].map(|window| (
                window.title().to_string(),
                window.canBecomeKeyWindow(),
                window.isExcludedFromWindowsMenu()
            ))
        );
        first.makeKeyAndOrderFront(None);
        sleep(300).await;
        let before = key_title();
        command_backtick();
        sleep(300).await;
        println!("CYCLE before={before} after={}", key_title());
    }
}
