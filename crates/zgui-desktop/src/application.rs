//! Winit desktop host: GPU rendering, native accessibility, clipboard and IME.
use crate::{accessibility::AccessibilityTree, presentation_retry::Presentation};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    error::Error,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key as WKey, KeyCode, NamedKey, PhysicalKey},
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    window::{CursorIcon, Window, WindowId},
};
use zgui::{
    frame::{Frame, FrameClock},
    input::{InputEvent, Key, Modifiers, PointerButton},
    native_view::PointerStep,
    reactive::Signal,
    scene::{NodeId, Rect},
    semantics::Role,
    task::LocalExecutor,
    widgets::Ui,
};
use zgui_gpu::{GpuContext, GpuRenderer};

/// How a window is made: its title, size, chrome and placement.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    /// The title in its title bar, window menu and accessibility tree.
    pub title: String,
    /// Logical client width.
    pub width: f64,
    /// Logical client height.
    pub height: f64,
    /// Whether the window is transparent where zgui paints nothing.
    pub transparent: bool,
    /// Whether the person can resize it.
    pub resizable: bool,
    /// Whether it has the platform's title bar and frame.
    pub decorations: bool,
    /// Whether it opens fullscreen.
    pub fullscreen: bool,
    /// Logical minimum client size.
    pub min_size: Option<(f64, f64)>,
    /// Initial physical bounds. Placement remains compositor policy on Wayland.
    pub bounds: Option<crate::WindowBounds>,
    /// The display it opens on; `None` lets the platform choose.
    pub display: Option<crate::DisplaySelector>,
    /// Upper bound on animation frames per second (see [`zgui::frame`]), for
    /// example to save power. Input-driven redraws are not limited. `None`
    /// follows the display.
    pub max_frame_rate: Option<f64>,
    /// The title bar's style (macOS and Windows; other platforms keep theirs).
    pub titlebar: crate::Titlebar,
    /// A frosted desktop behind the window (macOS; Windows 11 as Acrylic or
    /// Mica): the window is then transparent, whatever `transparent` says.
    pub glass: Option<crate::Glass>,
    /// Open with this point of the content (logical) under the pointer
    /// (macOS, Windows): a tab torn off its window (`View::on_drag_outside`)
    /// opens where it was dropped, the tab under the pointer.
    pub under_pointer: Option<(f64, f64)>,
    /// AppKit's window tabs (macOS): as the user's setting says unless set.
    /// Windows has no window tabs: there it does nothing.
    pub tabbing: crate::Tabbing,
    /// Windows with the same identifier tab together (macOS); by default,
    /// all of the app's. Nothing elsewhere.
    pub tabbing_identifier: Option<String>,
    /// Save the window's frame under this name as it moves and resizes, and
    /// open where it was left (macOS's frame autosave; on Windows its
    /// placement, maximized or not, in the registry, reopened only where a
    /// monitor still shows its title bar; on Linux the same in
    /// `$XDG_STATE_HOME/zgui/<executable>/frames`, its size and maximized
    /// state alone on Wayland).
    pub frame_autosave: Option<String>,
}
impl WindowOptions {
    /// Whether the window is transparent where zgui paints nothing.
    pub fn is_transparent(&self) -> bool {
        self.transparent || self.glass.is_some()
    }
}
impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "zgui".into(),
            width: 960.,
            height: 720.,
            transparent: false,
            resizable: true,
            decorations: true,
            fullscreen: false,
            min_size: None,
            bounds: None,
            display: None,
            max_frame_rate: None,
            titlebar: crate::Titlebar::Standard,
            glass: None,
            under_pointer: None,
            tabbing: crate::Tabbing::Automatic,
            tabbing_identifier: None,
            frame_autosave: None,
        }
    }
}
#[derive(Debug)]
enum Event {
    Quit,
    /// An event, with the token of the launch that sent it (Linux).
    Application(crate::ApplicationEvent, Option<String>),
    MenuAction(u64, crate::MenuAction),
    MenuFallback(u64, crate::MenuAction),
    MenusChanged,
    #[cfg(target_os = "linux")]
    NativeFiles(u64, crate::wayland_data::FileEvent),
    /// The desktop's appearance settings changed (`linux_appearance`).
    #[cfg(target_os = "linux")]
    AppearanceChanged,
    #[cfg(target_os = "macos")]
    NativeMenuAction(crate::MenuAction),
    Wake(u64),
    /// A sleep on another thread is now the earliest timer: see `zgui::timer::host`.
    Timers,
    Redraw(u64),
    /// A native display refresh (macOS and Windows; other platforms use timers).
    #[cfg(any(target_os = "macos", windows))]
    Frame(u64, Frame),
    Open,
    Accessibility(accesskit_winit::Event),
    Close(u64),
    RequestClose(u64),
    WindowCommand(u64, WindowCommand),
    /// Pointer input over one of the window's popup windows.
    Popup(u64, crate::popup_window::PopupInput),
    /// The pointer reached something in the window that opens a popup: have
    /// a popup window ready (`Ui::on_popup_soon`).
    PopupSoon(u64),
    /// The platform's drag, handed it by the window as the drag left it
    /// (`native_drag`).
    NativeDrag(u64, crate::native_drag::NativeDrag),
}
/// Commands are queued onto the UI thread; the operating system may decline requests.
#[derive(Debug)]
enum WindowCommand {
    Bounds(crate::WindowBounds),
    PointerPassthrough(bool),
    Fullscreen(bool),
    Decorations(bool),
    MinSize(Option<(f64, f64)>),
    Drag,
    RefreshInfo,
    Title(String),
    Size(f64, f64),
    Minimized(bool),
    Maximized(bool),
    Visible(bool),
    Focus,
    MaxFrameRate(Option<f64>),
    LoseGpu,
    ContextMenu {
        entries: Vec<crate::AppMenuEntry>,
        x: f64,
        y: f64,
    },
}
#[derive(Debug)]
struct WindowState {
    visible: bool,
    minimized: bool,
    maximized: bool,
    focus_requested: bool,
}
impl Default for WindowState {
    fn default() -> Self {
        Self {
            visible: true,
            minimized: false,
            maximized: false,
            focus_requested: false,
        }
    }
}
impl WindowState {
    fn apply(&mut self, options: &mut WindowOptions, command: &WindowCommand) {
        match command {
            WindowCommand::Bounds(bounds) => options.bounds = Some(*bounds),
            WindowCommand::Fullscreen(value) => options.fullscreen = *value,
            WindowCommand::Decorations(value) => options.decorations = *value,
            WindowCommand::PointerPassthrough(_) => {}
            WindowCommand::MinSize(value) => options.min_size = *value,
            WindowCommand::Drag
            | WindowCommand::RefreshInfo
            | WindowCommand::LoseGpu
            | WindowCommand::ContextMenu { .. } => {}
            WindowCommand::Title(title) => options.title.clone_from(title),
            WindowCommand::Size(width, height) => {
                options.width = *width;
                options.height = *height;
            }
            WindowCommand::Minimized(value) => self.minimized = *value,
            WindowCommand::Maximized(value) => self.maximized = *value,
            WindowCommand::Visible(value) => self.visible = *value,
            WindowCommand::Focus => self.focus_requested = true,
            WindowCommand::MaxFrameRate(hz) => options.max_frame_rate = *hz,
        }
    }
}
fn valid_window_size(width: f64, height: f64) -> bool {
    [width, height]
        .into_iter()
        .all(|value| value.is_finite() && value > 0. && value <= u32::MAX as f64)
}
impl From<accesskit_winit::Event> for Event {
    fn from(e: accesskit_winit::Event) -> Self {
        Self::Accessibility(e)
    }
}
type FutureTask = Pin<Box<dyn Future<Output = ()>>>;
/// Runs futures on a window's UI thread. Clones share the queue; spawning
/// does nothing once the window has closed.
#[derive(Clone)]
pub struct TaskSpawner {
    pending: Rc<RefCell<VecDeque<FutureTask>>>,
    proxy: EventLoopProxy<Event>,
    token: u64,
    closed: Arc<AtomicBool>,
}
impl TaskSpawner {
    /// Cancel automatically when the returned guard is dropped. This works even
    /// before the event loop drains the pending queue, and wakes sleeping tasks.
    ///
    /// ```no_run
    /// # fn mount(cx:&mut zgui_desktop::WindowContext,scope:&mut zgui::view::ViewScope) {
    /// let title=cx.ui.signal(String::from("Loading"));
    /// scope.retain(cx.tasks.spawn_scoped(async move {
    ///     zgui::timer::sleep(std::time::Duration::from_secs(1)).await;
    ///     title.set(String::from("Ready"));
    /// }));
    /// # }
    /// ```
    pub fn spawn_scoped(&self, future: impl Future<Output = ()> + 'static) -> crate::ScopedTask {
        let (guard, task) = crate::scoped_task::cancellable(future);
        self.spawn(task);
        guard
    }
    /// Run `future` on the UI thread until it finishes or the window closes.
    /// Use [`TaskSpawner::spawn_scoped`] for a task that a view can cancel.
    pub fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        self.pending.borrow_mut().push_back(Box::pin(future));
        let _ = self.proxy.send_event(Event::Wake(self.token));
    }
}
/// A handle addresses exactly one native window and becomes inert once it closes.
#[derive(Clone)]
pub struct WindowHandle {
    proxy: EventLoopProxy<Event>,
    token: u64,
    closed: Arc<AtomicBool>,
}
impl WindowHandle {
    /// Move and resize the window. A zero width or height is ignored.
    pub fn set_bounds(&self, bounds: crate::WindowBounds) {
        if bounds.size.0 > 0 && bounds.size.1 > 0 {
            self.command(WindowCommand::Bounds(bounds));
        }
    }
    /// Enter or leave fullscreen.
    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.command(WindowCommand::Fullscreen(fullscreen));
    }
    /// Show or hide the platform's title bar and frame.
    pub fn set_decorations(&self, decorations: bool) {
        self.command(WindowCommand::Decorations(decorations));
    }
    /// Let the real pointer pass through to whatever lies behind the window
    /// (a tool's window that must not hear the person's mouse); input
    /// dispatched to it in code still arrives.
    pub fn set_pointer_passthrough(&self, passthrough: bool) {
        self.command(WindowCommand::PointerPassthrough(passthrough));
    }
    /// The smallest logical client size the person can resize to; `None`
    /// removes the limit. A size that isn't finite and positive is ignored.
    pub fn set_min_inner_size(&self, size: Option<(f64, f64)>) {
        if size.is_none_or(|(w, h)| valid_window_size(w, h)) {
            self.command(WindowCommand::MinSize(size));
        }
    }
    /// Start a native move gesture from a primary-pointer press on a custom titlebar.
    /// Rejections are reported through WindowInfo::last_request_error.
    pub fn drag_window(&self) {
        self.command(WindowCommand::Drag);
    }
    /// Read the window's bounds, displays and capabilities into
    /// [`crate::WindowInfo`] again.
    pub fn refresh_window_info(&self) {
        self.command(WindowCommand::RefreshInfo);
    }
    /// Lose the window's GPU device, as a driver reset or a removed GPU
    /// would, for tests of recovery: the window makes a new renderer and
    /// draws on. Other windows on the same device recover alike.
    pub fn debug_lose_gpu(&self) {
        self.command(WindowCommand::LoseGpu);
    }
    /// Queue a validated application menu action for this window's focused UI.
    pub fn dispatch_menu_action(&self, action: impl Into<crate::MenuAction>) {
        if !self.is_closed() {
            let _ = self
                .proxy
                .send_event(Event::MenuAction(self.token, action.into()));
        }
    }
    fn command(&self, command: WindowCommand) {
        if !self.is_closed() {
            let _ = self
                .proxy
                .send_event(Event::WindowCommand(self.token, command));
        }
    }
    /// Show `entries` as a native pop-up menu at `(x, y)`, in points from the
    /// top left of the window's content (macOS; elsewhere it does nothing).
    /// A chosen item arrives through the window's menu action handler, and
    /// the press that opened the menu ends when it closes.
    pub fn show_context_menu(&self, entries: Vec<crate::AppMenuEntry>, x: f64, y: f64) {
        self.command(WindowCommand::ContextMenu { entries, x, y });
    }
    /// Change the native and accessible window title, including while suspended.
    pub fn set_title(&self, title: impl Into<String>) {
        self.command(WindowCommand::Title(title.into()));
    }
    /// Request a logical client size. Invalid dimensions are ignored. The compositor
    /// may constrain the request; `WindowContext::viewport` reports the actual size.
    pub fn set_inner_size(&self, width: f64, height: f64) {
        if valid_window_size(width, height) {
            self.command(WindowCommand::Size(width, height));
        }
    }
    /// Request minimization or restoration. Some compositors cannot unminimize windows.
    pub fn set_minimized(&self, minimized: bool) {
        self.command(WindowCommand::Minimized(minimized));
    }
    /// Request maximization or restoration.
    pub fn set_maximized(&self, maximized: bool) {
        self.command(WindowCommand::Maximized(maximized));
    }
    /// Show or hide this window. Visibility requests persist through native recreation.
    /// Wayland does not support changing visibility after creation.
    pub fn set_visible(&self, visible: bool) {
        self.command(WindowCommand::Visible(visible));
    }
    /// Cap animation frames per second, or `None` to follow the display.
    pub fn set_max_frame_rate(&self, hz: Option<f64>) {
        self.command(WindowCommand::MaxFrameRate(hz));
    }
    /// Ask the window manager to activate this window. Focus stealing prevention can
    /// reject this request; it does not change zgui's focused control directly.
    /// Currently unsupported on Wayland.
    pub fn request_focus(&self) {
        self.command(WindowCommand::Focus);
    }

    /// Ask the application close policy, as if the native close button was pressed.
    pub fn request_close(&self) {
        if !self.is_closed() {
            let _ = self.proxy.send_event(Event::RequestClose(self.token));
        }
    }
    /// Close this window immediately, bypassing the close-request policy.
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            let _ = self.proxy.send_event(Event::Close(self.token));
        }
    }
    /// Whether the window has closed; a closed handle ignores every request.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    /// Repaint the whole window on its next frame.
    pub fn request_redraw(&self) {
        if !self.closed.load(Ordering::Acquire) {
            let _ = self.proxy.send_event(Event::Redraw(self.token));
        }
    }
    /// Run the window's loop once: what changed since (input dispatched
    /// from elsewhere, state set off the loop) is laid out and painted as
    /// a frame of its own would be, its damage alone.
    pub fn wake(&self) {
        if !self.closed.load(Ordering::Acquire) {
            let _ = self.proxy.send_event(Event::Wake(self.token));
        }
    }
}
type WindowBuilder = Box<dyn FnOnce(&mut WindowContext)>;
struct WindowRequest {
    options: WindowOptions,
    state: WindowState,
    handle: WindowHandle,
    build: WindowBuilder,
}
/// Creates independent windows on the application's UI thread. Each owns its
/// scene, input focus, task executor, accessibility adapter, and GPU surface.
#[derive(Clone)]
pub struct WindowFactory {
    #[cfg(target_os = "linux")]
    file_routes: crate::wayland_data::Routes,
    /// The application's id (`Application::application_id`): its windows'
    /// Wayland app_id and X11 class, what docks match to its desktop file.
    #[cfg(target_os = "linux")]
    application_id: Option<Rc<str>>,
    /// The launcher's token (XDG activation on Wayland, startup
    /// notification on X11), for the next window made.
    #[cfg(target_os = "linux")]
    activation: Rc<RefCell<Option<winit::window::ActivationToken>>>,
    fonts: Rc<Vec<zgui_gpu::text::FontData>>,
    menus: Rc<RefCell<Vec<crate::AppMenu>>>,
    pending: Rc<RefCell<VecDeque<WindowRequest>>>,
    next: Rc<Cell<u64>>,
    alive: Rc<Cell<bool>>,
    clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
    gpu_contexts: Rc<RefCell<Vec<GpuContext>>>,
    proxy: EventLoopProxy<Event>,
    /// What every window's `Ui` runs on: one runtime, so state one window
    /// shows the others follow (`Ui::with_runtime`).
    runtime: zgui::reactive::Runtime,
}
impl WindowFactory {
    /// Replace the application menu model for existing and future windows.
    pub fn set_menus(&self, menus: Vec<crate::AppMenu>) -> Result<(), crate::AppMenuError> {
        crate::app_menu::validate(&menus)?;
        if !self.alive.get() {
            return Err(crate::AppMenuError("application is closed".into()));
        }
        *self.menus.borrow_mut() = menus;
        self.proxy
            .send_event(Event::MenusChanged)
            .map_err(|_| crate::AppMenuError("application event loop is closed".into()))
    }
    /// The application menu model, as last set.
    pub fn menus(&self) -> Vec<crate::AppMenu> {
        self.menus.borrow().clone()
    }
    /// Update command state without changing its label, shortcut or position.
    pub fn set_menu_state(
        &self,
        action: impl Into<crate::MenuAction>,
        enabled: Option<bool>,
        checked: Option<bool>,
    ) -> Result<(), crate::AppMenuError> {
        let mut menus = self.menus();
        if !crate::app_menu::update_entry(&mut menus, &action.into(), enabled, checked) {
            return Err(crate::AppMenuError("unknown menu action".into()));
        }
        self.set_menus(menus)
    }

