//! Native geometry snapshots and explicit platform capability reporting.
use winit::{
    event_loop::ActiveEventLoop,
    monitor::MonitorHandle,
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    window::{Window, WindowAttributes},
};
use zgx::{reactive::Signal, widgets::Ui};
/// Physical desktop coordinates and physical client-area size. Position is None
/// when the compositor does not expose global window placement (notably Wayland).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowBounds {
    /// The window's outer top-left corner in physical desktop coordinates.
    pub position: Option<(i32, i32)>,
    /// The client area's physical width and height.
    pub size: (u32, u32),
}
impl WindowBounds {
    /// A `width` by `height` window, placed by the platform.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            position: None,
            size: (width, height),
        }
    }
    /// Placed with its top-left corner at `x`, `y`.
    pub fn at(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }
    pub(crate) fn read(window: &Window) -> Self {
        let size = window.inner_size();
        Self {
            position: window.outer_position().ok().map(|p| (p.x, p.y)),
            size: (size.width, size.height),
        }
    }
}
/// Which display a window opens on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplaySelector {
    /// The primary display.
    Primary,
    /// The display at this index in [`WindowInfo::displays`].
    Index(usize),
    /// The display with this name.
    Name(String),
}
/// A display's geometry and scale, as the platform reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    /// Its index in [`WindowInfo::displays`].
    pub index: usize,
    /// Its name, when the platform gives one.
    pub name: Option<String>,
    /// Its top-left corner in physical desktop coordinates.
    pub position: (i32, i32),
    /// Its physical width and height.
    pub size: (u32, u32),
    /// Physical pixels per logical point.
    pub scale_factor: f64,
    /// Refresh rate in hertz, when the platform reports it. Variable-rate
    /// displays report their maximum.
    pub refresh_rate_hz: Option<f64>,
    /// Whether it is the primary display.
    pub primary: bool,
}
/// What this platform lets a window do, so an app can offer only what works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowCapabilities {
    /// The window's position on the desktop can be read (not on Wayland).
    pub global_position: bool,
    /// The window can be placed at an explicit position.
    pub placement: bool,
    /// [`crate::WindowHandle::drag_window`] can start a native move.
    pub interactive_drag: bool,
    /// The window can enter native fullscreen.
    pub native_fullscreen: bool,
}
/// A window's geometry and state, kept current as signals: reading one in
/// a view updates it when the window moves, resizes or changes display.
#[derive(Clone)]
pub struct WindowInfo {
    /// Whether the native window exists; the rest is meaningful once it does.
    pub ready: Signal<bool>,
    /// Its bounds.
    pub bounds: Signal<WindowBounds>,
    /// Whether it is fullscreen.
    pub fullscreen: Signal<bool>,
    /// The displays connected.
    pub displays: Signal<Vec<DisplayInfo>>,
    /// The index in `displays` of the one it is on, when known.
    pub current_display: Signal<Option<usize>>,
    /// What the platform lets it do.
    pub capabilities: Signal<WindowCapabilities>,
    /// The most recent rejected explicit placement/drag request, if any.
    pub last_request_error: Signal<Option<String>>,
    /// The window's corner radius in points, once it is made (macOS; 0
    /// elsewhere): content inset from the window's edge keeps its corners
    /// concentric with it.
    pub corner_radius: Signal<f64>,
    /// Where an app may draw in the title bar, in points: the strip a
    /// `Titlebar::Inset` or `Hidden` window's content runs under, less the
    /// window's own buttons (as the web's Window Controls Overlay gives
    /// it). `None` for a standard title bar, in fullscreen, and where zgx
    /// does not know it yet. It follows resizes, scale and fullscreen.
    pub titlebar_area: Signal<Option<zgx::scene::Rect>>,
    /// The window's own buttons in that strip, in points (Windows' caption
    /// buttons at the top right, macOS's traffic lights at the top left),
    /// or `None` where there are none.
    pub controls: Signal<Option<zgx::scene::Rect>>,
}
impl WindowInfo {
    pub(crate) fn new(ui: &Ui, width: f64, height: f64) -> Self {
        Self {
            ready: ui.signal(false),
            bounds: ui.signal(WindowBounds::new(
                width.max(1.) as u32,
                height.max(1.) as u32,
            )),
            fullscreen: ui.signal(false),
            displays: ui.signal(Vec::new()),
            current_display: ui.signal(None),
            capabilities: ui.signal(WindowCapabilities::default()),
            last_request_error: ui.signal(None),
            corner_radius: ui.signal(0.),
            titlebar_area: ui.signal(None),
            controls: ui.signal(None),
        }
    }
    pub(crate) fn refresh(&self, window: &Window, titlebar: crate::Titlebar) {
        let wayland = window
            .display_handle()
            .is_ok_and(|h| matches!(h.as_raw(), RawDisplayHandle::Wayland(_)));
        let primary = window.primary_monitor();
        let current = window.current_monitor();
        let monitors = window.available_monitors().collect::<Vec<_>>();
        self.current_display.set(
            current
                .as_ref()
                .and_then(|m| monitors.iter().position(|candidate| candidate == m)),
        );
        self.displays.set(
            monitors
                .iter()
                .enumerate()
                .map(|(index, m)| {
                    let p = m.position();
                    let s = m.size();
                    DisplayInfo {
                        index,
                        name: m.name(),
                        position: (p.x, p.y),
                        size: (s.width, s.height),
                        scale_factor: m.scale_factor(),
                        refresh_rate_hz: refresh_rate_hz(m),
                        primary: primary.as_ref() == Some(m),
                    }
                })
                .collect(),
        );
        self.bounds.set(WindowBounds::read(window));
        self.fullscreen.set(window.fullscreen().is_some());
        let (area, controls) = crate::window_chrome::titlebar_area(window, titlebar);
        if self.titlebar_area.with_untracked(|old| *old != area) {
            self.titlebar_area.set(area);
        }
        if self.controls.with_untracked(|old| *old != controls) {
            self.controls.set(controls);
        }
        self.capabilities.set(WindowCapabilities {
            global_position: !wayland,
            placement: !wayland,
            interactive_drag: true,
            native_fullscreen: true,
        });
        self.ready.set(true);
    }
}
pub(crate) fn refresh_rate_hz(monitor: &MonitorHandle) -> Option<f64> {
    monitor
        .refresh_rate_millihertz()
        .map(|millihertz| millihertz as f64 / 1000.)
        .filter(|hz| hz.is_finite() && *hz >= 1.)
}
pub(crate) fn select_monitor(
    event_loop: &ActiveEventLoop,
    selector: &DisplaySelector,
) -> Option<MonitorHandle> {
    match selector {
        DisplaySelector::Primary => event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next()),
        DisplaySelector::Index(index) => event_loop.available_monitors().nth(*index),
        DisplaySelector::Name(name) => event_loop
            .available_monitors()
            .find(|m| m.name().as_ref() == Some(name)),
    }
}
pub(crate) fn attributes(
    options: &crate::WindowOptions,
    event_loop: &ActiveEventLoop,
    #[cfg(target_os = "linux")] application_id: Option<&str>,
    #[cfg(target_os = "linux")] activation: Option<winit::window::ActivationToken>,
) -> Result<WindowAttributes, String> {
    let mut attrs = Window::default_attributes()
        .with_title(&options.title)
        .with_inner_size(winit::dpi::LogicalSize::new(options.width, options.height))
        .with_resizable(options.resizable)
        .with_transparent(options.is_transparent())
        .with_decorations(options.decorations)
        .with_visible(false);
    #[cfg(target_os = "linux")]
    if let Some(token) = activation {
        use winit::platform::startup_notify::WindowAttributesExtStartupNotify;
        attrs = attrs.with_activation_token(token);
    }
    // Wayland's app_id and X11's WM_CLASS: what docks and task switchers
    // match to the app's desktop file (its icon and name). The application
    // id, else the executable's name, as desktop files usually are.
    #[cfg(target_os = "linux")]
    if let Some(id) = application_id.map(str::to_owned).or_else(|| {
        let exe = std::env::current_exe().ok()?;
        Some(exe.file_stem()?.to_str()?.to_owned())
    }) {
        use winit::platform::wayland::WindowAttributesExtWayland;
        attrs = attrs.with_name(&id, &id);
    }
    // Linux: a title bar of the app's own is an undecorated window, as a
    // GTK header bar's (the rest is `window_chrome`).
    #[cfg(target_os = "linux")]
    if options.titlebar != crate::Titlebar::Standard {
        attrs = attrs.with_decorations(false);
    }
    // Windows: a title bar of the app's own is a frameless window keeping
    // the frame's shadow (the rest is `window_chrome`).
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        if options.titlebar != crate::Titlebar::Standard {
            attrs = attrs.with_decorations(false).with_undecorated_shadow(true);
        }
        // Frames show through a DirectComposition visual (`zgx_gpu`), not
        // a surface Windows keeps for the window, which would sit under the
        // visual, opaque, where zgx paints nothing.
        attrs = attrs.with_no_redirection_bitmap(true);
    }
    if let Some((w, h)) = options.min_size {
        if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. {
            return Err("minimum window dimensions must be finite and positive".into());
        }
        attrs = attrs.with_min_inner_size(winit::dpi::LogicalSize::new(w, h));
    }
    if let Some(selector) = &options.display {
        let monitor = select_monitor(event_loop, selector)
            .ok_or_else(|| "requested display is unavailable".to_owned())?;
        attrs = attrs.with_position(monitor.position());
    }
    if let Some(bounds) = options.bounds {
        if bounds.size.0 == 0 || bounds.size.1 == 0 {
            return Err("window bounds must have positive size".into());
        }
        attrs = attrs.with_inner_size(winit::dpi::PhysicalSize::new(bounds.size.0, bounds.size.1));
        if let Some((x, y)) = bounds.position {
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x, y));
        }
    }
    Ok(attrs)
}
