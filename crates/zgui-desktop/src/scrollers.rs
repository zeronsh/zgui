//! The system's scroll bar style: overlay scrollbars, hidden until a view
//! scrolls (`Ui::set_overlay_scrollbars`), where the user has them.

/// Whether the system draws overlay scrollbars: macOS's "Show scroll bars"
/// (automatic follows the pointing device: a trackpad's overlay, a mouse's
/// legacy); Windows' "Always show scrollbars" off (Accessibility, Visual
/// effects; Windows 10 2004 on). `None` where zgui does not read it, and
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

/// Read the system's style again and give it to `ui`: at startup, when the
/// window comes to the front (the user may have changed it meanwhile), and
/// on Windows when the system's settings change.
pub(crate) fn sync(ui: &zgui::widgets::Ui) {
    if let Some(overlay) = overlay() {
        ui.set_overlay_scrollbars(overlay);
    }
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