    /// Exit the application, including when it is retained with no open windows.
    pub fn quit(&self) {
        if self.alive.get() {
            let _ = self.proxy.send_event(Event::Quit);
        }
    }
    fn allocate(&self) -> WindowHandle {
        let token = self.next.get();
        self.next
            .set(token.checked_add(1).expect("window IDs exhausted"));
        WindowHandle {
            proxy: self.proxy.clone(),
            token,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Open a window with `options`; `build` mounts its content once the
    /// native window exists. On a closed application the handle is already
    /// closed.
    pub fn open(
        &self,
        options: WindowOptions,
        build: impl FnOnce(&mut WindowContext) + 'static,
    ) -> WindowHandle {
        let handle = self.allocate();
        if !self.alive.get() {
            handle.closed.store(true, Ordering::Release);
            return handle;
        }
        self.pending.borrow_mut().push_back(WindowRequest {
            options,
            state: WindowState::default(),
            handle: handle.clone(),
            build: Box::new(build),
        });
        let _ = self.proxy.send_event(Event::Open);
        handle
    }
}
/// What a window's content is built with: its UI, its native services and
/// handles to it and the application.
pub struct WindowContext {
    /// The window's native file dialogs and prompts.
    pub dialogs: crate::FileDialogs,
    /// The window's bounds, displays and capabilities, kept current.
    pub window_info: crate::WindowInfo,
    menu_model: Signal<Vec<crate::AppMenu>>,
    /// The window's UI: its scene, input and reactive runtime.
    pub ui: Ui,
    /// Runs futures on the window's UI thread.
    pub tasks: TaskSpawner,
    /// This window.
    pub window: WindowHandle,
    /// Opens other windows and holds the application menu.
    pub windows: WindowFactory,
    /// Logical viewport dimensions; reactive layouts update on resize and scale changes.
    pub viewport: Signal<(f32, f32)>,
    /// Display-paced animation frames for this window. Components reach it
    /// through `Context::frames`.
    pub frames: FrameClock,
    /// The layout inspector (`zgui::inspect`): Ctrl+Option+Cmd+I on macOS,
    /// Ctrl+Alt+Shift+I elsewhere, or `ZGUI_INSPECT=1`, toggles it; call
    /// `set_enabled` to turn it on from code. Components reach it as a
    /// service (`Context::try_service`), as `zgui-script` does to label
    /// what it creates. Off, it costs nothing.
    pub inspector: zgui::inspect::Inspector,
    natives: zgui::native_view::NativeViews,
    #[allow(dead_code)]
    native_diagnostics: crate::native_host::NativeDiagnosticsHandle,
    close_requested: Option<Box<dyn FnMut() -> bool>>,
    closed: Option<Box<dyn FnOnce()>>,
    menu_action: Option<Box<dyn FnMut(crate::MenuAction)>>,
}
impl WindowContext {
    /// Application command fallback after focused component action routing.
    pub fn on_menu_action(&mut self, handler: impl FnMut(crate::MenuAction) + 'static) {
        self.menu_action = Some(Box::new(handler));
    }
    /// A rendered menu bar for Linux; macOS normally uses the system menu bar.
    pub fn app_menu_bar(&self) -> zgui::compose::View {
        let window = self.window.clone();
        let model = self.menu_model.clone();
        zgui::compose::switch(
            move || model.get(),
            move |menus, _| {
                let window = window.clone();
                crate::app_menu_bar(menus, move |action| window.dispatch_menu_action(action))
                    .expect("application menu was validated")
            },
        )
    }
    /// Ask for a file to open, then call `callback` with it. Dropping the
    /// returned task cancels the delivery.
    pub fn prompt_open_file(
        &self,
        options: crate::FileDialogOptions,
        callback: impl FnOnce(crate::FileDialogResult<std::path::PathBuf>) + 'static,
    ) -> crate::ScopedTask {
        let dialogs = self.dialogs.clone();
        self.tasks.spawn_scoped(async move {
            callback(dialogs.open_file(options).await);
        })
    }
    /// Ask for a destination to save to, then call `callback` with it. Nothing
    /// is written. Dropping the returned task cancels the delivery.
    pub fn prompt_save_file(
        &self,
        options: crate::FileDialogOptions,
        callback: impl FnOnce(crate::FileDialogResult<std::path::PathBuf>) + 'static,
    ) -> crate::ScopedTask {
        let dialogs = self.dialogs.clone();
        self.tasks.spawn_scoped(async move {
            callback(dialogs.save_file(options).await);
        })
    }
    /// Ask for a folder, then call `callback` with it. Dropping the returned
    /// task cancels the delivery.
    pub fn prompt_open_folder(
        &self,
        options: crate::FileDialogOptions,
        callback: impl FnOnce(crate::FileDialogResult<std::path::PathBuf>) + 'static,
    ) -> crate::ScopedTask {
        let dialogs = self.dialogs.clone();
        self.tasks.spawn_scoped(async move {
            callback(dialogs.open_folder(options).await);
        })
    }
    /// Mount declarative components with this window's scoped async executor.
    pub fn render(&mut self, view: zgui::compose::View) -> zgui::compose::ViewHandle {
        let tasks = self.tasks.clone();
        let runner = zgui::compose::TaskRunner::new(move |future| {
            zgui::compose::TaskToken::new(tasks.spawn_scoped(future), |task| task.is_finished())
        });
        // The clipboard and file dialogs, for what asks (zgui-script's
        // `ui.clipboard`, `ui.open_files`).
        let platform = zgui::platform::PlatformService(Rc::new(crate::platform::Desktop {
            clipboard: self.windows.clipboard.clone(),
            dialogs: self.dialogs.clone(),
        }));
        self.ui.render(zgui::compose::provide(
            self.frames.clone(),
            zgui::compose::provide(
                runner,
                zgui::compose::provide(
                    self.natives.clone(),
                    zgui::compose::provide(
                        self.inspector.clone(),
                        zgui::compose::provide(platform, view),
                    ),
                ),
            ),
        ))
    }
    /// This window's mounted native views (web pages): provided to the
    /// components `render` mounts, as `web_view` uses it.
    pub fn native_views(&self) -> zgui::native_view::NativeViews {
        self.natives.clone()
    }
    /// The live AppKit state of this window's native views (for fixtures
    /// and diagnostics); empty until a native view first shows.
    #[cfg(target_os = "macos")]
    pub fn native_diagnostics(&self) -> crate::NativeDiagnostics {
        self.native_diagnostics.clone()
    }

    /// Return false to keep the window open, for example while asking about unsaved data.
    pub fn on_close_requested(&mut self, handler: impl FnMut() -> bool + 'static) {
        self.close_requested = Some(Box::new(handler));
    }
    /// Called once before UI state is disposed. Pending tasks are cancelled on closure.
    pub fn on_closed(&mut self, handler: impl FnOnce() + 'static) {
        self.closed = Some(Box::new(handler));
    }
}
/// A desktop application: its fonts, first window, menus and native
/// events. [`Application::run`] starts the event loop.
pub struct Application {
    fonts: Vec<zgui_gpu::text::FontData>,
    options: WindowOptions,
    menus: Vec<crate::AppMenu>,
    application_id: Option<String>,
    application_event: Option<ApplicationEventHandler>,
    quit_on_last_window_close: bool,
}
type ApplicationEventHandler = Box<dyn FnMut(crate::ApplicationEvent, &WindowFactory)>;
impl Application {
    /// An application with the default window, no menus and no application
    /// ID, which quits when its last window closes.
    pub fn new() -> Self {
        // Dates' "today" is the local one.
        zgui::compose::set_local_offset(crate::local_time::offset);
        Self {
            fonts: Vec::new(),
            options: WindowOptions::default(),
            menus: Vec::new(),
            application_id: None,
            application_event: None,
            quit_on_last_window_close: true,
        }
    }
    /// Register a bundled font for every window before native text is shaped.
    pub fn font(mut self, font: zgui_gpu::text::FontData) -> Self {
        self.fonts.push(font);
        self
    }
    /// The first window's options.
    pub fn window(mut self, options: WindowOptions) -> Self {
        self.options = options;
        self
    }
    /// The application menu (validated now), for every window.
    pub fn menus(mut self, menus: Vec<crate::AppMenu>) -> Result<Self, crate::AppMenuError> {
        crate::app_menu::validate(&menus)?;
        self.menus = menus;
        Ok(self)
    }
    /// Register native URL/reopen delivery. Linux exports org.freedesktop.Application;
    /// macOS owns GetURL, OpenDocuments, and Reopen AppleEvent routes for the
    /// event loop lifetime while preserving its NSApplication delegate. Do not
    /// install competing handlers for these routes while this service is active.
    /// On Linux it is also the windows' Wayland app_id and X11 class, and on
    /// X11 the name of their icon in the icon theme (else the executable's).
    pub fn application_id(
        mut self,
        id: impl Into<String>,
    ) -> Result<Self, crate::ApplicationEventError> {
        let id = id.into();
        crate::native_events::validate_id(&id)?;
        self.application_id = Some(id);
        Ok(self)
    }
    /// Call `handler` for each URL or reopen request the system delivers
    /// (requires [`Application::application_id`]).
    pub fn on_application_event(
        mut self,
        handler: impl FnMut(crate::ApplicationEvent, &WindowFactory) + 'static,
    ) -> Self {
        self.application_event = Some(Box::new(handler));
        self
    }
    /// Retain the event loop for native reopen/open-URL delivery after the last
    /// window closes. Call WindowFactory::quit to terminate a retained app.
    pub fn quit_on_last_window_close(mut self, quit: bool) -> Self {
        self.quit_on_last_window_close = quit;
        self
    }
    /// Run the event loop: open the first window, `build` its content, and
    /// return once the application quits.
    pub fn run(self, build: impl FnOnce(&mut WindowContext)) -> Result<(), Box<dyn Error>> {
        let event_loop = EventLoop::<Event>::with_user_event().build()?;
        let proxy = event_loop.create_proxy();
        let native_events = if let Some(id) = &self.application_id {
            let wake = proxy.clone();
            let events = crate::native_events::NativeApplicationEvents::new(
                id,
                Arc::new(move |event, token| {
                    let _ = wake.send_event(Event::Application(event, token));
                }),
            )?;
            // Windows: another instance runs and took this launch's files
            // and URLs; this one opens nothing.
            let Some(events) = events else {
                return Ok(());
            };
            Some(events)
        } else {
            None
        };
        #[cfg(target_os = "macos")]
        let native_menu = {
            let wake = proxy.clone();
            muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
                let _ = wake.send_event(Event::NativeMenuAction(crate::MenuAction(event.id.0)));
            }));
            // With none of the app's own, still the application and Window
            // menus every Mac app has.
            Some(crate::app_menu::NativeMenu::new(&self.menus)?)
        };
        let factory = WindowFactory {
            fonts: Rc::new(self.fonts),
            #[cfg(target_os = "linux")]
            file_routes: Default::default(),
            #[cfg(target_os = "linux")]
            application_id: self.application_id.as_deref().map(Rc::from),
            #[cfg(target_os = "linux")]
            activation: Rc::default(),
            menus: Rc::new(RefCell::new(self.menus)),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            next: Rc::new(Cell::new(1)),
            alive: Rc::new(Cell::new(true)),
            clipboard: Rc::new(RefCell::new(None)),
            gpu_contexts: Rc::new(RefCell::new(Vec::new())),
            proxy: proxy.clone(),
            runtime: zgui::reactive::Runtime::new(),
        };
        let handle = factory.allocate();
        let token = handle.token;
        let mut root = Host::new(self.options, handle, factory.clone());
        build(&mut root.context);
        // Timers fire as the loop wakes for its own deadlines, without a
        // scheduler thread waking it with a message per timer.
        let timers = {
            let (wake, ui_thread) = (proxy.clone(), std::thread::current().id());
            zgui::timer::host(move || {
                if std::thread::current().id() != ui_thread {
                    let _ = wake.send_event(Event::Timers);
                }
            })
        };
        let mut host = WindowManager {
            #[cfg(target_os = "linux")]
            native_data: None,
            #[cfg(target_os = "linux")]
            appearance: None,
            _native_events: native_events,
            application_event: self.application_event,
            quit_on_last_window_close: self.quit_on_last_window_close,
            #[cfg(target_os = "macos")]
            native_menu,
            hosts: HashMap::from([(token, root)]),
            panics: Vec::new(),
            native: HashMap::new(),
            drag_over: Cell::new(None),
            carrying: None,
            #[cfg(target_os = "linux")]
            last_focused: None,
            factory,
            active: false,
            error: None,
            timers,
        };
        event_loop.run_app(&mut host)?;
        if let Some(e) = host.error.take() {
            return Err(e.into());
        }
        Ok(())
    }
}
impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}
/// A window that has presented nothing for this long releases animation-only
/// renderer memory.
const TRIM_DELAY: std::time::Duration = std::time::Duration::from_millis(1500);
/// A window's frame is saved this long after it last moved or resized
/// (`frame_autosave`, Linux): once for a drag, not for each of its steps.
#[cfg(target_os = "linux")]
const FRAME_SAVE_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
/// Frames presented this close together are a steady run.
#[cfg(target_os = "macos")]
const STEADY_GAP: std::time::Duration = std::time::Duration::from_millis(50);
/// Sleep until `at`: a long sleep ends up to a millisecond or two late (the
/// kernel coalesces timers in proportion to their length), so the last
/// stretch is a short one of its own.
#[cfg(target_os = "macos")]
fn sleep_until(at: std::time::Instant) {
    const SHORT: std::time::Duration = std::time::Duration::from_micros(1500);
    let now = std::time::Instant::now();
    if at > now + SHORT {
        std::thread::sleep(at - now - SHORT);
    }
    let now = std::time::Instant::now();
    if at > now {
        std::thread::sleep(at - now);
    }
}
/// How long after a resize frames skip the refresh-point wait (the renderer
/// presents on the plain layer for as long: `zgui_gpu`'s `RESIZE_SETTLE`).
#[cfg(target_os = "macos")]
const RESIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);
/// How long after input frames stay at the display's full rate: a press's
/// release spring or a hover fade that follows input keeps its frames too.
#[cfg(any(target_os = "macos", windows))]
const INPUT_FULL_RATE: std::time::Duration = std::time::Duration::from_millis(250);
/// The refresh point steady frames present at (see `Host::refresh_grid`),
/// and when they render before it.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct Latch {
    /// When the last steady frame was ready, and when it presented.
    last: Option<(std::time::Instant, std::time::Instant)>,
    /// Frames in a row ready comfortably before their point while keeping
    /// a later cadence.
    early: u8,
    /// How long after it could start a frame starts rendering (`hold`).
    offset: std::time::Duration,
    /// The most a frame could wait before rendering and still be ready by
    /// its point, as last held.
    room: std::time::Duration,
    /// When the frame being drawn started rendering, and how long frames
    /// lately took from there until ready (the most, forgotten over seconds).
    started: Option<std::time::Instant>,
    work: std::time::Duration,
    /// The GPU time of an unhindered frame: the least seen lately.
    clean: Option<std::time::Duration>,
    /// Frames since one the GPU held up.
    since_held: u8,
    /// The point the frame rendering after a delay was held for, and frames
    /// left before holding again after one missed it.
    planned: Option<std::time::Instant>,
    resting: u8,
    /// How long the frame rendering was held (`hold`).
    held: std::time::Duration,
}
#[cfg(target_os = "macos")]
impl Latch {
    /// How long before a refresh shows a frame presents. At 2.3 ms a
    /// dashboard repainting most of its window still missed refreshes (2% of
    /// intervals late, p99 28 ms); at 2 ms none (p99 21 ms), lighter windows
    /// alike or better. Closer than 2 ms gained nothing.
    const LEAD: std::time::Duration = std::time::Duration::from_micros(2000);
    /// How early before its point a frame must be ready, `RETURN` frames in a
    /// row, before a later cadence gives way to the coming point: a stream
    /// ready near a point, some frames just before and some just after, all
    /// wait for the next.
    const KEEP: std::time::Duration = std::time::Duration::from_micros(1500);
    const RETURN: u8 = 8;
    /// How far frames the GPU held up move their successors (`ran`).
    const STEP: std::time::Duration = std::time::Duration::from_millis(2);
    /// Frames rendered at once after a held one missed its refresh.
    const REST: u8 = 120;
    /// When a frame complete at `now` presents, on the refresh grid through
    /// `origin` with `period`.
    fn decide(
        &mut self,
        now: std::time::Instant,
        origin: std::time::Instant,
        period: std::time::Duration,
    ) -> std::time::Instant {
        if let Some(started) = self.started.take() {
            self.work = now
                .saturating_duration_since(started)
                .max(self.work.mul_f64(0.99));
        }
        // Decided as if rendered when it could have been: held frames would
        // otherwise come ready milliseconds apart from one to the next, and
        // count whole refreshes between them wrongly. Past that point by
        // now (slower than any frame lately), the next on the grid.
        let ready = now
            .checked_sub(std::mem::take(&mut self.held))
            .unwrap_or(now);
        let (mut at, early) = self.choose(ready, origin, period);
        while at < now {
            at += period;
        }
        self.early = early;
        // Held into a later refresh than planned: hold none for a while.
        if self
            .planned
            .take()
            .is_some_and(|planned| at > planned + period / 2)
        {
            self.offset = std::time::Duration::ZERO;
            self.resting = Self::REST;
        }
        self.last = Some((ready, at));
        at
    }
    /// The point a frame ready at `ready` presents at, and the count of
    /// frames comfortably early on a later cadence that it leaves.
    fn choose(
        &self,
        ready: std::time::Instant,
        origin: std::time::Instant,
        period: std::time::Duration,
    ) -> (std::time::Instant, u8) {
        // The phase of `ready` on the grid, either side of `origin`.
        let period_ns = period.as_nanos().max(1);
        let since = match ready.checked_duration_since(origin) {
            Some(after) => after.as_nanos() % period_ns,
            None => (period_ns - origin.duration_since(ready).as_nanos() % period_ns) % period_ns,
        };
        let phase = std::time::Duration::from_nanos(since as u64);
        let point = ready - phase + period.saturating_sub(Self::LEAD);
        let first = if point >= ready {
            point
        } else {
            point + period
        };
        // A later cadence (the last frame's, whole refreshes after it) holds
        // until frames come comfortably early, several in a row.
        let late = self.last.is_some_and(|(last, at)| {
            let refreshes = (ready.saturating_duration_since(last).as_secs_f64()
                / period.as_secs_f64())
            .round();
            let cadence = at + period.mul_f64(refreshes);
            cadence > first + period / 2 && cadence < first + period * 3 / 2
        });
        let early = if late && first - ready >= Self::KEEP {
            self.early + 1
        } else {
            0
        };
        if late && early < Self::RETURN {
            (first + period, early)
        } else {
            (first, 0)
        }
    }
    /// When a frame that could start rendering at `now` does. It waits for
    /// its point once rendered anyway; rendering later within the wait, still
    /// `KEEP` ahead of the point it would have had (going by the longest any
    /// frame took lately), shows it on the same refresh. The point is still
    /// chosen once the frame is ready (`decide`); a held frame that misses it
    /// (work beyond any lately) stops holding for a couple of seconds.
    ///
    /// The window server composites other windows' changes on the GPU at
    /// their own times, a millisecond or more of work on a 5K display, and a
    /// frame rendering through it takes several times as long: beside a 30
    /// Hz animation, every other frame of a 60 Hz stream ran 1–1.4 ms instead
    /// of 0.3. Frames the GPU holds up (`ran`) move the time their successors
    /// render within the wait until they are not: a dashboard's stream in
    /// Metal System Trace went from a median 39 to 21 ms of GPU per second.
    fn hold(
        &mut self,
        now: std::time::Instant,
        origin: std::time::Instant,
        period: std::time::Duration,
    ) -> std::time::Instant {
        let ready = now + self.work;
        let (point, _) = self.choose(ready, origin, period);
        self.room = point
            .saturating_duration_since(ready)
            .saturating_sub(Self::KEEP);
        if self.offset > self.room || self.resting > 0 {
            self.offset = std::time::Duration::ZERO;
        }
        self.resting = self.resting.saturating_sub(1);
        self.planned = (!self.offset.is_zero()).then_some(point);
        self.held = self.offset;
        now + self.offset
    }
    /// The frame that started rendering at `at` (after `hold`).
    fn rendering(&mut self, at: std::time::Instant) {
        self.started = Some(at);
    }
    /// How long the GPU took to run the frame just presented: well over the
    /// least lately, twice within a few frames, is another client's rhythm.
    fn ran(&mut self, gpu: std::time::Duration) {
        let clean = self.clean.map_or(gpu, |clean| gpu.min(clean.mul_f64(1.02)));
        self.clean = Some(clean);
        if gpu <= clean.mul_f64(1.5) + std::time::Duration::from_micros(50) {
            self.since_held = self.since_held.saturating_add(1);
            return;
        }
        if self.since_held < 4 && self.resting == 0 {
            self.offset += Self::STEP;
            if self.offset > self.room {
                self.offset = std::time::Duration::ZERO;
            }
        }
        self.since_held = 0;
    }
}
/// After the first presented frame, when to release startup's freed heap.
const STARTUP_TRIM_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartupTrim {
    Pending,
    At(std::time::Instant),
    Done,
}
/// The system's light or dark appearance, as zgui's themes take it.
fn appearance_of(theme: winit::window::Theme) -> zgui::theme::Appearance {
    match theme {
        winit::window::Theme::Light => zgui::theme::Appearance::Light,
        winit::window::Theme::Dark => zgui::theme::Appearance::Dark,
    }
}
struct Host {
    #[cfg(target_os = "linux")]
    file_surface: Option<u32>,
    menu_binding: zgui::actions::KeymapBinding,
    _menu_listener: zgui::input::InputBinding,
    options: WindowOptions,
    state: WindowState,
    context: WindowContext,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    /// When to try making a renderer again after the GPU device was lost
    /// (`recover_renderer`), while none could be made.
    renderer_retry: Option<std::time::Instant>,
    /// The banner saying something failed (`show_error`), once one has.
    error_banner: Option<(Signal<String>, zgui::compose::ViewHandle)>,
    adapter: Option<accesskit_winit::Adapter>,
    tree: AccessibilityTree,
    proxy: EventLoopProxy<Event>,
    tasks: LocalExecutor,
    pending: Rc<RefCell<VecDeque<FutureTask>>>,
    clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
    /// The editor selection last published as the primary selection.
    #[cfg(target_os = "linux")]
    primary: Option<String>,
    cursor: (f32, f32),
    pending_file_drops: crate::file_drop::FileDropBatch,
    modifiers: Modifiers,
    ime_active: bool,
    ime_restart: crate::native_ime::RestartBarrier,
    ime_cancellation: crate::native_ime::CancellationTracker,
    native_focused: bool,
    x11: bool,
    scale_factor: f64,
    force: bool,
    presentation: Presentation,
    native_occluded: bool,
    frame_paced: bool,
    /// Until when input keeps frames at the display's full rate (`INPUT_FULL_RATE`).
    #[cfg(any(target_os = "macos", windows))]
    input_until: Option<std::time::Instant>,
    /// Input changed the scene since the last frame: drawn at once rather
    /// than at the next tick (Windows: any input; macOS: typing).
    #[cfg(any(target_os = "macos", windows))]
    input_arrived: bool,
    present_ready: bool,
    frames: FrameClock,
    #[cfg(any(target_os = "macos", windows))]
    display_link: Option<crate::frame_source::DisplayLink>,
    /// `Titlebar::Inset`'s caption buttons (Windows, Linux) and an
    /// undecorated window's resize bands (Linux).
    #[cfg(any(windows, target_os = "linux"))]
    caption: Option<crate::window_chrome::CaptionHost>,
    /// The resize an undecorated window's edge under the pointer would
    /// start (Linux): content hears nothing there.
    #[cfg(target_os = "linux")]
    resize_edge: Option<winit::window::ResizeDirection>,
    /// The pointer has been over a popup window of this one (X11): winit
    /// may not report its move back, so a press asks where it is.
    #[cfg(target_os = "linux")]
    had_popups: bool,
    /// When the display link last delivered a refresh, aligning `frame_timer`
    /// to its grid: until `VSYNC_RESYNC` later the timer paces frames and the
    /// link (a thread spinning to each refresh) stays stopped.
    #[cfg(target_os = "macos")]
    vsync_synced: Option<std::time::Instant>,
    /// Frames are wanted and the aligned timer paces them.
    #[cfg(target_os = "macos")]
    vsync_paced: bool,
    /// The display link paces frames at the display's full rate.
    #[cfg(target_os = "macos")]
    link_paced: bool,
    /// When a frame was last presented, and the cadence steady frames keep
    /// (see `Host::refresh_grid`).
    #[cfg(target_os = "macos")]
    presented_at: Option<std::time::Instant>,
    /// When the window last changed size: live-resize frames are drawn
    /// inside AppKit's resize transaction, which shows them with the new
    /// frame, so they skip the refresh-point wait (`refresh_grid`) that
    /// would hold the next resize step back.
    #[cfg(target_os = "macos")]
    resized_at: Option<std::time::Instant>,
    #[cfg(target_os = "macos")]
    latch: Latch,
    frame_timer: crate::frame_source::FrameTimer,
    frame_interval: std::time::Duration,
    /// A display refresh arrived since the last paint. While the display link
    /// runs, paints wait for it so input between refreshes cannot present
    /// extra frames (which would queue drawables and block the UI thread).
    frame_arrived: bool,
    /// When to release animation-only renderer memory if nothing presents
    /// before then (see `GpuRenderer::trim`).
    trim_at: Option<std::time::Instant>,
    /// When to save the window's frame (`frame_autosave`, Linux): once its
    /// moves and resizes settle.
    frame_save_at: Option<std::time::Instant>,
    startup_trim: StartupTrim,
    /// Timer tick (no native source) or watchdog deadline (native source).
    frame_deadline: Option<std::time::Instant>,
    /// When frames started being wanted, or the last native tick since.
    frame_heartbeat: Option<std::time::Instant>,
    /// A Wayland redraw is pending on the compositor's frame callback.
    frame_redraw_requested: bool,
    focus_memory: crate::focus_memory::FocusMemory,
    last_accessibility: Option<(u64, Option<NodeId>)>,
    accessibility_geometry_dirty: bool,
    /// The display text selection last published as the primary selection.
    #[cfg(target_os = "linux")]
    primary_revision: u64,
    native_cursor: Option<CursorIcon>,
    /// The window's cursor rects are off (see `disable_cursor_rects`).
    #[cfg(target_os = "macos")]
    cursor_rects_off: bool,
    cursor_query: Option<((f32, f32), u64, Option<zgui::cursor::Cursor>)>,
    /// The pointer is over this window. Outside it, the cursor shape does not
    /// matter, so animating geometry need not be hit-tested every frame.
    pointer_inside: bool,
    ime_geometry: Option<Rect>,
    ime_target: Option<NodeId>,
    gpu_stats: Option<crate::gpu_stats::GpuStatsLog>,
    /// `ZGUI_DAMAGE_CHECK=1`: compare every damaged frame with a full
    /// repaint and report stale pixels (slow; for debugging damage).
    damage_check: Option<u64>,
    /// Wheel and trackpad motion waiting for display refreshes.
    scroll: crate::scroll_resample::ScrollResampler,
    /// Fingers and pens on the screen, as the pointer.
    touch: crate::touch::Touches,
    /// What had focus before a finger pressed, for a pan to give it back.
    touch_focus: Option<zgui::scene::NodeId>,
    /// The last press was a finger's or a pen's, not a mouse's: an editor
    /// it focuses brings up the touch keyboard.
    #[cfg(windows)]
    touch_last: bool,
    /// zgui brought the touch keyboard up, and puts it away.
    #[cfg(windows)]
    touch_keyboard: bool,
    /// Where the trackpad's scroll gesture is: what its phases report
    /// (`InputEvent::ScrollGesture`, for elastic scrolling).
    trackpad: Trackpad,
    /// Native views (web pages) over and under this window's zgui content.
    native: crate::native_host::NativeHost,
    /// Popups shown past the window, in windows of their own.
    popups: crate::popup_window::PopupHost,
    /// Its drag handed to the platform (`native_drag`).
    native_drag: crate::native_drag::Handoff,
    /// It was in fullscreen as its geometry last changed: popups close as
    /// it goes in or out.
    popups_fullscreen: bool,
    /// Its content's top-left in global coordinates, and its number with
    /// the window server (macOS), for drags between windows: kept as the
    /// window moves, so a drag asks AppKit nothing as it moves.
    content_origin: Option<(f64, f64)>,
    window_number: Option<isize>,
    /// Where the pointer held the window as a drag moving it began
    /// (`View::drag_moves_window`), in its coordinates.
    window_grab: Option<(f32, f32)>,
    /// Where the window server has put its frame's top-left that AppKit
    /// has yet to hear (`window_place::move_to`).
    window_pending: Option<(f64, f64)>,
    /// The theme's `background` when it changes, for the renderer's clear
    /// colour, and the effect that watches it.
    theme_background: Rc<std::cell::Cell<Option<zgui::scene::Color>>>,
    applied_background: Option<zgui::scene::Color>,
    _background_effect: Option<zgui::reactive::Effect>,
}
impl Host {
    fn new(options: WindowOptions, handle: WindowHandle, windows: WindowFactory) -> Self {
        let proxy = handle.proxy.clone();
        let wake = proxy.clone();
        let token = handle.token;
        let pending = Rc::new(RefCell::new(VecDeque::new()));
        let ui = Ui::with_runtime(
            windows.runtime.clone(),
            options.width as f32,
            options.height as f32,
        );
        let viewport = ui.signal((options.width as f32, options.height as f32));
        let frames = FrameClock::new();
        frames.set_max_rate(options.max_frame_rate);
        let window_info = crate::WindowInfo::new(&ui, options.width, options.height);
        let clipboard = windows.clipboard.clone();
        let root = ui.scene.borrow().root();
        let menu_model = ui.signal(windows.menus());
        let menu_binding = ui
            .input
            .bind_keys(root, crate::app_menu::keymap(&windows.menus()));
        let fallback = proxy.clone();
        let menus = windows.menus.clone();
        let closed = handle.closed.clone();
        let menu_listener = ui.input.listen(root, move |event| {
            if event.phase == zgui::input::EventPhase::Capture
                || event.default_prevented()
                || closed.load(Ordering::Acquire)
            {
                return;
            }
            if let InputEvent::Action(action) = &event.event
                && let Some(action) = action.downcast_ref::<crate::MenuAction>()
                && crate::app_menu::enabled_action(&menus.borrow(), action)
            {
                let _ = fallback.send_event(Event::MenuFallback(token, action.clone()));
                event.prevent_default();
            }
        });
        let native = crate::native_host::NativeHost::new(&ui, {
            let wake = proxy.clone();
            move || {
                let _ = wake.send_event(Event::Wake(token));
            }
        });
        let popups = crate::popup_window::PopupHost::new({
            let proxy = proxy.clone();
            move |input| {
                let _ = proxy.send_event(Event::Popup(token, input));
            }
        });
        // Where popups have windows of their own, one is readied as the
        // pointer reaches a trigger.
        if crate::popup_window::enabled() {
            let proxy = proxy.clone();
            ui.on_popup_soon(move || {
                let _ = proxy.send_event(Event::PopupSoon(token));
            });
            // Drag previews too, moved with the pointer (`PopupHost::follow`).
            ui.set_popup_previews(cfg!(any(target_os = "macos", windows)));
        }
        let inspector = zgui::inspect::Inspector::new();
        {
            let handle = handle.clone();
            inspector.set_wake(move || handle.request_redraw());
        }
        let context = WindowContext {
            inspector,
            natives: native.natives.clone(),
            native_diagnostics: native.diagnostics.clone(),
            window_info,
            menu_model,
            dialogs: crate::FileDialogs::new(handle.clone()),
            menu_action: None,
            ui,
            viewport,
            frames: frames.clone(),
            close_requested: None,
            closed: None,
            tasks: TaskSpawner {
                pending: pending.clone(),
                proxy: proxy.clone(),
                token,
                closed: handle.closed.clone(),
            },
            window: handle,
            windows,
        };
        Self {
            #[cfg(target_os = "linux")]
            file_surface: None,
            menu_binding,
            _menu_listener: menu_listener,
            options,
            state: WindowState::default(),
            context,
            window: None,
            renderer: None,
            renderer_retry: None,
            error_banner: None,
            adapter: None,
            tree: AccessibilityTree::new(),
            proxy,
            tasks: {
                // Wakes on the UI thread happen inside an event-loop callback,
                // and `about_to_wait` polls ready tasks afterwards. Only other
                // threads (timers, dialogs) need to wake the loop.
                let ui_thread = std::thread::current().id();
                LocalExecutor::with_wake(move || {
                    if std::thread::current().id() != ui_thread {
                        let _ = wake.send_event(Event::Wake(token));
                    }
                })
            },
            pending,
            clipboard,
            #[cfg(target_os = "linux")]
            primary: None,
            cursor: (0., 0.),
            scroll: Default::default(),
            touch: Default::default(),
            touch_focus: None,
            #[cfg(windows)]
            touch_last: false,
            #[cfg(windows)]
            touch_keyboard: false,
            trackpad: Trackpad::Idle,
            pending_file_drops: Default::default(),
            modifiers: Modifiers::default(),
            ime_active: false,
            ime_restart: Default::default(),
            ime_cancellation: Default::default(),
            native_focused: false,
            x11: false,
            scale_factor: 1.,
            force: true,
            presentation: Presentation::default(),
            native_occluded: false,
            frame_paced: false,
            #[cfg(any(target_os = "macos", windows))]
            input_until: None,
            #[cfg(any(target_os = "macos", windows))]
            input_arrived: false,
            present_ready: false,
            gpu_stats: crate::gpu_stats::GpuStatsLog::from_env(),
            damage_check: std::env::var_os("ZGUI_DAMAGE_CHECK").map(|_| 0),
            frames,
            #[cfg(any(target_os = "macos", windows))]
            display_link: None,
            #[cfg(any(windows, target_os = "linux"))]
            caption: None,
            #[cfg(target_os = "linux")]
            resize_edge: None,
            #[cfg(target_os = "linux")]
            had_popups: false,
            #[cfg(target_os = "macos")]
            vsync_synced: None,
            #[cfg(target_os = "macos")]
            vsync_paced: false,
            #[cfg(target_os = "macos")]
            link_paced: false,
            #[cfg(target_os = "macos")]
            presented_at: None,
            #[cfg(target_os = "macos")]
            resized_at: None,
            #[cfg(target_os = "macos")]
            latch: Latch::default(),
            frame_arrived: false,
            trim_at: None,
            frame_save_at: None,
            startup_trim: StartupTrim::Pending,
            frame_timer: crate::frame_source::FrameTimer::new(std::time::Instant::now()),
            frame_interval: crate::frame_source::DEFAULT_INTERVAL,
            frame_deadline: None,
            frame_heartbeat: None,
            frame_redraw_requested: false,
            focus_memory: crate::focus_memory::FocusMemory::default(),
            last_accessibility: None,
            accessibility_geometry_dirty: true,
            #[cfg(target_os = "linux")]
            primary_revision: 0,
            native_cursor: None,
            #[cfg(target_os = "macos")]
            cursor_rects_off: false,
            cursor_query: None,
            pointer_inside: false,
            ime_geometry: None,
            ime_target: None,
            native,
            popups,
            native_drag: Default::default(),
            popups_fullscreen: false,
            content_origin: None,
            window_number: None,
            window_grab: None,
            window_pending: None,
            theme_background: Rc::default(),
            applied_background: None,
            _background_effect: None,
        }
    }
    fn dispatch(&mut self, event: InputEvent) -> Result<(), zgui::widgets::LayoutFeedbackError> {
        self.dispatch_prevented(event).map(|_| ())
    }
    /// Carry out what a touch asked (`touch::Step`).
    fn touch_steps(
        &mut self,
        steps: Vec<crate::touch::Step>,
    ) -> Result<(), zgui::widgets::LayoutFeedbackError> {
        for step in steps {
            match step {
                crate::touch::Step::Dispatch(event) => self.dispatch(event)?,
                crate::touch::Step::Unpress => {
                    self.dispatch(InputEvent::PointerCancel)?;
                    let ui = &self.context.ui;
                    if ui.input.focused() != self.touch_focus {
                        ui.input.focus(&ui.scene, self.touch_focus);
                    }
                }
                crate::touch::Step::TryPan { x, y, dx, dy } => {
                    let panned = self.dispatch_prevented(InputEvent::Scroll {
                        x,
                        y,
                        delta_x: dx,
                        delta_y: dy,
                    })?;
                    let next = self.touch.decided(panned);
                    self.touch_steps(next)?;
                }
            }
        }
        Ok(())
    }
    fn dispatch_prevented(
        &mut self,
        event: InputEvent,
    ) -> Result<bool, zgui::widgets::LayoutFeedbackError> {
        // Input shows on the next refresh even while the only animations run at a
        // reduced rate (a 12 fps dither): see `sync_frames`.
        #[cfg(any(target_os = "macos", windows))]
        {
            self.input_until = Some(std::time::Instant::now() + INPUT_FULL_RATE);
        }
        #[cfg(windows)]
        {
            self.input_arrived = true;
        }
        // macOS: typing draws at once even mid-animation (see `process`);
        // pointer motion and scrolling keep the refresh's pace.
        #[cfg(target_os = "macos")]
        if matches!(
            event,
            InputEvent::KeyDown { .. }
                | InputEvent::Text(_)
                | InputEvent::ImePreedit { .. }
                | InputEvent::ImeCommit(_)
        ) {
            self.input_arrived = true;
        }
        let result = self
            .context
            .ui
            .try_dispatch_with_modifiers(event, self.modifiers)?;
        if result.focus_changed {
            self.ime_active = false;
        }
        #[cfg(target_os = "linux")]
        self.publish_primary();
        Ok(result.default_prevented)
    }
    /// Selected display text becomes the primary selection (what a middle
    /// click pastes) once a gesture settles, as X11 and Wayland apps do.
    #[cfg(target_os = "linux")]
    fn publish_primary(&mut self) {
        let ui = &self.context.ui;
        let revision = ui.text_selection_revision();
        if revision == self.primary_revision || ui.is_selecting_text() {
            return;
        }
        self.primary_revision = revision;
        if !ui.has_text_selection() {
            return;
        }
        let text = ui.selected_text();
        if let Some(clipboard) = self.clipboard.borrow_mut().as_mut() {
            let _ = clipboard.set_primary_text(text);
        }
    }
    fn sync_accessibility(&mut self) {
        let state = (
            self.context.ui.semantics.borrow().revision(),
            self.context.ui.input.focused(),
        );
        if !self.accessibility_geometry_dirty && self.last_accessibility == Some(state) {
            return;
        }
        self.last_accessibility = Some(state);
        self.accessibility_geometry_dirty = false;
        if let (Some(adapter), Some(_)) = (&mut self.adapter, &self.window) {
            let ui = &self.context.ui;
            adapter.update_if_active(|| {
                self.tree.update(
                    &ui.scene.borrow(),
                    &ui.semantics.borrow(),
                    ui.input.focused(),
                    &self.options.title,
                    self.scale_factor,
                )
            });
        }
    }
    fn sync_ime_cancellation(&mut self) {
        // Update native permission before event delivery and before hidden-window
        // drawing gates. Read-only editors retain logical focus but no IME context.
        let target = crate::native_ime::editable_editor(&self.context.ui, self.native_focused)
            .map(|editor| editor.node);
        if let Some(window) = &self.window
            && target != self.ime_target
        {
            self.ime_restart
                .target_changed(self.ime_target, target, self.ime_active);
            self.ime_active = false;
            self.ime_geometry = None;
            if self.ime_target.is_some() {
                window.set_ime_allowed(false);
                crate::native_ime::discard_native_composition(window);
            }
            if target.is_some() {
                window.set_ime_allowed(true);
            }
            #[cfg(windows)]
            if target.is_some() && self.touch_last {
                self.touch_keyboard = crate::input_pane::show(window);
            } else if target.is_none() && std::mem::take(&mut self.touch_keyboard) {
                crate::input_pane::hide(window);
            }
            self.ime_target = target;
        }
        if self.ime_cancellation.sync(
            &self.context.ui,
            self.native_focused,
            self.ime_target,
            &mut self.ime_restart,
            &mut self.ime_active,
        ) {
            self.ime_geometry = None;
            if let Some(window) = &self.window {
                window.set_ime_allowed(false);
                crate::native_ime::discard_native_composition(window);
                window.set_ime_allowed(true);
            }
        }
    }
    fn draw(&mut self) -> Result<(), Box<dyn Error>> {
        if self
            .renderer_retry
            .is_some_and(|at| at <= std::time::Instant::now())
        {
            self.recover_renderer();
        }
        match self.draw_frame() {
            Err(error)
                if error
                    .downcast_ref::<zgui_gpu::GpuError>()
                    .is_some_and(zgui_gpu::GpuError::is_device_lost) =>
            {
                eprintln!("zgui: {error}; making a new renderer");
                self.recover_renderer();
                Ok(())
            }
            other => other,
        }
    }
    fn draw_frame(&mut self) -> Result<(), Box<dyn Error>> {
        self.sync_ime_cancellation();
        let Some(window) = self.window.clone() else {
            self.context.ui.cancel_interactions();
            return Ok(());
        };
        let size = window.inner_size();
        let visible = window.is_visible();
        // X11's minimized query is a synchronous property roundtrip. Focused
        // windows cannot be minimized, and known-hidden windows need no query.
        let minimized = if window.has_focus()
            || visible == Some(false)
            || self.native_occluded
            || size.width == 0
            || size.height == 0
        {
            None
        } else {
            window.is_minimized()
        };
        if !self.presentation.render_allowed(
            (size.width, size.height),
            visible,
            minimized,
            self.native_occluded,
        ) {
            self.context.ui.cancel_interactions();
            return Ok(());
        }
        self.context
            .ui
            .advance_interactions(std::time::Instant::now())?;
        self.context.ui.try_prepare_frame()?;
        // The layout inspector's overlay and warnings follow layout (and
        // the pointer); off, this is one flag.
        if self.context.inspector.active() {
            let pointer = self.pointer_inside.then_some(self.cursor);
            self.context.inspector.update(&self.context.ui, pointer);
        }
        self.sync_ime_cancellation();
        if let Some(color) = self.theme_background.take()
            && self.applied_background != Some(color)
            && let Some(renderer) = &mut self.renderer
        {
            self.applied_background = Some(color);
            renderer.set_background(color);
            self.force = true;
        }
        let scene = self.context.ui.scene.clone();
        let mut scene = scene.borrow_mut();
        let report = scene.flush();
        self.accessibility_geometry_dirty |= !report.damage.is_empty();
        let full = [scene.bounds(scene.root())];
        let forced = self.force;
        let damage = if self.force {
            &full[..]
        } else {
            &report.damage
        };
        // Native views: split this frame's paint into the planes under and
        // over them; paint moving between planes repaints on both.
        let planned = match &mut self.renderer {
            Some(renderer) if self.native.active() => {
                self.native
                    .plan(&scene, &window, renderer, self.scale_factor as f32)?
            }
            _ => Vec::new(),
        };
        let extended;
        let damage = if planned.is_empty() || self.force {
            damage
        } else {
            extended = [damage, &planned[..]].concat();
            &extended[..]
        };
        if !damage.is_empty() {
            if self.frame_paced && !self.present_ready {
                // Layout/model work proceeds, but defer GPU work until a frame
                // is granted. A full repaint retains damage across scene flushes.
                self.force = true;
                window.request_redraw();
            } else {
                // Steady frames render at a time of their own within their
                // wait for the refresh point (`Latch::hold`).
                #[cfg(target_os = "macos")]
                if let Some((origin, period)) = self.refresh_grid() {
                    let at = self.latch.hold(std::time::Instant::now(), origin, period);
                    sleep_until(at);
                    self.latch.rendering(std::time::Instant::now());
                }
                if let Some(renderer) = &mut self.renderer {
                    zgui_gpu::present_log::log(|| {
                        format!("render forced={forced} rects={}", damage.len())
                    });
                    let stats = renderer.render(&scene, damage)?;
                    self.native.render(&scene, damage)?;
                    if let Some(frame) = &mut self.damage_check {
                        *frame += 1;
                        check_damage(renderer, &scene, damage, &full, size.width as usize, *frame)?;
                    }
                    if let Some(log) = &mut self.gpu_stats {
                        log.record(stats, renderer);
                    }
                    self.presentation.damaged();
                }
                self.force = false;
            }
        }
        if self.presentation.ready(std::time::Instant::now()) && self.renderer.is_some() {
            if self.frame_paced && !self.present_ready {
                // winit withholds RedrawRequested until the compositor grants a
                // new frame. A covered surface must never block vkQueuePresent
                // on the shared UI thread. Keep its rendered target and damage.
                window.request_redraw();
            } else {
                // Steady frames show at a set point in a refresh (`refresh_grid`),
                // waited for once the frame is complete, just before it is
                // handed over.
                #[cfg(target_os = "macos")]
                let grid = self.refresh_grid();
                #[cfg(target_os = "macos")]
                let latch = &mut self.latch;
                // With native views, both planes and the views' geometry
                // change in one transaction (committed when it drops).
                let transaction = self.native.begin();
                self.native.present_overlay()?;
                // Checked above; `self` is borrowed in between, so no `if let`.
                #[allow(clippy::unnecessary_unwrap)]
                let renderer = self.renderer.as_mut().expect("checked above");
                let status = renderer.present_with_notify(|| {
                    window.pre_present_notify();
                    #[cfg(target_os = "macos")]
                    if let Some((origin, period)) = grid {
                        sleep_until(latch.decide(std::time::Instant::now(), origin, period));
                    }
                })?;
                self.native.apply();
                // Commits the transaction (macOS; elsewhere there is none).
                #[allow(dropping_copy_types)]
                drop(transaction);
                #[cfg(target_os = "macos")]
                {
                    self.presented_at = Some(std::time::Instant::now());
                    if let Some(gpu) = renderer.take_gpu_time()
                        && grid.is_some()
                    {
                        self.latch.ran(gpu);
                    }
                }
                if status == zgui_gpu::PresentationStatus::Presented {
                    self.present_ready = false;
                    let now = std::time::Instant::now();
                    self.trim_at = Some(now + TRIM_DELAY);
                    // Startup's garbage (shader compilation, first layout)
                    // is freed once, even if the window never goes idle.
                    if self.startup_trim == StartupTrim::Pending {
                        self.startup_trim = StartupTrim::At(now + STARTUP_TRIM_DELAY);
                    }
                }
                self.presentation
                    .completed(status, std::time::Instant::now());
            }
        }
        // Popups past the window: their own frames, in their own windows
        // (nothing at all while none shows).
        if let Some(renderer) = &self.renderer
            && (self.popups.showing() || !scene.popups().is_empty())
        {
            let full = forced || report.damage.first() == full.first();
            self.popups.update(
                &window,
                &scene,
                &self.context.ui.semantics.borrow(),
                &renderer.context(),
                self.scale_factor as f32,
                full,
                self.context.ui.input.drag_state().as_ref(),
                self.context.ui.input.take_drag_ended(),
            )?;
        }
        drop(scene);
        // Native geometry that changed without zgui pixels to show with it.
        self.native.flush();
        let ui = &self.context.ui;
        let pointer = ui
            .input
            .captured()
            .or(ui.input.hovered())
            .filter(|id| ui.input.is_enabled(&ui.scene.borrow(), *id));
        let role = pointer.and_then(|id| ui.semantics.borrow().get(id).map(|node| node.role));
        let scene = ui.scene.borrow();
        let revision = scene.interaction_revision();
        if self.pointer_inside
            && self
                .cursor_query
                .as_ref()
                .is_none_or(|(point, version, _)| *point != self.cursor || *version != revision)
        {
            self.cursor_query = Some((
                self.cursor,
                revision,
                scene.cursor_at(self.cursor.0, self.cursor.1),
            ));
        }
        let explicit = ui
            .input
            .captured()
            .and_then(|id| scene.contains(id).then(|| scene.cursor_for(id)).flatten())
            .or(self.cursor_query.and_then(|(_, _, cursor)| cursor));
        drop(scene);
        let cursor = explicit
            .map(native_cursor)
            .unwrap_or_else(|| cursor_for_role(role));
        // Over a native view, the view sets the cursor.
        let over_native = self.native.pointer_over_native()
            || ui
                .input
                .hovered()
                .is_some_and(|node| self.native.is_native(node));
        // Over an undecorated window's resize band, the band sets it.
        #[cfg(target_os = "linux")]
        let over_native = over_native || self.resize_edge.is_some();
        #[cfg(target_os = "macos")]
        if !std::mem::replace(&mut self.cursor_rects_off, true) {
            crate::native_host::disable_cursor_rects(&window);
        }
        if over_native {
            self.native_cursor = None;
        } else if Some(cursor) != self.native_cursor {
            // Only when zgui's own choice changes, so at a page's edge it never
            // takes back a cursor the page has just set.
            #[cfg(target_os = "macos")]
            crate::native_host::set_cursor(&window, cursor);
            #[cfg(not(target_os = "macos"))]
            window.set_cursor(cursor);
            self.native_cursor = Some(cursor);
        }
        let editor = crate::native_ime::editable_editor(ui, self.native_focused);
        // The clause an input method converts, else the caret: where its
        // candidates show, as under a native field's marked text.
        let caret = editor.map(|editor| {
            editor
                .composition_area()
                .unwrap_or_else(|| ui.scene.borrow().bounds(editor.caret))
        });
        if caret != self.ime_geometry {
            if let Some(r) = caret {
                // XIM takes a baseline spot and ignores the supplied rectangle
                // height; Wayland/macOS consume the actual caret rectangle.
                let y = if self.x11 { r.y + r.height } else { r.y };
                zgui_gpu::present_log::log(|| format!("ime area {r:?}"));
                window.set_ime_cursor_area(
                    LogicalPosition::new(r.x as f64, y as f64)
                        .to_physical::<f64>(self.scale_factor),
                    LogicalSize::new(r.width.max(1.) as f64, r.height as f64)
                        .to_physical::<f64>(self.scale_factor),
                );
                // winit's X11 sends the spot when it next handles an event.
                #[cfg(target_os = "linux")]
                if self.x11 {
                    crate::x11_nudge::nudge(&window);
                }
            }
            self.ime_geometry = caret;
        }
        self.sync_accessibility();
        Ok(())
    }
    /// X11's and Wayland's primary selection follows what is selected in
    /// the focused editor, published as each selection settles (a pointer
    /// gesture ends, a key changes it). A password field publishes nothing.
    #[cfg(target_os = "linux")]
    fn publish_editor_primary(&mut self) {
        let Some(editor) = self.context.ui.focused_editor() else {
            return;
        };
        let text = editor.copy();
        if text.is_empty() || self.primary.as_deref() == Some(text.as_str()) {
            return;
        }
        if let Some(clipboard) = self.clipboard.borrow_mut().as_mut()
            && clipboard.set_primary_text(text.clone()).is_ok()
        {
            self.primary = Some(text);
        }
    }
    /// A middle press put an editor's caret under the pointer: the primary
    /// selection goes in there, and the editor takes focus.
    #[cfg(target_os = "linux")]
    fn paste_primary(&mut self, node: NodeId) {
        let Some(editor) = self.context.ui.editor(node) else {
            return;
        };
        let text = self
            .clipboard
            .borrow_mut()
            .as_mut()
            .and_then(|clipboard| clipboard.get_primary_text().ok());
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            self.context
                .ui
                .input
                .focus(&self.context.ui.scene, Some(node));
            editor.paste(&text);
        }
    }
    fn clipboard_key(&mut self, key: &Key) -> bool {
        #[cfg(windows)]
        let alias = windows_clipboard_alias(key, self.modifiers);
        #[cfg(not(windows))]
        let alias: Option<&str> = None;
        let k: &str = match (alias, key) {
            (Some(k), _) => k,
            (None, Key::Character(k)) if self.modifiers.primary_shortcut() => k,
            _ => return false,
        };
        // Shift+Delete with nothing selected deletes, as without Shift.
        if alias == Some("x")
            && self
                .context
                .ui
                .focused_editor()
                .is_none_or(|editor| editor.copy().is_empty())
        {
            return false;
        }
        let Some(editor) = self.context.ui.focused_editor() else {
            // Selected display text (`View::selectable`) copies.
            if !k.eq_ignore_ascii_case("c") || !self.context.ui.has_text_selection() {
                return false;
            }
            let (text, html) = (
                self.context.ui.selected_text(),
                self.context.ui.selected_html(),
            );
            if let Some(clipboard) = self.clipboard.borrow_mut().as_mut() {
                let _ = clipboard.set_rich_text(text, html);
            }
            return true;
        };
        if !["c", "x", "v"].iter().any(|v| k.eq_ignore_ascii_case(v)) {
            return false;
        }
        let mut clipboard = self.clipboard.borrow_mut();
        let Some(clipboard) = clipboard.as_mut() else {
            return true;
        };
        if k.eq_ignore_ascii_case("v") {
            if let Ok(t) = clipboard.get_text() {
                editor.paste(&t);
            }
        } else {
            let text = editor.copy();
            if !text.is_empty() && clipboard.set_text(text).is_ok() && k.eq_ignore_ascii_case("x") {
                editor.cut();
            }
        }
        true
    }
    fn accessibility_action(&mut self, request: accesskit::ActionRequest) {
        let Some(node) = self.tree.scene_node(request.target_node) else {
            return;
        };
        if !self.context.ui.scene.borrow().contains(node) {
            return;
        }
        if crate::accessibility::scroll_action(&self.context.ui, node, request.action) {
            return;
        }
        match request.action {
            accesskit::Action::Focus => {
                self.context
                    .ui
                    .input
                    .focus(&self.context.ui.scene, Some(node));
            }
            accesskit::Action::Click => {
                self.context.ui.input.dispatch_to(
                    &self.context.ui.scene,
                    node,
                    InputEvent::Activate,
                );
            }
            accesskit::Action::SetValue => match request.data {
                Some(accesskit::ActionData::Value(value)) => {
                    self.context.ui.set_accessible_value(node, &value);
                }
                Some(accesskit::ActionData::NumericValue(value)) => {
                    self.context.ui.set_accessible_numeric_value(node, value);
                }
                _ => {}
            },
            accesskit::Action::SetTextSelection => {
                if let Some(accesskit::ActionData::SetTextSelection(selection)) = request.data {
                    match self.tree.resolve_selection(&selection) {
                        Some((owner, anchor, focus)) if owner == node => {
                            self.context
                                .ui
                                .set_accessible_text_selection(node, anchor, focus);
                        }
                        // Across a selectable region's labels.
                        _ => {
                            if let Some((anchor, focus)) =
                                self.tree.resolve_document_selection(&selection)
                            {
                                self.context.ui.select_accessible_text(anchor, focus);
                            }
                        }
                    }
                }
            }
            accesskit::Action::Increment => {
                self.context.ui.input.dispatch_to(
                    &self.context.ui.scene,
                    node,
                    InputEvent::Increment,
                );
            }
            accesskit::Action::Decrement => {
                self.context.ui.input.dispatch_to(
                    &self.context.ui.scene,
                    node,
                    InputEvent::Decrement,
                );
            }
            _ => {}
        }
        self.sync_ime_cancellation();
    }
}
impl Host {
    /// A renderer for `window`: on a context another window made, when its
    /// adapter suits the window, else on one of its own, which the app's
    /// fonts are installed in and other windows may share.
    fn make_renderer(&mut self, window: &Arc<Window>) -> Result<GpuRenderer, Box<dyn Error>> {
        let contexts = self.context.windows.gpu_contexts.borrow().clone();
        for context in contexts {
            match self.native.renderer_for(window.clone(), Some(&context)) {
                Ok(renderer) => return Ok(renderer),
                Err(error) if error.is_surface_incompatible() => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let renderer = self.native.renderer_for(window.clone(), None)?;
        for font in self.context.windows.fonts.iter() {
            font.install(&mut renderer.text_system().borrow_mut());
        }
        self.context
            .windows
            .gpu_contexts
            .borrow_mut()
            .push(renderer.context());
        Ok(renderer)
    }
    /// Say in the window that something failed (a contained panic): a
    /// banner over its content, read out by assistive technology, which a
    /// click dismisses; a later failure replaces its text.
    fn show_error(&mut self, message: &str) {
        let text = format!("Something went wrong: {message}");
        if let Some((shown, _)) = &self.error_banner {
            shown.set(text);
        } else {
            let shown = self.context.ui.signal(text);
            let (read, hide, hidden) = (shown.clone(), shown.clone(), shown.clone());
            use zgui::{
                compose::div,
                style::{Styled, Styles},
            };
            let view = self.context.ui.mount(
                div()
                    .absolute()
                    .top(8.)
                    .left(8.)
                    .right(8.)
                    .p(10.)
                    .rounded(8.)
                    .bg(zgui::style::rgb(0x6b2222))
                    .text_color(zgui::style::rgb(0xffffff))
                    .text_size(13.)
                    .live(zgui::semantics::Live::Assertive)
                    .reactive_style(move || {
                        if hidden.get().is_empty() {
                            Styles::new().hidden()
                        } else {
                            Styles::new()
                        }
                    })
                    .on_click(move || {
                        hide.set(String::new());
                    })
                    .child(zgui::compose::text_signal(move || read.get())),
            );
            self.error_banner = Some((shown, view));
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
    /// The GPU device was lost (a driver reset, a GPU removed): a renderer
    /// on a new context, its text and background set up again, and the
    /// window drawn in full. Until one can be made, the window keeps its
    /// state and tries again a second later.
    fn recover_renderer(&mut self) {
        self.renderer_retry = None;
        let Some(window) = self.window.clone() else {
            return;
        };
        self.renderer = None;
        self.context
            .windows
            .gpu_contexts
            .borrow_mut()
            .retain(|context| context.lost().is_none());
        match self.make_renderer(&window) {
            Ok(mut renderer) => {
                renderer.set_scale_factor(self.scale_factor as f32);
                if self.options.is_transparent() {
                    renderer.set_background(zgui::scene::Color(0, 0, 0, 0));
                } else if let Some(background) = self.applied_background {
                    renderer.set_background(background);
                }
                renderer.install_text(&mut self.context.ui.scene.borrow_mut());
                self.context.ui.refresh_text_geometry();
                self.renderer = Some(renderer);
                self.force = true;
                eprintln!("zgui: drawing again on a new GPU renderer");
                window.request_redraw();
            }
            Err(error) => {
                eprintln!("zgui: no GPU renderer yet ({error}); trying again");
                self.renderer_retry =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
            }
        }
    }
    /// Save the window's frame under its `frame_autosave` name (Linux; macOS
    /// and Windows save their own): the frame it had when last neither
    /// maximized nor fullscreen, and whether it is maximized. Fullscreen,
    /// the frame saved before stands.
    fn save_frame(&self) {
        #[cfg(target_os = "linux")]
        if let (Some(window), Some(name), Some(bounds)) = (
            &self.window,
            &self.options.frame_autosave,
            self.options.bounds,
        ) && window.fullscreen().is_none()
        {
            crate::window_place::save(name, bounds, window.is_maximized());
        }
    }
    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        if self.window.is_some() {
            return Ok(());
        }

        let window = Arc::new(event_loop.create_window(crate::window_info::attributes(
            &self.options,
            event_loop,
            #[cfg(target_os = "linux")]
            self.context.windows.application_id.as_deref(),
            #[cfg(target_os = "linux")]
            self.context.windows.activation.borrow_mut().take(),
        )?)?);
        #[cfg(target_os = "macos")]
        {
            content_never_moves_window();
            mouse_down::watch();
        }
        let sized = self.options.bounds;
        let placed = crate::window_place::place(
            &window,
            self.options.under_pointer.take(),
            sized.is_some_and(|bounds| bounds.position.is_some()) || self.options.display.is_some(),
            &self.options,
        );
        // Left maximized, it opens so (`frame_autosave`).
        self.state.maximized |= placed.maximized;
        let radius =
            crate::window_chrome::apply(&window, self.options.titlebar, self.options.glass);
        // A saved frame keeps its size.
        if self.options.titlebar != crate::Titlebar::Standard && !placed.restored {
            // Content now spans the title bar: the window shrinks to give
            // it the size asked for.
            let _ = match sized {
                Some(bounds) => {
                    window.request_inner_size(PhysicalSize::new(bounds.size.0, bounds.size.1))
                }
                None => window
                    .request_inner_size(LogicalSize::new(self.options.width, self.options.height)),
            };
        }
        // X11: the app's icon from the icon theme (Wayland's compositor
        // takes its desktop file's).
        #[cfg(target_os = "linux")]
        if window.display_handle().is_ok_and(|handle| {
            matches!(
                handle.as_raw(),
                RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
            )
        }) {
            crate::linux_icon::apply(&window, self.context.windows.application_id.as_deref());
        }
        // X11 shows a window past its screen; the app's own bounds stand.
        #[cfg(target_os = "linux")]
        if sized.is_none()
            && window.display_handle().is_ok_and(|handle| {
                matches!(
                    handle.as_raw(),
                    RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
                )
            })
        {
            crate::x11_work_area::fit(
                &window,
                self.options.min_size,
                self.options.decorations && self.options.titlebar == crate::Titlebar::Standard,
            );
        }
        self.context.window_info.corner_radius.set(radius);
        // Windows shows a window past its screen; the app's own bounds stand.
        #[cfg(windows)]
        if sized.is_none() {
            crate::window_chrome::fit_work_area(&window, self.options.min_size);
        }
        #[cfg(windows)]
        {
            let proxy = self.proxy.clone();
            let token = self.context.window.token;
            self.caption =
                crate::window_chrome::CaptionHost::new(&window, &self.context.ui, move || {
                    let _ = proxy.send_event(Event::Wake(token));
                });
        }
        #[cfg(target_os = "linux")]
        {
            self.caption = crate::window_chrome::CaptionHost::new(
                &window,
                &self.context.ui,
                self.options.titlebar,
                self.context.window.clone(),
            );
        }
        self.x11 = window.display_handle().is_ok_and(|handle| {
            matches!(
                handle.as_raw(),
                RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
            )
        });
        // X11 drag previews show in windows of their own too, moved by the
        // server as the pointer moves (`PopupHost::follow`).
        #[cfg(target_os = "linux")]
        self.context
            .ui
            .set_popup_previews(self.x11 && crate::popup_window::enabled());
        let adapter = accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        );
        let mut renderer = self.make_renderer(&window)?;
        self.scale_factor = window.scale_factor();
        renderer.set_scale_factor(self.scale_factor as f32);
        let themes = self.context.ui.theme().clone();
        // Glass windows are transparent too (`WindowOptions::is_transparent`).
        if self.options.is_transparent() {
            renderer.set_background(zgui::scene::Color(0, 0, 0, 0));
        } else {
            let background = themes.root().color_untracked(zgui::theme::tok::BACKGROUND);
            renderer.set_background(background);
            self.applied_background = Some(background);
            if self._background_effect.is_none() {
                // The clear colour follows the theme: the effect hands a new
                // background to the next frame and asks for that frame.
                let theme = themes.root().clone();
                let pending = self.theme_background.clone();
                let redraw = window.clone();
                let mut first = true;
                self._background_effect = Some(self.context.ui.runtime.effect(move || {
                    let color = theme.color(zgui::theme::tok::BACKGROUND);
                    if !std::mem::take(&mut first) {
                        pending.set(Some(color));
                        redraw.request_redraw();
                    }
                }));
            }
        }
        if let Some(appearance) = window.theme() {
            themes.set_system_appearance(appearance_of(appearance));
        }
        crate::system_colors::sync(&self.context.ui);
        crate::scrollers::sync(&self.context.ui);
        // A trackpad pushed past an edge stretches it, as AppKit's views do.
        self.context
            .ui
            .set_elastic_scrolling(cfg!(target_os = "macos"));
        renderer.install_text(&mut self.context.ui.scene.borrow_mut());
        self.context.ui.refresh_text_geometry();
        self.renderer = Some(renderer);
        self.adapter = Some(adapter);
        window.set_maximized(self.state.maximized);
        // Not shown yet, an X11 window's state is its own to set.
        #[cfg(target_os = "linux")]
        if self.state.maximized {
            crate::window_place::maximize_hidden(&window);
        }
        if self.options.bounds.is_none() {
            self.options.bounds = Some(crate::WindowBounds::read(&window));
        }
        if self.options.fullscreen {
            let display = self
                .options
                .display
                .as_ref()
                .and_then(|selector| crate::window_info::select_monitor(event_loop, selector));
            window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(display)));
        }
        window.set_visible(self.state.visible);
        if self.state.minimized {
            window.set_minimized(true);
        }
        if self.state.focus_requested {
            window.focus_window();
            self.state.focus_requested = false;
        }
        window.request_redraw();
        self.context.dialogs.attach(Some(&window));
        self.context
            .window_info
            .refresh(&window, self.options.titlebar);
        self.context
            .ui
            .set_popup_screens(crate::popup_window::popup_screens(&window));
        self.content_origin = crate::popup_window::content_origin(&window);
        self.window_number = crate::popup_window::window_number(&window);
        self.frame_paced = matches!(
            window.display_handle().map(|h| h.as_raw()),
            Ok(RawDisplayHandle::Wayland(_))
        );
        self.present_ready = false;
        self.refresh_frame_interval(&window);

        #[cfg(target_os = "linux")]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Wayland(handle) = handle.as_raw()
            {
                // SAFETY: the native window owns this wl_surface while registered.
                let object = unsafe {
                    wayland_backend::sys::client::ObjectId::from_ptr(<wayland_client::protocol::wl_surface::WlSurface as wayland_client::Proxy>::interface(),handle.surface.as_ptr().cast())
                };
                if let Ok(object) = object {
                    let id = object.protocol_id();
                    self.context
                        .windows
                        .file_routes
                        .lock()
                        .unwrap()
                        .insert(id, self.context.window.token);
                    self.file_surface = Some(id);
                }
            }
        }
        self.window = Some(window);
        self.context
            .ui
            .set_presented(self.state.visible && !self.state.minimized && !self.native_occluded);

