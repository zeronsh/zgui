//! The platform's own colours and settings, which the themes take over their
//! built-in values (`Themes::set_system_tokens`): the text selection's, so a
//! selection looks as the system draws one and follows the user's accent,
//! and on Windows and Linux high contrast, animations turned off and the
//! text size. An app theme or `theme_scope` that sets these tokens keeps its
//! own; the text size scales the font tokens of every theme.
use zgx::{
    scene::Color,
    theme::{SystemPreferences, TokenId, TokenInput, tok},
};

/// The system's values for theme tokens. Selection colours: macOS's in a key
/// window and in one at the back; Windows' accent, the selection colour of
/// its own text fields (WinUI's, the new Notepad's), with white text over it.
/// On Windows also high contrast (`windows_contrast`) and animations turned
/// off (`windows_motion`). On Linux the user's accent (`linux_appearance`,
/// the settings portal), translucent as GTK and KDE draw a selection, and
/// high contrast and reduced motion.
/// Empty where zgx reads none, and the built-in themes apply.
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
        let mut tokens = vec![
            (tok::SELECTION.0, active.into()),
            (tok::SELECTION_INACTIVE.0, inactive.into()),
        ];
        // Menus and popovers in windows of their own are AppKit's material
        // (`popup_window`), clear over it; a menu's chosen item, the accent,
        // as AppKit highlights it.
        if crate::popup_window::enabled() {
            let clear = Color(0, 0, 0, 0);
            tokens.push((tok::MENU_BG.0, clear.into()));
            tokens.push((tok::POPOVER_BG.0, clear.into()));
            if let Some(accent) = srgb(NSColor::controlAccentColor()) {
                tokens.push((tok::MENU_HOVER.0, accent.into()));
            }
        }
        tokens
    }
    #[cfg(windows)]
    {
        let mut tokens = windows_contrast().unwrap_or_else(windows_selection);
        tokens.extend(windows_motion());
        tokens
    }
    #[cfg(target_os = "linux")]
    {
        let settings = crate::linux_appearance::current();
        let mut tokens = if settings.contrast {
            // The portal says only that contrast is high, not in which
            // colours: the standard high-contrast palettes, light or dark.
            let light = settings.scheme == Some(zgx::theme::Appearance::Light);
            contrast_tokens(if light { Palette::LIGHT } else { Palette::DARK })
        } else if let Some(Color(r, g, b, _)) = settings.accent {
            vec![
                (tok::SELECTION.0, Color(r, g, b, 0x66).into()),
                (tok::SELECTION_INACTIVE.0, Color(r, g, b, 0x33).into()),
            ]
        } else {
            Vec::new()
        };
        if settings.reduced_motion {
            tokens.extend(still());
        }
        tokens
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        Vec::new()
    }
}

