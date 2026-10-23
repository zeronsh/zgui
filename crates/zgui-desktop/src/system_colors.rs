//! The platform's own colours and settings, which the themes take over their
//! built-in values (`Themes::set_system_tokens`): the text selection's, so a
//! selection looks as the system draws one and follows the user's accent,
//! and on Windows high contrast, animations turned off and the text size. An
//! app theme or `theme_scope` that sets these tokens keeps its own; the text
//! size scales the font tokens of every theme.
use zgui::{
    scene::Color,
    theme::{TokenId, TokenInput, tok},
};

/// The system's values for theme tokens. Selection colours: macOS's in a key
/// window and in one at the back; Windows' accent, the selection colour of
/// its own text fields (WinUI's, the new Notepad's), with white text over it.
/// On Windows also high contrast (`windows_contrast`) and animations turned
/// off (`windows_motion`). On Linux the user's accent (`linux_appearance`,
/// the settings portal), translucent as GTK and KDE draw a selection.
/// Empty where zgui reads none, and the built-in themes apply.
fn tokens() -> Vec<(TokenId, TokenInput)> {
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
        let (Some(active), Some(inactive)) = (
            srgb(NSColor::selectedTextBackgroundColor()),
            srgb(NSColor::unemphasizedSelectedTextBackgroundColor()),
        ) else {
            return Vec::new();
        };
        vec![
            (tok::SELECTION.0, active.into()),
            (tok::SELECTION_INACTIVE.0, inactive.into()),
        ]
    }
    #[cfg(windows)]
    {
        let mut tokens = windows_contrast().unwrap_or_else(windows_selection);
        tokens.extend(windows_motion());
        tokens
    }
    #[cfg(target_os = "linux")]
    {
        let Some(Color(r, g, b, _)) = crate::linux_appearance::current().accent else {
            return Vec::new();
        };
        vec![
            (tok::SELECTION.0, Color(r, g, b, 0x66).into()),
            (tok::SELECTION_INACTIVE.0, Color(r, g, b, 0x33).into()),
        ]
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn windows_selection() -> Vec<(TokenId, TokenInput)> {
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    let Some(accent) = UISettings::new()
        .and_then(|settings| settings.GetColorValue(UIColorType::Accent))
        .ok()
    else {
        return Vec::new();
    };
    vec![
        (
            tok::SELECTION.0,
            Color(accent.R, accent.G, accent.B, accent.A).into(),
        ),
        (tok::SELECTION_TEXT.0, Color(255, 255, 255, 255).into()),
        // At the back the built-in grey selection shows; its text keeps its
        // colour (white over the grey would vanish).
        (tok::SELECTION_TEXT_INACTIVE.0, Color(0, 0, 0, 0).into()),
    ]
}

/// High contrast's colours, while it is on: Windows' own controls take theirs
/// from the system's colours then (the user's chosen contrast theme), and so
/// do zgui's, through the tokens the built-in themes set. Colours an app sets
/// itself stay its own, as in a browser.
#[cfg(windows)]
fn windows_contrast() -> Option<Vec<(TokenId, TokenInput)>> {
    use windows::Win32::{
        Graphics::Gdi::{
            COLOR_BTNFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT,
            COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{
                SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        },
    };
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: fills `contrast`, whose size it is given.
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some(&mut contrast as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    if !contrast.dwFlags.contains(HCF_HIGHCONTRASTON) {
        return None;
    }
    let color = |index: SYS_COLOR_INDEX| -> TokenInput {
        // SAFETY: reads one of the system's colours (0x00BBGGRR).
        let bgr = unsafe { GetSysColor(index) };
        Color(bgr as u8, (bgr >> 8) as u8, (bgr >> 16) as u8, 255).into()
    };
    let (window, text) = (COLOR_WINDOW, COLOR_WINDOWTEXT);
    let (highlight, highlight_text) = (COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT);
    Some(vec![
        (tok::BACKGROUND.0, color(window)),
        (tok::SURFACE_SUNKEN.0, color(window)),
        (tok::SURFACE_BASE.0, color(window)),
        (tok::SURFACE_RAISED.0, color(window)),
        (tok::SURFACE_OVERLAY.0, color(window)),
        (tok::TEXT_PRIMARY.0, color(text)),
        (tok::TEXT_SECONDARY.0, color(text)),
        (tok::TEXT_TERTIARY.0, color(text)),
        (tok::TEXT_DISABLED.0, color(COLOR_GRAYTEXT)),
        (tok::BORDER.0, color(text)),
        (tok::BORDER_SUBTLE.0, color(text)),
        (tok::BORDER_STRONG.0, color(text)),
        (tok::ACCENT.0, color(COLOR_HOTLIGHT)),
        (tok::ACCENT_HOVER.0, color(highlight)),
        (tok::ACCENT_TEXT.0, color(highlight_text)),
        (tok::SELECTION.0, color(highlight)),
        (tok::SELECTION_INACTIVE.0, color(highlight)),
        (tok::SELECTION_TEXT.0, color(highlight_text)),
        (tok::SELECTION_TEXT_INACTIVE.0, color(highlight_text)),
        (tok::SELECTION_ITEM.0, color(highlight)),
        (tok::FOCUS_RING.0, color(text)),
        (tok::SCROLLBAR_THUMB.0, color(text)),
        (tok::SCROLLBAR_THUMB_ACTIVE.0, color(highlight)),
        (tok::SCROLLBAR_TRACK.0, color(window)),
        (tok::GLASS_TINT.0, color(window)),
        (tok::BUTTON_BG.0, color(COLOR_BTNFACE)),
        (tok::INPUT_BG.0, color(window)),
        (tok::MENU_BG.0, color(window)),
        // Over a control, the highlight, as Windows' own lists show it.
        (tok::HOVER.0, color(highlight)),
        (tok::PRESSED.0, color(highlight)),
        (tok::BUTTON_HOVER.0, color(highlight)),
        (tok::BUTTON_PRESSED.0, color(highlight)),
        (tok::MENU_HOVER.0, color(highlight)),
        (tok::DANGER.0, color(text)),
        (tok::WARNING.0, color(text)),
        (tok::SUCCESS.0, color(text)),
        (tok::INFO.0, color(text)),
    ])
}

/// Windows' "Animation effects" off (Settings, Accessibility, Visual effects):
/// the themes' motion durations are zero, so what takes them changes at once.
#[cfg(windows)]
fn windows_motion() -> Vec<(TokenId, TokenInput)> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut animate = windows::core::BOOL(1);
    // SAFETY: fills one BOOL.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut animate as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if read.is_err() || animate.as_bool() {
        return Vec::new();
    }
    vec![
        (tok::MOTION_DURATION_FAST.0, 0f32.into()),
        (tok::MOTION_DURATION_NORMAL.0, 0f32.into()),
        (tok::MOTION_DURATION_SLOW.0, 0f32.into()),
    ]
}

/// Read the system's values again and give them to `ui`'s themes: at startup,
/// and when the system's appearance or settings or the window's activity
/// change (the user may have picked another accent meanwhile).
pub(crate) fn sync(ui: &zgui::widgets::Ui) {
    ui.theme().set_system_tokens(tokens());
    #[cfg(windows)]
    ui.theme().set_system_text_scale(windows_text_scale());
}

/// Windows' "Text size" (Settings, Accessibility): 1 to 2.25, scaling text
/// alone, as WinUI's text follows it.
#[cfg(windows)]
fn windows_text_scale() -> f32 {
    use windows::UI::ViewManagement::UISettings;
    UISettings::new()
        .and_then(|settings| settings.TextScaleFactor())
        .map_or(1., |scale| scale as f32)
}
