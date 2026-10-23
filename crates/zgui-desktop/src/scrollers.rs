//! The system's scroll bar style: overlay scrollbars, hidden until a view
//! scrolls (`Ui::set_overlay_scrollbars`), where the user has them.

/// Whether the system draws overlay scrollbars: macOS's "Show scroll bars"
/// (automatic follows the pointing device: a trackpad's overlay, a mouse's
/// legacy). `None` where zgui does not read it, and scrollbars always show.
fn overlay() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSScroller, NSScrollerStyle};
        let main = objc2::MainThreadMarker::new()?;
        Some(NSScroller::preferredScrollerStyle(main) == NSScrollerStyle::Overlay)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Read the system's style again and give it to `ui`: at startup and when
/// the window comes to the front (the user may have changed it meanwhile).
pub(crate) fn sync(ui: &zgui::widgets::Ui) {
    if let Some(overlay) = overlay() {
        ui.set_overlay_scrollbars(overlay);
    }
}