/// High contrast's colours: the system's (Windows) or a standard palette
/// (Linux, where the portal gives none).
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
struct Palette {
    window: Color,
    text: Color,
    gray: Color,
    link: Color,
    highlight: Color,
    highlight_text: Color,
    button: Color,
}
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Palette {
    /// Windows' High Contrast Black and White, which GNOME's and KDE's
    /// high-contrast themes resemble.
    const DARK: Self = Self {
        window: Color(0, 0, 0, 255),
        text: Color(255, 255, 255, 255),
        gray: Color(63, 242, 63, 255),
        link: Color(255, 255, 0, 255),
        highlight: Color(26, 235, 255, 255),
        highlight_text: Color(0, 0, 0, 255),
        button: Color(0, 0, 0, 255),
    };
    const LIGHT: Self = Self {
        window: Color(255, 255, 255, 255),
        text: Color(0, 0, 0, 255),
        gray: Color(96, 0, 0, 255),
        link: Color(0, 0, 159, 255),
        highlight: Color(55, 0, 110, 255),
        highlight_text: Color(255, 255, 255, 255),
        button: Color(255, 255, 255, 255),
    };
}
/// The tokens high contrast sets, from its palette: the app's controls take
/// its colours then, through the tokens the built-in themes set. Colours an
/// app sets itself stay its own, as in a browser.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
fn contrast_tokens(p: Palette) -> Vec<(TokenId, TokenInput)> {
    vec![
        (tok::BACKGROUND.0, p.window.into()),
        (tok::SURFACE_SUNKEN.0, p.window.into()),
        (tok::SURFACE_BASE.0, p.window.into()),
        (tok::SURFACE_RAISED.0, p.window.into()),
        (tok::SURFACE_OVERLAY.0, p.window.into()),
        (tok::TEXT_PRIMARY.0, p.text.into()),
        (tok::TEXT_SECONDARY.0, p.text.into()),
        (tok::TEXT_TERTIARY.0, p.text.into()),
        (tok::TEXT_DISABLED.0, p.gray.into()),
        (tok::BORDER.0, p.text.into()),
        (tok::BORDER_SUBTLE.0, p.text.into()),
        (tok::BORDER_STRONG.0, p.text.into()),
        (tok::ACCENT.0, p.link.into()),
        (tok::ACCENT_HOVER.0, p.highlight.into()),
        (tok::ACCENT_TEXT.0, p.highlight_text.into()),
        (tok::SELECTION.0, p.highlight.into()),
        (tok::SELECTION_INACTIVE.0, p.highlight.into()),
        (tok::SELECTION_TEXT.0, p.highlight_text.into()),
        (tok::SELECTION_TEXT_INACTIVE.0, p.highlight_text.into()),
        (tok::SELECTION_ITEM.0, p.highlight.into()),
        (tok::FOCUS_RING.0, p.text.into()),
        (tok::SCROLLBAR_THUMB.0, p.text.into()),
        (tok::SCROLLBAR_THUMB_ACTIVE.0, p.highlight.into()),
        (tok::SCROLLBAR_TRACK.0, p.window.into()),
        (tok::GLASS_TINT.0, p.window.into()),
        (tok::BUTTON_BG.0, p.button.into()),
        (tok::INPUT_BG.0, p.window.into()),
        (tok::MENU_BG.0, p.window.into()),
        // Over a control, the highlight, as Windows' own lists show it.
        (tok::HOVER.0, p.highlight.into()),
        (tok::PRESSED.0, p.highlight.into()),
        (tok::BUTTON_HOVER.0, p.highlight.into()),
        (tok::BUTTON_PRESSED.0, p.highlight.into()),
        (tok::MENU_HOVER.0, p.highlight.into()),
        (tok::DANGER.0, p.text.into()),
        (tok::WARNING.0, p.text.into()),
        (tok::SUCCESS.0, p.text.into()),
        (tok::INFO.0, p.text.into()),
    ]
}
/// Animations turned off: every motion duration token at zero.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
fn still() -> Vec<(TokenId, TokenInput)> {
    vec![
        (tok::MOTION_DURATION_FAST.0, 0f32.into()),
        (tok::MOTION_DURATION_NORMAL.0, 0f32.into()),
        (tok::MOTION_DURATION_SLOW.0, 0f32.into()),
    ]
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
/// do zgx's, through the tokens the built-in themes set. Colours an app sets
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
    let color = |index: SYS_COLOR_INDEX| -> Color {
        // SAFETY: reads one of the system's colours (0x00BBGGRR).
        let bgr = unsafe { GetSysColor(index) };
        Color(bgr as u8, (bgr >> 8) as u8, (bgr >> 16) as u8, 255)
    };
    Some(contrast_tokens(Palette {
        window: color(COLOR_WINDOW),
        text: color(COLOR_WINDOWTEXT),
        gray: color(COLOR_GRAYTEXT),
        link: color(COLOR_HOTLIGHT),
        highlight: color(COLOR_HIGHLIGHT),
        highlight_text: color(COLOR_HIGHLIGHTTEXT),
        button: color(COLOR_BTNFACE),
    }))
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
    still()
}

