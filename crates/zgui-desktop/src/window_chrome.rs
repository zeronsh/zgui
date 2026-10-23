//! Window chrome from `WindowOptions`: the title bar's style and a frosted
//! glass behind a translucent window. Applied when the window is made, on
//! macOS and Windows; on Linux zgui draws an undecorated window's buttons
//! and resizes it from its edges, and draws no glass.

/// How a window's title bar looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Titlebar {
    /// The platform's title bar.
    #[default]
    Standard,
    /// Transparent, without the title: content runs under it, the window
    /// buttons (the traffic lights) inset over it in a 40-point strip, as
    /// Electron's `hiddenInset`. Mark the strip `View::window_drag` so it
    /// moves the window. On Windows, zgui draws Windows 11's caption buttons
    /// over the content's top right (46 by 32 points each) and Windows takes
    /// them for its own: their tooltips, and snap layouts on maximize. On
    /// Linux the window is undecorated, as a GTK header bar's, and zgui
    /// draws libadwaita's window buttons over the strip, in the order the
    /// desktop asks.
    Inset,
    /// None: content fills the window and the window buttons are hidden.
    /// The window keeps its rounded corners and shadow; on Windows and Linux
    /// its edges resize it (5 points in from them).
    Hidden,
}

/// A blurred desktop behind a translucent window (macOS's behind-window
/// `NSVisualEffectView`, cut to the window's rounded corners; on Windows 11
/// the system backdrop nearest its material, Acrylic or Mica). The window is
/// transparent where zgui paints nothing (or translucent colour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glass {
    pub material: GlassMaterial,
    /// Only the blur: no tint from the desktop, no saturation boost, a
    /// 60-point gaussian, as gpui's `BlurredView` (for apps laying their own
    /// tint over it).
    pub plain: bool,
}
impl Glass {
    pub fn new(material: GlassMaterial) -> Self {
        Self {
            material,
            plain: false,
        }
    }
    /// The same glass, untinted and unsaturated (see `Glass::plain`).
    pub fn plain(mut self) -> Self {
        self.plain = true;
        self
    }
}
impl Default for Glass {
    fn default() -> Self {
        Self::new(GlassMaterial::Hud)
    }
}
/// The glass's material (`NSVisualEffectMaterial`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassMaterial {
    /// A dark, saturated heads-up-display glass.
    #[default]
    Hud,
    /// The colourless material under a window's own background.
    UnderWindowBackground,
    UnderPageBackground,
    WindowBackground,
    ContentBackground,
    Titlebar,
    HeaderView,
    Sidebar,
    Menu,
    Popover,
    Sheet,
    ToolTip,
    FullScreenUi,
    Selection,
}
impl GlassMaterial {
    /// Its `NSVisualEffectMaterial` value.
    pub fn appkit_value(self) -> isize {
        match self {
            Self::Titlebar => 3,
            Self::Selection => 4,
            Self::Menu => 5,
            Self::Popover => 6,
            Self::Sidebar => 7,
            Self::HeaderView => 10,
            Self::Sheet => 11,
            Self::WindowBackground => 12,
            Self::Hud => 13,
            Self::FullScreenUi => 15,
            Self::ToolTip => 17,
            Self::ContentBackground => 18,
            Self::UnderWindowBackground => 21,
            Self::UnderPageBackground => 22,
        }
    }
}

/// Style `window` as `titlebar` and `glass` ask: its corner radius in points
/// (content inset from the window's edge keeps its corners concentric with
/// it), 0 where the platform does not round windows or zgui cannot tell.
pub(crate) fn apply(
    window: &winit::window::Window,
    titlebar: Titlebar,
    glass: Option<Glass>,
) -> f64 {
    #[cfg(target_os = "macos")]
    {
        mac::apply(window, titlebar, glass)
    }
    #[cfg(windows)]
    {
        win::apply(window, titlebar, glass)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (window, titlebar, glass);
        0.
    }
}

#[cfg(windows)]
pub(crate) use win::{CaptionHost, fit_work_area, menu_theme};