        self.tree.reset();
        self.last_accessibility = None;
        Ok(())
    }
    fn command(&mut self, command: WindowCommand) {
        self.state.apply(&mut self.options, &command);
        if matches!(command, WindowCommand::Title(_)) {
            self.last_accessibility = None;
        }
        if let WindowCommand::MaxFrameRate(hz) = command {
            self.frames.set_max_rate(hz);
        }
        let Some(window) = &self.window else { return };
        match command {
            WindowCommand::Bounds(bounds) => {
                if let Some((x, y)) = bounds.position {
                    if self.context.window_info.capabilities.get().placement {
                        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                        self.context.window_info.last_request_error.set(None);
                    } else {
                        self.context.window_info.last_request_error.set(Some(
                            "global window placement is unavailable on this backend".into(),
                        ));
                    }
                }
                let minimum = self
                    .options
                    .min_size
                    .map(|(w, h)| LogicalSize::new(w, h).to_physical::<u32>(self.scale_factor));
                let size = minimum.map_or(PhysicalSize::new(bounds.size.0, bounds.size.1), |min| {
                    PhysicalSize::new(bounds.size.0.max(min.width), bounds.size.1.max(min.height))
                });
                let _ = window.request_inner_size(size);
            }
            WindowCommand::Fullscreen(value) => {
                window.set_fullscreen(value.then_some(winit::window::Fullscreen::Borderless(None)))
            }
            WindowCommand::ContextMenu { entries, x, y } => {
                #[cfg(target_os = "macos")]
                {
                    let shown = crate::app_menu::show_context_menu(window, &entries, x, y);
                    self.context
                        .window_info
                        .last_request_error
                        .set(shown.err().map(|e| e.0));
                    // The menu's own tracking took the mouse-up: end the press that opened it.
                    self.context
                        .ui
                        .dispatch(zgui::input::InputEvent::PointerCancel);
                    // And it showed its own cursor: set zgui's again.
                    self.native_cursor = None;
                }
                #[cfg(not(target_os = "macos"))]
                let _ = (entries, x, y);
            }
            WindowCommand::Decorations(value) => window.set_decorations(value),
            WindowCommand::PointerPassthrough(value) => {
                let _ = window.set_cursor_hittest(!value);
            }
            WindowCommand::MinSize(value) => {
                window.set_min_inner_size(value.map(|(w, h)| LogicalSize::new(w, h)))
            }
            WindowCommand::Drag => {
                #[cfg(target_os = "macos")]
                if mouse_down::drag(window) {
                    return;
                }
                self.context
                    .window_info
                    .last_request_error
                    .set(window.drag_window().err().map(|e| e.to_string()));
            }
            WindowCommand::RefreshInfo => self
                .context
                .window_info
                .refresh(window, self.options.titlebar),
            WindowCommand::LoseGpu => {
                if let Some(renderer) = &self.renderer {
                    renderer.context().debug_lose_device();
                }
                window.request_redraw();
            }
            WindowCommand::Title(title) => window.set_title(&title),
            WindowCommand::Size(width, height) => {
                let (width, height) = self
                    .options
                    .min_size
                    .map_or((width, height), |(min_w, min_h)| {
                        (width.max(min_w), height.max(min_h))
                    });
                if let Some(size) = window.request_inner_size(
                    LogicalSize::new(width, height).to_physical::<u32>(self.scale_factor),
                ) && apply_viewport(
                    &mut self.options,
                    &self.context.ui,
                    &self.context.viewport,
                    size,
                    self.scale_factor,
                ) {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(size.width, size.height);
                    }
                    self.force = true;
                    self.presentation.expose();
                }
            }
            WindowCommand::Minimized(value) => {
                if value {
                    self.context.ui.cancel_interactions();
                }
                window.set_minimized(value);
                self.context
                    .ui
                    .set_presented(self.state.visible && !value && !self.native_occluded);
            }
            WindowCommand::Maximized(value) => window.set_maximized(value),
            WindowCommand::Visible(value) => {
                if !value {
                    self.context.ui.cancel_interactions();
                }
                window.set_visible(value);
                self.context
                    .ui
                    .set_presented(value && !self.state.minimized && !self.native_occluded);
                if value {
                    self.presentation.expose();
                    self.force = true;
                    window.request_redraw();
                }
            }
            WindowCommand::Focus => {
                window.focus_window();
                self.state.focus_requested = false;
            }
            // Applies without a native window too: the clock outlives it.
            WindowCommand::MaxFrameRate(_) => {}
        }
    }
    /// The pointer at (`x`, `y`): over an undecorated window's resize band
    /// (Linux), show the resize cursor while the content hears the pointer
    /// leave. Whether it is over one; none while a press is held.
    #[cfg(target_os = "linux")]
    fn resize_band(&mut self, x: f32, y: f32) -> Result<bool, zgui::widgets::LayoutFeedbackError> {
        let input = &self.context.ui.input;
        let held = input.captured().is_some() || input.is_dragging();
        let edge = self
            .caption
            .as_ref()
            .filter(|_| !held)
            .and_then(|caption| caption.edge(x, y, self.scale_factor));
        let was = std::mem::replace(&mut self.resize_edge, edge);
        if was == edge {
            return Ok(edge.is_some());
        }
        if let Some(window) = &self.window {
            match edge {
                Some(direction) => {
                    window.set_cursor(crate::window_chrome::resize_cursor(direction))
                }
                // Back on the content: its cursor, at the next frame.
                None => window.request_redraw(),
            }
        }
        self.native_cursor = None;
        if was.is_none() {
            self.dispatch(InputEvent::PointerLeave)?;
        }
        Ok(edge.is_some())
    }

    /// A press in a `window_drag` region: move the window, or what a double
    /// click on its title bar does.
    fn window_gesture(
        &mut self,
        gesture: zgui::widgets::WindowGesture,
    ) -> Result<(), zgui::widgets::LayoutFeedbackError> {
        use zgui::widgets::WindowGesture;
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        #[cfg(target_os = "macos")]
        {
            // AppKit's own count: zgui's press tracking ends with a drag.
            if gesture == WindowGesture::Menu {
                // No window menu on macOS.
            } else if gesture == WindowGesture::DoubleClick || mouse_down::click_count() == 2 {
                crate::window_chrome::double_click(&window);
            } else if mouse_down::drag(&window) {
                // The window server has the press now: AppKit sends no
                // release for it.
                self.dispatch(InputEvent::PointerCancel)?;
            }
        }
        #[cfg(not(target_os = "macos"))]
        match gesture {
            WindowGesture::Drag => {
                self.context
                    .window_info
                    .last_request_error
                    .set(window.drag_window().err().map(|e| e.to_string()));
            }
            WindowGesture::DoubleClick => crate::window_chrome::double_click(&window),
            WindowGesture::Menu => window.show_window_menu(
                LogicalPosition::new(f64::from(self.cursor.0), f64::from(self.cursor.1))
                    .to_physical::<f64>(self.scale_factor),
            ),
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.context.ui.cancel_interactions();
        self.context.ui.set_window_active(false);
        // A window at the back hears no pointer moves: what it hovered
        // has its leave now, not when the pointer next moves over it.
        self.native.pointer_left();
        self.context
            .ui
            .input
            .dispatch(&self.context.ui.scene, InputEvent::PointerLeave);
        self.native_focused = false;
        self.ime_active = false;
        self.ime_restart = Default::default();
        self.ime_cancellation = Default::default();
        if self.ime_target.take().is_some()
            && let Some(window) = &self.window
        {
            window.set_ime_allowed(false);
            crate::native_ime::discard_native_composition(window);
        }
        self.ime_geometry = None;
        self.modifiers = Modifiers::default();
        self.focus_memory.deactivate(&self.context.ui);
    }
    fn unregister_file_surface(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(id) = self.file_surface.take() {
            self.context.windows.file_routes.lock().unwrap().remove(&id);
        }
    }
    fn suspend(&mut self) {
        self.unregister_file_surface();
        self.context.ui.set_presented(false);
        self.context.window_info.ready.set(false);
        self.context.dialogs.attach(None);
        if let Some(window) = &self.window {
            window.set_visible(false);
            self.popups.hide_all(window);
        }
        self.deactivate();
        self.native_cursor = None;
        self.ime_geometry = None;
        self.ime_target = None;
        self.adapter = None;
        self.renderer = None;
        self.presentation = Presentation::default();
        self.native_occluded = false;
        self.present_ready = false;
        #[cfg(target_os = "macos")]
        {
            self.vsync_synced = None;
            self.vsync_paced = false;
        }
        #[cfg(any(target_os = "macos", windows))]
        {
            self.display_link = None;
        }
        #[cfg(any(windows, target_os = "linux"))]
        {
            self.caption = None;
        }
        self.frame_deadline = None;
        self.frame_heartbeat = None;
        self.frame_redraw_requested = false;
        self.native.suspend();
        // It redraws the window it was made with: the next one makes its own.
        self._background_effect = None;
        self.window = None;
        self.force = true;
    }
    fn window_event(&mut self, event: WindowEvent) -> Result<(), Box<dyn Error>> {
        if matches!(
            event,
            WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) && let Some(window) = &self.window
        {
            self.context
                .window_info
                .refresh(window, self.options.titlebar);
            self.context
                .ui
                .set_popup_screens(crate::popup_window::popup_screens(window));
            self.content_origin = crate::popup_window::content_origin(window);
            let window = window.clone();
            self.refresh_frame_interval(&window);
            let window = &window;
            // Winit's macOS is_maximized temporarily adds titled/resizable
            // styles to other windows. Querying it during resize creates another
            // resize event indefinitely. Those windows have no native zoom control;
            // use our explicit maximize command state without mutating AppKit.
            let maximized = read_maximized_without_style_change(
                self.options.decorations,
                self.options.resizable,
                self.state.maximized,
                || window.is_maximized(),
            );
            // Into or out of fullscreen: its popups close, as AppKit's do.
            let fullscreen = window.fullscreen().is_some();
            if std::mem::replace(&mut self.popups_fullscreen, fullscreen) != fullscreen {
                self.context.ui.dismiss_popups();
            }
            if !fullscreen && !maximized {
                let bounds = crate::WindowBounds::read(window);
                if bounds.size.0 > 0 && bounds.size.1 > 0 {
                    self.options.bounds = Some(bounds);
                }
            }
            #[cfg(target_os = "linux")]
            if self.options.frame_autosave.is_some() {
                self.frame_save_at = Some(std::time::Instant::now() + FRAME_SAVE_DELAY);
            }
        }
        self.sync_ime_cancellation();
        if let (Some(adapter), Some(window)) = (&mut self.adapter, &self.window) {
            adapter.process_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => {}
            WindowEvent::Occluded(true) => {
                // Minimized, or on a Space no longer shown: its menus and
                // popovers close, as AppKit's do (they would show again
                // with it, still open).
                self.context.ui.dismiss_popups();
                self.context.ui.cancel_interactions();
                self.native_occluded = true;
                self.context.ui.set_presented(false);
                self.presentation.occlude();
            }
            WindowEvent::Occluded(false) => {
                self.native_occluded = false;
                self.context
                    .ui
                    .set_presented(self.state.visible && !self.state.minimized);
                self.presentation.expose();
                // macOS keeps showing the layer's last frame: what changed
                // while covered is the scene's damage.
                self.force |= !cfg!(target_os = "macos");
            }
            WindowEvent::RedrawRequested => {
                self.present_ready = true;
                self.presentation.expose();
                // Other platforms may have lost the window's contents; on
                // macOS a redraw AppKit asks for (as the window first shows)
                // repaints only what changed.
                self.force |= !cfg!(target_os = "macos");
                if std::mem::take(&mut self.frame_redraw_requested) {
                    // Wayland: the compositor granted this frame. Let animations
                    // step before painting it.
                    let now = std::time::Instant::now();
                    let mut frame = self.frame_timer.frame(now, self.frame_interval);
                    frame.time = now + self.frame_interval;
                    self.deliver_frame(frame);
                    if self.tasks.has_ready() {
                        self.tasks.tick();
                    }
                }
                self.draw()?;
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.presentation.expose();
                }
                let scale = self.scale_factor;
                let viewport = self.context.viewport.with_untracked(|viewport| *viewport);
                if apply_viewport(
                    &mut self.options,
                    &self.context.ui,
                    &self.context.viewport,
                    size,
                    scale,
                ) && (self
                    .renderer
                    .as_mut()
                    .is_some_and(|renderer| renderer.resize(size.width, size.height))
                    || self.context.viewport.with_untracked(|v| *v != viewport))
                {
                    // AppKit reports the size again as a window shows and as
                    // it reaches its display: the same size repaints nothing.
                    #[cfg(target_os = "macos")]
                    {
                        self.resized_at = Some(std::time::Instant::now());
                    }
                    self.force = true;
                    // AppKit delivers live resizes synchronously and commits the
                    // new window frame when this callback returns. Paint now so
                    // the matching frame joins that commit instead of trailing it.
                    if cfg!(target_os = "macos") {
                        self.draw()?;
                    }
                }
            }
            // Reported as a window first reaches its display, unchanged.
            WindowEvent::ScaleFactorChanged { scale_factor, .. }
                if scale_factor == self.scale_factor => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // This event is authoritative. Some X11 XSETTINGS paths leave
                // Window::scale_factor() stale for an existing native window.
                if self.x11 {
                    let ratio = (self.scale_factor / scale_factor) as f32;
                    self.cursor.0 *= ratio;
                    self.cursor.1 *= ratio;
                }
                self.scale_factor = scale_factor;
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_scale_factor(scale_factor as f32);
                }
                // The native resize follows this notification. Applying the
                // new scale to the old physical size creates a false logical resize.
                self.context.ui.refresh_text_geometry();
                self.ime_geometry = None;
                self.force = true;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.modifiers = Modifiers {
                    shift: s.shift_key(),
                    control: s.control_key(),
                    alt: s.alt_key(),
                    meta: s.super_key(),
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(self.scale_factor);
                self.cursor = (p.x, p.y);

                self.pointer_inside = true;
                // An undecorated window's edge resizes it: the content
                // hears the pointer leave, and nothing more until it is
                // back inside the bands.
                #[cfg(target_os = "linux")]
                if self.resize_band(p.x, p.y)? {
                    return Ok(());
                }
                // Over a native view taking input zgui hears one leave, then
                // nothing until the pointer is back (unless a press of
                // zgui's follows it there).
                let input = &self.context.ui.input;
                let held = input.captured().is_some() || input.is_dragging();
                match self.native.pointer_moved(p.x, p.y, held) {
                    PointerStep::Move => {
                        self.dispatch(InputEvent::PointerMove { x: p.x, y: p.y })?
                    }
                    PointerStep::Leave(_) => self.dispatch(InputEvent::PointerLeave)?,
                    PointerStep::Over(_) => {}
                }
                if self.context.ui.input.is_dragging() {
                    self.drag_moved();
                }
            }
            // Back from a popup window (X11): where the pointer is, which
            // winit does not say.
            #[cfg(target_os = "linux")]
            WindowEvent::CursorEntered { device_id } => {
                if let Some(position) = self
                    .window
                    .as_deref()
                    .and_then(crate::popup_window::pointer)
                {
                    return self.window_event(WindowEvent::CursorMoved {
                        device_id,
                        position,
                    });
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer_inside = false;
                #[cfg(target_os = "linux")]
                {
                    self.resize_edge = None;
                }
                // Outside, others set the cursor: back in, zgui sets its own
                // again rather than trust what it last set.
                self.native_cursor = None;
                self.native.pointer_left();
                self.dispatch(InputEvent::PointerLeave)?
            }
            WindowEvent::HoveredFile(path) => self.dispatch(InputEvent::FileHover {
                x: self.cursor.0,
                y: self.cursor.1,
                path,
            })?,
            WindowEvent::DroppedFile(path) => {
                self.pending_file_drops.push(path.clone());
                self.dispatch(InputEvent::FileDrop {
                    x: self.cursor.0,
                    y: self.cursor.1,
                    path,
                })?;
            }
            WindowEvent::HoveredFileCancelled => self.dispatch(InputEvent::FileHoverCancelled)?,
            WindowEvent::MouseInput { state, button, .. } => {
                #[cfg(windows)]
                {
                    self.touch_last = false;
                }
                let button = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    MouseButton::Back => PointerButton::Other(4),
                    MouseButton::Forward => PointerButton::Other(5),
                    MouseButton::Other(n) => PointerButton::Other(n),
                };
                // Back from a popup window, the pointer sends no move to
                // where winit last saw it (X11; its enter does not count):
                // a press in a window that had popups asks where it is.
                #[cfg(target_os = "linux")]
                if self.had_popups
                    && let Some(position) = self
                        .window
                        .as_deref()
                        .and_then(crate::popup_window::pointer)
                {
                    self.window_event(WindowEvent::CursorMoved {
                        device_id: winit::event::DeviceId::dummy(),
                        position,
                    })?;
                }
                let (x, y) = self.cursor;
                // A click stops a coasting scroll, as in a browser.
                if state == ElementState::Pressed {
                    self.touch.stop();
                }
                #[cfg(target_os = "linux")]
                if let Some(direction) = self.resize_edge
                    && state == ElementState::Pressed
                    && button == PointerButton::Primary
                    && let Some(window) = &self.window
                {
                    let _ = window.drag_resize_window(direction);
                    return Ok(());
                }
                self.dispatch(if state == ElementState::Pressed {
                    InputEvent::PointerDown { x, y, button }
                } else {
                    InputEvent::PointerUp { x, y, button }
                })?;
                #[cfg(target_os = "linux")]
                {
                    if let Some(node) = self.context.ui.take_primary_paste() {
                        self.paste_primary(node);
                    }
                    if state == ElementState::Released {
                        self.publish_editor_primary();
                    }
                }
                // A press in a `window_drag` region.
                if let Some(gesture) = self.context.ui.take_window_gesture() {
                    self.window_gesture(gesture)?;
                }
            }
            WindowEvent::Touch(touch) => {
                let p = touch.location.to_logical::<f32>(self.scale_factor);
                self.cursor = (p.x, p.y);
                let pen = crate::touch::is_pen(touch.id);
                #[cfg(windows)]
                {
                    self.touch_last = true;
                }
                if touch.phase == winit::event::TouchPhase::Started && self.touch.idle() {
                    self.touch_focus = self.context.ui.input.focused();
                }
                let steps = self.touch.touch(
                    touch.id,
                    touch.phase,
                    (p.x, p.y),
                    pen,
                    std::time::Instant::now(),
                );
                self.touch_steps(steps)?;
            }
            WindowEvent::PinchGesture { delta, .. } if delta.is_finite() && delta != 0. => {
                self.dispatch(InputEvent::Pinch {
                    x: self.cursor.0,
                    y: self.cursor.1,
                    delta: delta as f32,
                })?;
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                // A trackpad gesture's phase: its start goes before its
                // motion, its end after all of it.
                let gesture = if cfg!(target_os = "macos")
                    && matches!(delta, MouseScrollDelta::PixelDelta(_))
                {
                    self.trackpad.phase(phase, std::time::Instant::now())
                } else {
                    None
                };
                if let Some(
                    gesture @ (zgui::input::ScrollGesture::Began
                    | zgui::input::ScrollGesture::MomentumBegan),
                ) = gesture
                {
                    self.dispatch(InputEvent::ScrollGesture(gesture))?;
                }
                let step = crate::scrollers::wheel_step(
                    self.context.viewport.with_untracked(|viewport| *viewport),
                );
                let (dx, dy) = wheel_delta(delta, self.scale_factor, step);
                // A touchpad's scroll coasts on after the fingers lift
                // (Wayland says when: libinput's axis stop).
                #[cfg(target_os = "linux")]
                if matches!(delta, MouseScrollDelta::PixelDelta(_)) {
                    self.touch
                        .wheel(phase, self.cursor, (dx, dy), std::time::Instant::now());
                }
                if crate::scroll_resample::enabled() {
                    // Applied on display refreshes; see `update`.
                    let now = std::time::Instant::now();
                    self.scroll.position = self.cursor;
                    match delta {
                        MouseScrollDelta::PixelDelta(_) => self.scroll.push_precise(now, (dx, dy)),
                        MouseScrollDelta::LineDelta(x, y) if continuous_lines(x, y) => {
                            self.scroll.push_precise(now, (dx, dy))
                        }
                        MouseScrollDelta::LineDelta(..) => self.scroll.push_wheel(now, (dx, dy)),
                    }
                } else {
                    self.dispatch(InputEvent::Scroll {
                        x: self.cursor.0,
                        y: self.cursor.1,
                        delta_x: dx,
                        delta_y: dy,
                    })?;
                }
                if let Some(
                    gesture @ (zgui::input::ScrollGesture::Ended
                    | zgui::input::ScrollGesture::MomentumEnded),
                ) = gesture
                {
                    // What the refreshes have not applied yet, first.
                    if let Some((delta_x, delta_y)) = self.scroll.flush() {
                        let (x, y) = self.scroll.position;
                        self.dispatch(InputEvent::Scroll {
                            x,
                            y,
                            delta_x,
                            delta_y,
                        })?;
                    }
                    self.dispatch(InputEvent::ScrollGesture(gesture))?;
                }
            }
            WindowEvent::Focused(false) => {
                zgui_gpu::present_log::log(|| "window-focus false".into());
                #[cfg(target_os = "linux")]
                if let Some(caption) = &mut self.caption {
                    caption.sync(&mut self.context.ui);
                }
                // Menus and popovers close as their window or the app
                // loses focus, as AppKit's do.
                self.context.ui.dismiss_popups();
                // A drag the platform has goes on: it deactivates as that
                // ends (`Handoff`).
                if self.native_drag.deactivate() {
                    self.deactivate()
                }
            }
            WindowEvent::ThemeChanged(appearance) => {
                self.context
                    .ui
                    .theme()
                    .set_system_appearance(appearance_of(appearance));
                crate::system_colors::sync(&self.context.ui);
                crate::scrollers::sync(&self.context.ui);
                #[cfg(windows)]
                if let Some(window) = &self.window {
                    crate::window_chrome::menu_theme(window);
                }
            }
            WindowEvent::Focused(true) => {
                // The user may have changed the accent while it was at the back.
                crate::system_colors::sync(&self.context.ui);
                crate::scrollers::sync(&self.context.ui);
                zgui_gpu::present_log::log(|| "window-focus true".into());
                // AppKit may have reset the cursor while another app had it.
                self.native_cursor = None;
                self.native_focused = true;
                self.native_drag.activate();
                self.focus_memory.activate(&self.context.ui);
                self.context.ui.set_window_active(true);
                #[cfg(target_os = "linux")]
                if let Some(caption) = &mut self.caption {
                    caption.sync(&mut self.context.ui);
                }
            }
            WindowEvent::Ime(event) => {
                if self.ime_restart.accepts(&event)
                    && let Some(composing) = crate::native_ime::dispatch(
                        &mut self.context.ui,
                        self.native_focused,
                        self.ime_target,
                        event,
                    )?
                {
                    self.ime_active = composing;
                }
            }
            // Winit synthesizes held-key presses/releases on X11 focus changes.
            // FocusMemory already cancels pressed controls; these are not user actions.
            WindowEvent::KeyboardInput {
                is_synthetic: true, ..
            } => {}
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::KeyI)
                    && inspector_shortcut(self.modifiers) =>
            {
                if !event.repeat {
                    self.context.inspector.toggle();
                    self.force = true;
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(&event.logical_key)
                    .map(|key| shortcut_key(key, event.physical_key, self.modifiers));
                let mut prevented = false;
                if let Some(key) = key
                    && !self.ime_active
                {
                    // The physical key, as the DOM's `KeyboardEvent.code`.
                    self.context
                        .ui
                        .input
                        .set_key_code(match event.physical_key {
                            PhysicalKey::Code(KeyCode::SuperLeft) => Some("MetaLeft".into()),
                            PhysicalKey::Code(KeyCode::SuperRight) => Some("MetaRight".into()),
                            PhysicalKey::Code(code) => Some(format!("{code:?}").into()),
                            PhysicalKey::Unidentified(_) => None,
                        });
                    if event.state == ElementState::Pressed {
                        // Windows' Shift+Delete cuts a selection, before the
                        // editor would only delete it.
                        #[cfg(windows)]
                        let cut = windows_clipboard_alias(&key, self.modifiers) == Some("x")
                            && self.clipboard_key(&key);
                        #[cfg(not(windows))]
                        let cut = false;
                        prevented = cut
                            || self.dispatch_prevented(InputEvent::KeyDown {
                                key: key.clone(),
                                modifiers: self.modifiers,
                                repeat: event.repeat,
                            })?;
                        if !prevented {
                            prevented = self.clipboard_key(&key);
                        }
                        #[cfg(target_os = "linux")]
                        self.publish_editor_primary();
                    } else {
                        self.dispatch(InputEvent::KeyUp {
                            key,
                            modifiers: self.modifiers,
                        })?;
                    }
                    self.context.ui.input.set_key_code(None);
                }
                if event.state == ElementState::Pressed
                    && (!prevented || self.context.ui.input.has_pending_keys())
                    && !self.modifiers.control
                    && !self.modifiers.meta
                    && !self.ime_active
                    && let Some(text) = event.text
                    && !text.chars().any(char::is_control)
                {
                    self.dispatch(InputEvent::Text(text.to_string()))?;
                }
            }
            _ => {}
        }

        Ok(())
    }
    /// The window's drag moved: a preview in a window of its own moves with
    /// the pointer, drawn once.
    fn drag_moved(&mut self) {
        let (Some(drag), Some(window)) = (self.context.ui.input.drag_state(), &self.window) else {
            return;
        };
        // X11 leaves the preview the app's to move, its drag the platform's
        // or not.
        if !self.native_drag.active() || cfg!(target_os = "linux") {
            self.popups.follow(window, &drag);
        }
    }
    /// Its drag left the app's windows: carrying what other apps take
    /// (`View::drag_export`), it becomes the platform's.
    fn hand_off_drag(&mut self, drag: &zgui::input::DragState) {
        let Some(window) = self.window.clone() else {
            return;
        };
        // On Wayland a drag carrying nothing to other apps (a tab) is handed
        // off too, so the app's other windows hear it (`native_drag::begin`):
        // its windows have no place on screen to find them by.
        #[cfg(target_os = "linux")]
        let internal = window
            .display_handle()
            .is_ok_and(|handle| matches!(handle.as_raw(), RawDisplayHandle::Wayland(_)));
        #[cfg(not(target_os = "linux"))]
        let internal = false;
        let items: &[zgui::input::DragItem] = match &drag.export {
            Some(items) => items,
            None if internal => &[],
            None => return,
        };
        if self.native_drag.active() {
            return;
        }
        let image = self.popups.preview_image(drag);
        // Wayland's icon: the preview, which shows in the window there,
        // drawn alone.
        #[cfg(target_os = "linux")]
        let image = image.or_else(|| self.drawn_preview(drag));
        let (proxy, token) = (self.proxy.clone(), self.context.window.token);
        let send = move |drag| {
            let _ = proxy.send_event(Event::NativeDrag(token, drag));
        };
        #[cfg(target_os = "linux")]
        let begun =
            crate::native_drag::begin(&window, items, image, (drag.x as f64, drag.y as f64), send);
        #[cfg(not(target_os = "linux"))]
        let begun = crate::native_drag::begin(&window, items, image, send);
        if begun {
            zgui_gpu::present_log::log(|| "native drag begin".into());
            self.native_drag.begun();
            self.popups.show_preview(&window, drag, None);
        }
    }
    /// A drag moving the window (`View::drag_moves_window`): the window
    /// follows the pointer, the point it was held at staying under it.
    fn move_with_drag(&mut self) {
        let input = &self.context.ui.input;
        if !input.drag_moves_window() {
            return;
        }
        let Some(drag) = input.drag_state() else {
            return;
        };
        // No place on screen (Wayland): the compositor moves the window
        // from here (`xdg_toplevel.move`, as a title bar's drag), holding
        // the pointer until it is let go, so zgui's drag ends now.
        #[cfg(target_os = "linux")]
        if self.content_origin.is_none()
            && let Some(window) = self.window.clone()
            && window
                .display_handle()
                .is_ok_and(|handle| matches!(handle.as_raw(), RawDisplayHandle::Wayland(_)))
        {
            let _ = window.drag_window();
            self.context.ui.cancel_interactions();
            return;
        }
        let (Some(window), Some((left, top))) = (&self.window, self.content_origin) else {
            return;
        };
        let (grab_x, grab_y) = *self.window_grab.get_or_insert((drag.x, drag.y));
        // Its content's top-left goes to the pointer less the grab point.
        let (x, y) =
            crate::window_place::pointer().unwrap_or((left + drag.x as f64, top + drag.y as f64));
        let to = (x - grab_x as f64, y - grab_y as f64);
        if to == (left, top) {
            return;
        }
        crate::window_place::move_to(window, to, &mut self.window_pending);
        self.content_origin = Some(to);
    }
    /// The drag's preview drawn alone, offscreen: a drag image where the
    /// preview shows in its window (Wayland). Premultiplied BGRA at the
    /// window's scale, with the pointer's place in it.
    #[cfg(target_os = "linux")]
    fn drawn_preview(
        &self,
        drag: &zgui::input::DragState,
    ) -> Option<crate::native_drag::DragImage> {
        let root = drag.preview?;
        let context = self.renderer.as_ref()?.context();
        let scene = self.context.ui.scene.borrow();
        let bounds = scene.bounds(root);
        let scale = self.scale_factor as f32;
        let (width, height) = (
            (bounds.width * scale).ceil().max(1.) as u32,
            (bounds.height * scale).ceil().max(1.) as u32,
        );
        let mut renderer = GpuRenderer::new_with_context(width, height, &context).ok()?;
        renderer.set_scale_factor(scale);
        renderer.set_popup(root, bounds);
        renderer.render(&scene, &[]).ok()?;
        let mut bgra = renderer.readback().ok()?;
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Some(crate::native_drag::DragImage {
            width,
            height,
            bgra,
            scale,
            grab: (drag.x - bounds.x, drag.y - bounds.y),
        })
    }
    /// Its content's box in global coordinates.
    fn content_box(&self) -> Option<[f64; 4]> {
        let (left, top) = self.content_origin?;
        let (width, height) = self.context.viewport.with_untracked(|viewport| *viewport);
        Some([left, top, width as f64, height as f64])
    }
    /// The platform's drag moved or ended, at a global point: the window's
    /// pointer, which the platform reports to its session meanwhile. Back
    /// over the window, its destinations hear the drag; released there, it
    /// drops on them. Taken by another app, it ends accepted; else its
    /// preview shows where it was let go, to slide back from there.
    fn native_drag_event(
        &mut self,
        event: crate::native_drag::NativeDrag,
    ) -> Result<(), Box<dyn Error>> {
        use crate::native_drag::NativeDrag;
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        let (x, y, ended) = match event {
            NativeDrag::Moved { x, y } => (x, y, None),
            NativeDrag::Ended { x, y, accepted } => (x, y, Some(accepted)),
            // Over another of the app's windows: that window's to hear
            // (`WindowManager::native_drag`).
            NativeDrag::MovedIn { .. } => return Ok(()),
            // Dropped in another window (the manager dropped it there), or
            // let go past every window: the platform's part ends untaken.
            NativeDrag::DroppedIn { x, y, .. } | NativeDrag::Left { x, y } => (x, y, Some(false)),
        };
        let (left, top) = crate::popup_window::content_origin(&window).unwrap_or((0., 0.));
        let at = ((x - left) as f32, (y - top) as f32);
        let device_id = winit::event::DeviceId::dummy();
        let position = LogicalPosition::new(at.0, at.1).to_physical(self.scale_factor);
        let Some(taken) = ended else {
            return self.window_event(WindowEvent::CursorMoved {
                device_id,
                position,
            });
        };
        let deactivate = self.native_drag.ended();
        if let Some(drag) = self.context.ui.input.drag_state() {
            if taken {
                let ui = &self.context.ui;
                ui.input.end_drag_elsewhere(&ui.scene, true, false);
            } else {
                self.popups.show_preview(&window, &drag, Some(at));
            }
        }
        // The session took the button's release.
        self.window_event(WindowEvent::CursorMoved {
            device_id,
            position,
        })?;
        let released = self.window_event(WindowEvent::MouseInput {
            device_id,
            state: ElementState::Released,
            button: MouseButton::Left,
        });
        // It lost focus meanwhile (the app taking it came to the front).
        if deactivate {
            self.deactivate();
        }
        released
    }
    /// The platform's drag ended after zgui's had (its source went): the
    /// window's next drag hands off again.
    fn native_drag_gone(&mut self) {
        if self.native_drag.ended() {
            self.deactivate();
        }
    }
    /// Pointer input over a popup window, as the window's own (its point in
    /// the window's coordinates, past its edges).
    fn popup_input(
        &mut self,
        input: crate::popup_window::PopupInput,
    ) -> Result<(), Box<dyn Error>> {
        use crate::popup_window::PopupInput;
        // Winit's placeholder device, for events it did not report.
        let device_id = winit::event::DeviceId::dummy();
        let at = |x: f32, y: f32| LogicalPosition::new(x, y).to_physical(self.scale_factor);
        match input {
            PopupInput::Moved { x, y } => {
                let moved = self.window_event(WindowEvent::CursorMoved {
                    device_id,
                    position: at(x, y),
                });
                #[cfg(target_os = "linux")]
                {
                    self.had_popups = true;
                }
                moved
            }
            PopupInput::Button {
                x,
                y,
                button,
                pressed,
            } => {
                self.cursor = (x, y);
                self.window_event(WindowEvent::MouseInput {
                    device_id,
                    state: if pressed {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
                    button,
                })
            }
            PopupInput::Wheel { x, y, delta, phase } => {
                self.cursor = (x, y);
                self.window_event(WindowEvent::MouseWheel {
                    device_id,
                    delta,
                    phase,
                })
            }
            PopupInput::Left => self.window_event(WindowEvent::CursorLeft { device_id }),
            PopupInput::Dismiss => {
                self.context.ui.dismiss_popups();
                Ok(())
            }
            PopupInput::Screens => {
                if let Some(window) = &self.window {
                    self.context
                        .ui
                        .set_popup_screens(crate::popup_window::popup_screens(window));
                }
                Ok(())
            }
        }
    }
    fn accessibility_event(&mut self, event: accesskit_winit::WindowEvent) {
        match event {
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                self.accessibility_action(request)
            }
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                self.tree.reset();
                self.last_accessibility = None;
                self.sync_accessibility();
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }
    fn update(&mut self) -> Result<bool, Box<dyn Error>> {
        // Native views' reports (navigation, script replies), and keyboard
        // focus they took or gave back, at this safe point.
        zgui::native_view::deliver_posted();
        // Moves zgui heard through the native views' pointer monitor.
        if let Some(pointer) = self.native.take_unheard_pointer() {
            self.cursor = pointer;
            self.pointer_inside = true;
        }
        self.native.sync_focus(&self.context.ui);
        self.poll_frame_deadline(std::time::Instant::now());
        if self.scroll.pending() {
            // Each refresh applies the motion due by then; with no refreshes
            // coming (hidden, minimized), everything applies at once.
            let delta = if !self.frames_presented() {
                self.scroll.flush()
            } else if self.frame_arrived {
                self.scroll.sample(std::time::Instant::now())
            } else {
                None
            };
            if let Some((delta_x, delta_y)) = delta {
                let (x, y) = self.scroll.position;
                self.dispatch(InputEvent::Scroll {
                    x,
                    y,
                    delta_x,
                    delta_y,
                })?;
            }
        }
        // A finger held on selectable text selects the word there.
        if let Some((x, y)) = self.touch.hold(std::time::Instant::now())
            && self.context.ui.select_word_at(x, y)
        {
            self.touch.selecting();
            // A link or button under the finger gives the press up now, so
            // lifting there selects rather than clicks.
            self.dispatch(InputEvent::PointerMove { x, y })?;
        }
        // A fling coasts a step each refresh, and stops at the end of what
        // it scrolls.
        if self.touch.flinging()
            && (self.frame_arrived || !self.frames_presented())
            && let Some((x, y, delta_x, delta_y)) = self.touch.coast(std::time::Instant::now())
            && !self.dispatch_prevented(InputEvent::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            })?
        {
            self.touch.stop();
        }
        if let StartupTrim::At(at) = self.startup_trim
            && std::time::Instant::now() >= at
        {
            self.startup_trim = StartupTrim::Done;
            release_freed_heap();
        }
        if self
            .frame_save_at
            .is_some_and(|at| std::time::Instant::now() >= at)
        {
            self.frame_save_at = None;
            self.save_frame();
        }
        if self
            .trim_at
            .is_some_and(|at| std::time::Instant::now() >= at)
        {
            self.trim_at = None;
            if let Some(renderer) = &mut self.renderer {
                renderer.trim();
            }
            self.native.trim();
            release_freed_heap();
        }
        loop {
            let task = self.pending.borrow_mut().pop_front();
            let Some(task) = task else {
                break;
            };
            self.tasks.spawn(task);
        }
        if self.tasks.has_ready() {
            self.tasks.tick();
        }
        #[cfg(any(windows, target_os = "linux"))]
        if let Some(caption) = &mut self.caption {
            caption.sync(&mut self.context.ui);
        }
        // A key typed while frames follow the refresh draws at once: waiting
        // for the next refresh's frame cost it a refresh (8 ms at 120 Hz)
        // more than at rest. The newest frame is the one shown.
        #[cfg(target_os = "macos")]
        let paced = !std::mem::take(&mut self.input_arrived)
            && (self.vsync_paced
                || self
                    .display_link
                    .as_ref()
                    .is_some_and(|link| link.running()));
        // Input draws at once: presents replace one another until the
        // compositor takes the newest (`present_mode`), so a key typed
        // mid-refresh shows on the next, as it does at rest.
        #[cfg(windows)]
        let paced = !std::mem::take(&mut self.input_arrived)
            && self
                .display_link
                .as_ref()
                .is_some_and(|link| link.running());
        #[cfg(not(any(target_os = "macos", windows)))]
        let paced = false;
        // Scene damage accumulates until the refresh paints it in one frame.
        if !paced || self.frame_arrived || self.force {
            self.frame_arrived = false;
            self.draw()?;
        }
        self.sync_frames();
        Ok(self.tasks.has_ready() || !self.pending.borrow().is_empty())
    }
    fn frames_presented(&self) -> bool {
        self.window.is_some()
            && self.state.visible
            && !self.state.minimized
            && !self.native_occluded
    }
    fn refresh_frame_interval(&mut self, window: &Window) {
        self.frame_interval = crate::frame_source::interval_for(
            window
                .current_monitor()
                .as_ref()
                .and_then(crate::window_info::refresh_rate_hz),
        );
    }
    /// Deliver one display refresh. Hidden windows keep their requests pending.
    fn deliver_frame(&mut self, frame: Frame) {
        #[cfg(any(target_os = "macos", windows))]
        if let Some(link) = &self.display_link {
            link.consumed();
        }
        self.frame_arrived = true;
        self.frame_deadline = None;
        self.frame_heartbeat = Some(std::time::Instant::now());
        if self.frames_presented() {
            self.frames.deliver(frame);
        }
    }
    fn poll_frame_deadline(&mut self, now: std::time::Instant) {
        if self.frame_deadline.is_some_and(|deadline| now >= deadline) {
            let frame = self.frame_timer.frame(now, self.frame_interval);
            self.deliver_frame(frame);
        }
    }
    /// The display's refresh grid (a refresh's start, and the period) for
    /// handing over a steady frame at a set point before a refresh, where the
    /// window server reliably shows it on that refresh (`Latch::decide`).
    ///
    /// Frames an app's own timer produces (a 60 Hz stream, say) are ready at
    /// whatever phase that timer has against the display, fixed for as long
    /// as both run. Handed over at once, those near the window server's
    /// latch make a refresh or miss it by chance, so showing every other
    /// refresh becomes one refresh, then three: in captures of a window at
    /// 120 Hz, 1–5% of intervals, and in some runs 20%. Handed over about
    /// 2 ms before a refresh shows (`Latch::LEAD`, on the display's exact
    /// refresh grid: see `DisplayLink::refresh_phase`), none in 99.9%; 2 ms
    /// or more away from that, runs again miss. Each frame keeps the last one's cadence (whole
    /// refreshes after it) while that stays ahead, so one ready near a point
    /// does not alternate between it and the next. Frames the link paces
    /// start at a refresh and are left alone.
    #[cfg(target_os = "macos")]
    fn refresh_grid(&mut self) -> Option<(std::time::Instant, std::time::Duration)> {
        let now = std::time::Instant::now();
        if self
            .resized_at
            .is_some_and(|at| now.duration_since(at) < RESIZE_SETTLE)
        {
            self.latch = Latch::default();
            return None;
        }
        let steady = self
            .presented_at
            .is_some_and(|at| now.duration_since(at) < STEADY_GAP);
        let refresh = self
            .display_link
            .as_ref()
            .filter(|_| steady && !self.link_paced)
            .and_then(|link| link.refresh_phase(now, crate::frame_source::VSYNC_RESYNC * 3));
        let Some((phase, period)) = refresh else {
            self.latch = Latch::default();
            return None;
        };
        Some((now - phase, period))
    }
    /// Start, stop or schedule this window's frame source to match demand.
    fn sync_frames(&mut self) {
        let now = std::time::Instant::now();
        let scrolling = self.scroll.active(now) || self.touch.flinging();
        let wanted = (self.frames.wants_frame() || scrolling) && self.frames_presented();
        if !wanted {
            self.frame_heartbeat = None;
            self.frame_deadline = None;
        }
        // Steady frames of the app's own timing present against the refresh
        // grid (`refresh_grid`), which the link reports.
        #[cfg(target_os = "macos")]
        let steady = self
            .presented_at
            .is_some_and(|at| now.duration_since(at) < STEADY_GAP);
        #[cfg(target_os = "macos")]
        if (wanted || steady)
            && self.display_link.is_none()
            && let Some(window) = &self.window
        {
            // Created on first demand, so windows that never animate hold none.
            let proxy = self.proxy.clone();
            let token = self.context.window.token;
            self.display_link = crate::frame_source::DisplayLink::new(window, move |frame| {
                let _ = proxy.send_event(Event::Frame(token, frame));
            });
        }
        #[cfg(target_os = "macos")]
        if let Some(link) = &mut self.display_link {
            let refresh = 1. / self.frame_interval.as_secs_f64();
            // Scrolling and other input run at the display's full rate: frames
            // paced to a reduced rate (animations that asked for one) would
            // otherwise hold what input changed until their next tick, a drag
            // shown at 12 fps up to 80 ms late.
            let inputting = self.input_until.is_some_and(|until| now < until);
            let rate = self
                .frames
                .demanded_rate(refresh)
                .filter(|rate| !scrolling && !inputting && *rate < refresh - 0.5);
            // Animating at the display's rate, the link paces every frame:
            // locked to the display, frames reach the window server one a
            // refresh, where a timer's wake-ups wander by milliseconds and
            // leave some refreshes without a new frame. Otherwise (slower
            // updates, scrolling) a timer paces them on the grid the link last
            // reported, and the link (a thread spinning to each refresh) runs
            // again only to resync.
            let animating = wanted && rate.is_none() && !scrolling;
            self.link_paced = animating;
            if let Some(renderer) = &mut self.renderer {
                renderer.set_full_rate(wanted && rate.is_none());
            }
            let synced = !animating
                && self
                    .vsync_synced
                    .is_some_and(|at| now.duration_since(at) < crate::frame_source::VSYNC_RESYNC);
            let timed = wanted && synced;
            if timed && !self.vsync_paced {
                // The link's watchdog deadline, if any, is not a refresh.
                self.frame_deadline = None;
            }
            self.vsync_paced = timed;
            // Steady frames of the app's own timing need the refresh grid
            // only: the link runs a refresh or so a second to report it.
            let stale = self
                .vsync_synced
                .is_none_or(|at| now.duration_since(at) >= crate::frame_source::VSYNC_RESYNC);
            link.set_running((wanted && !synced) || (!wanted && steady && stale));
            if wanted {
                if timed {
                    if self.frame_deadline.is_none() {
                        // On the refreshes the link delivers at this rate
                        // (whole refreshes apart, on the same ones), so the
                        // cadence holds as the link and the timer take turns.
                        let every = rate.map_or(1, |rate| zgui::frame::divisor(refresh, rate));
                        self.frame_deadline = Some(self.frame_timer.next_deadline_every(
                            now,
                            self.frame_interval,
                            every,
                        ));
                    }
                } else {
                    link.set_preferred_rate(rate);
                    let heartbeat = *self.frame_heartbeat.get_or_insert(now);
                    self.frame_deadline = Some(heartbeat + crate::frame_source::WATCHDOG);
                }
            }
            return;
        }
        // Windows: frames start as the compositor ticks (see
        // `frame_source::compositor_clock`), the timer only a watchdog.
        #[cfg(windows)]
        {
            if wanted
                && self.display_link.is_none()
                && let Some(window) = &self.window
            {
                // Created on first demand, so windows that never animate hold none.
                let proxy = self.proxy.clone();
                let token = self.context.window.token;
                self.display_link = crate::frame_source::DisplayLink::new(window, move |frame| {
                    let _ = proxy.send_event(Event::Frame(token, frame));
                });
            }
            if let Some(link) = &mut self.display_link {
                let refresh = 1. / self.frame_interval.as_secs_f64();
                // Scrolling, and frames just after input, run at the
                // display's full rate.
                let inputting = self.input_until.is_some_and(|until| now < until);
                let rate = self
                    .frames
                    .demanded_rate(refresh)
                    .filter(|rate| !scrolling && !inputting && *rate < refresh - 0.5);
                link.set_preferred_rate(rate);
                link.set_running(wanted);
                if wanted {
                    let heartbeat = *self.frame_heartbeat.get_or_insert(now);
                    self.frame_deadline = Some(heartbeat + crate::frame_source::WATCHDOG);
                }
                return;
            }
        }
        if !wanted {
            return;
        }
        if self.frame_paced {
            if !self.frame_redraw_requested
                && let Some(window) = &self.window
            {
                self.frame_redraw_requested = true;
                window.request_redraw();
            }
            return;
        }
        if self.frame_deadline.is_none() {
            self.frame_deadline = Some(self.frame_timer.next_deadline(now, self.frame_interval));
        }
    }
}
/// Windows' older clipboard keys, which its own fields still take:
/// Ctrl+Insert copies, Shift+Insert pastes, Shift+Delete cuts.
#[cfg(windows)]
fn windows_clipboard_alias(key: &Key, m: zgui::input::Modifiers) -> Option<&'static str> {
    match key {
        Key::Insert if m.control && !m.shift && !m.alt => Some("c"),
        Key::Insert if m.shift && !m.control && !m.alt => Some("v"),
        Key::Delete if m.shift && !m.control && !m.alt => Some("x"),
        _ => None,
    }
}
fn read_maximized_without_style_change(
    decorated: bool,
    resizable: bool,
    requested: bool,
    native_query: impl FnOnce() -> bool,
) -> bool {
    if cfg!(target_os = "macos") && (!decorated || !resizable) {
        requested
    } else {
        native_query()
    }
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn borderless_resize_never_calls_the_style_mutating_native_zoom_query() {
    for (decorated, resizable) in [(false, true), (true, false), (false, false)] {
        for requested in [false, true] {
            assert_eq!(
                read_maximized_without_style_change(decorated, resizable, requested, || {
                    panic!("native query would mutate style and enqueue another resize")
                }),
                requested,
            );
        }
    }
    // A normal window must still report user-initiated zoom, independent of the
    // last programmatic command.
    assert!(read_maximized_without_style_change(
        true,
        true,
        false,
        || true
    ));
    assert!(!read_maximized_without_style_change(
        true,
        true,
        true,
        || false
    ));
}

impl Drop for Host {
    fn drop(&mut self) {
        // Closed before its last move settled: saved now.
        if self.frame_save_at.take().is_some() {
            self.save_frame();
        }
        self.unregister_file_surface();
        self.context.window.closed.store(true, Ordering::Release);
        self.context.ui.set_presented(false);
        self.context.window_info.ready.set(false);
        self.context.dialogs.attach(None);
        // A pending native dialog may retain the native parent until its worker
        // completes. Closing the logical window must still remove it visually.
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        self.pending.borrow_mut().clear();
        if let Some(callback) = self.context.closed.take() {
            callback();
        }
        // The renderer's surface first, while its window still is (fields
        // drop in order, and `window` comes before `renderer`); then its
        // Metal layer leaves the view. The layer (raw-window-metal's)
        // observes the view's layer, which holds it as a sublayer, and
        // stops observing only as it goes: freed after the view's layer,
        // it would leave AppKit's records of the observation behind, a
        // few hundred bytes for every window ever closed.
        self.renderer = None;
        #[cfg(target_os = "macos")]
        if let Some(window) = &self.window {
            detach_metal_layers(window);
        }
    }
}
/// Take the Metal layers a renderer put in `window`'s view out of it, so
/// they go now, while what they observe is still there.
#[cfg(target_os = "macos")]
fn detach_metal_layers(window: &Window) {
    use objc2::runtime::AnyClass;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(metal) = AnyClass::get(c"CAMetalLayer") else {
        return;
    };
    // SAFETY: winit's handle names its window's live view, on the main
    // thread (hosts are dropped there).
    let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    if let Some(layer) = view.layer() {
        remove_sublayers(&layer, metal);
    }
}
/// Take `layer`'s sublayers of `class` out of it.
#[cfg(target_os = "macos")]
fn remove_sublayers(layer: &objc2_quartz_core::CALayer, class: &objc2::runtime::AnyClass) {
    use objc2::runtime::NSObjectProtocol;
    // SAFETY: CALayer's own sublayer list. It is the layer's live list
    // (each removal changes it, which its enumeration may not see), so
    // those going are gathered first.
    let going: Vec<_> = unsafe { layer.sublayers() }
        .iter()
        .flat_map(|list| list.iter())
        .filter(|sublayer| sublayer.isKindOfClass(class))
        .collect();
    for sublayer in going {
        sublayer.removeFromSuperlayer();
    }
}
struct WindowManager {
    #[cfg(target_os = "linux")]
    native_data: Option<crate::wayland_data::NativeData>,
    #[cfg(target_os = "linux")]
    appearance: Option<crate::linux_appearance::Watcher>,
    _native_events: Option<crate::native_events::NativeApplicationEvents>,
    application_event: Option<ApplicationEventHandler>,
    quit_on_last_window_close: bool,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::app_menu::NativeMenu>,
    hosts: HashMap<u64, Host>,
    /// When the last contained panics were (`contain`): many close together
    /// end the app.
    panics: Vec<std::time::Instant>,
    native: HashMap<WindowId, u64>,
    /// The window a drag was last found over and its content's box in global
    /// coordinates, and when: while the pointer stays in it, it still is,
    /// asked of the window server again after `DRAG_OVER_FOR` (another
    /// app's window may have come over it).
    drag_over: Cell<Option<(u64, [f64; 4], std::time::Instant)>>,
    /// The window whose drag others are hearing (`carry_drag`).
    carrying: Option<u64>,
    /// The window last focused: the one another launch raises (Linux).
    #[cfg(target_os = "linux")]
    last_focused: Option<u64>,
    factory: WindowFactory,
    active: bool,
    error: Option<String>,
    timers: zgui::timer::Hosted,
}
/// The app's window a drag is over, and the pointer in its coordinates.
type Over = (u64, f32, f32);
/// How long the window a drag is over stays known while the pointer stays
/// in it (`WindowManager::drag_over`).
const DRAG_OVER_FOR: std::time::Duration = std::time::Duration::from_millis(100);
impl WindowManager {
    /// The app's window at global point `(x, y)` and the point in its
    /// coordinates; `None` over none of them (another app's window, the
    /// desktop). Known again without asking while the pointer stays in the
    /// window it was last found in (`drag_over`).
    fn window_under(&self, x: f64, y: f64) -> Option<Over> {
        self.window_under_past(x, y, None)
    }
    /// `window_under`, past window `moving` (whose drag moves it, so it is
    /// always under the pointer): what is beneath it.
    fn window_under_past(&self, x: f64, y: f64, moving: Option<u64>) -> Option<Over> {
        let inside = |[left, top, width, height]: [f64; 4]| {
            (left..left + width).contains(&x) && (top..top + height).contains(&y)
        };
        let at = |token: u64, rect: [f64; 4]| (token, (x - rect[0]) as f32, (y - rect[1]) as f32);
        if let Some((token, rect, when)) = self.drag_over.get()
            && inside(rect)
            && when.elapsed() < DRAG_OVER_FOR
        {
            return Some(at(token, rect));
        }
        let boxes: Vec<(u64, [f64; 4])> = self
            .hosts
            .iter()
            .filter(|(token, _)| Some(**token) != moving)
            .filter_map(|(token, host)| Some((*token, host.content_box()?)))
            .filter(|(_, rect)| inside(*rect))
            .collect();
        // Which is in front there (another app's window may be) is the
        // window server's to say.
        let over = match boxes.as_slice() {
            [] => None,
            _ => {
                let number = match moving.and_then(|moving| self.hosts[&moving].window_number) {
                    Some(moving) => crate::window_place::window_below(x, y, moving),
                    None => crate::popup_window::window_at(x, y),
                };
                boxes.into_iter().find(|(token, _)| {
                    let own = self.hosts[token].window_number;
                    own.is_some() && own == number
                })
            }
        };
        self.drag_over
            .set(over.map(|(token, rect)| (token, rect, std::time::Instant::now())));
        over.map(|(token, rect)| at(token, rect))
    }
    /// Where window `token`'s drag is: its state, and the app's window under
    /// its pointer (`None`: over none of them). `None` without a drag, or
    /// where windows cannot tell where others are (drags then stay in their
    /// window).
    fn drag_target(&self, token: u64) -> Option<(Option<Over>, zgui::input::DragState)> {
        let source = self.hosts.get(&token)?;
        let drag = source.context.ui.input.drag_state()?;
        if drag.moves_window {
            // The window follows the pointer: what is under the pointer is
            // beneath the window.
            let (left, top) = source.content_origin?;
            let (x, y) = crate::window_place::pointer()
                .unwrap_or((left + drag.x as f64, top + drag.y as f64));
            return Some((self.window_under_past(x, y, Some(token)), drag));
        }
        let (width, height) = source.context.viewport.with_untracked(|viewport| *viewport);
        // Inside its own window, that is in front (the drag began in it,
        // so it is key): no need to ask the window server.
        if (0. ..width).contains(&drag.x) && (0. ..height).contains(&drag.y) {
            return Some((Some((token, drag.x, drag.y)), drag));
        }
        // Where windows have no place on screen (Wayland), out of its own
        // window is out of the app's.
        let Some((left, top)) = source.content_origin else {
            return Some((None, drag));
        };
        let over = self.window_under(left + drag.x as f64, top + drag.y as f64);
        Some((over, drag))
    }
    /// After window `token`'s input: carry its drag to the app's window under
    /// the pointer, whose destinations hear it (and none in the source
    /// meanwhile); out of every window, carrying what other apps take, it
    /// becomes the platform's. Once it ends, others hear it go.
    fn carry_drag(&mut self, token: u64) {
        let dragging = self
            .hosts
            .get(&token)
            .is_some_and(|host| host.context.ui.input.is_dragging() && !host.native_drag.active());
        if !dragging {
            if let Some(host) = self.hosts.get_mut(&token)
                && host.window_grab.take().is_some()
                && let Some(window) = &host.window
            {
                crate::window_place::settle(window, &mut host.window_pending);
            }
            if self.carrying == Some(token) {
                self.carrying = None;
                self.drag_over.set(None);
                for host in self.hosts.values() {
                    host.context.ui.input.drag_out(&host.context.ui.scene);
                }
            }
            return;
        }
        if let Some(source) = self.hosts.get_mut(&token) {
            source.move_with_drag();
        }
        let Some((over, drag)) = self.drag_target(token) else {
            return;
        };
        let over_other = over.filter(|(over, ..)| *over != token);
        if self.carrying.is_some() || over_other.is_some() {
            for (other, host) in &self.hosts {
                if over_other.is_none_or(|(over, ..)| over != *other) {
                    host.context.ui.input.drag_out(&host.context.ui.scene);
                }
            }
        }
        self.carrying = (over != Some((token, drag.x, drag.y))).then_some(token);
        if let Some(source) = self.hosts.get_mut(&token) {
            let ui = &source.context.ui;
            // A drag moving its window is always over it: its own
            // destinations never hear it.
            let away = drag.moves_window || over.is_none_or(|(over, ..)| over != token);
            ui.input.set_drag_away(&ui.scene, away);
            if over.is_none() && !drag.moves_window {
                source.hand_off_drag(&drag);
            }
        }
        if let Some((other, x, y)) = over_other
            && let Some(host) = self.hosts.get(&other)
        {
            let ui = &host.context.ui;
            ui.input.drag_over(&ui.scene, &drag.payload, x, y);
        }
    }
    /// Window `token`'s drag is released: over another of the app's
    /// windows, it drops there; over none, it is torn off (`outside`).
    fn drop_elsewhere(&mut self, token: u64) {
        if self
            .hosts
            .get(&token)
            .is_none_or(|host| host.native_drag.active())
        {
            return;
        }
        let Some((over, drag)) = self.drag_target(token) else {
            return;
        };
        let (accepted, outside) = match over {
            Some((over, ..)) if over == token => return,
            Some((over, x, y)) => {
                let Some(host) = self.hosts.get(&over) else {
                    return;
                };
                let ui = &host.context.ui;
                (ui.input.drag_drop(&ui.scene, &drag.payload, x, y), false)
            }
            // A window moved by its only tab stays where it was let go.
            None => (false, !drag.moves_window),
        };
        if let Some(source) = self.hosts.get(&token) {
            let ui = &source.context.ui;
            ui.input.end_drag_elsewhere(&ui.scene, accepted, outside);
        }
    }
    /// Window `token`'s pointer while its drag is X11's (`native_drag`):
    /// its moves, release and Escape go to the drag, which the host then
    /// hears as the platform's; other pointer input waits until it ends.
    /// Whether the event was the drag's.
    #[cfg(target_os = "linux")]
    fn x11_drag_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        token: u64,
        id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        use crate::native_drag::{self, NativeDrag};
        if !native_drag::active(id) {
            return false;
        }
        let Some(host) = self.hosts.get(&token) else {
            return false;
        };
        let (scale, (left, top)) = (host.scale_factor, host.content_origin.unwrap_or_default());
        let ended = match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (left + position.x / scale, top + position.y / scale);
                let own = self.window_under(x, y).is_some();
                let root = ((x * scale).round() as i32, (y * scale).round() as i32);
                native_drag::moved(root, (x, y), own);
                if let Err(error) = self.native_drag(token, NativeDrag::Moved { x, y }) {
                    self.fail(event_loop, error);
                }
                return true;
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => native_drag::released(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == WKey::Named(NamedKey::Escape) =>
            {
                native_drag::cancel().map(|(x, y)| (x, y, false))
            }
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } => return true,
            _ => return false,
        };
        if let Some((x, y, accepted)) = ended {
            native_drag::end_session();
            if let Err(error) = self.native_drag(token, NativeDrag::Ended { x, y, accepted }) {
                self.fail(event_loop, error);
            }
        }
        true
    }
    /// The platform's drag of window `token` moved or ended, at a global
    /// point. Over another of the app's windows, that window's destinations
    /// hear it, and a release drops there; else the source's host takes it
    /// (`Host::native_drag_event`).
    fn native_drag(
        &mut self,
        token: u64,
        event: crate::native_drag::NativeDrag,
    ) -> Result<(), Box<dyn Error>> {
        use crate::native_drag::NativeDrag;
        // Where the platform says (Wayland: in which of the app's windows),
        // else at a global point.
        let (x, y, ended, within) = match event {
            NativeDrag::Moved { x, y } => (x, y, None, None),
            NativeDrag::Ended { x, y, accepted } => (x, y, Some(accepted), None),
            NativeDrag::MovedIn { window, x, y } => (x, y, None, Some(window)),
            NativeDrag::DroppedIn { window, x, y } => (x, y, Some(false), Some(window)),
            NativeDrag::Left { x, y } => (x, y, Some(false), None),
        };
        let Some(drag) = self
            .hosts
            .get(&token)
            .and_then(|host| host.context.ui.input.drag_state())
        else {
            if ended.is_some()
                && let Some(source) = self.hosts.get_mut(&token)
            {
                source.native_drag_gone();
            }
            return Ok(());
        };
        // Let go past every window, a drag of the app's own is torn off, as
        // one released past them all without the platform (`drop_elsewhere`).
        if let NativeDrag::Left { .. } = event
            && drag.export.is_none()
            && let Some(source) = self.hosts.get(&token)
        {
            let ui = &source.context.ui;
            ui.input.end_drag_elsewhere(&ui.scene, false, true);
        }
        let over = match within {
            Some(window) => self
                .hosts
                .contains_key(&window)
                .then_some((window, x as f32, y as f32)),
            None => self.window_under(x, y),
        }
        .filter(|(over, ..)| *over != token);
        for (other, host) in &self.hosts {
            if over.is_none_or(|(over, ..)| over != *other) {
                host.context.ui.input.drag_out(&host.context.ui.scene);
            }
        }
        if let Some(source) = self.hosts.get(&token) {
            let ui = &source.context.ui;
            ui.input.set_drag_away(&ui.scene, over.is_some());
        }
        let Some((other, ox, oy)) = over else {
            return match self.hosts.get_mut(&token) {
                Some(source) => source.native_drag_event(event),
                None => Ok(()),
            };
        };
        let dropped = match ended {
            None => {
                if let Some(host) = self.hosts.get(&other) {
                    let ui = &host.context.ui;
                    ui.input.drag_over(&ui.scene, &drag.payload, ox, oy);
                }
                false
            }
            Some(taken) => {
                !taken
                    && self.hosts.get(&other).is_some_and(|host| {
                        let ui = &host.context.ui;
                        ui.input.drag_drop(&ui.scene, &drag.payload, ox, oy)
                    })
            }
        };
        let Some(source) = self.hosts.get_mut(&token) else {
            return Ok(());
        };
        if dropped {
            let ui = &source.context.ui;
            ui.input.end_drag_elsewhere(&ui.scene, true, false);
        }
        // Dropped in another window and not taken there: not taken at all.
        if ended.is_some() && !dropped && within.is_some() {
            let ui = &source.context.ui;
            ui.input.end_drag_elsewhere(&ui.scene, false, false);
        }
        source.native_drag_event(event)
    }
    /// Raise the window in front (the last focused, else any) for another
    /// launch of the app, with its `token`: on Wayland the compositor
    /// activates it (`xdg_activation_v1`); on X11 the window manager does,
    /// restoring a minimized window, and that launch's startup notification
    /// ends.
    #[cfg(target_os = "linux")]
    fn raise(&mut self, token: String) {
        let Some(window) = self
            .last_focused
            .and_then(|focused| self.hosts.get(&focused))
            .or_else(|| self.hosts.values().next())
            .and_then(|host| host.window.clone())
        else {
            return;
        };
        let wayland = window
            .display_handle()
            .is_ok_and(|handle| matches!(handle.as_raw(), RawDisplayHandle::Wayland(_)));
        if wayland {
            if let Some(data) = &self.native_data {
                data.activate(token, window);
            }
        } else {
            window.set_minimized(false);
            window.focus_window();
            crate::x11_startup::complete(&window, &token);
        }
    }
    fn menu_action(&mut self, token: u64, action: crate::MenuAction) {
        if !crate::app_menu::enabled_action(&self.factory.menus.borrow(), &action) {
            return;
        }
        if let Some(host) = self.hosts.get_mut(&token) {
            if host.context.window.is_closed() {
                return;
            }
            let handled = host
                .context
                .ui
                .input
                .dispatch_action(
                    &host.context.ui.scene,
                    zgui::actions::Action::new(action.clone()),
                )
                .default_prevented;
            if handled {
                return;
            }
            if let Some(callback) = &mut host.context.menu_action {
                callback(action);
            }
        }
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        if self.error.is_none() {
            self.error = Some(error.to_string());
        }
        event_loop.exit();
    }
    fn create_pending(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        loop {
            let request = self.factory.pending.borrow_mut().pop_front();
            let Some(request) = request else {
                break;
            };
            if request.handle.is_closed() {
                continue;
            }
            let token = request.handle.token;
            let mut host = Host::new(request.options, request.handle, self.factory.clone());
            host.state = request.state;
            (request.build)(&mut host.context);
            if host.context.window.is_closed() {
                continue;
            }
            if self.active {
                host.create_window(event_loop)?;
                self.native
                    .insert(host.window.as_ref().unwrap().id(), token);
            }
            self.hosts.insert(token, host);
        }
        Ok(())
    }
    fn request_close(&mut self, token: u64) {
        let allowed = self.hosts.get_mut(&token).is_some_and(|host| {
            host.context
                .close_requested
                .as_mut()
                .is_none_or(|callback| callback())
        });
        if allowed {
            self.close(token);
        }
    }
    fn close(&mut self, token: u64) {
        if let Some(host) = self.hosts.remove(&token)
            && let Some(window) = &host.window
        {
            self.native.remove(&window.id());
            // Its drag, if X11's, goes with it.
            #[cfg(target_os = "linux")]
            if crate::native_drag::active(window.id()) {
                crate::native_drag::end_session();
            }
        }
    }
}
impl Drop for WindowManager {
    fn drop(&mut self) {
        self.factory.alive.set(false);
        self.factory.gpu_contexts.borrow_mut().clear();
        // Cloned public factories become inert at shutdown and must not keep
        // the clipboard worker or its owned display connection alive.
        let clipboard = self.factory.clipboard.borrow_mut().take();
        drop(clipboard);
        let mut pending = self.factory.pending.borrow_mut();
        for request in pending.iter() {
            request.handle.closed.store(true, Ordering::Release);
        }
        pending.clear();
    }
}
impl WindowManager {
    fn resumed_turn(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        if let Some(events) = &self._native_events {
            events.install();
        }
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            menu.install();
        }
        self.active = true;
        // The launcher's token: the first window takes it, so the desktop
        // focuses it (Wayland) and ends the launch's busy cursor (X11).
        // Gone from the environment, processes the app starts later do not
        // take it as theirs.
        #[cfg(target_os = "linux")]
        {
            use winit::platform::startup_notify::{
                EventLoopExtStartupNotify, reset_activation_token_env,
            };
            if let Some(token) = event_loop.read_token_from_env() {
                *self.factory.activation.borrow_mut() = Some(token);
                reset_activation_token_env();
            }
        }
        #[cfg(target_os = "linux")]
        if self.native_data.is_none() {
            let proxy = self.factory.proxy.clone();
            match crate::wayland_data::NativeData::new(
                event_loop.owned_display_handle(),
                self.factory.file_routes.clone(),
                Arc::new(move |token, event| {
                    let _ = proxy.send_event(Event::NativeFiles(token, event));
                }),
            ) {
                Ok(service) => self.native_data = service,
                Err(error) => {
                    eprintln!("zgui: native clipboard and file drops unavailable: {error}")
                }
            }
        }
        #[cfg(target_os = "linux")]
        if self.appearance.is_none() {
            let proxy = self.factory.proxy.clone();
            self.appearance = crate::linux_appearance::watch(move || {
                let _ = proxy.send_event(Event::AppearanceChanged);
            });
        }
        if self.factory.clipboard.borrow().is_none() {
            match crate::clipboard::Clipboard::new(
                event_loop.owned_display_handle(),
                #[cfg(target_os = "linux")]
                self.native_data.as_ref().map(|data| data.clipboard()),
            ) {
                Ok(clipboard) => *self.factory.clipboard.borrow_mut() = Some(clipboard),
                Err(error) => eprintln!("zgui: clipboard unavailable: {error}"),
            }
        }
        for (token, host) in &mut self.hosts {
            if host.context.window.is_closed() {
                continue;
            }
            if let Err(e) = host.create_window(event_loop) {
                self.fail(event_loop, e);
                return;
            }
            self.native
                .insert(host.window.as_ref().unwrap().id(), *token);
        }
        if let Err(e) = self.create_pending(event_loop) {
            self.fail(event_loop, e);
        }
    }
    fn suspended_turn(&mut self) {
        self.active = false;
        self.native.clear();
        for host in self.hosts.values_mut() {
            host.suspend();
        }
    }
    fn user_event_turn(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::Quit => event_loop.exit(),
            Event::Application(event, token) => {
                // The window this brings up takes the launch's token...
                #[cfg(target_os = "linux")]
                if let Some(token) = &token {
                    *self.factory.activation.borrow_mut() =
                        Some(winit::window::ActivationToken::from_raw(token.clone()));
                }
                #[cfg(not(target_os = "linux"))]
                let _ = token;
                if let Some(handler) = &mut self.application_event {
                    handler(event, &self.factory);
                }
                // ...or, bringing up none, the one in front is raised.
                #[cfg(target_os = "linux")]
                if self.factory.pending.borrow().is_empty() {
                    let token = self.factory.activation.borrow_mut().take();
                    if let Some(token) = token {
                        self.raise(token.into_raw());
                    }
                }
            }
            Event::MenuAction(token, action) => self.menu_action(token, action),
            Event::MenuFallback(token, action) => {
                if crate::app_menu::enabled_action(&self.factory.menus.borrow(), &action)
                    && let Some(host) = self.hosts.get_mut(&token)
                    && !host.context.window.is_closed()
                    && let Some(callback) = &mut host.context.menu_action
                {
                    callback(action);
                }
            }
            #[cfg(target_os = "linux")]
            Event::NativeFiles(token, event) => {
                if let Some(host) = self.hosts.get_mut(&token)
                    && !host.context.window.is_closed()
                {
                    use crate::wayland_data::FileEvent;
                    let result = match event {
                        FileEvent::Hover { x, y, path } => {
                            host.dispatch(InputEvent::FileHover { x, y, path })
                        }
                        FileEvent::Leave => host.dispatch(InputEvent::FileHoverCancelled),
                        FileEvent::Rejected { x, y, reason } => {
                            host.dispatch(InputEvent::FilesDropRejected { x, y, reason })
                        }
                        FileEvent::Drop {
                            x,
                            y,
                            paths,
                            accepted,
                        } => {
                            let mut result = Ok(());
                            let mut handled = false;
                            for path in paths.iter() {
                                if host.context.window.is_closed() {
                                    break;
                                }
                                match host.dispatch_prevented(InputEvent::FileDrop {
                                    x,
                                    y,
                                    path: path.clone(),
                                }) {
                                    Ok(value) => handled |= value,
                                    Err(error) => {
                                        result = Err(error);
                                        break;
                                    }
                                }
                            }
                            if result.is_ok() && !host.context.window.is_closed() {
                                match host.dispatch_prevented(InputEvent::FilesDrop { x, y, paths })
                                {
                                    Ok(value) => handled |= value,
                                    Err(error) => result = Err(error),
                                }
                            }
                            let _ = accepted.send(
                                result.is_ok() && handled && !host.context.window.is_closed(),
                            );
                            result
                        }
                    };
                    if let Err(error) = result {
                        self.fail(event_loop, error);
                    }
                }
            }
            #[cfg(target_os = "linux")]
            Event::AppearanceChanged => {
                for host in self.hosts.values_mut() {
                    crate::system_colors::sync(&host.context.ui);
                    // The desktop's window buttons may have moved.
                    #[cfg(target_os = "linux")]
                    if let Some(caption) = &mut host.caption {
                        caption.sync(&mut host.context.ui);
                        if let Some(window) = &host.window {
                            host.context
                                .window_info
                                .refresh(window, host.options.titlebar);
                        }
                    }
                }
            }
            Event::MenusChanged => {
                let menus = self.factory.menus();
                #[cfg(target_os = "macos")]
                {
                    let replacement = match crate::app_menu::NativeMenu::new(&menus) {
                        Ok(menu) => menu,
                        Err(error) => {
                            self.fail(event_loop, error);
                            return;
                        }
                    };
                    // Old menu removes itself from NSApp on drop; install replacement afterwards.
                    self.native_menu = None;
                    replacement.install();
                    self.native_menu = Some(replacement);
                }
                for host in self.hosts.values_mut() {
                    host.context.menu_model.set(menus.clone());
                    let root = host.context.ui.scene.borrow().root();
                    host.menu_binding = host
                        .context
                        .ui
                        .input
                        .bind_keys(root, crate::app_menu::keymap(&menus));
                }
            }
            #[cfg(target_os = "macos")]
            Event::NativeMenuAction(action) => {
                // The key window: winit reports a window unfocused while a
                // native view in it (a web page, a terminal) has first
                // responder, so ask AppKit too.
                let focused = self
                    .hosts
                    .iter()
                    .find(|(_, host)| host.native_focused && !host.context.window.is_closed())
                    .or_else(|| {
                        self.hosts.iter().find(|(_, host)| {
                            !host.context.window.is_closed()
                                && host.window.as_ref().is_some_and(|w| is_key_window(w))
                        })
                    })
                    .map(|(token, _)| *token)
                    // No key window (the app is in the background, a menu
                    // chosen through accessibility): its only window.
                    .or_else(|| {
                        let mut open = self
                            .hosts
                            .iter()
                            .filter(|(_, host)| !host.context.window.is_closed());
                        match (open.next(), open.next()) {
                            (Some((token, _)), None) => Some(*token),
                            _ => None,
                        }
                    });
                if let Some(token) = focused {
                    self.menu_action(token, action);
                }
            }
            Event::WindowCommand(token, command) => {
                if let Some(host) = self.hosts.get_mut(&token) {
                    if !host.context.window.is_closed() {
                        host.command(command);
                    }
                } else if let Some(request) = self
                    .factory
                    .pending
                    .borrow_mut()
                    .iter_mut()
                    .find(|request| request.handle.token == token && !request.handle.is_closed())
                {
                    request.state.apply(&mut request.options, &command);
                }
            }
            Event::Close(token) => self.close(token),
            Event::RequestClose(token) => self.request_close(token),
            Event::Popup(token, input) => {
                if let Some(host) = self.hosts.get_mut(&token)
                    && !host.context.window.is_closed()
                    && let Err(error) = host.popup_input(input)
                {
                    self.fail(event_loop, error);
                }
            }
            Event::NativeDrag(token, drag) => {
                if self
                    .hosts
                    .get(&token)
                    .is_some_and(|host| !host.context.window.is_closed())
                    && let Err(error) = self.native_drag(token, drag)
                {
                    self.fail(event_loop, error);
                }
                // An X11 drag's target finished with it.
                #[cfg(target_os = "linux")]
                if matches!(drag, crate::native_drag::NativeDrag::Ended { .. }) {
                    crate::native_drag::end_session();
                }
            }
            Event::PopupSoon(token) => {
                #[cfg(target_os = "linux")]
                crate::popup_window::make(event_loop);
                if let Some(host) = self.hosts.get(&token) {
                    host.context.ui.popup_soon_heard();
                    if let Some(renderer) = &host.renderer {
                        crate::popup_window::warm(&renderer.context(), host.scale_factor as f32);
                    }
                    // The work areas as the popup opens (the taskbar may have
                    // moved since the window last did); none if no popup
                    // window could be readied.
                    #[cfg(windows)]
                    if let Some(window) = &host.window {
                        host.context
                            .ui
                            .set_popup_screens(crate::popup_window::popup_screens(window));
                    }
                }
            }
            Event::Wake(token) => {
                let _ = self.hosts.get(&token);
            }
            Event::Timers => {}
            #[cfg(target_os = "macos")]
            Event::Frame(token, frame) => {
                if let Some(host) = self.hosts.get_mut(&token)
                    && !host.context.window.is_closed()
                {
                    // The refresh after this one starts as this one shows.
                    host.frame_timer =
                        crate::frame_source::FrameTimer::aligned(frame.time, frame.index + 1);
                    host.frame_interval = frame.interval;
                    host.vsync_synced = Some(std::time::Instant::now());
                    // `about_to_wait` runs next in this same loop pass, stepping
                    // the woken animations and painting once.
                    host.deliver_frame(frame);
                }
            }
            #[cfg(windows)]
            Event::Frame(token, frame) => {
                if let Some(host) = self.hosts.get_mut(&token)
                    && !host.context.window.is_closed()
                {
                    // The watchdog timer, should the clock stall, continues
                    // its grid.
                    host.frame_timer =
                        crate::frame_source::FrameTimer::aligned(frame.time, frame.index + 1);
                    host.frame_interval = frame.interval;
                    host.deliver_frame(frame);
                }
            }
            Event::Redraw(token) => {
                if let Some(host) = self.hosts.get_mut(&token) {
                    host.presentation.expose();
                    host.force = true;
                }
            }
            Event::Open => {}
            Event::Accessibility(event) => {
                if let Some(token) = self.native.get(&event.window_id).copied()
                    && let Some(host) = self.hosts.get_mut(&token)
                {
                    host.accessibility_event(event.window_event);
                }
            }
        }
        // While a drag is Windows' (`native_drag`), `DoDragDrop`'s loop runs
        // in place of winit's, which does not turn: each event ends as a turn
        // would, stepping animations and painting, so windows go on as
        // through a live resize. Frames keep coming every refresh meanwhile.
        #[cfg(windows)]
        if self.hosts.values().any(|host| host.native_drag.active()) {
            self.about_to_wait(event_loop);
        }
    }
    fn window_event_turn(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) {
        // A popup window's input goes to its parent's host (X11).
        #[cfg(target_os = "linux")]
        if crate::popup_window::route(id, &event) {
            return;
        }
        let Some(token) = self.native.get(&id).copied() else {
            return;
        };
        if matches!(event, WindowEvent::CloseRequested) {
            self.request_close(token);
            return;
        }
        if matches!(event, WindowEvent::Destroyed) {
            self.close(token);
            return;
        }
        #[cfg(target_os = "linux")]
        if matches!(event, WindowEvent::Focused(true)) {
            self.last_focused = Some(token);
        }
        #[cfg(target_os = "linux")]
        if self.x11_drag_input(event_loop, token, id, &event) {
            return;
        }
        // A drag released over another of the app's windows drops there.
        let pointer = matches!(
            event,
            WindowEvent::CursorMoved { .. } | WindowEvent::MouseInput { .. }
        );
        if matches!(
            event,
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            }
        ) {
            self.drop_elsewhere(token);
        }
        if let Some(host) = self.hosts.get_mut(&token)
            && let Err(e) = host.window_event(event)
        {
            self.fail(event_loop, e);
        }
        if pointer {
            self.carry_drag(token);
        }
    }
    fn about_to_wait_turn(&mut self, event_loop: &ActiveEventLoop) {
        // A close request must terminate even while native surfaces are suspended.
        let closed: Vec<_> = self
            .hosts
            .iter()
            .filter_map(|(token, host)| host.context.window.is_closed().then_some(*token))
            .collect();
        for token in closed {
            self.close(token);
        }
        if self.quit_on_last_window_close
            && self.hosts.is_empty()
            && self.factory.pending.borrow().is_empty()
        {
            event_loop.exit();
            return;
        }
        // A popup found no window ready: one is made for its next frame.
        #[cfg(target_os = "linux")]
        if crate::popup_window::wanted() {
            crate::popup_window::make(event_loop);
        }
        // What Wayland popups heard (winit read the socket for them).
        #[cfg(target_os = "linux")]
        crate::popup_window::dispatch_wayland();
        // Wake tasks whose sleeps are due before the hosts poll them.
        self.timers.fire();
        if !self.active {
            event_loop.set_control_flow(match self.timers.deadline() {
                Some(deadline) => ControlFlow::WaitUntil(deadline),
                None => ControlFlow::Wait,
            });
            return;
        }
        if let Err(e) = self.create_pending(event_loop) {
            self.fail(event_loop, e);
            return;
        }
        // macOS's Reduce motion, Increase contrast or Reduce transparency
        // changed: every window's themes follow at once.
        if crate::system_colors::display_options_changed() {
            for host in self.hosts.values() {
                crate::system_colors::sync(&host.context.ui);
            }
        }
        // And the scroll bar style: overlay scrollbars come and go with it.
        if crate::scrollers::style_changed() {
            for host in self.hosts.values() {
                crate::scrollers::sync(&host.context.ui);
            }
        }
        let closed: Vec<_> = self
            .hosts
            .iter()
            .filter_map(|(token, host)| host.context.window.is_closed().then_some(*token))
            .collect();
        for token in closed {
            self.close(token);
        }
        if self.quit_on_last_window_close
            && self.hosts.is_empty()
            && self.factory.pending.borrow().is_empty()
        {
            event_loop.exit();
            return;
        }
        let mut ready = false;
        for host in self.hosts.values_mut() {
            // A renderer lost with its GPU device, due another try.
            if host
                .renderer_retry
                .is_some_and(|at| at <= std::time::Instant::now())
                && let Some(window) = &host.window
            {
                window.request_redraw();
            }
            let drop_event = match host.pending_file_drops.take() {
                Ok(Some(paths)) => Some(InputEvent::FilesDrop {
                    x: host.cursor.0,
                    y: host.cursor.1,
                    paths,
                }),
                Ok(None) => None,
                Err(reason) => Some(InputEvent::FilesDropRejected {
                    x: host.cursor.0,
                    y: host.cursor.1,
                    reason,
                }),
            };
            if let Some(event) = drop_event
                && let Err(error) = host.dispatch(event)
            {
                self.fail(event_loop, error);
                return;
            }

            match host.update() {
                Ok(pending) => ready |= pending,
                Err(e) => {
                    self.fail(event_loop, e);
                    return;
                }
            }
        }
        ready |= !self.factory.pending.borrow().is_empty();
        let deadline = self
            .hosts
            .values()
            .flat_map(|host| {
                [
                    host.presentation.deadline(),
                    host.context.ui.next_interaction_deadline(),
                    host.frame_deadline,
                    host.renderer_retry,
                    host.touch.hold_deadline(),
                    host.trim_at,
                    host.frame_save_at,
                    match host.startup_trim {
                        StartupTrim::At(at) => Some(at),
                        _ => None,
                    },
                ]
            })
            .flatten()
            .chain(self.timers.deadline())
            .min();
        event_loop.set_control_flow(if ready {
            ControlFlow::Poll
        } else if let Some(deadline) = deadline {
            ControlFlow::WaitUntil(deadline)
        } else {
            ControlFlow::Wait
        });
    }
}