/// The system's accessibility display settings (`SystemPreferences`):
/// macOS's Reduce motion, Increase contrast and Reduce transparency;
/// Windows' animation effects off and contrast themes; the Linux desktop's
/// reduced motion and high contrast (`linux_appearance`).
fn preferences() -> SystemPreferences {
    #[cfg(target_os = "macos")]
    {
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        SystemPreferences {
            reduced_motion: workspace.accessibilityDisplayShouldReduceMotion(),
            increased_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
            reduced_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
        }
    }
    #[cfg(windows)]
    {
        SystemPreferences {
            reduced_motion: !windows_motion().is_empty(),
            increased_contrast: windows_contrast().is_some(),
            reduced_transparency: false,
        }
    }
    #[cfg(target_os = "linux")]
    {
        let settings = crate::linux_appearance::current();
        SystemPreferences {
            reduced_motion: settings.reduced_motion,
            increased_contrast: settings.contrast,
            reduced_transparency: false,
        }
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        SystemPreferences::default()
    }
}

/// What macOS's display settings make of the themes' tokens. Reduce motion:
/// motion durations of zero, so what takes them changes at once. Increase
/// contrast: borders as strong as secondary text and tertiary text as
/// secondary, as AppKit darkens separators and secondary labels. Reduce
/// transparency: glass opaque, unblurred. (Windows' come with its own
/// colours, in `tokens`.)
#[cfg(target_os = "macos")]
fn preference_tokens(preferences: SystemPreferences) -> Vec<(TokenId, TokenInput)> {
    let expr = |text: &str| TokenInput::parse(text).expect("a theme expression");
    let mut tokens = Vec::new();
    if preferences.reduced_motion {
        tokens.extend([
            (tok::MOTION_DURATION_FAST.0, 0f32.into()),
            (tok::MOTION_DURATION_NORMAL.0, 0f32.into()),
            (tok::MOTION_DURATION_SLOW.0, 0f32.into()),
        ]);
    }
    if preferences.increased_contrast {
        tokens.extend([
            (tok::BORDER_SUBTLE.0, expr("{text.secondary}")),
            (tok::BORDER.0, expr("{text.secondary}")),
            (tok::BORDER_STRONG.0, expr("{text.primary}")),
            (tok::TEXT_TERTIARY.0, expr("{text.secondary}")),
        ]);
    }
    if preferences.reduced_transparency {
        tokens.extend([
            (tok::GLASS_TINT.0, expr("{surface.raised}")),
            (tok::GLASS_BLUR.0, 0f32.into()),
        ]);
    }
    tokens
}

/// The way the user's language runs (`ZGX_DIRECTION=rtl` or `ltr`
/// overrides it): macOS's first preferred language, Windows' display
/// locale's reading layout, the Linux environment's `LC_ALL`,
/// `LC_MESSAGES` or `LANG`.
pub(crate) fn locale_direction() -> zgx::scene::Direction {
    use zgx::scene::Direction;
    match zgx::env::var("ZGX_DIRECTION").as_deref() {
        Ok("rtl") => return Direction::Rtl,
        Ok("ltr") => return Direction::Ltr,
        _ => {}
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSLocale, NSLocaleLanguageDirection};
        let languages = NSLocale::preferredLanguages();
        if let Some(first) = languages.firstObject() {
            return if NSLocale::characterDirectionForLanguage(&first)
                == NSLocaleLanguageDirection::RightToLeft
            {
                Direction::Rtl
            } else {
                Direction::Ltr
            };
        }
        Direction::Ltr
    }
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_IREADINGLAYOUT};
        let mut buffer = [0_u16; 4];
        // 1: right to left (Arabic, Hebrew, Persian, Urdu).
        // SAFETY: the user's locale; writes at most `buffer`'s length.
        let written = unsafe { GetLocaleInfoEx(None, LOCALE_IREADINGLAYOUT, Some(&mut buffer)) };
        if written > 0 && buffer[0] == u16::from(b'1') {
            Direction::Rtl
        } else {
            Direction::Ltr
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
            .map_or(Direction::Ltr, |tag| Direction::of_language(&tag))
    }
}

