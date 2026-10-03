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
    keyboard::{Key as WKey, NamedKey},
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    window::{CursorIcon, Window, WindowId},
};
use zgui::{
    frame::{Frame, FrameClock},
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Signal,
    scene::{NodeId, Rect},
    semantics::Role,
    task::LocalExecutor,
    widgets::Ui,
};
use zgui_gpu::{GpuContext, GpuRenderer};

#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub transparent: bool,
    pub resizable: bool,
    pub decorations: bool,
    pub fullscreen: bool,
    /// Logical minimum client size.
    pub min_size: Option<(f64, f64)>,
    /// Initial physical bounds. Placement remains compositor policy on Wayland.
    pub bounds: Option<crate::WindowBounds>,
    pub display: Option<crate::DisplaySelector>,
    /// Upper bound on animation frames per second (see [`zgui::frame`]), for
    /// example to save power. Input-driven redraws are not limited. `None`
    /// follows the display.
    pub max_frame_rate: Option<f64>,
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
        }
    }
}
#[derive(Debug)]
enum Event {
    Quit,
    Application(crate::ApplicationEvent),
    MenuAction(u64, crate::MenuAction),
    MenuFallback(u64, crate::MenuAction),
    MenusChanged,
    #[cfg(target_os = "linux")]
    NativeFiles(u64, crate::wayland_files::FileEvent),
    #[cfg(target_os = "macos")]
    NativeMenuAction(crate::MenuAction),
    Wake(u64),
    Redraw(u64),
    /// A native display refresh (macOS; other platforms use timers).
    #[cfg(target_os = "macos")]
    Frame(u64, Frame),
    Open,
    Accessibility(accesskit_winit::Event),
    Close(u64),
    RequestClose(u64),
    WindowCommand(u64, WindowCommand),
}
/// Commands are queued onto the UI thread; the operating system may decline requests.
#[derive(Debug)]
enum WindowCommand {
    Bounds(crate::WindowBounds),
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
            WindowCommand::MinSize(value) => options.min_size = *value,
            WindowCommand::Drag | WindowCommand::RefreshInfo => {}
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
    pub fn set_bounds(&self, bounds: crate::WindowBounds) {
        if bounds.size.0 > 0 && bounds.size.1 > 0 {
            self.command(WindowCommand::Bounds(bounds));
        }
    }
    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.command(WindowCommand::Fullscreen(fullscreen));
    }
    pub fn set_decorations(&self, decorations: bool) {
        self.command(WindowCommand::Decorations(decorations));
    }
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
    pub fn refresh_window_info(&self) {
        self.command(WindowCommand::RefreshInfo);
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
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    pub fn request_redraw(&self) {
        if !self.closed.load(Ordering::Acquire) {
            let _ = self.proxy.send_event(Event::Redraw(self.token));
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
    file_routes: crate::wayland_files::Routes,
    fonts: Rc<Vec<zgui_gpu::text::FontData>>,
    menus: Rc<RefCell<Vec<crate::AppMenu>>>,
    pending: Rc<RefCell<VecDeque<WindowRequest>>>,
    next: Rc<Cell<u64>>,
    alive: Rc<Cell<bool>>,
    clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
    gpu_contexts: Rc<RefCell<Vec<GpuContext>>>,
    proxy: EventLoopProxy<Event>,
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
pub struct WindowContext {
    pub dialogs: crate::FileDialogs,
    pub window_info: crate::WindowInfo,
    menu_model: Signal<Vec<crate::AppMenu>>,
    pub ui: Ui,
    pub tasks: TaskSpawner,
    pub window: WindowHandle,
    pub windows: WindowFactory,
    /// Logical viewport dimensions; reactive layouts update on resize and scale changes.
    pub viewport: Signal<(f32, f32)>,
    /// Display-paced animation frames for this window. Components reach it
    /// through `Context::frames`.
    pub frames: FrameClock,
    close_requested: Option<Box<dyn FnMut() -> bool>>,
    closed: Option<Box<dyn FnOnce()>>,
    menu_action: Option<Box<dyn FnMut(crate::MenuAction)>>,
}
impl WindowContext {
    /// The live native window; unavailable during initial UI construction.
    /// Clone `dialogs` and call its `native_window()` later for deferred access.
    pub fn native_window(&self) -> Option<Arc<Window>> {
        self.dialogs.native_window()
    }

    /// Application command fallback after focused component action routing.
    pub fn on_menu_action(&mut self, handler: impl FnMut(crate::MenuAction) + 'static) {
        self.menu_action = Some(Box::new(handler));
    }
    /// A rendered menu bar for Linux and Windows; macOS uses the system menu bar.
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
        self.ui.render(zgui::compose::provide(
            self.frames.clone(),
            zgui::compose::provide(runner, view),
        ))
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
pub struct Application {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    message_pump: Option<Box<dyn FnMut()>>,
    fonts: Vec<zgui_gpu::text::FontData>,
    options: WindowOptions,
    menus: Vec<crate::AppMenu>,
    application_id: Option<String>,
    application_event: Option<ApplicationEventHandler>,
    quit_on_last_window_close: bool,
}
type ApplicationEventHandler = Box<dyn FnMut(crate::ApplicationEvent, &WindowFactory)>;
impl Application {
    pub fn new() -> Self {
        Self {
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            message_pump: None,
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
    pub fn window(mut self, options: WindowOptions) -> Self {
        self.options = options;
        self
    }
    pub fn menus(mut self, menus: Vec<crate::AppMenu>) -> Result<Self, crate::AppMenuError> {
        crate::app_menu::validate(&menus)?;
        self.menus = menus;
        Ok(self)
    }
    /// Register native URL/reopen delivery. Linux exports org.freedesktop.Application;
    /// macOS owns GetURL, OpenDocuments, and Reopen AppleEvent routes for the
    /// event loop lifetime while preserving its NSApplication delegate. Do not
    /// install competing handlers for these routes while this service is active.
    /// Windows has no activation backend; opting in makes `run` return an error.
    pub fn application_id(
        mut self,
        id: impl Into<String>,
    ) -> Result<Self, crate::ApplicationEventError> {
        let id = id.into();
        crate::native_events::validate_id(&id)?;
        self.application_id = Some(id);
        Ok(self)
    }
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
    /// Run with a native-message pump, such as `CefDoMessageLoopWork`.
    ///
    /// On Windows the pump runs before native message dispatch, with no
    /// application callback active. Calling a foreign pump from a UI task can
    /// otherwise starve painting: winit requeues paints during nested callbacks.
    /// The caller must arrange wakeups while idle, for example with a timed task.
    /// On macOS a 10 ms default-mode timer invokes it outside winit callbacks;
    /// pumping pauses during native modal and tracking loops.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub fn run_with_pump(
        mut self,
        pump: impl FnMut() + 'static,
        build: impl FnOnce(&mut WindowContext),
    ) -> Result<(), Box<dyn Error>> {
        self.message_pump = Some(Box::new(pump));
        self.run(build)
    }

    pub fn run(self, build: impl FnOnce(&mut WindowContext)) -> Result<(), Box<dyn Error>> {
        let mut builder = EventLoop::<Event>::with_user_event();
        #[cfg(target_os = "windows")]
        if let Some(mut pump) = self.message_pump {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_msg_hook(move |_| {
                pump();
                false
            });
        }
        let event_loop = builder.build()?;
        let proxy = event_loop.create_proxy();
        let native_events = if let Some(id) = &self.application_id {
            let wake = proxy.clone();
            Some(crate::native_events::NativeApplicationEvents::new(
                id,
                Arc::new(move |event| {
                    let _ = wake.send_event(Event::Application(event));
                }),
            )?)
        } else {
            None
        };
        #[cfg(target_os = "macos")]
        let native_menu = {
            let wake = proxy.clone();
            muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
                let _ = wake.send_event(Event::NativeMenuAction(crate::MenuAction(event.id.0)));
            }));
            if self.menus.is_empty() {
                None
            } else {
                Some(crate::app_menu::NativeMenu::new(&self.menus)?)
            }
        };
        let factory = WindowFactory {
            fonts: Rc::new(self.fonts),
            #[cfg(target_os = "linux")]
            file_routes: Default::default(),
            menus: Rc::new(RefCell::new(self.menus)),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            next: Rc::new(Cell::new(1)),
            alive: Rc::new(Cell::new(true)),
            clipboard: Rc::new(RefCell::new(None)),
            gpu_contexts: Rc::new(RefCell::new(Vec::new())),
            proxy: proxy.clone(),
        };
        let handle = factory.allocate();
        let token = handle.token;
        let mut root = Host::new(self.options, handle, factory.clone());
        build(&mut root.context);
        let mut host = WindowManager {
            #[cfg(target_os = "linux")]
            native_files: None,
            _native_events: native_events,
            application_event: self.application_event,
            quit_on_last_window_close: self.quit_on_last_window_close,
            #[cfg(target_os = "macos")]
            native_menu,
            hosts: HashMap::from([(token, root)]),
            native: HashMap::new(),
            factory,
            active: false,
            error: None,
        };
        #[cfg(target_os = "macos")]
        let message_pump = self
            .message_pump
            .map(crate::message_pump_macos::MessagePump::new);
        let result = event_loop.run_app(&mut host);
        #[cfg(target_os = "macos")]
        drop(message_pump);
        result?;
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
/// After the first presented frame, when to release startup's freed heap.
const STARTUP_TRIM_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartupTrim {
    Pending,
    At(std::time::Instant),
    Done,
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
    adapter: Option<accesskit_winit::Adapter>,
    tree: AccessibilityTree,
    proxy: EventLoopProxy<Event>,
    tasks: LocalExecutor,
    pending: Rc<RefCell<VecDeque<FutureTask>>>,
    clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
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
    present_ready: bool,
    frames: FrameClock,
    #[cfg(target_os = "macos")]
    display_link: Option<crate::frame_source::DisplayLink>,
    frame_timer: crate::frame_source::FrameTimer,
    frame_interval: std::time::Duration,
    /// A display refresh arrived since the last paint. While the display link
    /// runs, paints wait for it so input between refreshes cannot present
    /// extra frames (which would queue drawables and block the UI thread).
    frame_arrived: bool,
    /// When to release animation-only renderer memory if nothing presents
    /// before then (see `GpuRenderer::trim`).
    trim_at: Option<std::time::Instant>,
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
    native_cursor: Option<CursorIcon>,
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
}
impl Host {
    fn new(options: WindowOptions, handle: WindowHandle, windows: WindowFactory) -> Self {
        let proxy = handle.proxy.clone();
        let wake = proxy.clone();
        let token = handle.token;
        let pending = Rc::new(RefCell::new(VecDeque::new()));
        let ui = Ui::new(options.width as f32, options.height as f32);
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
        let context = WindowContext {
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
            cursor: (0., 0.),
            scroll: Default::default(),
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
            present_ready: false,
            gpu_stats: crate::gpu_stats::GpuStatsLog::from_env(),
            damage_check: std::env::var_os("ZGUI_DAMAGE_CHECK").map(|_| 0),
            frames,
            #[cfg(target_os = "macos")]
            display_link: None,
            frame_arrived: false,
            trim_at: None,
            startup_trim: StartupTrim::Pending,
            frame_timer: crate::frame_source::FrameTimer::new(std::time::Instant::now()),
            frame_interval: crate::frame_source::DEFAULT_INTERVAL,
            frame_deadline: None,
            frame_heartbeat: None,
            frame_redraw_requested: false,
            focus_memory: crate::focus_memory::FocusMemory::default(),
            last_accessibility: None,
            accessibility_geometry_dirty: true,
            native_cursor: None,
            cursor_query: None,
            pointer_inside: false,
            ime_geometry: None,
            ime_target: None,
        }
    }
    fn dispatch(&mut self, event: InputEvent) -> Result<(), zgui::widgets::LayoutFeedbackError> {
        self.dispatch_prevented(event).map(|_| ())
    }
    fn dispatch_prevented(
        &mut self,
        event: InputEvent,
    ) -> Result<bool, zgui::widgets::LayoutFeedbackError> {
        let result = self
            .context
            .ui
            .try_dispatch_with_modifiers(event, self.modifiers)?;
        if result.focus_changed {
            self.ime_active = false;
        }
        Ok(result.default_prevented)
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
        self.sync_ime_cancellation();
        let scene = self.context.ui.scene.clone();
        let mut scene = scene.borrow_mut();
        let report = scene.flush();
        self.accessibility_geometry_dirty |= !report.damage.is_empty();
        let full = [scene.bounds(scene.root())];
        let damage = if self.force {
            &full[..]
        } else {
            &report.damage
        };
        if !damage.is_empty() {
            if self.frame_paced && !self.present_ready {
                // Layout/model work proceeds, but defer GPU work until a frame
                // is granted. A full repaint retains damage across scene flushes.
                self.force = true;
                window.request_redraw();
            } else {
                if let Some(renderer) = &mut self.renderer {
                    let stats = renderer.render(&scene, damage)?;
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
        if self.presentation.ready(std::time::Instant::now())
            && let Some(renderer) = &mut self.renderer
        {
            if self.frame_paced && !self.present_ready {
                // winit withholds RedrawRequested until the compositor grants a
                // new frame. A covered surface must never block vkQueuePresent
                // on the shared UI thread. Keep its rendered target and damage.
                window.request_redraw();
            } else {
                let status = renderer.present_with_notify(|| window.pre_present_notify())?;
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
        drop(scene);
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
        if Some(cursor) != self.native_cursor {
            window.set_cursor(cursor);
            self.native_cursor = Some(cursor);
        }
        let editor = crate::native_ime::editable_editor(ui, self.native_focused);
        let caret = editor.map(|editor| ui.scene.borrow().bounds(editor.caret));
        if caret != self.ime_geometry {
            if let Some(r) = caret {
                // XIM takes a baseline spot and ignores the supplied rectangle
                // height; Wayland/macOS consume the actual caret rectangle.
                let y = if self.x11 { r.y + r.height } else { r.y };
                window.set_ime_cursor_area(
                    LogicalPosition::new(r.x as f64, y as f64)
                        .to_physical::<f64>(self.scale_factor),
                    LogicalSize::new(r.width.max(1.) as f64, r.height as f64)
                        .to_physical::<f64>(self.scale_factor),
                );
            }
            self.ime_geometry = caret;
        }
        self.sync_accessibility();
        Ok(())
    }
    fn clipboard_key(&mut self, key: &Key) -> bool {
        if !self.modifiers.primary_shortcut() {
            return false;
        }
        let Key::Character(k) = key else {
            return false;
        };
        let Some(editor) = self.context.ui.focused_editor() else {
            return false;
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
                if let Some(accesskit::ActionData::SetTextSelection(selection)) = request.data
                    && let Some((owner, anchor, focus)) = self.tree.resolve_selection(&selection)
                    && owner == node
                {
                    self.context
                        .ui
                        .set_accessible_text_selection(node, anchor, focus);
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
    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        if self.window.is_some() {
            return Ok(());
        }

        let window = Arc::new(
            event_loop.create_window(crate::window_info::attributes(&self.options, event_loop)?)?,
        );
        self.x11 = window.display_handle().is_ok_and(|handle| {
            matches!(
                handle.as_raw(),
                RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
            )
        });
        let adapter = accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        );
        let contexts = self.context.windows.gpu_contexts.borrow().clone();
        let mut shared_renderer = None;
        for context in contexts {
            match GpuRenderer::for_window_with_context(window.clone(), &context) {
                Ok(renderer) => {
                    shared_renderer = Some(renderer);
                    break;
                }
                Err(error) if error.is_surface_incompatible() => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let mut renderer = match shared_renderer {
            Some(renderer) => renderer,
            None => {
                let renderer = GpuRenderer::for_window(window.clone())?;
                for font in self.context.windows.fonts.iter() {
                    font.install(&mut renderer.text_system().borrow_mut());
                }
                self.context
                    .windows
                    .gpu_contexts
                    .borrow_mut()
                    .push(renderer.context());
                renderer
            }
        };
        self.scale_factor = window.scale_factor();
        renderer.set_scale_factor(self.scale_factor as f32);
        renderer.set_background(if self.options.transparent {
            zgui::scene::Color(0, 0, 0, 0)
        } else {
            self.context.ui.theme.background
        });
        renderer.install_text(&mut self.context.ui.scene.borrow_mut());
        self.context.ui.refresh_text_geometry();
        self.renderer = Some(renderer);
        self.adapter = Some(adapter);
        window.set_maximized(self.state.maximized);
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
        self.context.window_info.refresh(&window);
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
            WindowCommand::Decorations(value) => window.set_decorations(value),
            WindowCommand::MinSize(value) => {
                window.set_min_inner_size(value.map(|(w, h)| LogicalSize::new(w, h)))
            }
            WindowCommand::Drag => {
                self.context
                    .window_info
                    .last_request_error
                    .set(window.drag_window().err().map(|e| e.to_string()));
            }
            WindowCommand::RefreshInfo => self.context.window_info.refresh(window),
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
    fn deactivate(&mut self) {
        self.context.ui.cancel_interactions();
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
            self.display_link = None;
        }
        self.frame_deadline = None;
        self.frame_heartbeat = None;
        self.frame_redraw_requested = false;
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
            self.context.window_info.refresh(window);
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
            if window.fullscreen().is_none() && !maximized {
                let bounds = crate::WindowBounds::read(window);
                if bounds.size.0 > 0 && bounds.size.1 > 0 {
                    self.options.bounds = Some(bounds);
                }
            }
        }
        self.sync_ime_cancellation();
        if let (Some(adapter), Some(window)) = (&mut self.adapter, &self.window) {
            adapter.process_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => {}
            WindowEvent::Occluded(true) => {
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
                self.force = true;
            }
            WindowEvent::RedrawRequested => {
                self.present_ready = true;
                self.presentation.expose();
                self.force = true;
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
                if apply_viewport(
                    &mut self.options,
                    &self.context.ui,
                    &self.context.viewport,
                    size,
                    scale,
                ) {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(size.width, size.height);
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
                self.dispatch(InputEvent::PointerMove { x: p.x, y: p.y })?;
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer_inside = false;
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
                let button = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    MouseButton::Back => PointerButton::Other(4),
                    MouseButton::Forward => PointerButton::Other(5),
                    MouseButton::Other(n) => PointerButton::Other(n),
                };
                let (x, y) = self.cursor;
                self.dispatch(if state == ElementState::Pressed {
                    InputEvent::PointerDown { x, y, button }
                } else {
                    InputEvent::PointerUp { x, y, button }
                })?;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = wheel_delta(delta, self.scale_factor);
                if crate::scroll_resample::enabled() {
                    // Applied on display refreshes; see `update`.
                    let now = std::time::Instant::now();
                    self.scroll.position = self.cursor;
                    match delta {
                        MouseScrollDelta::PixelDelta(_) => self.scroll.push_precise(now, (dx, dy)),
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
            }
            WindowEvent::Focused(false) => self.deactivate(),
            WindowEvent::Focused(true) => {
                self.native_focused = true;
                self.focus_memory.activate(&self.context.ui);
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
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(&event.logical_key);
                let mut prevented = false;
                if let Some(key) = key
                    && !self.ime_active
                {
                    if event.state == ElementState::Pressed {
                        prevented = self.dispatch_prevented(InputEvent::KeyDown {
                            key: key.clone(),
                            modifiers: self.modifiers,
                            repeat: event.repeat,
                        })?;
                        if !prevented {
                            prevented = self.clipboard_key(&key);
                        }
                    } else {
                        self.dispatch(InputEvent::KeyUp {
                            key,
                            modifiers: self.modifiers,
                        })?;
                    }
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
        if let StartupTrim::At(at) = self.startup_trim
            && std::time::Instant::now() >= at
        {
            self.startup_trim = StartupTrim::Done;
            release_freed_heap();
        }
        if self
            .trim_at
            .is_some_and(|at| std::time::Instant::now() >= at)
        {
            self.trim_at = None;
            if let Some(renderer) = &mut self.renderer {
                renderer.trim();
            }
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
        #[cfg(target_os = "macos")]
        let paced = self
            .display_link
            .as_ref()
            .is_some_and(|link| link.running());
        #[cfg(not(target_os = "macos"))]
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
        #[cfg(target_os = "macos")]
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
    /// Start, stop or schedule this window's frame source to match demand.
    fn sync_frames(&mut self) {
        let now = std::time::Instant::now();
        let scrolling = self.scroll.active(now);
        let wanted = (self.frames.wants_frame() || scrolling) && self.frames_presented();
        if !wanted {
            self.frame_heartbeat = None;
            self.frame_deadline = None;
        }
        #[cfg(target_os = "macos")]
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
        #[cfg(target_os = "macos")]
        if let Some(link) = &mut self.display_link {
            link.set_running(wanted);
            if wanted {
                let refresh = 1. / self.frame_interval.as_secs_f64();
                // Scrolling always runs at the display's full rate.
                link.set_preferred_rate(
                    self.frames
                        .demanded_rate(refresh)
                        .filter(|rate| !scrolling && *rate < refresh - 0.5),
                );
                let heartbeat = *self.frame_heartbeat.get_or_insert(now);
                self.frame_deadline = Some(heartbeat + crate::frame_source::WATCHDOG);
            }
            return;
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
    }
}
struct WindowManager {
    #[cfg(target_os = "linux")]
    native_files: Option<crate::wayland_files::NativeFiles>,
    _native_events: Option<crate::native_events::NativeApplicationEvents>,
    application_event: Option<ApplicationEventHandler>,
    quit_on_last_window_close: bool,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::app_menu::NativeMenu>,
    hosts: HashMap<u64, Host>,
    native: HashMap<WindowId, u64>,
    factory: WindowFactory,
    active: bool,
    error: Option<String>,
}
impl WindowManager {
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
impl ApplicationHandler<Event> for WindowManager {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        if let Some(events) = &self._native_events {
            events.install();
        }
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            menu.install();
        }
        self.active = true;
        #[cfg(target_os = "linux")]
        if self.native_files.is_none() {
            let proxy = self.factory.proxy.clone();
            match crate::wayland_files::NativeFiles::new(
                event_loop.owned_display_handle(),
                self.factory.file_routes.clone(),
                Arc::new(move |token, event| {
                    let _ = proxy.send_event(Event::NativeFiles(token, event));
                }),
            ) {
                Ok(service) => self.native_files = service,
                Err(error) => eprintln!("zgui: native file drops unavailable: {error}"),
            }
        }
        if self.factory.clipboard.borrow().is_none() {
            match crate::clipboard::Clipboard::new(event_loop.owned_display_handle()) {
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
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.active = false;
        self.native.clear();
        for host in self.hosts.values_mut() {
            host.suspend();
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::Quit => event_loop.exit(),
            Event::Application(event) => {
                if let Some(handler) = &mut self.application_event {
                    handler(event, &self.factory);
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
                    use crate::wayland_files::FileEvent;
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
                let focused = self
                    .hosts
                    .iter()
                    .find(|(_, host)| host.native_focused && !host.context.window.is_closed())
                    .map(|(token, _)| *token);
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
            Event::Wake(token) => {
                let _ = self.hosts.get(&token);
            }
            #[cfg(target_os = "macos")]
            Event::Frame(token, frame) => {
                if let Some(host) = self.hosts.get_mut(&token)
                    && !host.context.window.is_closed()
                {
                    // `about_to_wait` runs next in this same loop pass, stepping
                    // the woken animations and painting once.
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
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
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
        if let Some(host) = self.hosts.get_mut(&token)
            && let Err(e) = host.window_event(event)
        {
            self.fail(event_loop, e);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
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
        if !self.active {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if let Err(e) = self.create_pending(event_loop) {
            self.fail(event_loop, e);
            return;
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
                    host.trim_at,
                    match host.startup_trim {
                        StartupTrim::At(at) => Some(at),
                        _ => None,
                    },
                ]
            })
            .flatten()
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
fn wheel_delta(delta: MouseScrollDelta, scale: f64) -> (f32, f32) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (-x * 28., -y * 28.),
        MouseScrollDelta::PixelDelta(p) => {
            let p = p.to_logical::<f32>(scale);
            (-p.x, -p.y)
        }
    }
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
    renderer.render(scene, full)?;
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
#[cfg(test)]
mod tests {
    use super::*;
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
    fn wheel_down_is_positive_in_logical_scene_coordinates() {
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(1., -2.), 2.),
            (-28., 56.)
        );
        assert_eq!(
            wheel_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(20., -40.)),
                2.
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
            menus: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            next: Rc::new(Cell::new(1)),
            alive: Rc::new(Cell::new(true)),
            clipboard: Rc::new(RefCell::new(Some(
                crate::clipboard::Clipboard::new(event_loop.owned_display_handle()).unwrap(),
            ))),
            gpu_contexts: Rc::new(RefCell::new(Vec::new())),
            proxy: event_loop.create_proxy(),
        };
        let retained = factory.clone();
        let manager = WindowManager {
            #[cfg(target_os = "linux")]
            native_files: None,
            _native_events: None,
            application_event: None,
            quit_on_last_window_close: true,
            #[cfg(target_os = "macos")]
            native_menu: None,
            hosts: HashMap::new(),
            native: HashMap::new(),
            factory,
            active: false,
            error: None,
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

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    #[ignore = "requires a desktop and GPU; run alone with --test-threads=1"]
    fn host_suspend_recreates_native_resources_and_retains_editor_state() {
        #[cfg(target_os = "windows")]
        use winit::platform::windows::EventLoopBuilderExtWindows;
        #[cfg(target_os = "linux")]
        use winit::platform::x11::EventLoopBuilderExtX11;
        struct Probe {
            factory: WindowFactory,
            completed: bool,
        }
        impl ApplicationHandler<Event> for Probe {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                let handle = self.factory.allocate();
                let mut host = Host::new(WindowOptions::default(), handle, self.factory.clone());
                let native_provider = host.context.dialogs.clone();
                assert!(native_provider.native_window().is_none());
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
                assert_eq!(native_provider.native_window().unwrap().id(), first_id);
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
                assert!(native_provider.native_window().is_none());
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
                assert!(native_provider.native_window().is_some());
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
        let mut builder = EventLoop::<Event>::with_user_event();
        #[cfg(target_os = "linux")]
        builder.with_x11();
        let event_loop = builder.with_any_thread(true).build().unwrap();
        let factory = WindowFactory {
            fonts: Rc::new(Vec::new()),
            #[cfg(target_os = "linux")]
            file_routes: Default::default(),
            menus: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            next: Rc::new(Cell::new(1)),
            alive: Rc::new(Cell::new(true)),
            clipboard: Rc::new(RefCell::new(None)),
            gpu_contexts: Rc::new(RefCell::new(Vec::new())),
            proxy: event_loop.create_proxy(),
        };
        let mut probe = Probe {
            factory,
            completed: false,
        };
        event_loop.run_app(&mut probe).unwrap();
        assert!(probe.completed);
    }

    #[test]
    fn window_handles_can_close_from_worker_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowHandle>();
    }
}
