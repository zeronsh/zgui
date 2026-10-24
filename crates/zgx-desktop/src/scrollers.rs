//! The system's scrolling settings: overlay scrollbars, hidden until a view
//! scrolls (`Ui::set_overlay_scrollbars`), where the user has them, and on
//! Windows how far a wheel notch scrolls.

#[cfg(windows)]
use std::sync::atomic::{AtomicU32, Ordering};

/// Whether the system draws overlay scrollbars: macOS's "Show scroll bars"
/// (automatic follows the pointing device: a trackpad's overlay, a mouse's
/// legacy); Windows' "Always show scrollbars" off (Accessibility, Visual
/// effects; Windows 10 2004 on). `None` where zgx does not read it, and
/// scrollbars always show.
fn overlay() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSScroller, NSScrollerStyle};
        let main = objc2::MainThreadMarker::new()?;
        Some(NSScroller::preferredScrollerStyle(main) == NSScrollerStyle::Overlay)
    }
    #[cfg(windows)]
    {
        use windows::UI::ViewManagement::UISettings;
        UISettings::new()
            .and_then(|settings| settings.AutoHideScrollBars())
            .ok()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

/// Windows' wheel settings (Mouse: lines to scroll at a time, and the
/// characters across for a tilted wheel), as last read.
#[cfg(windows)]
static WHEEL_LINES: AtomicU32 = AtomicU32::new(3);
#[cfg(windows)]
static WHEEL_CHARS: AtomicU32 = AtomicU32::new(3);

#[cfg(windows)]
fn read_wheel() {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        SystemParametersInfoW,
    };
    for (action, setting) in [
        (SPI_GETWHEELSCROLLLINES, &WHEEL_LINES),
        (SPI_GETWHEELSCROLLCHARS, &WHEEL_CHARS),
    ] {
        let mut value = 0u32;
        // SAFETY: fills one UINT.
        let read = unsafe {
            SystemParametersInfoW(
                action,
                0,
                Some(&mut value as *mut _ as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        if read.is_ok() {
            setting.store(value, Ordering::Relaxed);
        }
    }
}

/// How far a wheel's step scrolls one way, in logical pixels, given the
/// view's `page` that way: a browser's line, 40 (Chromium's and WebKit's
/// line step, and Chromium's per Cocoa tick). Windows reports notches, not
/// lines: a notch is the user's lines (3 by default) at a third of 100 each,
/// as Chromium scrolls, and "one screen at a time" a page less an eighth,
/// as Chromium pages.
fn step(lines: Option<u32>, page: f32) -> f32 {
    match lines {
        None => 40.,
        Some(u32::MAX) => page * 0.875,
        Some(lines) => lines as f32 * 100. / 3.,
    }
}

/// The wheel's steps across and down, in logical pixels, for a window of
/// `viewport` logical size.
pub(crate) fn wheel_step(viewport: (f32, f32)) -> (f32, f32) {
    #[cfg(windows)]
    let (across, down) = (
        Some(WHEEL_CHARS.load(Ordering::Relaxed)),
        Some(WHEEL_LINES.load(Ordering::Relaxed)),
    );
    #[cfg(not(windows))]
    let (across, down) = (None, None);
    // KDE's wheel lines ("Scroll by", 3 by default) scale a browser's
    // line as Windows' do: 40 is three lines.
    #[cfg(target_os = "linux")]
    if let Some(lines) = crate::linux_appearance::current().wheel_lines {
        let line = 40. * lines as f32 / 3.;
        return (line, line);
    }
    (step(across, viewport.0), step(down, viewport.1))
}

/// Read the system's settings again and give them to `ui`: at startup,
/// when the window comes to the front (the user may have changed them
/// meanwhile), and on Windows when the system's settings change.
pub(crate) fn sync(ui: &zgx::widgets::Ui) {
    if let Some(overlay) = overlay() {
        ui.set_overlay_scrollbars(overlay);
    }
    #[cfg(windows)]
    read_wheel();
}

/// macOS's scroll bar style changed since the last call ("Show scroll bars",
/// or a mouse plugged in under "Automatically"): the host syncs every window
/// then. The first call starts listening.
pub(crate) fn style_changed() -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static CHANGED: AtomicBool = AtomicBool::new(false);
        static LISTENING: std::sync::Once = std::sync::Once::new();
        LISTENING.call_once(|| {
            let center = objc2_foundation::NSNotificationCenter::defaultCenter();
            let block =
                block2::RcBlock::new(|_: std::ptr::NonNull<objc2_foundation::NSNotification>| {
                    CHANGED.store(true, Ordering::Relaxed);
                });
            // SAFETY: a notification name, no sender filter, delivered on the
            // posting (main) thread; the block only sets a flag. The
            // observer lives as long as the app.
            let observer = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(objc2_app_kit::NSPreferredScrollerStyleDidChangeNotification),
                    None,
                    None,
                    &block,
                )
            };
            std::mem::forget(observer);
        });
        CHANGED.swap(false, Ordering::Relaxed)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn a_wheel_step_is_a_browsers_line_or_the_users_lines_or_a_page() {
        assert_eq!(step(None, 600.), 40.);
        // Windows: three lines a notch, the default, scroll 100.
        assert_eq!(step(Some(3), 600.), 100.);
        assert_eq!(step(Some(6), 600.), 200.);
        assert_eq!(step(Some(0), 600.), 0.);
        // "One screen at a time" (WHEEL_PAGESCROLL).
        assert_eq!(step(Some(u32::MAX), 600.), 525.);
    }
}