/// Read the system's values again and give them to `ui`'s themes: at startup,
/// and when the system's appearance or settings or the window's activity
/// change (the user may have picked another accent meanwhile).
pub(crate) fn sync(ui: &zgx::widgets::Ui) {
    // Linux's dark or light: winit reports none there. The portal's
    // preference, or with none (XFCE's), on X11 the GTK theme's, as GTK's
    // own apps are dark under a dark theme.
    #[cfg(target_os = "linux")]
    if let Some(scheme) = crate::linux_appearance::current().scheme.or_else(|| {
        use zgx::theme::Appearance::{Dark, Light};
        let dark = crate::x11_settings::current().dark_theme?;
        Some(if dark { Dark } else { Light })
    }) {
        ui.theme().set_system_appearance(scheme);
    }
    let preferences = preferences();
    // The window runs the way the user's language does; an app's own
    // `direction` (`dir`) on its views decides from there.
    {
        let mut scene = ui.scene.borrow_mut();
        let root = scene.root();
        let direction = locale_direction();
        scene.set_direction(
            root,
            (direction == zgx::scene::Direction::Rtl).then_some(direction),
        );
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut tokens = tokens();
    #[cfg(target_os = "macos")]
    tokens.extend(preference_tokens(preferences));
    ui.theme().set_system_tokens(tokens);
    ui.theme().set_system_preferences(preferences);
    if let Some(interval) = double_click_time() {
        ui.input.set_double_click_time(interval);
    }
    // On X11 the session's XSETTINGS come first, as GTK takes them there.
    #[cfg(target_os = "linux")]
    {
        let x11 = crate::x11_settings::current();
        if let Some(ms) = x11.double_click {
            ui.input
                .set_double_click_time(std::time::Duration::from_millis(ms.into()));
        }
        ui.input.set_drag_threshold(linux_drag_threshold(
            x11.drag_threshold
                .or(crate::linux_appearance::current().drag_threshold),
        ));
    }
    #[cfg(windows)]
    ui.theme().set_system_text_scale(windows_text_scale());
    #[cfg(target_os = "linux")]
    ui.theme()
        .set_system_text_scale(crate::linux_appearance::current().text_scale.unwrap_or(1.));
}

/// The system's double-click time (macOS's and Windows' mouse settings,
/// GNOME's and KDE's), which people who press slowly lengthen.
fn double_click_time() -> Option<std::time::Duration> {
    #[cfg(target_os = "macos")]
    {
        let seconds = objc2_app_kit::NSEvent::doubleClickInterval();
        (seconds > 0.).then(|| std::time::Duration::from_secs_f64(seconds))
    }
    #[cfg(windows)]
    {
        // SAFETY: reads one system setting.
        let ms = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() };
        (ms > 0).then(|| std::time::Duration::from_millis(ms.into()))
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_appearance::current()
            .double_click
            .map(|ms| std::time::Duration::from_millis(ms.into()))
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        None
    }
}

/// The desktop's drag threshold (GNOME's and KDE's), in the logical pixels
/// GTK reads it in, zgx's points; GTK's own default, 8, where it gives none.
#[cfg(target_os = "linux")]
fn linux_drag_threshold(setting: Option<u32>) -> f32 {
    setting.map_or(8., |pixels| pixels as f32)
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

/// macOS's display settings changed since the last call: the host syncs
/// every window then. The first call starts listening.
pub(crate) fn display_options_changed() -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static CHANGED: AtomicBool = AtomicBool::new(false);
        static LISTENING: std::sync::Once = std::sync::Once::new();
        LISTENING.call_once(|| {
            let center = objc2_app_kit::NSWorkspace::sharedWorkspace().notificationCenter();
            let block =
                block2::RcBlock::new(|_: std::ptr::NonNull<objc2_foundation::NSNotification>| {
                    CHANGED.store(true, Ordering::Relaxed);
                });
            // SAFETY: a notification name, no sender filter, delivered on the
            // posting (main) thread; the block only sets a flag. The
            // observer lives as long as the app.
            let observer = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(
                        objc2_app_kit::NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
                    ),
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
