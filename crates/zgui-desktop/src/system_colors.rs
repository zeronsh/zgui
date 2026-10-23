//! The platform's own colours, which the themes take over their built-in
//! values (`Themes::set_system_tokens`): today the text selection's, so a
//! selection looks as the system draws one and follows the user's accent.
//! An app theme or `theme_scope` that sets these tokens keeps its own.
use zgui::{scene::Color, theme::tok};

/// The system's selection colours: in a key window and in one at the back.
/// `None` where zgui does not read them (Windows and Linux: hooks for their
/// hosts), and the built-in theme's apply.
fn selection() -> Option<(Color, Color)> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSColor, NSColorSpace};
        let srgb = |color: objc2::rc::Retained<NSColor>| {
            let color = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
            let channel = |value: f64| (value.clamp(0., 1.) * 255.).round() as u8;
            Some(Color(
                channel(color.redComponent()),
                channel(color.greenComponent()),
                channel(color.blueComponent()),
                channel(color.alphaComponent()),
            ))
        };
        Some((
            srgb(NSColor::selectedTextBackgroundColor())?,
            srgb(NSColor::unemphasizedSelectedTextBackgroundColor())?,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Read the system's colours again and give them to `ui`'s themes: at
/// startup, and when the system's appearance or the window's activity
/// changes (the user may have picked another accent meanwhile).
pub(crate) fn sync(ui: &zgui::widgets::Ui) {
    if let Some((active, inactive)) = selection() {
        ui.theme().set_system_tokens([
            (tok::SELECTION.0, active.into()),
            (tok::SELECTION_INACTIVE.0, inactive.into()),
        ]);
    }
}