#[cfg(windows)]
mod win {
    //! Windows title bars. `Titlebar::Hidden` is winit's frameless window
    //! (its client the whole window, with the system's shadow) with what
    //! the frame gave back: rounded corners on Windows 11 and resize bands
    //! along its edges. `Titlebar::Inset` adds the caption buttons, drawn
    //! by zgui over the content's top right (`CaptionHost`) as Windows 11
    //! draws its own, and reported to Windows as its buttons
    //! (`WM_NCHITTEST`), so it shows the maximize button's snap layouts and
    //! the buttons' tooltips. Windows' own handling of a press on them draws
    //! classic buttons into the window, so the subclass handles them itself.
    use super::{Glass, GlassMaterial, Titlebar};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        sync::Arc,
    };
    use windows::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_TABBEDWINDOW,
                DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
                DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
                DwmSetWindowAttribute,
            },
            Gdi::ScreenToClient,
        },
        UI::{
            Controls::MARGINS,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLOSE, HTLEFT, HTMAXBUTTON,
                HTMINBUTTON, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, PostMessageW,
                SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE, SendMessageW, WM_MOUSEMOVE,
                WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP,
                WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SETTINGCHANGE, WM_SYSCOLORCHANGE,
                WM_SYSCOMMAND, WM_THEMECHANGED,
            },
        },
    };
    use winit::{
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };
    use zgui::{compose::prelude::*, scene::Color, style::Styled, widgets::Ui};

    /// Windows 11's caption buttons, in logical pixels.
    const BUTTON_WIDTH: f64 = 46.;
    const BUTTON_HEIGHT: f64 = 32.;
    /// Resize bands inside a frameless window's edges, in logical pixels:
    /// it has no frame outside them to take the pointer.
    const RESIZE_BAND: f64 = 5.;
    /// Windows 11's window corner radius, in logical pixels.
    const CORNER_RADIUS: f64 = 8.;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum CaptionButton {
        Minimize,
        Maximize,
        Close,
    }
    impl CaptionButton {
        fn hit(self) -> u32 {
            match self {
                Self::Minimize => HTMINBUTTON,
                Self::Maximize => HTMAXBUTTON,
                Self::Close => HTCLOSE,
            }
        }
        fn from_hit(hit: usize) -> Option<Self> {
            [Self::Minimize, Self::Maximize, Self::Close]
                .into_iter()
                .find(|button| button.hit() as usize == hit)
        }
    }

    /// What the window procedure knows of the caption, shared with its host.
    struct Caption {
        /// The window has no frame (Inset, Hidden): its edges resize it.
        frameless: bool,
        buttons: bool,
        /// The system's settings or colours changed (high contrast, the
        /// accent, animations) since the host last read them.
        settings_changed: Cell<bool>,
        hovered: Cell<Option<CaptionButton>>,
        pressed: Cell<Option<CaptionButton>>,
        tracking: Cell<bool>,
        /// Asks the host for a frame when the buttons' look changed.
        wake: RefCell<Option<Box<dyn Fn()>>>,
    }
    impl Caption {
        fn set(&self, hovered: Option<CaptionButton>, pressed: Option<CaptionButton>) {
            let hover_changed = self.hovered.replace(hovered) != hovered;
            let press_changed = self.pressed.replace(pressed) != pressed;
            if (hover_changed || press_changed)
                && let Some(wake) = &*self.wake.borrow()
            {
                wake();
            }
        }
    }

    fn hwnd(window: &Window) -> Option<HWND> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
            _ => None,
        }
    }

    /// The system backdrop nearest a glass material: Acrylic (a blur, as
    /// macOS's materials are) for the transient and heads-up ones, Mica (the
    /// wallpaper's tint) under a window's or page's background, Mica Alt
    /// under title bars and headers.
    fn backdrop(material: GlassMaterial) -> DWM_SYSTEMBACKDROP_TYPE {
        match material {
            GlassMaterial::UnderWindowBackground
            | GlassMaterial::UnderPageBackground
            | GlassMaterial::WindowBackground
            | GlassMaterial::ContentBackground
            | GlassMaterial::Sidebar => DWMSBT_MAINWINDOW,
            GlassMaterial::Titlebar | GlassMaterial::HeaderView => DWMSBT_TABBEDWINDOW,
            GlassMaterial::Hud
            | GlassMaterial::Menu
            | GlassMaterial::Popover
            | GlassMaterial::ToolTip
            | GlassMaterial::Sheet
            | GlassMaterial::FullScreenUi
            | GlassMaterial::Selection => DWMSBT_TRANSIENTWINDOW,
        }
    }

    /// Put the system's backdrop behind the window: the frame, and its
    /// material, extended under the whole client, which shows it where zgui
    /// paints nothing. Windows 11 (22H2) only; elsewhere the window is
    /// transparent alone. `Glass::plain` has no Windows equivalent.
    fn frost(hwnd: HWND, glass: Glass, dark: bool) {
        // From the bottom, over the whole client: extended from the top, the
        // frame brings Windows' own caption buttons into the client, under
        // zgui's (or under nothing, where it paints none).
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 0,
            cyBottomHeight: i32::from(u16::MAX),
        };
        let kind = backdrop(glass.material);
        let dark = i32::from(dark);
        // SAFETY: our own window; each attribute is given its own size.
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const _ as *const _,
                size_of_val(&dark) as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &kind as *const _ as *const _,
                size_of_val(&kind) as u32,
            );
        }
    }

    /// The executable's own icon (its first icon group, the one Explorer
    /// shows) on the window's title bar, taskbar button and Alt+Tab: winit's
    /// windows have none, and Windows drew a blank page. An executable
    /// without one keeps that. Sized for the window's DPI.
    fn icon(hwnd: HWND) {
        use windows::{
            Win32::{
                Foundation::{HMODULE, LPARAM, WPARAM},
                System::LibraryLoader::{EnumResourceNamesW, GetModuleHandleW},
                UI::{
                    HiDpi::GetSystemMetricsForDpi,
                    WindowsAndMessaging::{
                        ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW,
                        RT_GROUP_ICON, SM_CXICON, SM_CXSMICON, SendMessageW, WM_SETICON,
                    },
                },
            },
            core::{BOOL, PCWSTR},
        };
        /// The first icon group's name: an integer ID, or a string copied out.
        enum Name {
            Id(usize),
            Text(Vec<u16>),
        }
        unsafe extern "system" fn first(_: HMODULE, _: PCWSTR, name: PCWSTR, found: isize) -> BOOL {
            // SAFETY: `found` is the `Option<Name>` below, for this call;
            // a string name is valid during it.
            unsafe {
                let found = &mut *(found as *mut Option<Name>);
                *found = Some(if (name.0 as usize) >> 16 == 0 {
                    Name::Id(name.0 as usize)
                } else {
                    let mut text = name.as_wide().to_vec();
                    text.push(0);
                    Name::Text(text)
                });
            }
            false.into()
        }
        // SAFETY: this executable's resources; the callback writes `found`
        // only while it runs.
        unsafe {
            let Ok(module) = GetModuleHandleW(None) else {
                return;
            };
            let mut found: Option<Name> = None;
            let _ = EnumResourceNamesW(
                Some(module),
                RT_GROUP_ICON,
                Some(first),
                &mut found as *mut _ as isize,
            );
            let name = match &found {
                Some(Name::Id(id)) => PCWSTR(*id as *const u16),
                Some(Name::Text(text)) => PCWSTR(text.as_ptr()),
                None => return,
            };
            let dpi = GetDpiForWindow(hwnd);
            for (size, which) in [(SM_CXICON, ICON_BIG), (SM_CXSMICON, ICON_SMALL)] {
                let side = GetSystemMetricsForDpi(size, dpi);
                if let Ok(icon) = LoadImageW(
                    Some(module.into()),
                    name,
                    IMAGE_ICON,
                    side,
                    side,
                    LR_DEFAULTCOLOR,
                ) {
                    SendMessageW(
                        hwnd,
                        WM_SETICON,
                        Some(WPARAM(which as usize)),
                        Some(LPARAM(icon.0 as isize)),
                    );
                }
            }
        }
    }

    /// Fit a new window to its monitor's work area (the screen less the
    /// taskbar): a size asked for in points can be more pixels than the
    /// screen has at a high scale (1180 by 760 points is 1770 by 1140
    /// pixels at 150%, past a 1080-pixel screen), and Windows, unlike macOS,
    /// shows it so. It shrinks to fit, though not below `min` (logical), and
    /// moves inside.
    pub(crate) fn fit_work_area(window: &Window, min: Option<(f64, f64)>) {
        use windows::Win32::{
            Graphics::Gdi::{
                GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
            },
            UI::WindowsAndMessaging::{GetWindowRect, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
        };
        let Some(hwnd) = hwnd(window) else {
            return;
        };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut rect = RECT::default();
        // SAFETY: our own window; each call writes into a local.
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if !GetMonitorInfoW(monitor, &mut info).as_bool()
                || GetWindowRect(hwnd, &mut rect).is_err()
            {
                return;
            }
        }
        let work = info.rcWork;
        let scale = scale(hwnd);
        let (min_w, min_h) = min.map_or((0, 0), |(w, h)| {
            ((w * scale).ceil() as i32, (h * scale).ceil() as i32)
        });
        let fitted = fit(
            (
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            ),
            (
                work.left,
                work.top,
                work.right - work.left,
                work.bottom - work.top,
            ),
            (min_w, min_h),
        );
        if fitted
            != (
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            )
        {
            let (x, y, width, height) = fitted;
            // SAFETY: our own window, still hidden.
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
        }
    }

    /// Window rectangle `(x, y, width, height)` fitted inside `work`: no
    /// larger than it (nor smaller than `min`), and moved inside it.
    fn fit(
        (x, y, width, height): (i32, i32, i32, i32),
        (wx, wy, ww, wh): (i32, i32, i32, i32),
        (min_w, min_h): (i32, i32),
    ) -> (i32, i32, i32, i32) {
        let width = width.min(ww).max(min_w);
        let height = height.min(wh).max(min_h);
        let x = x.min(wx + ww - width).max(wx);
        let y = y.min(wy + wh - height).max(wy);
        (x, y, width, height)
    }

    /// Windows' own menu for the window (Alt+Space, a right click on its
    /// title bar) follows the system's dark mode, as Explorer's and
    /// Terminal's do; winit darkens only the title bar. Called as the window
    /// is made and when the mode changes. uxtheme exports this only by
    /// ordinal, its numbers other functions' before Windows 10 1903 (18362).
    /// "Allow dark", not forced: winit reads the system's mode through
    /// uxtheme, and a forced mode would hide its changes.
    pub(crate) fn menu_theme(window: &Window) {
        use std::sync::OnceLock;
        use windows::{
            Wdk::System::SystemServices::RtlGetVersion,
            Win32::System::{
                LibraryLoader::{GetProcAddress, LoadLibraryW},
                SystemInformation::OSVERSIONINFOW,
            },
            core::{BOOL, PCSTR, w},
        };
        type Proc = unsafe extern "system" fn() -> isize;
        type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
        type Refresh = unsafe extern "system" fn();
        type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
        type Uxtheme = (SetPreferredAppMode, [Refresh; 2], AllowDarkModeForWindow);
        static UXTHEME: OnceLock<Option<Uxtheme>> = OnceLock::new();
        let uxtheme = UXTHEME.get_or_init(|| {
            let mut version = OSVERSIONINFOW {
                dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
                ..Default::default()
            };
            // SAFETY: fills `version`, whose size it is given.
            if unsafe { RtlGetVersion(&mut version) }.is_err()
                || version.dwMajorVersion < 10
                || version.dwBuildNumber < 18362
            {
                return None;
            }
            // SAFETY: uxtheme stays loaded (comctl32's controls use it), and
            // from 1903 (18362) the ordinals are these functions.
            unsafe {
                let uxtheme = LoadLibraryW(w!("uxtheme.dll")).ok()?;
                let ordinal = |n: usize| GetProcAddress(uxtheme, PCSTR(n as _));
                let refresh = |n| Some(std::mem::transmute::<Proc, Refresh>(ordinal(n)?));
                Some((
                    std::mem::transmute::<Proc, SetPreferredAppMode>(ordinal(135)?),
                    // RefreshImmersiveColorPolicyState, FlushMenuThemes.
                    [refresh(104)?, refresh(136)?],
                    std::mem::transmute::<Proc, AllowDarkModeForWindow>(ordinal(133)?),
                ))
            }
        });
        let (Some((set_mode, refresh, allow)), Some(hwnd)) = (uxtheme, hwnd(window)) else {
            return;
        };
        // SAFETY: our own window; none of these keeps anything of ours. The
        // window opened its menu's theme when created: it opens it again
        // for the new mode on WM_THEMECHANGED.
        unsafe {
            let _ = allow(hwnd, true.into());
            set_mode(1); // Allow dark.
            refresh.iter().for_each(|refresh| refresh());
            SendMessageW(hwnd, WM_THEMECHANGED, None, None);
        }
    }

    pub(super) fn apply(window: &Window, titlebar: Titlebar, glass: Option<Glass>) -> f64 {
        let Some(hwnd) = hwnd(window) else {
            return 0.;
        };
        menu_theme(window);
        icon(hwnd);
        if let Some(glass) = glass {
            frost(
                hwnd,
                glass,
                window.theme() != Some(winit::window::Theme::Light),
            );
        }
        // A frameless window is square unless asked; Windows 10 has no
        // rounded windows and refuses.
        let frameless = titlebar != Titlebar::Standard;
        // SAFETY: sets one documented attribute on our own window.
        let rounded = frameless
            && unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &DWMWCP_ROUND as *const _ as *const _,
                    size_of_val(&DWMWCP_ROUND) as u32,
                )
            }
            .is_ok();
        // Every window is subclassed: for the frame's work of a frameless
        // one, and to hear the system's settings change.
        let caption = Rc::new(Caption {
            frameless,
            buttons: titlebar == Titlebar::Inset,
            settings_changed: Cell::new(false),
            hovered: Cell::new(None),
            pressed: Cell::new(None),
            tracking: Cell::new(false),
            wake: RefCell::new(None),
        });
        // SAFETY: the subclass runs on this (the window's) thread; its
        // reference is released when the window is destroyed.
        let installed = unsafe {
            SetWindowSubclass(
                hwnd,
                Some(subclass),
                SUBCLASS_ID,
                Rc::into_raw(caption.clone()) as usize,
            )
        }
        .as_bool();
        if installed {
            CAPTIONS.with(|captions| captions.borrow_mut().push((hwnd.0 as usize, caption)));
        }
        if rounded { CORNER_RADIUS } else { 0. }
    }

    const SUBCLASS_ID: usize = 0x7a67_7569; // "zgui"
    thread_local! {
        /// Each subclassed window's caption, for its host to find.
        static CAPTIONS: RefCell<Vec<(usize, Rc<Caption>)>> = const { RefCell::new(Vec::new()) };
    }

    fn scale(hwnd: HWND) -> f64 {
        // SAFETY: reads our own window's DPI.
        f64::from(unsafe { GetDpiForWindow(hwnd) }.max(96)) / 96.
    }

    /// What lies at screen point (`x`, `y`) of the window's client, if not
    /// the content: a resize band or a caption button.
    fn hit_test(hwnd: HWND, caption: &Caption, x: i32, y: i32) -> Option<u32> {
        let mut point = POINT { x, y };
        let mut client = RECT::default();
        // SAFETY: our own window; both write into locals.
        unsafe {
            if !ScreenToClient(hwnd, &mut point).as_bool()
                || GetClientRect(hwnd, &mut client).is_err()
            {
                return None;
            }
        }
        // SAFETY: reads our own window's state.
        let maximized = unsafe { IsZoomed(hwnd) }.as_bool();
        hit(
            (point.x, point.y),
            (client.right, client.bottom),
            scale(hwnd),
            maximized,
            caption.buttons,
        )
    }

    /// What client point (`x`, `y`) of a frameless window `width` by
    /// `height` physical pixels is at `scale`, if not its content: a resize
    /// band or one of its caption buttons (`buttons`).
    fn hit(
        (x, y): (i32, i32),
        (width, height): (i32, i32),
        scale: f64,
        maximized: bool,
        buttons: bool,
    ) -> Option<u32> {
        let band = if maximized {
            0
        } else {
            (RESIZE_BAND * scale).round() as i32
        };
        let (left, right) = (x < band, x >= width - band);
        let (top, bottom) = (y < band, y >= height - band);
        // The caption buttons keep all but the window's outermost pixels.
        if buttons && y >= band.min(1) && f64::from(y) < BUTTON_HEIGHT * scale {
            let button = (BUTTON_WIDTH * scale).round() as i32;
            let from_right = width - 1 - x;
            if (band.min(1)..3 * button).contains(&from_right) {
                return Some(match from_right / button {
                    0 => HTCLOSE,
                    1 => HTMAXBUTTON,
                    _ => HTMINBUTTON,
                });
            }
        }
        Some(match (top, bottom, left, right) {
            (true, _, true, _) => HTTOPLEFT,
            (true, _, _, true) => HTTOPRIGHT,
            (_, true, true, _) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, ..) => HTTOP,
            (_, true, ..) => HTBOTTOM,
            (.., true, _) => HTLEFT,
            (.., true) => HTRIGHT,
            _ => return None,
        })
    }

    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        // SAFETY: `data` is the `Rc` `apply` leaked for this subclass, alive
        // until `WM_NCDESTROY` below takes it back.
        let caption = unsafe { &*(data as *const Caption) };
        let point = || {
            (
                (lparam.0 & 0xffff) as i16 as i32,
                ((lparam.0 >> 16) & 0xffff) as i16 as i32,
            )
        };
        match message {
            // High contrast on or off, the accent, animations, the colours.
            WM_SETTINGCHANGE | WM_SYSCOLORCHANGE => {
                caption.settings_changed.set(true);
                if let Some(wake) = &*caption.wake.borrow() {
                    wake();
                }
            }
            WM_NCHITTEST if caption.frameless => {
                let (x, y) = point();
                if let Some(hit) = hit_test(hwnd, caption, x, y) {
                    return LRESULT(hit as isize);
                }
            }
            // A standard frame's own buttons are the frame's.
            _ if !caption.buttons => {}
            WM_NCMOUSEMOVE => {
                if !caption.tracking.replace(true) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: tracks our own window.
                    let _ = unsafe { TrackMouseEvent(&mut track) };
                }
                let hovered = CaptionButton::from_hit(wparam.0);
                let pressed = caption.pressed.get().filter(|_| hovered.is_some());
                caption.set(hovered, pressed);
            }
            WM_NCMOUSELEAVE => {
                caption.tracking.set(false);
                caption.set(None, None);
            }
            WM_MOUSEMOVE => caption.set(None, None),
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                if let Some(button) = CaptionButton::from_hit(wparam.0) {
                    caption.set(Some(button), Some(button));
                    return LRESULT(0);
                }
            }
            WM_NCLBUTTONUP => {
                if let Some(button) = CaptionButton::from_hit(wparam.0) {
                    let pressed = caption.pressed.get() == Some(button);
                    caption.set(Some(button), None);
                    if pressed {
                        let command = match button {
                            CaptionButton::Minimize => SC_MINIMIZE,
                            // SAFETY: reads our own window's state.
                            CaptionButton::Maximize if unsafe { IsZoomed(hwnd) }.as_bool() => {
                                SC_RESTORE
                            }
                            CaptionButton::Maximize => SC_MAXIMIZE,
                            CaptionButton::Close => SC_CLOSE,
                        };
                        // The system command animates and asks winit, as
                        // the frame's own buttons do.
                        // SAFETY: posts to our own window.
                        let _ = unsafe {
                            PostMessageW(
                                Some(hwnd),
                                WM_SYSCOMMAND,
                                WPARAM(command as usize),
                                LPARAM(0),
                            )
                        };
                    }
                    return LRESULT(0);
                }
            }
            WM_NCDESTROY => {
                // SAFETY: removes this subclass and takes back its reference.
                unsafe {
                    let _ = RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                    drop(Rc::from_raw(data as *const Caption));
                }
                CAPTIONS.with(|captions| {
                    captions
                        .borrow_mut()
                        .retain(|(window, _)| *window != hwnd.0 as usize)
                });
            }
            _ => {}
        }
        // SAFETY: forwards to the next procedure in the chain (winit's).
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    /// A window's side of its procedure: the caption buttons of a window
    /// with `Titlebar::Inset`, drawn over its content (hovered, pressed, the
    /// window's state and appearance follow the window procedure, a frame
    /// after they change), and the system's settings read again when they
    /// change.
    pub(crate) struct CaptionHost {
        caption: Rc<Caption>,
        window: std::sync::Weak<Window>,
        hovered: zgui::reactive::Signal<Option<CaptionButton>>,
        pressed: zgui::reactive::Signal<Option<CaptionButton>>,
        maximized: zgui::reactive::Signal<bool>,
        active: zgui::reactive::Signal<bool>,
        dark: zgui::reactive::Signal<bool>,
        view: Option<zgui::compose::ViewHandle>,
    }
    impl CaptionHost {
        /// For a window `apply` subclassed; `wake` asks for a frame.
        pub(crate) fn new(
            window: &Arc<Window>,
            ui: &Ui,
            wake: impl Fn() + 'static,
        ) -> Option<Self> {
            let hwnd = hwnd(window)?.0 as usize;
            let caption = CAPTIONS.with(|captions| {
                captions
                    .borrow()
                    .iter()
                    .find(|(window, _)| *window == hwnd)
                    .map(|(_, caption)| caption.clone())
            })?;
            *caption.wake.borrow_mut() = Some(Box::new(wake));
            Some(Self {
                caption,
                window: Arc::downgrade(window),
                hovered: ui.signal(None),
                pressed: ui.signal(None),
                maximized: ui.signal(false),
                active: ui.signal(true),
                dark: ui.signal(true),
                view: None,
            })
        }
        /// Before a frame: bring the buttons up to date, and back if the
        /// content was rendered again over them.
        pub(crate) fn sync(&mut self, ui: &mut Ui) {
            if self.caption.settings_changed.take() {
                crate::system_colors::sync(ui);
                crate::scrollers::sync(ui);
            }
            if !self.caption.buttons {
                return;
            }
            if let Some(window) = self.window.upgrade() {
                self.maximized.set(window.is_maximized());
                self.active.set(window.has_focus());
                self.dark
                    .set(window.theme() != Some(winit::window::Theme::Light));
            }
            self.hovered.set(self.caption.hovered.get());
            self.pressed.set(self.caption.pressed.get());
            if !self.view.as_ref().is_some_and(|view| view.is_mounted()) {
                self.view = Some(ui.mount(self.buttons()));
            }
        }
        fn buttons(&self) -> View {
            // Windows 11's icon font; Windows 10's has the same glyphs.
            let font = if std::path::Path::new(r"C:\Windows\Fonts\SegoeIcons.ttf").exists() {
                "Segoe Fluent Icons"
            } else {
                "Segoe MDL2 Assets"
            };
            let button = |kind: CaptionButton| {
                let (hovered, pressed, dark) = (
                    self.hovered.clone(),
                    self.pressed.clone(),
                    self.dark.clone(),
                );
                let (glyph_hovered, glyph_dark, active, maximized) = (
                    self.hovered.clone(),
                    self.dark.clone(),
                    self.active.clone(),
                    self.maximized.clone(),
                );
                div()
                    .size(BUTTON_WIDTH as f32, BUTTON_HEIGHT as f32)
                    .items_center()
                    .justify_center()
                    .reactive_style(move || {
                        let hover = hovered.get() == Some(kind);
                        let down = hover && pressed.get() == Some(kind);
                        // WinUI's caption button brushes.
                        let fill = match (kind, hover, down, dark.get()) {
                            (_, false, ..) => Color(0, 0, 0, 0),
                            (CaptionButton::Close, _, false, _) => Color(196, 43, 28, 255),
                            (CaptionButton::Close, _, true, _) => Color(196, 43, 28, 230),
                            (_, _, false, true) => Color(255, 255, 255, 15),
                            (_, _, true, true) => Color(255, 255, 255, 10),
                            (_, _, false, false) => Color(0, 0, 0, 9),
                            (_, _, true, false) => Color(0, 0, 0, 6),
                        };
                        Styles::new().bg(fill)
                    })
                    .child(
                        text_signal(move || {
                            match kind {
                                CaptionButton::Minimize => "\u{e921}",
                                CaptionButton::Maximize if maximized.get() => "\u{e923}",
                                CaptionButton::Maximize => "\u{e922}",
                                CaptionButton::Close => "\u{e8bb}",
                            }
                            .to_owned()
                        })
                        .font_family(font)
                        .text_size(10.)
                        .reactive_style(move || {
                            let red =
                                kind == CaptionButton::Close && glyph_hovered.get() == Some(kind);
                            let color = match (red, glyph_dark.get(), active.get()) {
                                (true, ..) => Color(255, 255, 255, 255),
                                (_, true, true) => Color(255, 255, 255, 255),
                                (_, true, false) => Color(255, 255, 255, 93),
                                (_, false, true) => Color(0, 0, 0, 228),
                                (_, false, false) => Color(0, 0, 0, 92),
                            };
                            Styles::new().text_color(color)
                        }),
                    )
            };
            row()
                .absolute()
                .top(0.)
                .right(0.)
                .pointer_events(false)
                .child(button(CaptionButton::Minimize))
                .child(button(CaptionButton::Maximize))
                .child(button(CaptionButton::Close))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_frameless_windows_edges_resize_and_its_corner_buttons_answer() {
            let size = (800, 600);
            let at = |x, y| hit((x, y), size, 1., false, true);
            assert_eq!(at(400, 300), None, "content");
            assert_eq!(at(0, 300), Some(HTLEFT));
            assert_eq!(at(799, 300), Some(HTRIGHT));
            assert_eq!(at(400, 599), Some(HTBOTTOM));
            assert_eq!(at(0, 0), Some(HTTOPLEFT));
            assert_eq!(at(799, 599), Some(HTBOTTOMRIGHT));
            // The buttons, 46 wide from the right, 32 tall; the outermost
            // pixel stays the resize band.
            assert_eq!(at(790, 10), Some(HTCLOSE));
            assert_eq!(at(799, 10), Some(HTRIGHT));
            assert_eq!(at(799, 2), Some(HTTOPRIGHT));
            assert_eq!(at(740, 10), Some(HTMAXBUTTON));
            assert_eq!(at(700, 10), Some(HTMINBUTTON));
            assert_eq!(at(640, 10), None, "left of the buttons");
            assert_eq!(at(790, 40), None, "below the buttons");
            // Without buttons (`Titlebar::Hidden`) the corner resizes.
            assert_eq!(hit((790, 2), size, 1., false, false), Some(HTTOP));
        }

        #[test]
        fn bands_and_buttons_scale_and_a_maximized_window_has_no_bands() {
            let size = (1600, 1200);
            // At 200% the band is 10 pixels and the buttons 92 by 64.
            assert_eq!(hit((9, 600), size, 2., false, false), Some(HTLEFT));
            assert_eq!(hit((10, 600), size, 2., false, false), None);
            assert_eq!(hit((1599 - 91, 60), size, 2., false, true), Some(HTCLOSE));
            assert_eq!(
                hit((1599 - 92, 60), size, 2., false, true),
                Some(HTMAXBUTTON)
            );
            // Maximized, the edges are the screen's: nothing resizes, and the
            // buttons reach the very corner.
            assert_eq!(hit((0, 600), size, 1., true, true), None);
            assert_eq!(hit((1599, 0), size, 1., true, true), Some(HTCLOSE));
        }

        #[test]
        fn a_window_larger_than_the_work_area_shrinks_and_moves_inside_it() {
            let work = (0, 0, 1920, 1032);
            // 1770 by 1140 at (208, 208): too tall, and off the right.
            assert_eq!(
                fit((208, 208, 1770, 1140), work, (0, 0)),
                (150, 0, 1770, 1032)
            );
            // A window that fits stays as it is.
            assert_eq!(
                fit((100, 100, 800, 600), work, (0, 0)),
                (100, 100, 800, 600)
            );
            // Never below the app's minimum, even past the work area.
            assert_eq!(
                fit((0, 0, 3000, 2000), work, (2000, 1200)),
                (0, 0, 2000, 1200)
            );
            // A second monitor's work area, left of the primary.
            assert_eq!(
                fit((-1700, 50, 1600, 900), (-1600, 0, 1600, 860), (0, 0)),
                (-1600, 0, 1600, 860)
            );
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::{ButtonLayout, CaptionHost, resize_cursor};

#[cfg(target_os = "linux")]
mod linux {
    //! Linux title bars, drawn by the app as GTK's header bars are (client
    //! side). `Titlebar::Hidden` and `Titlebar::Inset` are undecorated
    //! windows that zgui resizes from bands along their edges (winit's
    //! `drag_resize_window`: the window manager or compositor moves the
    //! edge). `Titlebar::Inset` adds the window buttons over the content's
    //! top corners, in the order and on the side the desktop asks (GNOME's
    //! `button-layout`, through the settings portal, as GTK 4 reads it),
    //! looking as libadwaita's: 24-point circles centred in the 40-point
    //! strip, a faint fill that deepens under the pointer.
    use std::sync::{Arc, Weak};
    use winit::window::{CursorIcon, ResizeDirection, Window};
    use zgui::{
        compose::prelude::*,
        reactive::Signal,
        style::Styled,
        theme::{Field, tok},
        widgets::Ui,
    };

    /// Resize bands inside an undecorated window's edges, in logical pixels:
    /// it has no frame outside them to take the pointer.
    const RESIZE_BAND: f32 = 5.;
    /// Along an edge, how far from a corner the corner's diagonal resize
    /// reaches, as GTK's: a corner is easier to catch than a 5-point square.
    const CORNER: f32 = 16.;
    /// libadwaita's window buttons: circles of this size, this far apart and
    /// from the window's edge, centred in `Titlebar::Inset`'s strip.
    const BUTTON: f32 = 24.;
    const GAP: f32 = 8.;
    const MARGIN: f32 = 8.;
    const STRIP: f32 = 40.;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum WindowButton {
        Minimize,
        Maximize,
        Close,
    }

    /// Which window buttons go on which side, in order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct ButtonLayout {
        pub left: [Option<WindowButton>; 3],
        pub right: [Option<WindowButton>; 3],
    }

    impl Default for ButtonLayout {
        /// Minimize, maximize and close on the right: KDE's, Ubuntu's and
        /// most window managers' (GNOME's own asks for close alone).
        fn default() -> Self {
            Self {
                left: [None; 3],
                right: [
                    Some(WindowButton::Minimize),
                    Some(WindowButton::Maximize),
                    Some(WindowButton::Close),
                ],
            }
        }
    }

    impl ButtonLayout {
        /// GNOME's `button-layout` (and GTK's `gtk-decoration-layout`):
        /// the buttons left of the colon, then right of it, by name
        /// (`"appmenu:minimize,maximize,close"`); the names zgui draws no
        /// button for (`appmenu`, `icon`, `spacer`) are left out.
        pub fn parse(layout: &str) -> Self {
            let side = |names: &str| {
                let mut buttons = [None; 3];
                let mut found = names
                    .split(',')
                    .filter_map(|name| match name.trim() {
                        "minimize" => Some(WindowButton::Minimize),
                        "maximize" => Some(WindowButton::Maximize),
                        "close" => Some(WindowButton::Close),
                        _ => None,
                    })
                    .fuse();
                for slot in &mut buttons {
                    *slot = found.next();
                }
                buttons
            };
            let (left, right) = layout.split_once(':').unwrap_or(("", layout));
            Self {
                left: side(left),
                right: side(right),
            }
        }
    }

    /// The edge or corner at (`x`, `y`) of a window `width` by `height`
    /// points, if the pointer there resizes it.
    pub(super) fn edge(x: f32, y: f32, width: f32, height: f32) -> Option<ResizeDirection> {
        let near = |at: f32, size: f32, reach: f32| (at < reach, at >= size - reach);
        let (left, right) = near(x, width, RESIZE_BAND);
        let (top, bottom) = near(y, height, RESIZE_BAND);
        let (corner_x, corner_y) = (near(x, width, CORNER), near(y, height, CORNER));
        use ResizeDirection::*;
        Some(match () {
            _ if (top || left) && corner_x.0 && corner_y.0 => NorthWest,
            _ if (top || right) && corner_x.1 && corner_y.0 => NorthEast,
            _ if (bottom || left) && corner_x.0 && corner_y.1 => SouthWest,
            _ if (bottom || right) && corner_x.1 && corner_y.1 => SouthEast,
            _ if top => North,
            _ if bottom => South,
            _ if left => West,
            _ if right => East,
            _ => return None,
        })
    }

    /// The cursor a resize toward `direction` shows.
    pub(crate) fn resize_cursor(direction: ResizeDirection) -> CursorIcon {
        use ResizeDirection::*;
        match direction {
            North => CursorIcon::NResize,
            South => CursorIcon::SResize,
            East => CursorIcon::EResize,
            West => CursorIcon::WResize,
            NorthEast => CursorIcon::NeResize,
            NorthWest => CursorIcon::NwResize,
            SouthEast => CursorIcon::SeResize,
            SouthWest => CursorIcon::SwResize,
        }
    }

    /// An undecorated window's side of its title bar: the window buttons of
    /// `Titlebar::Inset` over its content (the window's state and the
    /// desktop's layout follow, a frame after they change), and the resize
    /// bands along its edges.
    pub(crate) struct CaptionHost {
        window: Weak<Window>,
        close: crate::WindowHandle,
        buttons: bool,
        layout: ButtonLayout,
        maximized: Signal<bool>,
        active: Signal<bool>,
        view: Option<zgui::compose::ViewHandle>,
    }

    impl CaptionHost {
        /// For a window `titlebar` made undecorated; `None` for a standard
        /// one, which the platform decorates.
        pub(crate) fn new(
            window: &Arc<Window>,
            ui: &Ui,
            titlebar: super::Titlebar,
            close: crate::WindowHandle,
        ) -> Option<Self> {
            if titlebar == super::Titlebar::Standard {
                return None;
            }
            Some(Self {
                window: Arc::downgrade(window),
                close,
                buttons: titlebar == super::Titlebar::Inset,
                layout: ButtonLayout::default(),
                maximized: ui.signal(false),
                active: ui.signal(true),
                view: None,
            })
        }

        /// Before a frame and when the window's state changes: bring the
        /// buttons up to date, and back if the content was rendered again
        /// over them.
        pub(crate) fn sync(&mut self, ui: &mut Ui) {
            if !self.buttons {
                return;
            }
            if let Some(window) = self.window.upgrade() {
                self.maximized.set(window.is_maximized());
                self.active.set(window.has_focus());
            }
            let layout = crate::linux_appearance::current()
                .buttons
                .unwrap_or_default();
            if layout != self.layout {
                self.layout = layout;
                if let Some(view) = self.view.take() {
                    view.unmount();
                }
            }
            if !self.view.as_ref().is_some_and(|view| view.is_mounted()) {
                self.view = Some(ui.mount(self.buttons()));
            }
        }

        /// The resize the pointer at (`x`, `y`) would start, if any: none
        /// while the window is maximized, fullscreen or of a fixed size.
        pub(crate) fn edge(&self, x: f32, y: f32, scale: f64) -> Option<ResizeDirection> {
            let window = self.window.upgrade()?;
            if !window.is_resizable() || window.is_maximized() || window.fullscreen().is_some() {
                return None;
            }
            let size = window.inner_size().to_logical::<f32>(scale);
            edge(x, y, size.width, size.height)
        }

        fn buttons(&self) -> View {
            let side = |buttons: [Option<WindowButton>; 3], right: bool| {
                let strip = row()
                    .absolute()
                    .top((STRIP - BUTTON) / 2.)
                    .gap(GAP)
                    .children(buttons.into_iter().flatten().map(|kind| self.button(kind)));
                if right {
                    strip.right(MARGIN)
                } else {
                    strip.left(MARGIN)
                }
            };
            div()
                .absolute()
                .top(0.)
                .left(0.)
                .w_full()
                .h(STRIP)
                .pointer_events(false)
                .child(side(self.layout.left, false))
                .child(side(self.layout.right, true))
        }

        fn button(&self, kind: WindowButton) -> View {
            let window = self.window.clone();
            let close = self.close.clone();
            let maximized = self.maximized.clone();
            // libadwaita's: the text colour at 10% over what is behind,
            // 15% under the pointer, 30% pressed (mixed into the window's
            // background, where these sit).
            let fill = |t: f32| {
                move |s: Styles| {
                    s.token_mix(Field::Background, tok::BACKGROUND, tok::TEXT_PRIMARY, t)
                }
            };
            button()
                .size(BUTTON, BUTTON)
                .p(0.)
                .border(0.)
                .rounded(BUTTON / 2.)
                .items_center()
                .justify_center()
                .shadow_none()
                .focusable(false)
                .pointer_events(true)
                .token_mix(Field::Background, tok::BACKGROUND, tok::TEXT_PRIMARY, 0.1)
                .hover(fill(0.15))
                .active(fill(0.3))
                .accessible_label(match kind {
                    WindowButton::Minimize => "Minimize",
                    WindowButton::Maximize => "Maximize",
                    WindowButton::Close => "Close",
                })
                .on_click(move || {
                    let Some(window) = window.upgrade() else {
                        return;
                    };
                    match kind {
                        WindowButton::Minimize => window.set_minimized(true),
                        WindowButton::Maximize => window.set_maximized(!window.is_maximized()),
                        // As the window manager's close: the app may refuse.
                        WindowButton::Close => close.request_close(),
                    }
                })
                .child(self.glyph(kind, maximized))
        }

        /// The button's symbol, drawn in the text colour (dimmer while the
        /// window is in the back), as Adwaita's symbolic icons.
        fn glyph(&self, kind: WindowButton, maximized: Signal<bool>) -> View {
            let active = self.active.clone();
            let ink = move |s: Styles| {
                s.bg(if active.get() {
                    tok::TEXT_PRIMARY
                } else {
                    tok::TEXT_TERTIARY
                })
            };
            let bar = |w: f32, h: f32| div().size(w, h).rounded(h / 2.);
            match kind {
                WindowButton::Minimize => div()
                    .size(10., 10.)
                    .items_center()
                    .justify_end()
                    .flex_col()
                    .child(bar(8., 2.).reactive_style(move || ink(Styles::new()))),
                WindowButton::Maximize => {
                    let active = self.active.clone();
                    div().reactive_style(move || {
                        let size = if maximized.get() { 7. } else { 9. };
                        Styles::new()
                            .size(size, size)
                            .rounded(1.5)
                            .border(1.5)
                            .border_color(if active.get() {
                                tok::TEXT_PRIMARY
                            } else {
                                tok::TEXT_TERTIARY
                            })
                    })
                }
                WindowButton::Close => {
                    let line = |turn: f32| {
                        let ink = ink.clone();
                        bar(11., 2.)
                            .absolute()
                            .left(0.)
                            .top(4.5)
                            .rotate(turn)
                            .reactive_style(move || ink(Styles::new()))
                    };
                    div()
                        .size(11., 11.)
                        .child(line(std::f32::consts::FRAC_PI_4))
                        .child(line(-std::f32::consts::FRAC_PI_4))
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{ButtonLayout, WindowButton::*, edge};
        use winit::window::ResizeDirection::*;

        #[test]
        fn button_layouts_read_as_gnome_writes_them() {
            let gnome = ButtonLayout::parse("appmenu:close");
            assert_eq!(gnome.left, [None; 3]);
            assert_eq!(gnome.right, [Some(Close), None, None]);
            let left = ButtonLayout::parse("close,minimize,maximize:");
            assert_eq!(left.left, [Some(Close), Some(Minimize), Some(Maximize)]);
            assert_eq!(left.right, [None; 3]);
            assert_eq!(
                ButtonLayout::parse("icon:minimize,spacer,maximize,close"),
                ButtonLayout::default()
            );
            assert_eq!(
                ButtonLayout::parse("minimize,maximize,close").right,
                ButtonLayout::default().right
            );
        }

        #[test]
        fn edges_resize_and_corners_reach_further() {
            let at = |x, y| edge(x, y, 800., 600.);
            assert_eq!(at(400., 300.), None);
            assert_eq!(at(400., 2.), Some(North));
            assert_eq!(at(400., 598.), Some(South));
            assert_eq!(at(1., 300.), Some(West));
            assert_eq!(at(799., 300.), Some(East));
            assert_eq!(at(2., 2.), Some(NorthWest));
            // Along an edge near a corner: the corner's diagonal.
            assert_eq!(at(12., 598.), Some(SouthWest));
            assert_eq!(at(797., 10.), Some(NorthEast));
            assert_eq!(at(790., 590.), None, "inside, near a corner");
        }
    }
}

/// A double click in a window drag region: what a double click on the title
/// bar does. macOS follows the system setting (zoom, minimize or nothing);
/// elsewhere the window toggles maximized.
pub(crate) fn double_click(window: &winit::window::Window) {
    #[cfg(target_os = "macos")]
    mac::double_click(window);
    #[cfg(not(target_os = "macos"))]
    window.set_maximized(!window.is_maximized());
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{Glass, Titlebar};
    use objc2::{
        DefinedClass, MainThreadOnly, define_class, msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject},
        sel,
    };
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowStyleMask,
        NSWindowTitleVisibility,
    };
    use objc2_foundation::NSString;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn ns_window(window: &winit::window::Window) -> Option<Retained<NSWindow>> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's view, alive while its window is; the main thread.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        view.window()
    }

    pub(super) fn apply(
        window: &winit::window::Window,
        titlebar: Titlebar,
        glass: Option<Glass>,
    ) -> f64 {
        let Some(window) = ns_window(window) else {
            return 0.;
        };
        match titlebar {
            Titlebar::Standard => {}
            Titlebar::Inset => {
                content_under_titlebar(&window);
                inset_buttons(&window);
            }
            Titlebar::Hidden => {
                content_under_titlebar(&window);
                // NSWindowCloseButton, MiniaturizeButton, ZoomButton.
                for button in [0_isize, 1, 2] {
                    unsafe {
                        let button: *mut AnyObject =
                            msg_send![&*window, standardWindowButton: button];
                        if let Some(button) = button.as_ref() {
                            let _: () = msg_send![button, setHidden: true];
                        }
                    }
                }
            }
        }
        if let Some(glass) = glass {
            frost(&window, glass);
        }
        radius(&window)
    }

    /// Content under a transparent, untitled title bar with no separator.
    fn content_under_titlebar(window: &NSWindow) {
        window.setStyleMask(window.styleMask() | NSWindowStyleMask::FullSizeContentView);
        window.setTitlebarAppearsTransparent(true);
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        // NSTitlebarSeparatorStyleNone
        unsafe {
            let _: () = msg_send![window, setTitlebarSeparatorStyle: 1_isize];
        }
    }

    /// An empty compact unified toolbar: it centres the traffic lights in a
    /// 40-point strip.
    fn inset_buttons(window: &NSWindow) {
        let Some(class) = AnyClass::get(c"NSToolbar") else {
            return;
        };
        unsafe {
            let id = NSString::from_str("zgui.titlebar.inset");
            let toolbar: Retained<AnyObject> =
                msg_send![msg_send![class, alloc], initWithIdentifier: &*id];
            let _: () = msg_send![&*toolbar, setShowsBaselineSeparator: false];
            let _: () = msg_send![window, setToolbar: &*toolbar];
            // NSWindowToolbarStyleUnifiedCompact
            let _: () = msg_send![window, setToolbarStyle: 4_isize];
        }
        toolbar_passes_through();
    }

    /// The empty toolbar is only there to centre the traffic lights: presses
    /// in it go to the content under it. Otherwise it keeps a press's drag
    /// and release events (zgui would start a drag and never see it end).
    /// The traffic lights are in the titlebar view, not the toolbar's, and
    /// stay clickable.
    fn toolbar_passes_through() {
        use objc2::runtime::Sel;
        use objc2_foundation::NSPoint;
        extern "C-unwind" fn nothing(
            _this: *mut AnyObject,
            _cmd: Sel,
            _point: NSPoint,
        ) -> *mut AnyObject {
            std::ptr::null_mut()
        }
        let Some(class) = AnyClass::get(c"NSToolbarView") else {
            return;
        };
        unsafe {
            let imp: objc2::runtime::Imp = std::mem::transmute(
                nothing as extern "C-unwind" fn(*mut AnyObject, Sel, NSPoint) -> *mut AnyObject,
            );
            objc2::ffi::class_replaceMethod(
                class as *const AnyClass as *mut AnyClass,
                sel!(hitTest:),
                imp,
                c"@@:{CGPoint=dd}".as_ptr(),
            );
        }
    }

    /// The window's corner radius: it varies by macOS version and toolbar
    /// style.
    pub(super) fn radius(window: &NSWindow) -> f64 {
        if window.class().responds_to(sel!(_cornerRadius)) {
            unsafe { msg_send![window, _cornerRadius] }
        } else {
            10.
        }
    }

    pub(crate) struct GlassState {
        plain: bool,
    }
    define_class!(
        /// A behind-window effect view that, plain, strips the layers AppKit
        /// builds for it each time it builds them (gpui's `BlurredView`).
        #[unsafe(super(NSVisualEffectView, NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "ZguiWindowGlass"]
        #[ivars = GlassState]
        pub(crate) struct GlassView;
        impl GlassView {
            #[unsafe(method(updateLayer))]
            fn update_layer(&self) {
                let _: () = unsafe { msg_send![super(self), updateLayer] };
                if self.ivars().plain
                    && let Some(layer) = self.layer()
                {
                    unsafe { strip_layer(&layer) };
                }
            }
        }
    );

    /// Put a behind-window effect view under the (transparent) zgui view.
    /// Unlike the private CGS window blur, it can be clipped to the window's
    /// rounded shape, so neither blur nor shadow is left square.
    fn frost(window: &NSWindow, glass: Glass) {
        let Some(content) = window.contentView() else {
            return;
        };
        let Some(frame) = (unsafe { content.superview() }) else {
            return;
        };
        let mtm = window.mtm();
        let effect: Retained<GlassView> = unsafe {
            let object = mtm
                .alloc::<GlassView>()
                .set_ivars(GlassState { plain: glass.plain });
            msg_send![super(object), initWithFrame: frame.bounds()]
        };
        effect.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // Behind-window blending, always active.
        effect.setMaterial(NSVisualEffectMaterial(glass.material.appkit_value()));
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::Active);
        effect.setWantsLayer(true);
        if let Some(layer) = effect.layer() {
            layer.setCornerRadius(radius(window));
            unsafe {
                let continuous = NSString::from_str("continuous");
                let _: () = msg_send![&*layer, setCornerCurve: &*continuous];
            }
            layer.setMasksToBounds(true);
            if glass.plain {
                unsafe { strip_layer(&layer) };
            }
        }
        frame.addSubview_positioned_relativeTo(
            &effect,
            objc2_app_kit::NSWindowOrderingMode::Below,
            Some(&content),
        );
        window.invalidateShadow();
    }

    /// No desktop tint (`CAChameleonLayer` hidden), a 60-point gaussian and
    /// no saturation boost, through the effect's layer tree.
    unsafe fn strip_layer(layer: &AnyObject) {
        unsafe {
            let none: *const AnyObject = std::ptr::null();
            let _: () = msg_send![layer, setBackgroundColor: none];
            let class_name: Retained<NSString> = msg_send![layer, className];
            if class_name.to_string() == "CAChameleonLayer" {
                let _: () = msg_send![layer, setHidden: true];
                return;
            }
            let filters: *mut AnyObject = msg_send![layer, filters];
            if let Some(filters) = filters.as_ref() {
                let count: usize = msg_send![filters, count];
                let mut kept: Vec<*mut AnyObject> = Vec::with_capacity(count);
                for i in 0..count {
                    let filter: *mut AnyObject = msg_send![filters, objectAtIndex: i];
                    let Some(filter_ref) = filter.as_ref() else {
                        continue;
                    };
                    let description: Retained<NSString> = msg_send![filter_ref, description];
                    let description = description.to_string();
                    if description.contains("Saturat") {
                        continue;
                    }
                    if description.contains("Blur") {
                        let radius = objc2_foundation::NSNumber::new_f64(60.);
                        let key = NSString::from_str("inputRadius");
                        let _: () = msg_send![filter_ref, setValue: &*radius, forKey: &*key];
                    }
                    kept.push(filter);
                }
                if let Some(array_class) = AnyClass::get(c"NSArray") {
                    let array: Retained<AnyObject> = msg_send![
                        array_class,
                        arrayWithObjects: kept.as_ptr(),
                        count: kept.len()
                    ];
                    let _: () = msg_send![layer, setFilters: &*array];
                }
            }
            let sublayers: *mut AnyObject = msg_send![layer, sublayers];
            if let Some(sublayers) = sublayers.as_ref() {
                let count: usize = msg_send![sublayers, count];
                for i in 0..count {
                    let sublayer: *mut AnyObject = msg_send![sublayers, objectAtIndex: i];
                    if let Some(sublayer) = sublayer.as_ref() {
                        strip_layer(sublayer);
                    }
                }
            }
        }
    }

    /// The system's title bar double click (Desktop & Dock settings).
    pub(super) fn double_click(window: &winit::window::Window) {
        let Some(window) = ns_window(window) else {
            return;
        };
        let action = unsafe {
            let defaults: *mut AnyObject =
                msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
            let Some(defaults) = defaults.as_ref() else {
                return;
            };
            let key = NSString::from_str("AppleActionOnDoubleClick");
            let action: *mut NSString = msg_send![defaults, stringForKey: &*key];
            match action.as_ref() {
                Some(action) => action.to_string(),
                None => {
                    // Before the setting had choices: minimize or zoom.
                    let key = NSString::from_str("AppleMiniaturizeOnDoubleClick");
                    let minimize: bool = msg_send![defaults, boolForKey: &*key];
                    if minimize { "Minimize" } else { "Maximize" }.to_owned()
                }
            }
        };
        match action.as_str() {
            "Minimize" => window.miniaturize(None),
            "None" => {}
            // "Maximize" (zoom), "Fill" and anything newer: zoom.
            _ => window.zoom(None),
        }
    }
}