/// The event loop's turns, each contained (`WindowManager::contain`).
impl ApplicationHandler<Event> for WindowManager {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.contain(event_loop, None, |this| this.resumed_turn(event_loop));
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.contain(event_loop, None, |this| this.suspended_turn());
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        self.contain(event_loop, None, |this| {
            this.user_event_turn(event_loop, event)
        });
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let token = self.native.get(&id).copied();
        self.contain(event_loop, token, |this| {
            this.window_event_turn(event_loop, id, event)
        });
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.contain(event_loop, None, |this| this.about_to_wait_turn(event_loop));
    }
}
impl WindowManager {
    /// Run one turn of the event loop. A panic in it (an app's handler or
    /// effect) is contained: the panic hook has logged it, its window (or
    /// every window, when the turn was no one window's) says so in a
    /// banner, and the app keeps running; zgui's state stays consistent
    /// across an unwind (its borrows, subscriptions and focus recover).
    /// Panics that keep coming (20 within a second) end the app instead.
    fn contain(
        &mut self,
        event_loop: &ActiveEventLoop,
        token: Option<u64>,
        turn: impl FnOnce(&mut Self),
    ) {
        let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| turn(self)))
        else {
            return;
        };
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".into());
        let now = std::time::Instant::now();
        self.panics
            .retain(|at| now.duration_since(*at) < std::time::Duration::from_secs(1));
        self.panics.push(now);
        if self.panics.len() >= 20 {
            self.fail(event_loop, format!("panicking repeatedly: {message}"));
            return;
        }
        eprintln!("zgui: contained a panic ({message}); the app keeps running");
        for (key, host) in self.hosts.iter_mut() {
            if token.is_none_or(|token| token == *key) {
                host.show_error(&message);
            }
        }
    }
}
/// The layout inspector's toggle, with I: Ctrl+Option+Cmd on macOS (no
/// system or AppKit shortcut uses it), Ctrl+Alt+Shift elsewhere (where the
/// Super key belongs to the desktop).
fn inspector_shortcut(m: Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        m.control && m.alt && m.meta && !m.shift
    } else {
        m.control && m.alt && m.shift && !m.meta
    }
}
/// A shortcut's key on a layout of other letters (Russian, Greek, Hebrew,
/// Arabic…): with Ctrl or Cmd held (and not Alt, which with Ctrl is
/// Windows' AltGr, typing text), a character outside ASCII is the physical
/// key's on a US layout, as browsers and GTK take shortcuts, so Ctrl+C
/// copies whatever letter the layout puts on that key.
fn shortcut_key(key: Key, physical: PhysicalKey, modifiers: zgui::input::Modifiers) -> Key {
    let Key::Character(character) = &key else {
        return key;
    };
    if !(modifiers.control || modifiers.meta) || modifiers.alt || character.is_ascii() {
        return key;
    }
    let PhysicalKey::Code(code) = physical else {
        return key;
    };
    let us = match code {
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Backquote => "`",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        _ => return key,
    };
    Key::Character(us.into())
}
fn map_key(key: &WKey) -> Option<Key> {
    Some(match key {
        WKey::Character(k) if k.as_str() == " " => Key::Space,
        WKey::Character(k) => Key::Character(k.to_string()),
        WKey::Named(k) => match k {
            NamedKey::Tab => Key::Tab,
            NamedKey::Enter => Key::Enter,
            NamedKey::Space => Key::Space,
            NamedKey::Escape => Key::Escape,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Insert => Key::Insert,
            NamedKey::ContextMenu => Key::ContextMenu,
            NamedKey::F1 => Key::Function(1),
            NamedKey::F2 => Key::Function(2),
            NamedKey::F3 => Key::Function(3),
            NamedKey::F4 => Key::Function(4),
            NamedKey::F5 => Key::Function(5),
            NamedKey::F6 => Key::Function(6),
            NamedKey::F7 => Key::Function(7),
            NamedKey::F8 => Key::Function(8),
            NamedKey::F9 => Key::Function(9),
            NamedKey::F10 => Key::Function(10),
            NamedKey::F11 => Key::Function(11),
            NamedKey::F12 => Key::Function(12),
            NamedKey::F13 => Key::Function(13),
            NamedKey::F14 => Key::Function(14),
            NamedKey::F15 => Key::Function(15),
            NamedKey::F16 => Key::Function(16),
            NamedKey::F17 => Key::Function(17),
            NamedKey::F18 => Key::Function(18),
            NamedKey::F19 => Key::Function(19),
            NamedKey::F20 => Key::Function(20),
            NamedKey::F21 => Key::Function(21),
            NamedKey::F22 => Key::Function(22),
            NamedKey::F23 => Key::Function(23),
            NamedKey::F24 => Key::Function(24),
            NamedKey::F25 => Key::Function(25),
            NamedKey::F26 => Key::Function(26),
            NamedKey::F27 => Key::Function(27),
            NamedKey::F28 => Key::Function(28),
            NamedKey::F29 => Key::Function(29),
            NamedKey::F30 => Key::Function(30),
            NamedKey::F31 => Key::Function(31),
            NamedKey::F32 => Key::Function(32),
            NamedKey::F33 => Key::Function(33),
            NamedKey::F34 => Key::Function(34),
            NamedKey::F35 => Key::Function(35),

            _ => return None,
        },
        _ => return None,
    })
}

fn apply_viewport(
    options: &mut WindowOptions,
    ui: &Ui,
    viewport: &Signal<(f32, f32)>,
    size: PhysicalSize<u32>,
    scale: f64,
) -> bool {
    if size.width == 0 || size.height == 0 || !scale.is_finite() || scale <= 0. {
        return false;
    }
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;
    if !width.is_finite()
        || !height.is_finite()
        || width > f32::MAX as f64
        || height > f32::MAX as f64
    {
        return false;
    }
    options.width = width;
    options.height = height;
    ui.scene.borrow_mut().resize(width as f32, height as f32);
    viewport.set((width as f32, height as f32));
    true
}
fn native_cursor(cursor: zgui::cursor::Cursor) -> CursorIcon {
    use zgui::cursor::Cursor as C;
    match cursor {
        C::Default => CursorIcon::Default,
        C::Pointer => CursorIcon::Pointer,
        C::Text => CursorIcon::Text,
        C::VerticalText => CursorIcon::VerticalText,
        C::Crosshair => CursorIcon::Crosshair,
        C::Move => CursorIcon::Move,
        C::Grab => CursorIcon::Grab,
        C::Grabbing => CursorIcon::Grabbing,
        C::NotAllowed => CursorIcon::NotAllowed,
        C::Wait => CursorIcon::Wait,
        C::Progress => CursorIcon::Progress,
        C::Help => CursorIcon::Help,
        C::ContextMenu => CursorIcon::ContextMenu,
        C::Copy => CursorIcon::Copy,
        C::Alias => CursorIcon::Alias,
        C::NoDrop => CursorIcon::NoDrop,
        C::AllScroll => CursorIcon::AllScroll,
        C::ColResize => CursorIcon::ColResize,
        C::RowResize => CursorIcon::RowResize,
        C::EwResize => CursorIcon::EwResize,
        C::NsResize => CursorIcon::NsResize,
        C::NeswResize => CursorIcon::NeswResize,
        C::NwseResize => CursorIcon::NwseResize,
        C::ZoomIn => CursorIcon::ZoomIn,
        C::ZoomOut => CursorIcon::ZoomOut,
    }
}
fn cursor_for_role(role: Option<Role>) -> CursorIcon {
    match role {
        Some(Role::Button | Role::Link | Role::CheckBox | Role::MenuItem) => CursorIcon::Pointer,
        Some(Role::TextInput | Role::MultilineTextInput) => CursorIcon::Text,
        Some(Role::Slider) => CursorIcon::EwResize,
        _ => CursorIcon::Default,
    }
}
/// A trackpad scroll gesture, from winit's phases: the fingers' Started,
/// Moved and Ended, then (if they flicked) the momentum's, reported alike.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Trackpad {
    Idle,
    Fingers,
    /// The fingers lifted then: a start within a moment is momentum's.
    Lifted(std::time::Instant),
    Momentum,
}
impl Trackpad {
    /// The gesture phase `phase` reports at `now`, if any.
    fn phase(
        &mut self,
        phase: winit::event::TouchPhase,
        now: std::time::Instant,
    ) -> Option<zgui::input::ScrollGesture> {
        use winit::event::TouchPhase;
        use zgui::input::ScrollGesture;
        const MOMENTUM: std::time::Duration = std::time::Duration::from_millis(50);
        let (next, gesture) = match (phase, *self) {
            (TouchPhase::Started, Trackpad::Lifted(at))
                if now.saturating_duration_since(at) < MOMENTUM =>
            {
                (Trackpad::Momentum, Some(ScrollGesture::MomentumBegan))
            }
            (TouchPhase::Started, Trackpad::Momentum) => (Trackpad::Momentum, None),
            (TouchPhase::Started, _) => (Trackpad::Fingers, Some(ScrollGesture::Began)),
            (TouchPhase::Ended | TouchPhase::Cancelled, Trackpad::Fingers) => {
                (Trackpad::Lifted(now), Some(ScrollGesture::Ended))
            }
            (TouchPhase::Ended | TouchPhase::Cancelled, Trackpad::Momentum) => {
                (Trackpad::Idle, Some(ScrollGesture::MomentumEnded))
            }
            (_, state) => (state, None),
        };
        *self = next;
        gesture
    }
}
/// A wheel's motion in logical pixels: its lines (Windows' notches) at
/// `step`'s pixels each across and down (`scrollers::wheel_step`).
fn wheel_delta(delta: MouseScrollDelta, scale: f64, step: (f32, f32)) -> (f32, f32) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (-x * step.0, -y * step.1),
        MouseScrollDelta::PixelDelta(p) => {
            let p = p.to_logical::<f32>(scale);
            (-p.x, -p.y)
        }
    }
}
/// Whether wheel lines are continuous motion rather than notches. Windows
/// reports a precision touchpad's (and a free-spinning wheel's) motion as
/// wheel deltas of less than a notch (120); eased like notches, a swipe
/// trailed the fingers. Whole notches stay notches.
fn continuous_lines(x: f32, y: f32) -> bool {
    let fraction = |lines: f32| (lines * 120.).round() as i32 % 120 != 0;
    cfg!(windows) && (fraction(x) || fraction(y))
}
/// Report where a partially repainted frame differs from a full repaint:
/// the bounding box of stale device pixels, and the frame's damage.
fn check_damage(
    renderer: &mut zgui_gpu::GpuRenderer,
    scene: &zgui::scene::Scene,
    damage: &[Rect],
    full: &[Rect],
    width: usize,
    frame: u64,
) -> Result<(), zgui_gpu::GpuError> {
    let partial = renderer.readback()?;
    renderer.render_uncounted(scene, full)?;
    let fresh = renderer.readback()?;
    let mut stale: Option<(usize, usize, usize, usize)> = None;
    let mut count = 0;
    for (index, (a, b)) in partial.chunks(4).zip(fresh.chunks(4)).enumerate() {
        if a != b {
            let (x, y) = (index % width, index / width);
            count += 1;
            stale = Some(stale.map_or((x, y, x, y), |(x0, y0, x1, y1)| {
                (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
            }));
        }
    }
    if let Some((x0, y0, x1, y1)) = stale {
        eprintln!(
            "ZGUI_DAMAGE_CHECK frame {frame}: {count} stale device pixels in ({x0},{y0})..({x1},{y1}); damage {damage:?}"
        );
    }
    Ok(())
}

/// Return freed heap pages to the system. glibc keeps what it frees mapped,
/// including a software rasterizer's shader compilation, so an idle window
/// holds its peak. Elsewhere the allocator already returns them.
fn release_freed_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: glibc's malloc_trim only releases memory it has freed.
        unsafe {
            malloc_trim(0);
        }
    }
}
/// zgui content never moves its window by itself (a custom titlebar's tabs
/// and buttons drag and click; `WindowHandle::drag_window` moves it). AppKit
/// otherwise moves the window from any press on a non-opaque view in a
/// transparent titlebar, and a window made unmovable instead swallows the
/// drag. winit's view gets `mouseDownCanMoveWindow` answering NO.
#[cfg(target_os = "macos")]
fn content_never_moves_window() {
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    extern "C-unwind" fn no(_this: *mut AnyObject, _cmd: Sel) -> Bool {
        Bool::NO
    }
    let Some(class) = AnyClass::get(c"WinitView") else {
        return;
    };
    // Added once; later calls find it there and change nothing.
    unsafe {
        let imp: objc2::runtime::Imp =
            std::mem::transmute(no as extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool);
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            objc2::sel!(mouseDownCanMoveWindow),
            imp,
            c"c@:".as_ptr(),
        );
    }
}

/// Window drags from the mouse-down that started them. `drag_window` is a
/// command, run after the press was handled, and AppKit's window drag needs
/// the original mouse-down event (`performWindowDragWithEvent:`), not
/// whatever event is current by then.
#[cfg(target_os = "macos")]
mod mouse_down {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSEventMask, NSView};
    use std::{cell::RefCell, ptr::NonNull};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    thread_local! {
        static LAST: RefCell<Option<Retained<NSEvent>>> = const { RefCell::new(None) };
        static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    /// Keep each left mouse-down (once per process).
    pub fn watch() {
        if WATCHING.replace(true) {
            return;
        }
        let block = block2::RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a valid event, retained here to keep.
            let kept = unsafe { Retained::retain(event.as_ptr()) };
            LAST.with(|last| *last.borrow_mut() = kept);
            event.as_ptr()
        });
        // The monitor lives as long as the app.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseDown,
                &block,
            )
        };
        std::mem::forget(monitor);
    }
    /// The last mouse-down's click count (2: a double click's second).
    pub fn click_count() -> isize {
        LAST.with(|last| last.borrow().as_ref().map_or(0, |event| event.clickCount()))
    }
    /// Start a window drag from the last mouse-down, if the button is still
    /// down; whether one started.
    pub fn drag(window: &winit::window::Window) -> bool {
        if NSEvent::pressedMouseButtons() & 1 == 0 {
            return false;
        }
        let Some(event) = LAST.with(|last| last.borrow().clone()) else {
            return false;
        };
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return false;
        };
        // SAFETY: the borrowed winit window keeps its view alive; main thread.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let Some(ns_window) = view.window() else {
            return false;
        };
        if event.windowNumber() != ns_window.windowNumber() {
            return false;
        }
        ns_window.performWindowDragWithEvent(&event);
        true
    }
}

/// Whether AppKit has `window` as its key window.
#[cfg(target_os = "macos")]
fn is_key_window(window: &winit::window::Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    // SAFETY: the borrowed winit window keeps its NSView alive; this runs on
    // the main thread, handling the event loop's events.
    let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    view.window().is_some_and(|window| window.isKeyWindow())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn steady_frames_present_on_the_point_and_keep_their_cadence() {
        use std::time::{Duration, Instant};
        let period = Duration::from_nanos(8_333_333);
        let ms = |v: f64| Duration::from_secs_f64(v / 1e3);
        let origin = Instant::now();
        let phase =
            |at: Instant| (at.duration_since(origin).as_nanos() % period.as_nanos()) as f64 / 1e6;
        let point = (period - Latch::LEAD).as_secs_f64() * 1e3;
        let mut latch = Latch::default();
        // Ready before the point: it presents on this refresh's point.
        let ready = origin + period * 10 + ms(2.);
        let at = latch.decide(ready, origin, period);
        assert!((phase(at) - point).abs() < 0.01 && at - ready < period);
        // A 60 Hz stream ready right at the point, some frames just after it
        // and some just before, keeps one cadence: every frame two refreshes
        // after the last, all on the next refresh's point.
        let mut last = at;
        for (i, offset) in [0.1, -0.1, 0.2, -0.2, 0.05, -0.3].into_iter().enumerate() {
            let ready = origin + period * (12 + 2 * i as u32) + ms(point + offset);
            let at = latch.decide(ready, origin, period);
            assert!((phase(at) - point).abs() < 0.01);
            if i > 0 {
                assert_eq!(
                    (at.duration_since(last).as_secs_f64() / period.as_secs_f64()).round(),
                    2.
                );
            }
            last = at;
        }
        // Comfortably early again, frame after frame: back to the coming point.
        let mut at = latch.decide(origin + period * 30 + ms(point - 3.), origin, period);
        for i in 0..Latch::RETURN as u32 {
            let ready = origin + period * (32 + 2 * i) + ms(point - 3.);
            at = latch.decide(ready, origin, period);
            assert!(at - ready < ms(3.1) || i + 1 < Latch::RETURN as u32);
        }
        assert!((phase(at) - point).abs() < 0.01);
        // Never more than a refresh's wait, never in the past.
        let ready = origin + period * 40 + ms(7.);
        let at = latch.decide(ready, origin, period);
        assert!(at >= ready && at - ready < period);
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn held_up_frames_render_later_within_their_wait() {
        use std::time::{Duration, Instant};
        let period = Duration::from_nanos(8_333_333);
        let ms = |v: f64| Duration::from_secs_f64(v / 1e3);
        let origin = Instant::now();
        let point = (period - Latch::LEAD).as_secs_f64() * 1e3;
        let phase =
            |at: Instant| (at.duration_since(origin).as_nanos() % period.as_nanos()) as f64 / 1e6;
        let mut latch = Latch::default();
        // A 60 Hz stream that can render 1 ms into a refresh, ready 0.5 ms
        // later: about 3 ms to spare before its point, less `KEEP`.
        let mut frame = |i: u32, gpu: f64| {
            let could = origin + period * (10 + 2 * i) + ms(1.);
            let start = latch.hold(could, origin, period);
            latch.rendering(start);
            let at = latch.decide(start + ms(0.5), origin, period);
            latch.ran(ms(gpu));
            assert!((phase(at) - point).abs() < 0.01);
            assert!(at - could < period);
            start - could
        };
        for i in 0..4 {
            assert_eq!(frame(i, 0.3), Duration::ZERO);
        }
        // Held up once: chance. Twice within a few frames: the next renders
        // a step later, still on the same point.
        frame(4, 1.2);
        assert_eq!(frame(5, 1.2), Duration::ZERO);
        assert_eq!(frame(6, 0.3), Latch::STEP);
        assert_eq!(frame(7, 0.3), Latch::STEP);
        // Held up there too: no room for another step, back to the start.
        assert_eq!(frame(8, 1.2), Latch::STEP);
        assert_eq!(frame(9, 0.3), Duration::ZERO);
    }
    #[test]
    fn window_commands_preserve_recreation_options() {
        let mut options = WindowOptions::default();
        let mut state = WindowState::default();
        for command in [
            WindowCommand::Title("Document — saved".into()),
            WindowCommand::Size(640., 480.),
            WindowCommand::Visible(false),
            WindowCommand::Minimized(true),
            WindowCommand::Maximized(true),
            WindowCommand::Focus,
        ] {
            state.apply(&mut options, &command);
        }
        assert_eq!(options.title, "Document — saved");
        assert_eq!((options.width, options.height), (640., 480.));
        assert!(!state.visible);
        assert!(state.minimized && state.maximized && state.focus_requested);
        state.apply(&mut options, &WindowCommand::Visible(true));
        state.apply(&mut options, &WindowCommand::Minimized(false));
        state.apply(&mut options, &WindowCommand::Maximized(false));
        assert!(state.visible);
        assert!(!state.minimized && !state.maximized);
    }
    #[test]
    fn requested_window_size_rejects_nonfinite_and_unrepresentable_dimensions() {
        for value in [
            0.,
            -1.,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            u32::MAX as f64 + 1.,
        ] {
            assert!(!valid_window_size(value, 480.));
            assert!(!valid_window_size(640., value));
        }
        assert!(valid_window_size(640.5, 480.25));
    }
    #[test]
    fn resized_and_scaled_geometry_persists_for_native_recreation() {
        let mut options = WindowOptions::default();
        let ui = Ui::new(960., 720.);
        let viewport = ui.signal((960., 720.));
        assert!(apply_viewport(
            &mut options,
            &ui,
            &viewport,
            PhysicalSize::new(1800, 1200),
            2.
        ));
        assert_eq!((options.width, options.height), (900., 600.));
        assert_eq!(viewport.get(), (900., 600.));
        ui.scene.borrow_mut().prepare_layout();
        assert_eq!(ui.scene.borrow().bounds(ui.root()).width, 900.);
        assert!(!apply_viewport(
            &mut options,
            &ui,
            &viewport,
            PhysicalSize::new(0, 0),
            2.
        ));
        assert!(!apply_viewport(
            &mut options,
            &ui,
            &viewport,
            PhysicalSize::new(1800, 1200),
            0.
        ));
        assert_eq!((options.width, options.height), (900., 600.));
        assert!(apply_viewport(
            &mut options,
            &ui,
            &viewport,
            PhysicalSize::new(1800, 1200),
            1.5
        ));
        assert_eq!((options.width, options.height), (1200., 800.));
        assert_eq!(viewport.get(), (1200., 800.));
    }
    #[test]
    fn shortcuts_on_other_layouts_take_the_keys_us_letters() {
        use zgui::input::Modifiers;
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        let key = |c: &str| Key::Character(c.into());
        let code = |c| PhysicalKey::Code(c);
        // Russian: Ctrl and the C key, which types с, copy.
        assert_eq!(shortcut_key(key("с"), code(KeyCode::KeyC), ctrl), key("c"));
        assert_eq!(shortcut_key(key("ф"), code(KeyCode::KeyA), ctrl), key("a"));
        // A US letter, a key typed without Ctrl, and AltGr's text stay.
        assert_eq!(shortcut_key(key("q"), code(KeyCode::KeyA), ctrl), key("q"));
        assert_eq!(
            shortcut_key(key("с"), code(KeyCode::KeyC), Modifiers::default()),
            key("с")
        );
        let altgr = Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(shortcut_key(key("ą"), code(KeyCode::KeyA), altgr), key("ą"));
        assert_eq!(
            shortcut_key(Key::Enter, code(KeyCode::Enter), ctrl),
            Key::Enter
        );
    }
    #[test]
    fn native_cursor_roles_distinguish_editable_and_actionable_controls() {
        assert_eq!(cursor_for_role(Some(Role::Button)), CursorIcon::Pointer);
        assert_eq!(
            cursor_for_role(Some(Role::MultilineTextInput)),
            CursorIcon::Text
        );
        assert_eq!(cursor_for_role(Some(Role::Slider)), CursorIcon::EwResize);
        assert_eq!(cursor_for_role(None), CursorIcon::Default);
    }
    #[test]
    fn keyboard_space_encodings_and_unicode_are_preserved() {
        assert_eq!(map_key(&WKey::Character(" ".into())), Some(Key::Space));
        assert_eq!(map_key(&WKey::Named(NamedKey::Space)), Some(Key::Space));
        assert_eq!(
            map_key(&WKey::Character("é".into())),
            Some(Key::Character("é".into()))
        );
        assert_eq!(map_key(&WKey::Named(NamedKey::F1)), Some(Key::Function(1)));
        assert_eq!(
            map_key(&WKey::Named(NamedKey::F35)),
            Some(Key::Function(35))
        );
        assert_eq!(map_key(&WKey::Named(NamedKey::Insert)), Some(Key::Insert));
    }
    #[test]
    fn trackpad_phases_tell_the_fingers_from_their_momentum() {
        use winit::event::TouchPhase::{Ended, Moved, Started};
        use zgui::input::ScrollGesture::{self as G, Began, MomentumBegan, MomentumEnded};
        let start = std::time::Instant::now();
        let at = |ms| start + std::time::Duration::from_millis(ms);
        let mut pad = Trackpad::Idle;
        assert_eq!(pad.phase(Started, at(0)), Some(Began));
        assert_eq!(pad.phase(Moved, at(8)), None);
        assert_eq!(pad.phase(Ended, at(16)), Some(G::Ended));
        // A start right after the lift is the flick's momentum.
        assert_eq!(pad.phase(Started, at(24)), Some(MomentumBegan));
        assert_eq!(pad.phase(Moved, at(32)), None);
        assert_eq!(pad.phase(Ended, at(400)), Some(MomentumEnded));
        // Lifted without a flick, a later touch is a new gesture.
        assert_eq!(pad.phase(Started, at(1000)), Some(Began));
        assert_eq!(pad.phase(Ended, at(1100)), Some(G::Ended));
        assert_eq!(pad.phase(Started, at(1500)), Some(Began));
    }
    #[test]
    fn wheel_down_is_positive_in_logical_scene_coordinates() {
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(1., -2.), 2., (30., 40.)),
            (-30., 80.)
        );
        // Windows: a precision touchpad's fractions of a notch are motion.
        assert!(!continuous_lines(0., -1.));
        assert!(!continuous_lines(2., 0.));
        assert_eq!(continuous_lines(0., 10. / 120.), cfg!(windows));
        assert_eq!(continuous_lines(-1.25, 0.), cfg!(windows));
        assert_eq!(
            wheel_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(20., -40.)),
                2.,
                (30., 40.)
            ),
            (-10., 20.)
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an owned X11 display; run alone with --test-threads=1"]
    fn retained_window_factory_releases_native_clipboard_after_manager_shutdown() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let event_loop = EventLoop::<Event>::with_user_event()
            .with_x11()
            .with_any_thread(true)
            .build()
            .unwrap();
        let factory = WindowFactory {
            fonts: Rc::new(Vec::new()),
            #[cfg(target_os = "linux")]
            file_routes: Default::default(),
            #[cfg(target_os = "linux")]
            application_id: None,
            #[cfg(target_os = "linux")]
            activation: Rc::default(),
            menus: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            next: Rc::new(Cell::new(1)),
            alive: Rc::new(Cell::new(true)),
            clipboard: Rc::new(RefCell::new(Some(
                crate::clipboard::Clipboard::new(
                    event_loop.owned_display_handle(),
                    #[cfg(target_os = "linux")]
                    None,
                )
                .unwrap(),
            ))),
            gpu_contexts: Rc::new(RefCell::new(Vec::new())),
            proxy: event_loop.create_proxy(),
            runtime: zgui::reactive::Runtime::new(),
        };
        let retained = factory.clone();
        let manager = WindowManager {
            #[cfg(target_os = "linux")]
            native_data: None,
            #[cfg(target_os = "linux")]
            appearance: None,
            _native_events: None,
            application_event: None,
            quit_on_last_window_close: true,
            #[cfg(target_os = "macos")]
            native_menu: None,
            hosts: HashMap::new(),
            panics: Vec::new(),
            native: HashMap::new(),
            drag_over: Cell::new(None),
            carrying: None,
            #[cfg(target_os = "linux")]
            last_focused: None,
            factory,
            active: false,
            error: None,
            timers: zgui::timer::host(|| {}),
        };
        assert!(retained.clipboard.borrow().is_some());
        drop(manager);
        assert!(!retained.alive.get());
        assert!(retained.clipboard.borrow().is_none());
        let called = Rc::new(Cell::new(false));
        let callback = called.clone();
        let handle = retained.open(WindowOptions::default(), move |_| callback.set(true));
        assert!(handle.is_closed());
        assert!(!called.get());
        assert!(retained.pending.borrow().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an owned X11 display and GPU; run alone with --test-threads=1"]
    fn host_suspend_recreates_native_resources_and_retains_editor_state() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        struct Probe {
            factory: WindowFactory,
            completed: bool,
        }
        impl ApplicationHandler<Event> for Probe {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                let handle = self.factory.allocate();
                let mut host = Host::new(WindowOptions::default(), handle, self.factory.clone());
                let value = host.context.ui.signal(String::from("retained document"));
                let editor = host.context.ui.text_input(
                    host.context.ui.root(),
                    "Document",
                    value.clone(),
                    300.,
                    false,
                );
                host.create_window(event_loop).unwrap();
                let window = Arc::downgrade(host.window.as_ref().unwrap());
                let first_id = host.window.as_ref().unwrap().id();
                host.create_window(event_loop).unwrap();
                assert_eq!(host.window.as_ref().unwrap().id(), first_id);
                host.window_event(WindowEvent::Focused(true)).unwrap();
                host.dispatch(InputEvent::PointerDown {
                    x: 10.,
                    y: 10.,
                    button: PointerButton::Primary,
                })
                .unwrap();
                assert_eq!(host.context.ui.input.captured(), Some(editor.node));
                host.dispatch(InputEvent::ImePreedit {
                    text: "composition".into(),
                    cursor: None,
                })
                .unwrap();
                assert!(editor.editor.borrow().preedit().is_some());
                host.ime_target = Some(editor.node);
                host.ime_active = true;
                host.presentation.damaged();
                host.presentation.completed(
                    zgui_gpu::PresentationStatus::Timeout,
                    std::time::Instant::now(),
                );
                assert!(host.presentation.deadline().is_some());
                host.suspend();
                host.suspend(); // Platform callbacks may repeat.
                assert!(host.window.is_none() && host.renderer.is_none() && host.adapter.is_none());
                assert!(window.upgrade().is_none());
                assert!(!host.native_focused && !host.ime_active);
                assert!(host.ime_target.is_none() && host.ime_geometry.is_none());
                assert!(host.context.ui.input.captured().is_none());
                assert!(host.context.ui.next_interaction_deadline().is_none());
                assert!(host.presentation.deadline().is_none());
                assert!(editor.editor.borrow().preedit().is_none());
                assert_eq!(value.get(), "retained document");
                assert!(host.context.ui.scene.borrow().contains(editor.node));
                value.set("updated while suspended".into());
                assert_eq!(editor.editor.borrow().text(), "updated while suspended");
                host.create_window(event_loop).unwrap();
                assert!(host.window.is_some() && host.renderer.is_some() && host.adapter.is_some());
                assert!(host.force);
                host.window_event(WindowEvent::Focused(true)).unwrap();
                assert_eq!(host.context.ui.input.focused(), Some(editor.node));
                assert_eq!(editor.editor.borrow().text(), "updated while suspended");
                host.context.ui.prepare_frame();
                let scene = host.context.ui.scene.borrow();
                let damage = [scene.bounds(scene.root())];
                host.renderer
                    .as_mut()
                    .unwrap()
                    .render(&scene, &damage)
                    .unwrap();
                drop(scene);
                host.suspend();
                self.completed = true;
                event_loop.exit();
            }
            fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        }
        let event_loop = EventLoop::<Event>::with_user_event()
            .with_x11()
            .with_any_thread(true)
            .build()
            .unwrap();
        let factory = WindowFactory {
            fonts: Rc::new(Vec::new()),
            #[cfg(target_os = "linux")]
            file_routes: Default::default(),
            #[cfg(target_os = "linux")]
            application_id: None,
            #[cfg(target_os = "linux")]
            activation: Rc::default(),
            menus: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            next: Rc::new(Cell::new(1)),
            alive: Rc::new(Cell::new(true)),
            clipboard: Rc::new(RefCell::new(None)),
            gpu_contexts: Rc::new(RefCell::new(Vec::new())),
            proxy: event_loop.create_proxy(),
            runtime: zgui::reactive::Runtime::new(),
        };
        let mut probe = Probe {
            factory,
            completed: false,
        };
        event_loop.run_app(&mut probe).unwrap();
        assert!(probe.completed);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_closing_windows_metal_layers_leave_its_view_alone() {
        use objc2::{ClassType, msg_send, rc::Retained, runtime::AnyClass};
        use objc2_quartz_core::CALayer;
        let metal = AnyClass::get(c"CAMetalLayer").unwrap();
        let host = CALayer::new();
        let other = CALayer::new();
        // Metal layers on both sides of another: each removal shifts the
        // list under its enumeration.
        for index in 0..3 {
            let sublayer: Retained<CALayer> = if index == 1 {
                other.clone()
            } else {
                // SAFETY: CAMetalLayer is a CALayer, made by `new`.
                unsafe { msg_send![metal, new] }
            };
            host.addSublayer(&sublayer);
        }
        super::remove_sublayers(&host, metal);
        let left: Vec<_> = unsafe { host.sublayers() }
            .unwrap()
            .iter()
            .map(|layer| layer.class() == CALayer::class())
            .collect();
        assert_eq!(left, [true]);
    }

    #[test]
    fn window_handles_can_close_from_worker_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowHandle>();
    }
}
