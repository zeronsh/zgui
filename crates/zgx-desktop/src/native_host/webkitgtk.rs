//! Web pages on Linux (the `webkitgtk` feature): WebKitGTK pages drawn
//! offscreen and shown as images.
//!
//! A page is a `WebKitWebView` in a `GtkOffscreenWindow`, on a thread of
//! GTK's own that runs its main loop (`gtk_main`): GLib's loop never enters
//! winit's, and an idle page costs nothing but that thread's wait. What the
//! page draws is copied into a frame the two threads share (`Frame`); the
//! page's box is a GPU view (`zgx::gpu_view`) whose painter uploads the
//! frame's damage into a texture, so the page is clipped, rounded, faded and
//! transformed as an image is, and zgx content over it is ordinary paint, on
//! X11 and Wayland alike. Pointer and wheel input reach the box as any
//! node's does and go to the page as GDK events; keys go to a focused page
//! before zgx hears them (`page_key`), and those it leaves alone come back
//! to the app's shortcuts (`take_unhandled_keys`), as a native page's do;
//! an input method composes through an input method context of zgx's
//! (`ZgxInputMethodContext`), fed by the window's.
//!
//! GTK's side keeps pages by id (`Gtk`), reached from the UI thread only
//! through jobs (`on_gtk`); it reports back through deliveries (`deliver`),
//! run at the UI thread's next safe point (`deliver_pages`).
use super::gtk::{self, Api, Ptr, c, connect, string};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    ffi::{c_char, c_int, c_uint, c_void},
    ptr::null_mut,
    rc::{Rc, Weak},
    sync::{Arc, Mutex, OnceLock},
};
use zgx::{
    compose::{View, component, div, gpu_view, menu_item, switch},
    input::{EventContext, InputEvent, Modifiers, PointerButton},
    scene::{NodeId, Rect, Scene},
    style::Styled,
    web_view::{
        NavigationKind, NavigationRequest, PageViews, SchemeHandler, SchemeRequest,
        SchemeResponder, SchemeResponse, ScriptResult, WeakWebView, WebProfile, WebView,
        WebViewBackend, WebViewCommand, WebViewEvent,
    },
    widgets::Ui,
};
use zgx_gpu::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

// The two threads.

type Job = Box<dyn FnOnce(&Gtk) + Send>;
type Delivery = Box<dyn FnOnce() + Send>;
type Waker = Arc<dyn Fn() + Send + Sync>;
static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static DELIVERIES: Mutex<Vec<Delivery>> = Mutex::new(Vec::new());
/// The windows' wakers, by window: a delivery wakes the first.
static WAKERS: Mutex<Vec<(u64, Waker)>> = Mutex::new(Vec::new());

/// GTK runs: a page was asked for and WebKitGTK is there. Until then the
/// window's hooks return at once.
static LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn live() -> bool {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}
/// GTK's display is Wayland's: its clipboard reaches nothing (its own
/// connection never has the keyboard, so the compositor takes no selection
/// from it and offers it none), and the pages' goes through zgx's instead
/// (`Command::Clipboard`, `Page::copied`). On X11 GTK's is the system's.
static BRIDGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether GTK runs (started now, on the first ask): pages are made only then.
fn started() -> bool {
    static STARTED: OnceLock<bool> = OnceLock::new();
    *STARTED.get_or_init(|| {
        if gtk::api().is_none() {
            return false;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("zgx-webkitgtk".into())
            .spawn(move || {
                let api = gtk::api().expect("loaded above");
                // SAFETY: GTK's initialization, on the thread that runs it
                // from now on; no arguments.
                let ready = unsafe { (api.gtk_init_check)(null_mut(), null_mut()) } != 0;
                if ready {
                    // SAFETY: the display GTK opened, on its thread; an X
                    // display's name has a colon (":0"), a Wayland one's not.
                    let name = unsafe {
                        string((api.gdk_display_get_name)((api.gdk_display_get_default)()))
                    };
                    BRIDGED.store(!name.contains(':'), std::sync::atomic::Ordering::Relaxed);
                    // SAFETY: ATK's socket class on GTK's thread:
                    // `AtkSocketClass` is an `AtkObjectClass` (its size
                    // GObject's registry gives), then `embed`, which ATK's
                    // bridge set. WebKit names a page's root through it (as
                    // the web process's accessibility starts, maybe after
                    // the page loaded): wrapped, the page hears it then.
                    unsafe {
                        let mut query = std::mem::zeroed::<gtk::GTypeQuery>();
                        (api.g_type_query)((api.atk_object_get_type)(), &mut query);
                        let class = (api.g_type_class_ref)((api.atk_socket_get_type)());
                        let slot =
                            (class as *mut u8).add(query.class_size as usize) as *mut Option<Embed>;
                        let _ = BRIDGE_EMBED.set(*slot);
                        *slot = Some(embedded);
                    }
                }
                let _ = sender.send(ready);
                if ready {
                    // SAFETY: GDK's error trap and GTK's main loop, on its
                    // thread; it never returns. The trap stays: on X11, GDK
                    // asks the server about the pages' offscreen windows,
                    // which have none of their own there, and an error
                    // would end the program (GTK's connection is its own:
                    // winit's hears nothing of it). A no-op on Wayland.
                    unsafe {
                        (api.gdk_error_trap_push)();
                        (api.gtk_main)();
                    }
                }
            });
        let ready = thread.is_ok() && receiver.recv().unwrap_or(false);
        if !ready {
            eprintln!("zgx: web views unavailable: GTK found no display");
        }
        LIVE.store(ready, std::sync::atomic::Ordering::Relaxed);
        ready
    })
}

/// Run `job` on GTK's thread, at its loop's next turn.
fn on_gtk(job: impl FnOnce(&Gtk) + Send + 'static) {
    let first = {
        let mut jobs = JOBS.lock().unwrap();
        jobs.push(Box::new(job));
        jobs.len() == 1
    };
    if first && let Some(api) = gtk::api() {
        // SAFETY: GLib's idle sources may be added from any thread; the
        // callback takes no data.
        unsafe { (api.g_idle_add_full)(gtk::G_PRIORITY_DEFAULT, run_jobs, null_mut(), None) };
    }
}
unsafe extern "C" fn run_jobs(_: Ptr) -> gtk::gboolean {
    let jobs = std::mem::take(&mut *JOBS.lock().unwrap());
    GTK.with(|gtk| {
        for job in jobs {
            job(gtk);
        }
    });
    0
}

/// Run `delivery` on the UI thread at its next safe point, waking it.
fn deliver(delivery: impl FnOnce() + Send + 'static) {
    DELIVERIES.lock().unwrap().push(Box::new(delivery));
    let waker = WAKERS
        .lock()
        .unwrap()
        .first()
        .map(|(_, waker)| waker.clone());
    if let Some(waker) = waker {
        waker();
    }
}
/// `deliver` what the web process answered: in a test of a slow one
/// (`ZGX_WEBKITGTK_REPLY_DELAY_MS`), that late, as on a loaded machine.
/// Debug builds only (the probes'): no release build reads it.
fn deliver_reply(delivery: impl FnOnce() + Send + 'static) {
    #[cfg(debug_assertions)]
    {
        static DELAY: OnceLock<u64> = OnceLock::new();
        let delay = *DELAY.get_or_init(|| {
            std::env::var("ZGX_WEBKITGTK_REPLY_DELAY_MS")
                .ok()
                .and_then(|ms| ms.parse().ok())
                .unwrap_or(0)
        });
        if delay > 0 {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                deliver(delivery);
            });
            return;
        }
    }
    deliver(delivery);
}
/// Run what GTK's thread delivered (the UI thread's safe point, before
/// `zgx::native_view::deliver_posted`).
pub(crate) fn deliver_pages() {
    if !live() {
        return;
    }
    loop {
        let deliveries = std::mem::take(&mut *DELIVERIES.lock().unwrap());
        if deliveries.is_empty() {
            return;
        }
        for delivery in deliveries {
            delivery();
        }
    }
}
/// Keeps a window's waker among those deliveries wake, until dropped.
pub(crate) struct WakerGuard(u64);
impl Drop for WakerGuard {
    fn drop(&mut self) {
        WAKERS.lock().unwrap().retain(|(id, _)| *id != self.0);
    }
}
/// Wake a window with `wake` when GTK's thread delivers.
pub(crate) fn add_waker(wake: Waker) -> WakerGuard {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    WAKERS.lock().unwrap().push((id, wake));
    WakerGuard(id)
}

/// Milliseconds on a clock both threads read: GDK events' times.
fn now_ms() -> u32 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u32
}

// What a page draws, shared.

/// A page's last drawing, as GTK's thread copied it: premultiplied BGRA
/// rows, and the part changed since the painter last took it.
#[derive(Default)]
struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    damage: Option<(u32, u32, u32, u32)>,
    /// The UI thread was told of the damage and has not taken it yet.
    told: bool,
}
fn union(a: Option<(u32, u32, u32, u32)>, b: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let Some(a) = a else { return b };
    let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
    let (x1, y1) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
    (x0, y0, x1 - x0, y1 - y0)
}

// GTK's side.

/// GTK's thread's pages, by controller id.
#[derive(Default)]
struct Gtk {
    pages: RefCell<HashMap<u64, Rc<Page>>>,
    /// Profiles' contexts (the default context is WebKit's own).
    contexts: RefCell<HashMap<WebProfile, Ptr>>,
    /// Schemes registered with each context.
    schemes: RefCell<Vec<(Ptr, String)>>,
    /// Requests being served, by id, until answered.
    scheme_tasks: RefCell<HashMap<u64, Ptr>>,
    next_task: Cell<u64>,
    /// Named cursors, to tell which one a page set (GDK keeps one per name).
    cursors: RefCell<Vec<(Ptr, zgx::cursor::Cursor)>>,
    /// The clipboard text last given to zgx's clipboard or taken from it
    /// (Wayland's bridge).
    clipboard: RefCell<String>,
}
thread_local! {
    static GTK: Gtk = Gtk::default();
}
impl Gtk {
    fn page(&self, id: u64) -> Option<Rc<Page>> {
        self.pages.borrow().get(&id).cloned()
    }
    fn page_of_view(&self, view: Ptr) -> Option<Rc<Page>> {
        self.pages
            .borrow()
            .values()
            .find(|page| page.view == view)
            .cloned()
    }
    fn page_of_context(&self, im: Ptr) -> Option<Rc<Page>> {
        self.pages
            .borrow()
            .values()
            .find(|page| page.im == im)
            .cloned()
    }
}

/// A page, on GTK's thread.
struct Page {
    id: u64,
    window: Ptr,
    view: Ptr,
    content: Ptr,
    im: Ptr,
    frame: Arc<Mutex<Frame>>,
    surface: Cell<Ptr>,
    surface_size: Cell<(i32, i32)>,
    damage: Cell<Option<(u32, u32, u32, u32)>>,
    draw_queued: Cell<bool>,
    html: RefCell<Option<(String, Option<String>)>>,
    /// The app's zoom of the page, and device pixels a point.
    zoom: Cell<(f64, f64)>,
    decisions: RefCell<HashMap<u64, Ptr>>,
    next_decision: Cell<u64>,
    new_windows: bool,
    nonce: String,
    /// Buttons held, as GDK's state mask.
    buttons: Cell<c_uint>,
    cursor: Cell<Ptr>,
    preedit: RefCell<(String, u32)>,
    /// The `<select>` popup showing, a reference held.
    options: Cell<Ptr>,
}

/// Defines `window.zgx.postMessage(value)`: what it posts carries the
/// sending document's origin and whether it is the top frame, read before
/// the page's own scripts run, with the page's secret (`NONCE`), which no
/// script of the page can read, so none can post as another origin.
const CHANNEL_SCRIPT: &str = "(() => { const handler = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.zgx; if (!handler || window.zgx) return; const post = handler.postMessage.bind(handler); const stringify = JSON.stringify; const origin = location.origin; const main = window === window.top; Object.defineProperty(window, 'zgx', { value: Object.freeze({ postMessage(value) { post({ n: NONCE, o: origin, m: main, b: stringify(value === undefined ? null : value) }); } }) }); Object.defineProperty(window, 'zgui', { value: window.zgx }); })();"; // rename-to-zgx: keep

/// What a page is made with, sent to GTK's thread.
struct Spec {
    id: u64,
    profile: WebProfile,
    transparent: bool,
    inspectable: bool,
    user_agent: Option<String>,
    messages: bool,
    new_windows: bool,
    scripts: Vec<(String, bool)>,
    schemes: Vec<String>,
    frame: Arc<Mutex<Frame>>,
}

fn nonce(id: u64) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(id);
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_nanos()),
    );
    let a = hasher.finish();
    hasher.write_u64(a);
    format!("{a:016x}{:016x}", hasher.finish())
}

impl Gtk {
    fn context(&self, api: &Api, profile: &WebProfile) -> Ptr {
        // SAFETY (each WebKit call): constructors and getters on GTK's
        // thread; the contexts are kept for the process's life.
        match profile {
            WebProfile::Default => unsafe { (api.webkit_web_context_get_default)() },
            WebProfile::Ephemeral => unsafe { (api.webkit_web_context_new_ephemeral)() },
            WebProfile::Named(name) => *self
                .contexts
                .borrow_mut()
                .entry(profile.clone())
                .or_insert_with(|| {
                    let (data, cache) = profile_directories(name);
                    let (data, cache) = (c(&data), c(&cache));
                    unsafe {
                        let manager = (api.g_object_new)(
                            (api.webkit_website_data_manager_get_type)(),
                            c"base-data-directory".as_ptr(),
                            data.as_ptr(),
                            c"base-cache-directory".as_ptr(),
                            cache.as_ptr(),
                            std::ptr::null::<c_char>(),
                        );
                        (api.webkit_web_context_new_with_website_data_manager)(manager)
                    }
                }),
        }
    }
    fn make(&self, spec: Spec) {
        let api = gtk::api().expect("GTK runs");
        let context = self.context(api, &spec.profile);
        let nonce = nonce(spec.id);
        // SAFETY: WebKit and GTK constructors and setters on GTK's thread,
        // each object used as its type; strings live through each call.
        // Signal handlers have their signals' signatures and get the page,
        // which the map keeps until `close` disconnects them first.
        unsafe {
            for scheme in &spec.schemes {
                let registered = self
                    .schemes
                    .borrow()
                    .iter()
                    .any(|(at, name)| *at == context && name == scheme);
                if !registered {
                    let name = c(scheme);
                    (api.webkit_web_context_register_uri_scheme)(
                        context,
                        name.as_ptr(),
                        serve_scheme,
                        null_mut(),
                        None,
                    );
                    let security = (api.webkit_web_context_get_security_manager)(context);
                    (api.webkit_security_manager_register_uri_scheme_as_secure)(
                        security,
                        name.as_ptr(),
                    );
                    (api.webkit_security_manager_register_uri_scheme_as_cors_enabled)(
                        security,
                        name.as_ptr(),
                    );
                    self.schemes.borrow_mut().push((context, scheme.clone()));
                }
            }
            let content = (api.webkit_user_content_manager_new)();
            let add = |source: &str, top: bool| {
                let source = c(source);
                let script = (api.webkit_user_script_new)(
                    source.as_ptr(),
                    top as c_int,
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                );
                (api.webkit_user_content_manager_add_script)(content, script);
                (api.webkit_user_script_unref)(script);
            };
            for (source, top) in &spec.scripts {
                add(source, *top);
            }
            if spec.messages {
                (api.webkit_user_content_manager_register_script_message_handler)(
                    content,
                    c"zgx".as_ptr(),
                );
                add(
                    &CHANNEL_SCRIPT.replace("NONCE", &format!("'{nonce}'")),
                    false,
                );
            }
            let view = (api.g_object_new)(
                (api.webkit_web_view_get_type)(),
                c"web-context".as_ptr(),
                context,
                c"user-content-manager".as_ptr(),
                content,
                std::ptr::null::<c_char>(),
            );
            let settings = (api.webkit_web_view_get_settings)(view);
            (api.webkit_settings_set_enable_developer_extras)(settings, spec.inspectable as c_int);
            if let Some(agent) = &spec.user_agent {
                (api.webkit_settings_set_user_agent)(settings, c(agent).as_ptr());
            }
            if spec.transparent {
                let clear = gtk::GdkRGBA {
                    red: 0.,
                    green: 0.,
                    blue: 0.,
                    alpha: 0.,
                };
                (api.webkit_web_view_set_background_color)(view, &clear);
            }
            let im = (api.g_object_new)(im_context_type(api), std::ptr::null::<c_char>());
            (api.webkit_web_view_set_input_method_context)(view, im);
            let window = (api.gtk_offscreen_window_new)();
            // No window a screen reader should follow: focus given to the
            // page activates this one, and a reader taking it for the active
            // window would leave the app's (ATK_ROLE_FILLER).
            (api.atk_object_set_role)((api.gtk_widget_get_accessible)(window), 20);
            (api.gtk_window_resize)(window, 1, 1);
            (api.gtk_container_add)(window, view);
            let page = Rc::new(Page {
                id: spec.id,
                window,
                view,
                content,
                im,
                frame: spec.frame,
                surface: Cell::new(null_mut()),
                surface_size: Cell::new((0, 0)),
                damage: Cell::new(None),
                draw_queued: Cell::new(false),
                html: RefCell::new(None),
                zoom: Cell::new((1., 1.)),
                decisions: RefCell::new(HashMap::new()),
                next_decision: Cell::new(0),
                new_windows: spec.new_windows,
                nonce,
                buttons: Cell::new(0),
                cursor: Cell::new(null_mut()),
                preedit: RefCell::new((String::new(), 0)),
                options: Cell::new(null_mut()),
            });
            let data = Rc::as_ptr(&page) as Ptr;
            let handler = |f: *const ()| std::mem::transmute::<*const (), gtk::GCallback>(f);
            connect(
                api,
                window,
                "damage-event",
                handler(on_damage as *const ()),
                data,
                false,
            );
            connect(
                api,
                window,
                "key-press-event",
                handler(on_unhandled_key as *const ()),
                data,
                true,
            );
            connect(
                api,
                view,
                "load-changed",
                handler(on_load_changed as *const ()),
                data,
                false,
            );
            connect(
                api,
                view,
                "load-failed",
                handler(on_load_failed as *const ()),
                data,
                false,
            );
            for property in ["title", "uri", "estimated-load-progress", "is-loading"] {
                connect(
                    api,
                    view,
                    &format!("notify::{property}"),
                    handler(on_notify as *const ()),
                    data,
                    false,
                );
            }
            let list = (api.webkit_web_view_get_back_forward_list)(view);
            connect(
                api,
                list,
                "changed",
                handler(on_history as *const ()),
                data,
                false,
            );
            connect(
                api,
                view,
                "decide-policy",
                handler(on_decide_policy as *const ()),
                data,
                false,
            );
            connect(
                api,
                view,
                "web-process-terminated",
                handler(on_terminated as *const ()),
                data,
                false,
            );
            connect(
                api,
                view,
                "context-menu",
                handler(on_context_menu as *const ()),
                data,
                false,
            );
            connect(
                api,
                view,
                "show-option-menu",
                handler(on_option_menu as *const ()),
                data,
                false,
            );
            connect(
                api,
                view,
                "enter-fullscreen",
                handler(on_refuse as *const ()),
                data,
                false,
            );
            if spec.messages {
                connect(
                    api,
                    content,
                    "script-message-received::zgx",
                    handler(on_message as *const ()),
                    data,
                    false,
                );
            }
            (api.gtk_widget_show_all)(window);
            (api.gtk_widget_grab_focus)(view);
            self.pages.borrow_mut().insert(spec.id, page);
        }
    }
    fn close(&self, id: u64) {
        let Some(page) = self.pages.borrow_mut().remove(&id) else {
            return;
        };
        let api = gtk::api().expect("GTK runs");
        let data = Rc::as_ptr(&page) as Ptr;
        // SAFETY: disconnects every handler given this page, then ends the
        // window (and the view it holds) and what the page owns, on GTK's
        // thread; undecided navigations are ignored, as they would be.
        unsafe {
            let list = (api.webkit_web_view_get_back_forward_list)(page.view);
            for object in [page.window, page.view, page.content, list] {
                (api.g_signal_handlers_disconnect_matched)(
                    object,
                    gtk::G_SIGNAL_MATCH_DATA,
                    0,
                    0,
                    null_mut(),
                    null_mut(),
                    data,
                );
            }
            for (_, decision) in page.decisions.borrow_mut().drain() {
                (api.webkit_policy_decision_ignore)(decision);
                (api.g_object_unref)(decision);
            }
            page.choose(None);
            (api.webkit_web_view_stop_loading)(page.view);
            (api.gtk_widget_destroy)(page.window);
            (api.g_object_unref)(page.content);
            (api.g_object_unref)(page.im);
            let surface = page.surface.replace(null_mut());
            if !surface.is_null() {
                (api.cairo_surface_destroy)(surface);
            }
        }
    }
}

/// A named profile's data and cache directories, under the user's XDG
/// directories and the program's name.
fn profile_directories(name: &str) -> (String, String) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let base = |variable: &str, fallback: &str| {
        std::env::var(variable)
            .ok()
            .filter(|dir| dir.starts_with('/'))
            .unwrap_or_else(|| format!("{home}/{fallback}"))
    };
    let program = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "zgx".into());
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    (
        format!(
            "{}/{program}/web-profiles/{name}",
            base("XDG_DATA_HOME", ".local/share")
        ),
        format!(
            "{}/{program}/web-profiles/{name}",
            base("XDG_CACHE_HOME", ".cache")
        ),
    )
}

type Embed = unsafe extern "C" fn(Ptr, *const c_char);
/// ATK's bridge's `AtkSocketClass::embed`, which `embedded` calls first.
static BRIDGE_EMBED: OnceLock<Option<Embed>> = OnceLock::new();
/// A socket (a page's view's accessible) was given its plug: the bridge's
/// own embedding, then the page reports it (`Page::report_plug_id`).
unsafe extern "C" fn embedded(socket: Ptr, plug: *const c_char) {
    if let Some(Some(bridge)) = BRIDGE_EMBED.get() {
        // SAFETY: the bridge's function with the arguments ATK gave.
        unsafe { bridge(socket, plug) };
    }
    // SAFETY: ATK passes the plug's id, a string, for this call.
    let plug = unsafe { string(plug) };
    let api = gtk::api().expect("GTK runs");
    GTK.with(|gtk| {
        // SAFETY: GTK's accessibles of live views, on GTK's thread.
        let page = gtk
            .pages
            .borrow()
            .values()
            .find(|page| unsafe { (api.gtk_widget_get_accessible)(page.view) } == socket)
            .cloned();
        if let Some(page) = page {
            page.report_plug_id(&plug);
        }
    });
}
/// The page a handler was given.
///
/// # Safety
/// `data` is a page's pointer, alive while its handlers are connected.
unsafe fn page<'a>(data: Ptr) -> &'a Page {
    // SAFETY: as the caller promises.
    unsafe { &*(data as *const Page) }
}

impl Page {
    fn report(&self, history: bool) {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: getters of the live view on GTK's thread; strings copied.
        let (url, title, loading, progress, back, forward, lists) = unsafe {
            let view = self.view;
            let lists = history.then(|| {
                let list = (api.webkit_web_view_get_back_forward_list)(view);
                let read = |items: *mut gtk::GList| {
                    let mut out = Vec::new();
                    let mut at = items;
                    while !at.is_null() {
                        let item = (*at).data;
                        out.push(zgx::web_view::HistoryItem {
                            url: string((api.webkit_back_forward_list_item_get_uri)(item)),
                            title: string((api.webkit_back_forward_list_item_get_title)(item)),
                        });
                        at = (*at).next;
                    }
                    (api.g_list_free)(items);
                    out
                };
                // WebKit lists the back pages oldest first: nearest first here.
                let mut back = read((api.webkit_back_forward_list_get_back_list_with_limit)(
                    list, 50,
                ));
                back.reverse();
                (
                    back,
                    read((api.webkit_back_forward_list_get_forward_list_with_limit)(
                        list, 50,
                    )),
                )
            });
            (
                string((api.webkit_web_view_get_uri)(view)),
                string((api.webkit_web_view_get_title)(view)),
                (api.webkit_web_view_is_loading)(view) != 0,
                (api.webkit_web_view_get_estimated_load_progress)(view),
                (api.webkit_web_view_can_go_back)(view) != 0,
                (api.webkit_web_view_can_go_forward)(view) != 0,
                lists,
            )
        };
        self.post(move |web| {
            let (old_url, old_title) = web
                .state()
                .with_untracked(|state| (state.url.clone(), state.title.clone()));
            web.update_state(|state| {
                if !url.is_empty() {
                    state.url = url.clone();
                }
                state.title = title.clone();
                state.loading = loading;
                state.progress = progress;
                state.can_go_back = back;
                state.can_go_forward = forward;
                if let Some((back_list, forward_list)) = lists {
                    state.back_list = back_list;
                    state.forward_list = forward_list;
                }
            });
            if !url.is_empty() && url != old_url {
                web.emit(WebViewEvent::UrlChanged { url });
            }
            if title != old_title {
                web.emit(WebViewEvent::TitleChanged { title });
            }
        });
    }
    /// Deliver `delivery` with this page's controller, if it still has one.
    fn post(&self, delivery: impl FnOnce(&WebView) + Send + 'static) {
        let id = self.id;
        deliver(move || {
            if let Some(page) = ui_page(id)
                && let Some(web) = page.web.upgrade()
            {
                delivery(&web);
            }
        });
    }
    /// Tell the UI thread where the page's accessible tree is: WebKit's
    /// view's accessible is an ATK socket (`AtkSocket`) embedding the web
    /// process's root, named by its `embedded_plug_id` ("bus:path"), the
    /// last field of the socket's instance.
    /// WebKit names the root once the web process's accessibility has
    /// started, which on a busy machine can be after the page loaded (then
    /// `embedded` reports it).
    fn report_plug(&self) {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: GTK's accessible for the live view, on GTK's thread, read
        // as an AtkSocket only once GObject says it is one; its plug id is a
        // string WebKit keeps (or null), copied now.
        let plug = unsafe {
            let accessible = (api.gtk_widget_get_accessible)(self.view);
            let socket = (api.atk_socket_get_type)();
            if accessible.is_null() || (api.g_type_check_instance_is_a)(accessible, socket) == 0 {
                return;
            }
            let mut query = std::mem::zeroed::<gtk::GTypeQuery>();
            (api.g_type_query)(socket, &mut query);
            let at = query.instance_size as usize - std::mem::size_of::<*const c_char>();
            string(*((accessible as *const u8).add(at) as *const *const c_char))
        };
        self.report_plug_id(&plug);
    }
    /// The page's root is `plug` ("bus:path"): the box embeds it.
    fn report_plug_id(&self, plug: &str) {
        let Some((bus, path)) = plug.split_once(':') else {
            return;
        };
        let (bus, path) = (bus.to_owned(), path.to_owned());
        let id = self.id;
        deliver(move || {
            if let Some(page) = ui_page(id) {
                page.embed(bus, path);
            }
        });
    }
    fn uri(&self) -> String {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: a getter of the live view on GTK's thread.
        unsafe { string((api.webkit_web_view_get_uri)(self.view)) }
    }
    /// Draw the damaged part into the shared frame, and tell the UI thread.
    fn draw(&self) {
        self.draw_queued.set(false);
        let api = gtk::api().expect("GTK runs");
        // SAFETY: GTK and cairo on GTK's thread: the window is live; the
        // surface is this page's, sized as made, and its pixels are read
        // (after a flush) within its stride and height.
        unsafe {
            let width = (api.gtk_widget_get_allocated_width)(self.view).max(1);
            let height = (api.gtk_widget_get_allocated_height)(self.view).max(1);
            if self.surface_size.get() != (width, height) || self.surface.get().is_null() {
                let old = self.surface.replace((api.cairo_image_surface_create)(
                    gtk::CAIRO_FORMAT_ARGB32,
                    width,
                    height,
                ));
                if !old.is_null() {
                    (api.cairo_surface_destroy)(old);
                }
                self.surface_size.set((width, height));
                self.damage.set(Some((0, 0, width as u32, height as u32)));
            }
            let Some((x, y, w, h)) = self.damage.take() else {
                return;
            };
            let (x, y) = (x.min(width as u32), y.min(height as u32));
            let (w, h) = (w.min(width as u32 - x), h.min(height as u32 - y));
            if w == 0 || h == 0 {
                return;
            }
            let surface = self.surface.get();
            let cr = (api.cairo_create)(surface);
            (api.cairo_rectangle)(cr, x as f64, y as f64, w as f64, h as f64);
            (api.cairo_clip)(cr);
            (api.cairo_set_operator)(cr, gtk::CAIRO_OPERATOR_CLEAR);
            (api.cairo_paint)(cr);
            (api.cairo_set_operator)(cr, gtk::CAIRO_OPERATOR_OVER);
            (api.gtk_widget_draw)(self.view, cr);
            (api.cairo_destroy)(cr);
            (api.cairo_surface_flush)(surface);
            let data = (api.cairo_image_surface_get_data)(surface);
            let stride = (api.cairo_image_surface_get_stride)(surface) as usize;
            let mut frame = self.frame.lock().unwrap();
            let (fw, fh) = (width as u32, height as u32);
            let full = frame.width != fw || frame.height != fh;
            if full {
                frame.width = fw;
                frame.height = fh;
                frame.pixels.resize(fw as usize * fh as usize * 4, 0);
            }
            let (x, y, w, h) = if full { (0, 0, fw, fh) } else { (x, y, w, h) };
            let row = fw as usize * 4;
            for line in y..y + h {
                let from = std::slice::from_raw_parts(
                    data.add(line as usize * stride + x as usize * 4),
                    w as usize * 4,
                );
                let at = line as usize * row + x as usize * 4;
                frame.pixels[at..at + w as usize * 4].copy_from_slice(from);
            }
            frame.damage = Some(if full {
                (0, 0, fw, fh)
            } else {
                union(frame.damage, (x, y, w, h))
            });
            if !std::mem::replace(&mut frame.told, true) {
                let id = self.id;
                deliver(move || {
                    if let Some(page) = ui_page(id) {
                        page.gpu.invalidate();
                    }
                });
            }
        }
    }
    fn queue_draw(&self, area: (u32, u32, u32, u32)) {
        self.damage.set(Some(union(self.damage.get(), area)));
        if !self.draw_queued.replace(true) {
            let id = self.id;
            let api = gtk::api().expect("GTK runs");
            // SAFETY: an idle source on GTK's thread; its data is the id.
            unsafe {
                (api.g_idle_add_full)(
                    // Below GTK's redraws (G_PRIORITY_HIGH_IDLE + 20), so a
                    // frame's damage is drawn once it all came.
                    200,
                    draw_page,
                    id as usize as Ptr,
                    None,
                )
            };
        }
    }
    fn run(&self, command: Command) {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: WebKit's methods on the live view, on GTK's thread;
        // strings live through each call.
        unsafe {
            let view = self.view;
            match command {
                Command::LoadUrl(url) => {
                    self.html.replace(None);
                    let url = if url.starts_with('/') {
                        format!("file://{url}")
                    } else {
                        url
                    };
                    (api.webkit_web_view_load_uri)(view, c(&url).as_ptr());
                }
                Command::LoadHtml { html, base_url } => {
                    let base = base_url.as_deref().map(c);
                    (api.webkit_web_view_load_html)(
                        view,
                        c(&html).as_ptr(),
                        base.as_ref().map_or(std::ptr::null(), |b| b.as_ptr()),
                    );
                    self.html.replace(Some((html, base_url)));
                }
                Command::Reload => {
                    let html = self.html.borrow().clone().filter(|(_, base)| {
                        self.uri() == base.clone().unwrap_or_else(|| "about:blank".into())
                    });
                    match html {
                        Some((html, base)) => self.run(Command::LoadHtml {
                            html,
                            base_url: base,
                        }),
                        None => (api.webkit_web_view_reload)(view),
                    }
                }
                Command::Stop => (api.webkit_web_view_stop_loading)(view),
                Command::GoBack => (api.webkit_web_view_go_back)(view),
                Command::GoForward => (api.webkit_web_view_go_forward)(view),
                Command::GoToHistory(n) => {
                    let list = (api.webkit_web_view_get_back_forward_list)(view);
                    let item = (api.webkit_back_forward_list_get_nth_item)(list, n);
                    if !item.is_null() {
                        (api.webkit_web_view_go_to_back_forward_list_item)(view, item);
                    }
                }
                Command::Evaluate { script, reply } => {
                    let data = Box::into_raw(Box::new((self.id, reply))) as Ptr;
                    (api.webkit_web_view_evaluate_javascript)(
                        view,
                        c(&script).as_ptr(),
                        -1,
                        std::ptr::null(),
                        std::ptr::null(),
                        null_mut(),
                        Some(evaluated),
                        data,
                    );
                }
                Command::Focus(focus) => self.focus(focus),
                Command::OpenDevtools => {
                    (api.webkit_web_inspector_show)((api.webkit_web_view_get_inspector)(view));
                }
                Command::SetZoom(zoom) => {
                    self.zoom.set((zoom, self.zoom.get().1));
                    self.show_zoom();
                }
                Command::Size {
                    width,
                    height,
                    scale,
                } => {
                    if self.zoom.get().1 != scale {
                        self.zoom.set((self.zoom.get().0, scale));
                        self.show_zoom();
                    }
                    // An offscreen window takes its child's requested size.
                    (api.gtk_widget_set_size_request)(self.view, width as c_int, height as c_int);
                    (api.gtk_window_resize)(self.window, width as c_int, height as c_int);
                }
                Command::Shown(shown) => {
                    if shown {
                        (api.gtk_widget_show)(self.window);
                    } else {
                        (api.gtk_widget_hide)(self.window);
                    }
                }
                Command::Pointer(pointer) => self.pointer(pointer),
                Command::Key {
                    press,
                    keyval,
                    keycode,
                    state,
                    time,
                } => self.key(press, keyval, keycode, state, time),
                Command::Ime(ime) => self.ime(ime),
                Command::Choose(item) => self.choose(item),
                Command::Clipboard(text) => {
                    // Before the paste key that follows: GTK serves it to the page.
                    let clipboard = (api.gtk_clipboard_get_for_display)(
                        (api.gdk_display_get_default)(),
                        gtk::GDK_SELECTION_CLIPBOARD,
                    );
                    (api.gtk_clipboard_set_text)(clipboard, c(&text).as_ptr(), -1);
                    GTK.with(|gtk| *gtk.clipboard.borrow_mut() = text);
                }
                Command::Decide { decision, allowed } => {
                    if let Some(decision) = self.decisions.borrow_mut().remove(&decision) {
                        if allowed {
                            (api.webkit_policy_decision_use)(decision);
                        } else {
                            (api.webkit_policy_decision_ignore)(decision);
                        }
                        (api.g_object_unref)(decision);
                    }
                }
            }
        }
    }
    fn show_zoom(&self) {
        let (page, scale) = self.zoom.get();
        let api = gtk::api().expect("GTK runs");
        // SAFETY: a setter of the live view on GTK's thread. The view's
        // pixels are device pixels: the page shows at its zoom times theirs.
        unsafe { (api.webkit_web_view_set_zoom_level)(self.view, page * scale) };
    }
    fn focus(&self, focus: bool) {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: a focus event GDK allocates, for the live window's own
        // GDK window (referenced, as the event frees it), sent on GTK's thread.
        unsafe {
            let event = (api.gdk_event_new)(gtk::GDK_FOCUS_CHANGE);
            let focus_event = event as *mut gtk::GdkEventFocus;
            (*focus_event).window = (api.g_object_ref)((api.gtk_widget_get_window)(self.window));
            (*focus_event).send_event = 1;
            (*focus_event).in_ = focus as i16;
            (api.gtk_widget_send_focus_change)(self.window, focus_event);
            (api.gdk_event_free)(event);
            if focus {
                (api.gtk_widget_grab_focus)(self.view);
            }
        }
        self.post(move |web| web.update_state(|state| state.focused = focus));
    }
    fn pointer(&self, pointer: Pointer) {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: events GDK allocates, filled as their types lay them out,
        // for the view's live GDK window (referenced: the event frees it),
        // given the default seat's pointer, handled on GTK's thread.
        unsafe {
            let window = (api.gtk_widget_get_window)(self.view);
            if window.is_null() {
                return;
            }
            let display = (api.gdk_display_get_default)();
            let device = (api.gdk_seat_get_pointer)((api.gdk_display_get_default_seat)(display));
            let time = now_ms();
            let (event, kind) = match pointer {
                Pointer::Move { x, y, state } => {
                    let event = (api.gdk_event_new)(gtk::GDK_MOTION_NOTIFY);
                    let motion = &mut *(event as *mut gtk::GdkEventMotion);
                    (
                        motion.time,
                        motion.x,
                        motion.y,
                        motion.x_root,
                        motion.y_root,
                    ) = (time, x, y, x, y);
                    motion.state = state | self.buttons.get();
                    motion.device = device;
                    (event, 0)
                }
                Pointer::Button {
                    press,
                    x,
                    y,
                    button,
                    state,
                } => {
                    let mask = gtk::GDK_BUTTON1_MASK << (button - 1).min(4);
                    let held = self.buttons.get();
                    let event = (api.gdk_event_new)(if press {
                        gtk::GDK_BUTTON_PRESS
                    } else {
                        gtk::GDK_BUTTON_RELEASE
                    });
                    let pressed = &mut *(event as *mut gtk::GdkEventButton);
                    (
                        pressed.time,
                        pressed.x,
                        pressed.y,
                        pressed.x_root,
                        pressed.y_root,
                    ) = (time, x, y, x, y);
                    pressed.state = state | held;
                    pressed.button = button;
                    pressed.device = device;
                    self.buttons
                        .set(if press { held | mask } else { held & !mask });
                    (event, 1)
                }
                Pointer::Scroll {
                    x,
                    y,
                    dx,
                    dy,
                    state,
                } => {
                    let event = (api.gdk_event_new)(gtk::GDK_SCROLL);
                    let scroll = &mut *(event as *mut gtk::GdkEventScroll);
                    (
                        scroll.time,
                        scroll.x,
                        scroll.y,
                        scroll.x_root,
                        scroll.y_root,
                    ) = (time, x, y, x, y);
                    scroll.state = state | self.buttons.get();
                    scroll.direction = gtk::GDK_SCROLL_SMOOTH;
                    (scroll.delta_x, scroll.delta_y) = (dx, dy);
                    scroll.device = device;
                    (event, 2)
                }
                Pointer::Cross { enter, x, y } => {
                    let event = (api.gdk_event_new)(if enter {
                        gtk::GDK_ENTER_NOTIFY
                    } else {
                        gtk::GDK_LEAVE_NOTIFY
                    });
                    let crossing = &mut *(event as *mut gtk::GdkEventCrossing);
                    (
                        crossing.time,
                        crossing.x,
                        crossing.y,
                        crossing.x_root,
                        crossing.y_root,
                    ) = (time, x, y, x, y);
                    crossing.state = self.buttons.get();
                    (event, 3)
                }
            };
            let any = event as *mut gtk::GdkEventFocus;
            (*any).window = (api.g_object_ref)(window);
            (*any).send_event = 1;
            (api.gdk_event_set_device)(event, device);
            (api.gdk_event_set_source_device)(event, device);
            (api.gtk_main_do_event)(event);
            (api.gdk_event_free)(event);
            if kind == 1 && self.buttons.get() == 0 {
                check_clipboard();
            }
            if kind != 3 {
                self.report_cursor(api, window);
            }
        }
    }
    /// Tell the UI thread the cursor the page set, if it changed.
    ///
    /// # Safety
    /// On GTK's thread, `window` the view's live GDK window.
    unsafe fn report_cursor(&self, api: &Api, window: Ptr) {
        // SAFETY: as the caller promises.
        let cursor = unsafe { (api.gdk_window_get_cursor)(window) };
        if cursor == self.cursor.replace(cursor) {
            return;
        }
        let shape = GTK.with(|gtk| {
            let mut cursors = gtk.cursors.borrow_mut();
            if cursors.is_empty() {
                // SAFETY: GDK's named cursors for its default display, on
                // GTK's thread; kept (GDK keeps one per name and display).
                let display = unsafe { (api.gdk_display_get_default)() };
                for (name, shape) in CURSORS {
                    let named =
                        unsafe { (api.gdk_cursor_new_from_name)(display, c(name).as_ptr()) };
                    if !named.is_null() {
                        cursors.push((named, *shape));
                    }
                }
            }
            cursors
                .iter()
                .find(|(named, _)| *named == cursor)
                .map_or(zgx::cursor::Cursor::Default, |(_, shape)| *shape)
        });
        let id = self.id;
        deliver(move || {
            if let Some(page) = ui_page(id) {
                page.set_cursor(shape);
            }
        });
    }
    fn key(&self, press: bool, keyval: u32, keycode: u16, state: u32, time: u32) {
        let api = gtk::api().expect("GTK runs");
        // SAFETY: a key event GDK allocates, filled as GdkEventKey lays it
        // out, for the window's live GDK window (referenced: the event frees
        // it), with the default seat's keyboard, handled on GTK's thread.
        unsafe {
            let window = (api.gtk_widget_get_window)(self.window);
            let event = (api.gdk_event_new)(if press {
                gtk::GDK_KEY_PRESS
            } else {
                gtk::GDK_KEY_RELEASE
            });
            let key = &mut *(event as *mut gtk::GdkEventKey);
            key.window = (api.g_object_ref)(window);
            key.send_event = 1;
            key.time = time;
            key.state = state;
            key.keyval = keyval;
            key.hardware_keycode = keycode;
            let display = (api.gdk_display_get_default)();
            let device = (api.gdk_seat_get_keyboard)((api.gdk_display_get_default_seat)(display));
            (api.gdk_event_set_device)(event, device);
            (api.gdk_event_set_source_device)(event, device);
            (api.gtk_main_do_event)(event);
            (api.gdk_event_free)(event);
        }
        if press && state & (gtk::GDK_CONTROL_MASK | gtk::GDK_SHIFT_MASK) != 0 {
            check_clipboard();
        }
    }
    /// The page's `<select>` popup: `Some(item)` chosen, or closed.
    fn choose(&self, item: Option<u32>) {
        let menu = self.options.replace(null_mut());
        if menu.is_null() {
            return;
        }
        let api = gtk::api().expect("GTK runs");
        // SAFETY: the option menu this page holds a reference to, on GTK's
        // thread: an item WebKit listed, then closed and released.
        unsafe {
            (api.g_signal_handlers_disconnect_matched)(
                menu,
                gtk::G_SIGNAL_MATCH_DATA,
                0,
                0,
                null_mut(),
                null_mut(),
                self as *const Page as Ptr,
            );
            if let Some(item) = item {
                (api.webkit_option_menu_activate_item)(menu, item);
            }
            (api.webkit_option_menu_close)(menu);
            (api.g_object_unref)(menu);
        }
    }
    fn ime(&self, ime: Ime) {
        let api = gtk::api().expect("GTK runs");
        let emit = |signal: &std::ffi::CStr| {
            // SAFETY: a signal of zgx's input method context (its class's),
            // taking no arguments, on GTK's thread.
            unsafe { (api.g_signal_emit_by_name)(self.im, signal.as_ptr()) }
        };
        match ime {
            Ime::Preedit(text, cursor) => {
                let was = !self.preedit.borrow().0.is_empty();
                let now = !text.is_empty();
                *self.preedit.borrow_mut() = (text, cursor);
                if now && !was {
                    emit(c"preedit-started");
                }
                emit(c"preedit-changed");
                if was && !now {
                    emit(c"preedit-finished");
                }
            }
            Ime::Commit(text) => {
                if !std::mem::take(&mut *self.preedit.borrow_mut()).0.is_empty() {
                    emit(c"preedit-changed");
                    emit(c"preedit-finished");
                }
                let text = c(&text);
                // SAFETY: as above, with the signal's one string argument.
                unsafe {
                    (api.g_signal_emit_by_name)(self.im, c"committed".as_ptr(), text.as_ptr())
                };
            }
        }
    }
}

/// Cursors a page may set, by their names in CSS and the cursor theme.
const CURSORS: &[(&str, zgx::cursor::Cursor)] = {
    use zgx::cursor::Cursor::*;
    &[
        ("default", Default),
        ("pointer", Pointer),
        ("text", Text),
        ("vertical-text", VerticalText),
        ("crosshair", Crosshair),
        ("cell", Crosshair),
        ("move", Move),
        ("grab", Grab),
        ("grabbing", Grabbing),
        ("not-allowed", NotAllowed),
        ("wait", Wait),
        ("progress", Progress),
        ("help", Help),
        ("context-menu", ContextMenu),
        ("copy", Copy),
        ("alias", Alias),
        ("no-drop", NoDrop),
        ("all-scroll", AllScroll),
        ("col-resize", ColResize),
        ("row-resize", RowResize),
        ("ew-resize", EwResize),
        ("e-resize", EwResize),
        ("w-resize", EwResize),
        ("ns-resize", NsResize),
        ("n-resize", NsResize),
        ("s-resize", NsResize),
        ("nesw-resize", NeswResize),
        ("ne-resize", NeswResize),
        ("sw-resize", NeswResize),
        ("nwse-resize", NwseResize),
        ("nw-resize", NwseResize),
        ("se-resize", NwseResize),
        ("zoom-in", ZoomIn),
        ("zoom-out", ZoomOut),
    ]
};

unsafe extern "C" fn draw_page(id: Ptr) -> gtk::gboolean {
    GTK.with(|gtk| {
        if let Some(page) = gtk.page(id as usize as u64) {
            page.draw();
        }
    });
    0
}
unsafe extern "C" fn on_damage(
    _: Ptr,
    event: *const gtk::GdkEventExpose,
    data: Ptr,
) -> gtk::gboolean {
    // SAFETY: GTK passes the window's expose event and this handler's page.
    let (page, area) = unsafe { (page(data), &(*event).area) };
    page.queue_draw((
        area.x.max(0) as u32,
        area.y.max(0) as u32,
        area.width.max(0) as u32,
        area.height.max(0) as u32,
    ));
    0
}
/// A key the page left alone, which WebKit sent on to its window.
unsafe extern "C" fn on_unhandled_key(
    _: Ptr,
    event: *const gtk::GdkEventKey,
    data: Ptr,
) -> gtk::gboolean {
    // SAFETY: GTK passes the key event and this handler's page.
    let (page, time) = unsafe { (page(data), (*event).time) };
    let id = page.id;
    // Slow, the answer comes after the modifiers are up.
    deliver_reply(move || {
        if let Some(page) = ui_page(id) {
            page.unhandled(time);
        }
    });
    1
}
unsafe extern "C" fn on_load_changed(_: Ptr, event: c_int, data: Ptr) {
    // SAFETY: GTK passes this handler's page.
    let page = unsafe { page(data) };
    let url = page.uri();
    match event {
        // WEBKIT_LOAD_STARTED.
        0 => page.post(move |web| {
            web.update_state(|state| state.error = None);
            web.emit(WebViewEvent::NavigationStarted { url });
        }),
        // WEBKIT_LOAD_COMMITTED.
        2 => page.post(move |web| web.emit(WebViewEvent::NavigationCommitted { url })),
        // WEBKIT_LOAD_FINISHED (after a failure too).
        3 => page.post(move |web| {
            if web.state().with_untracked(|state| state.error.is_none()) {
                web.emit(WebViewEvent::NavigationFinished { url });
            }
        }),
        _ => {}
    }
    page.report(true);
    // The page's accessible tree, maybe in a web process new for this
    // navigation, for the window's (`embedding`).
    if event == 2 || event == 3 {
        page.report_plug();
    }
}
unsafe extern "C" fn on_load_failed(
    _: Ptr,
    _: c_int,
    uri: *const c_char,
    error: *const gtk::GError,
    data: Ptr,
) -> gtk::gboolean {
    // SAFETY: GTK passes the failing URI, the error and this handler's page.
    let (page, url, message, code) = unsafe {
        (
            page(data),
            string(uri),
            string((*error).message),
            (*error).code,
        )
    };
    // WEBKIT_NETWORK_ERROR_CANCELLED (a new navigation replaced it), and
    // WEBKIT_POLICY_ERROR_FRAME_LOAD_INTERRUPTED_BY_POLICY_CHANGE (a policy
    // cancelled it, or a download).
    if code == 302 || code == 102 {
        return 1;
    }
    page.post(move |web| {
        web.update_state(|state| {
            state.loading = false;
            state.error = Some(message.clone());
        });
        web.emit(WebViewEvent::NavigationFailed {
            url,
            error: message,
        });
    });
    // No error page of WebKit's: the app shows the failure as it likes.
    1
}
unsafe extern "C" fn on_notify(_: Ptr, _: Ptr, data: Ptr) {
    // SAFETY: GTK passes this handler's page.
    unsafe { page(data) }.report(false);
}
unsafe extern "C" fn on_history(_: Ptr, _: Ptr, _: Ptr, data: Ptr) {
    // SAFETY: GTK passes this handler's page.
    unsafe { page(data) }.report(true);
}
unsafe extern "C" fn on_decide_policy(
    _: Ptr,
    decision: Ptr,
    kind: c_int,
    data: Ptr,
) -> gtk::gboolean {
    let api = gtk::api().expect("GTK runs");
    // SAFETY: GTK passes the decision, its type and this handler's page;
    // a navigation decision's action and request are its own, read now.
    let page = unsafe { page(data) };
    let (url, navigation) = unsafe {
        if kind > 1 {
            // A response: WebKit's own decision.
            return 0;
        }
        let action = (api.webkit_navigation_policy_decision_get_navigation_action)(decision);
        let request = (api.webkit_navigation_action_get_request)(action);
        (
            string((api.webkit_uri_request_get_uri)(request)),
            (api.webkit_navigation_action_get_navigation_type)(action),
        )
    };
    if kind == 1 {
        // WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION: opens nothing.
        if page.new_windows {
            page.post(move |web| web.emit(WebViewEvent::NewWindowRequested { url }));
        }
        // SAFETY: the decision GTK passed, decided once.
        unsafe { (api.webkit_policy_decision_ignore)(decision) };
        return 1;
    }
    let id = page.next_decision.get() + 1;
    page.next_decision.set(id);
    // SAFETY: kept until the UI thread decides (`Command::Decide`) or the page closes.
    page.decisions
        .borrow_mut()
        .insert(id, unsafe { (api.g_object_ref)(decision) });
    let request = NavigationRequest {
        url,
        main_frame: true,
        kind: match navigation {
            0 => NavigationKind::Link,
            1 => NavigationKind::FormSubmission,
            2 => NavigationKind::BackForward,
            3 => NavigationKind::Reload,
            4 => NavigationKind::FormResubmission,
            _ => NavigationKind::Other,
        },
        source_url: page.uri(),
    };
    let page_id = page.id;
    deliver(move || {
        let allowed = ui_page(page_id)
            .and_then(|page| page.web.upgrade())
            .is_none_or(|web| web.allows(&request));
        on_gtk(move |gtk| {
            if let Some(page) = gtk.page(page_id) {
                page.run(Command::Decide {
                    decision: id,
                    allowed,
                });
            }
        });
    });
    1
}
unsafe extern "C" fn on_terminated(_: Ptr, _: c_int, data: Ptr) {
    // SAFETY: GTK passes this handler's page.
    unsafe { page(data) }.post(|web| {
        web.update_state(|state| {
            state.loading = false;
            state.error = Some("The page stopped responding.".into());
        });
        web.emit(WebViewEvent::ProcessTerminated);
    });
}
/// No context menu or full screen of WebKit's own: it would open a window
/// of GTK's somewhere off the page.
unsafe extern "C" fn on_context_menu(_: Ptr, _: Ptr, _: Ptr, _: Ptr, _: Ptr) -> gtk::gboolean {
    1
}
/// A `<select>`'s popup: drawn by zgx (`option_menu`), as GTK's own would
/// open off the page.
unsafe extern "C" fn on_option_menu(
    _: Ptr,
    menu: Ptr,
    _: Ptr,
    rect: *const gtk::GdkRectangle,
    data: Ptr,
) -> gtk::gboolean {
    let api = gtk::api().expect("GTK runs");
    // SAFETY: GTK passes the menu, the element's box in the view and this
    // handler's page; the menu's items are read now, and a reference to it
    // kept until it is chosen from or closed (`Page::choose`).
    let (page, menu) = unsafe { (page(data), menu) };
    page.choose(None);
    let (items, at) = unsafe {
        let items = (0..(api.webkit_option_menu_get_n_items)(menu))
            .map(|i| {
                let item = (api.webkit_option_menu_get_item)(menu, i);
                OptionItem {
                    label: string((api.webkit_option_menu_item_get_label)(item)),
                    group: (api.webkit_option_menu_item_is_group_label)(item) != 0,
                    enabled: (api.webkit_option_menu_item_is_enabled)(item) != 0,
                    selected: (api.webkit_option_menu_item_is_selected)(item) != 0,
                }
            })
            .collect::<Vec<_>>();
        let rect = &*rect;
        page.options.set((api.g_object_ref)(menu));
        connect(
            api,
            menu,
            "close",
            std::mem::transmute::<*const (), gtk::GCallback>(on_option_menu_closed as *const ()),
            data,
            false,
        );
        (
            items,
            (
                rect.x as f32,
                rect.y as f32,
                rect.width as f32,
                rect.height as f32,
            ),
        )
    };
    let id = page.id;
    deliver(move || {
        if let Some(page) = ui_page(id) {
            page.show_options(Some(OptionMenu { items, at }));
        }
    });
    1
}
/// WebKit closed the popup (the page went, or its element did).
unsafe extern "C" fn on_option_menu_closed(menu: Ptr, data: Ptr) {
    // SAFETY: GTK passes the menu and this handler's page.
    let page = unsafe { page(data) };
    if page.options.get() == menu {
        page.choose(None);
    }
    let id = page.id;
    deliver(move || {
        if let Some(page) = ui_page(id) {
            page.show_options(None);
        }
    });
}
/// Where GTK's clipboard reaches nothing (`BRIDGED`), what a page copies
/// goes to zgx's: a copy (a shortcut, a page's copy button) is the web
/// process's, done after the input that asked for it, and GTK says nothing
/// of a clipboard set in the process; so after a key with Control or Shift,
/// or a click, the clipboard is read a little later, twice, and new text
/// goes on (not what zgx's gave it for a paste).
fn check_clipboard() {
    if !BRIDGED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let api = gtk::api().expect("GTK runs");
    for delay in [100, 400] {
        // SAFETY: a timeout on GTK's thread, taking no data.
        unsafe {
            (api.g_timeout_add_full)(
                gtk::G_PRIORITY_DEFAULT,
                delay,
                read_clipboard,
                null_mut(),
                None,
            )
        };
    }
}
unsafe extern "C" fn read_clipboard(_: Ptr) -> gtk::gboolean {
    let api = gtk::api().expect("GTK runs");
    // SAFETY: GTK's clipboard on its thread, read in the process (a page owns it).
    unsafe {
        let clipboard = (api.gtk_clipboard_get_for_display)(
            (api.gdk_display_get_default)(),
            gtk::GDK_SELECTION_CLIPBOARD,
        );
        on_clipboard(clipboard);
    }
    0
}
/// Read GTK's `clipboard` and give new text to zgx's.
///
/// # Safety
/// On GTK's thread, `clipboard` GTK's clipboard for its display.
unsafe fn on_clipboard(clipboard: Ptr) {
    unsafe extern "C" fn read(_: Ptr, text: *const c_char, _: Ptr) {
        // SAFETY: GTK passes the clipboard's text, or null.
        let Some(text) = (unsafe { (!text.is_null()).then(|| string(text)) }) else {
            return;
        };
        if GTK.with(|gtk| *gtk.clipboard.borrow() == text) {
            return;
        }
        GTK.with(|gtk| *gtk.clipboard.borrow_mut() = text.clone());
        deliver(move || {
            // The windows share one clipboard: any page's reaches it.
            let page = UI_PAGES.with(|pages| pages.borrow().values().find_map(Weak::upgrade));
            if let Some(page) = page
                && let Some(clipboard) = page.clipboard.borrow_mut().as_mut()
                && let Err(error) = clipboard.set_text(text)
            {
                eprintln!("zgx: a page's copy did not reach the clipboard: {error}");
            }
        });
    }
    let api = gtk::api().expect("GTK runs");
    // SAFETY: as the caller promises.
    unsafe { (api.gtk_clipboard_request_text)(clipboard, read, null_mut()) };
}
unsafe extern "C" fn on_refuse(_: Ptr, _: Ptr) -> gtk::gboolean {
    1
}
unsafe extern "C" fn on_message(_: Ptr, result: Ptr, data: Ptr) {
    let api = gtk::api().expect("GTK runs");
    // SAFETY: GTK passes the posted value (the 4.1 API's JavaScript result,
    // which holds it) and this handler's page; its properties are read as
    // JavaScriptCore gives them, strings freed.
    let page = unsafe { page(data) };
    let value = unsafe { (api.webkit_javascript_result_get_js_value)(result) };
    let text = |value: Ptr| unsafe {
        let raw = (api.jsc_value_to_string)(value);
        let out = string(raw);
        (api.g_free)(raw as Ptr);
        out
    };
    let property = |name: &std::ffi::CStr| unsafe {
        (api.jsc_value_object_get_property)(value, name.as_ptr())
    };
    let sealed = unsafe { (api.jsc_value_is_object)(value) } != 0 && {
        let nonce = property(c"n");
        let sealed = text(nonce) == page.nonce;
        unsafe { (api.g_object_unref)(nonce) };
        sealed
    };
    let (body, origin, main_frame) = if sealed {
        let (body, origin, main) = (property(c"b"), property(c"o"), property(c"m"));
        let read = (
            text(body),
            text(origin),
            unsafe { (api.jsc_value_to_boolean)(main) } != 0,
        );
        for value in [body, origin, main] {
            unsafe { (api.g_object_unref)(value) };
        }
        read
    } else {
        // Posted to the handler directly: no origin to vouch for.
        (json(api, value), String::new(), false)
    };
    page.post(move |web| {
        web.emit(WebViewEvent::Message {
            body,
            origin,
            main_frame,
        })
    });
}
/// A JavaScript value as JSON (`undefined` is `null`).
fn json(api: &Api, value: Ptr) -> String {
    // SAFETY: JavaScriptCore on GTK's thread, its string freed.
    unsafe {
        if value.is_null() || (api.jsc_value_is_undefined)(value) != 0 {
            return "null".into();
        }
        let raw = (api.jsc_value_to_json)(value, 0);
        if raw.is_null() {
            return "null".into();
        }
        let out = string(raw);
        (api.g_free)(raw as Ptr);
        out
    }
}
unsafe extern "C" fn evaluated(view: Ptr, result: Ptr, data: Ptr) {
    let api = gtk::api().expect("GTK runs");
    // SAFETY: the data `Command::Evaluate` boxed, taken once; the result
    // finished as WebKit's call requires.
    let (page, reply) = *unsafe { Box::from_raw(data as *mut (u64, u64)) };
    let result: ScriptResult = unsafe {
        let mut error: *mut gtk::GError = null_mut();
        let value = (api.webkit_web_view_evaluate_javascript_finish)(view, result, &mut error);
        if !error.is_null() {
            let message = string((*error).message);
            (api.g_error_free)(error);
            Err(message)
        } else {
            let out = json(api, value);
            if !value.is_null() {
                (api.g_object_unref)(value);
            }
            Ok(out)
        }
    };
    deliver(move || {
        if let Some(page) = ui_page(page)
            && let Some(reply) = page.replies.borrow_mut().remove(&reply)
        {
            reply(result);
        }
    });
}
unsafe extern "C" fn serve_scheme(request: Ptr, _: Ptr) {
    let api = gtk::api().expect("GTK runs");
    GTK.with(|gtk| {
        // SAFETY: the request WebKit passes, on GTK's thread: its getters'
        // strings copied, its body read to the end, its headers walked, and
        // a reference kept until it is answered (`finish_scheme`).
        unsafe {
            let page = gtk.page_of_view((api.webkit_uri_scheme_request_get_web_view)(request));
            let Some(page) = page else {
                fail_scheme(api, request);
                return;
            };
            let mut headers: Vec<(String, String)> = Vec::new();
            let fields = (api.webkit_uri_scheme_request_get_http_headers)(request);
            if !fields.is_null() {
                unsafe extern "C" fn each(name: *const c_char, value: *const c_char, out: Ptr) {
                    // SAFETY: libsoup passes the header and the vector given below.
                    unsafe {
                        (*(out as *mut Vec<(String, String)>)).push((string(name), string(value)))
                    };
                }
                (api.soup_message_headers_foreach)(fields, each, &mut headers as *mut _ as Ptr);
            }
            let mut body = Vec::new();
            let stream = (api.webkit_uri_scheme_request_get_http_body)(request);
            if !stream.is_null() {
                loop {
                    let bytes =
                        (api.g_input_stream_read_bytes)(stream, 64 * 1024, null_mut(), null_mut());
                    if bytes.is_null() {
                        break;
                    }
                    let mut size = 0;
                    let data = (api.g_bytes_get_data)(bytes, &mut size);
                    if size > 0 {
                        body.extend_from_slice(std::slice::from_raw_parts(data as *const u8, size));
                    }
                    (api.g_bytes_unref)(bytes);
                    if size == 0 {
                        break;
                    }
                }
                (api.g_object_unref)(stream);
            }
            let task = gtk.next_task.get() + 1;
            gtk.next_task.set(task);
            gtk.scheme_tasks
                .borrow_mut()
                .insert(task, (api.g_object_ref)(request));
            let request = SchemeRequest {
                web_view: page.id,
                url: string((api.webkit_uri_scheme_request_get_uri)(request)),
                main_document_url: String::new(),
                method: {
                    let method = string((api.webkit_uri_scheme_request_get_http_method)(request));
                    if method.is_empty() {
                        "GET".into()
                    } else {
                        method
                    }
                },
                headers,
                body,
            };
            let id = page.id;
            deliver(move || {
                let responder = SchemeResponder::new(move |response| {
                    on_gtk(move |gtk| finish_scheme(gtk, task, response));
                });
                let scheme = request
                    .url
                    .split(':')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let handler = ui_page(id)
                    .and_then(|page| page.schemes.iter().find(|h| h.scheme == scheme).cloned());
                if let Some(handler) = handler {
                    (handler.serve)(request, responder);
                }
                // Unserved, the responder's drop fails it.
            });
        }
    });
}
/// Fail a scheme request as a network error would.
///
/// # Safety
/// `request` is a live request not yet answered, on GTK's thread.
unsafe fn fail_scheme(api: &Api, request: Ptr) {
    // SAFETY: as the caller promises; WebKit takes the error.
    unsafe {
        let domain = (api.g_quark_from_static_string)(c"zgx-scheme-error".as_ptr());
        let error = (api.g_error_new_literal)(domain, 1, c"Not served".as_ptr());
        (api.webkit_uri_scheme_request_finish_error)(request, error);
        (api.g_error_free)(error);
    }
}
fn finish_scheme(gtk: &Gtk, task: u64, response: Option<SchemeResponse>) {
    let Some(request) = gtk.scheme_tasks.borrow_mut().remove(&task) else {
        return;
    };
    let api = gtk::api().expect("GTK runs");
    // SAFETY: the request kept for this task, answered once on GTK's
    // thread, then released; the response's stream holds its own copy.
    unsafe {
        match response {
            None => fail_scheme(api, request),
            Some(response) => {
                let bytes =
                    (api.g_bytes_new)(response.body.as_ptr() as *const c_void, response.body.len());
                let stream = (api.g_memory_input_stream_new_from_bytes)(bytes);
                (api.g_bytes_unref)(bytes);
                let answer =
                    (api.webkit_uri_scheme_response_new)(stream, response.body.len() as i64);
                (api.webkit_uri_scheme_response_set_status)(
                    answer,
                    response.status as c_uint,
                    std::ptr::null(),
                );
                let headers = (api.soup_message_headers_new)(1);
                for (name, value) in &response.headers {
                    if name.eq_ignore_ascii_case("content-type") {
                        let mime = value.split(';').next().unwrap_or_default().trim();
                        (api.webkit_uri_scheme_response_set_content_type)(answer, c(mime).as_ptr());
                    }
                    (api.soup_message_headers_append)(headers, c(name).as_ptr(), c(value).as_ptr());
                }
                (api.webkit_uri_scheme_response_set_http_headers)(answer, headers);
                (api.webkit_uri_scheme_request_finish_with_response)(request, answer);
                (api.g_object_unref)(answer);
                (api.g_object_unref)(stream);
            }
        }
        (api.g_object_unref)(request);
    }
}

// zgx's input method context: WebKit asks it what is being composed and
// tells it where the caret is; the window's input method composes.

/// `ZgxInputMethodContext`, registered once.
fn im_context_type(api: &Api) -> gtk::GType {
    static TYPE: OnceLock<gtk::GType> = OnceLock::new();
    *TYPE.get_or_init(|| {
        // SAFETY: GObject's type registry on GTK's thread: the subclass
        // takes its parent's class and instance sizes as GObject reports
        // them, and `class_init` sets the parent's virtual methods.
        unsafe {
            let parent = (api.webkit_input_method_context_get_type)();
            let mut query = std::mem::zeroed::<gtk::GTypeQuery>();
            (api.g_type_query)(parent, &mut query);
            let info = gtk::GTypeInfo {
                class_size: query.class_size as u16,
                base_init: null_mut(),
                base_finalize: null_mut(),
                class_init: Some(im_class_init),
                class_finalize: null_mut(),
                class_data: null_mut(),
                instance_size: query.instance_size as u16,
                n_preallocs: 0,
                instance_init: null_mut(),
                value_table: null_mut(),
            };
            (api.g_type_register_static)(parent, c"ZgxInputMethodContext".as_ptr(), &info, 0)
        }
    })
}
/// `WebKitInputMethodContextClass`'s methods after `GObjectClass` (whose
/// size GObject reports), in its order: five signals' class closures, then
/// these.
#[repr(C)]
struct ImClass {
    preedit_started: Ptr,
    preedit_changed: Ptr,
    preedit_finished: Ptr,
    committed: Ptr,
    delete_surrounding: Ptr,
    set_enable_preedit: Ptr,
    get_preedit: Option<unsafe extern "C" fn(Ptr, *mut *mut c_char, *mut Ptr, *mut c_uint)>,
    filter_key_event: Option<unsafe extern "C" fn(Ptr, Ptr) -> gtk::gboolean>,
    notify_focus_in: Option<unsafe extern "C" fn(Ptr)>,
    notify_focus_out: Option<unsafe extern "C" fn(Ptr)>,
    notify_cursor_area: Option<unsafe extern "C" fn(Ptr, c_int, c_int, c_int, c_int)>,
    notify_surrounding: Ptr,
    reset: Option<unsafe extern "C" fn(Ptr)>,
}
unsafe extern "C" fn im_class_init(class: Ptr, _: Ptr) {
    let api = gtk::api().expect("GTK runs");
    // SAFETY: GObject's class size (its fundamental type, 20 << 2) puts
    // `ImClass` where the WebKit class's own members begin.
    unsafe {
        let mut query = std::mem::zeroed::<gtk::GTypeQuery>();
        (api.g_type_query)(20 << 2, &mut query);
        let methods = &mut *((class as *mut u8).add(query.class_size as usize) as *mut ImClass);
        methods.get_preedit = Some(im_get_preedit);
        methods.filter_key_event = Some(im_filter_key_event);
        methods.notify_focus_in = Some(im_focus_in);
        methods.notify_focus_out = Some(im_focus_out);
        methods.notify_cursor_area = Some(im_cursor_area);
        methods.reset = Some(im_reset);
    }
}
unsafe extern "C" fn im_get_preedit(
    im: Ptr,
    text: *mut *mut c_char,
    underlines: *mut Ptr,
    cursor: *mut c_uint,
) {
    let api = gtk::api().expect("GTK runs");
    let (preedit, at) = GTK
        .with(|gtk| {
            gtk.page_of_context(im)
                .map(|page| page.preedit.borrow().clone())
        })
        .unwrap_or_default();
    // SAFETY: WebKit's out-pointers, each null or writable; it frees the text.
    unsafe {
        if !text.is_null() {
            *text = (api.g_strdup)(c(&preedit).as_ptr());
        }
        if !underlines.is_null() {
            *underlines = null_mut();
        }
        if !cursor.is_null() {
            *cursor = at;
        }
    }
}
/// Keys are not the context's to compose: the window's input method has
/// them before the page does.
unsafe extern "C" fn im_filter_key_event(_: Ptr, _: Ptr) -> gtk::gboolean {
    0
}
fn im_report(im: Ptr, change: ImChange) {
    let Some(id) = GTK.with(|gtk| gtk.page_of_context(im).map(|page| page.id)) else {
        return;
    };
    let caret = matches!(change, ImChange::Area(..));
    let report = move || {
        if let Some(page) = ui_page(id) {
            page.ime_changed(change);
        }
    };
    if caret {
        // Slow, the caret comes after the candidates showed.
        deliver_reply(report);
    } else {
        deliver(report);
    }
}
unsafe extern "C" fn im_focus_in(im: Ptr) {
    im_report(im, ImChange::Editing(true));
}
unsafe extern "C" fn im_focus_out(im: Ptr) {
    im_report(im, ImChange::Editing(false));
}
unsafe extern "C" fn im_cursor_area(im: Ptr, x: c_int, y: c_int, width: c_int, height: c_int) {
    im_report(
        im,
        ImChange::Area(x as f32, y as f32, width as f32, height as f32),
    );
}
unsafe extern "C" fn im_reset(im: Ptr) {
    GTK.with(|gtk| {
        if let Some(page) = gtk.page_of_context(im) {
            page.preedit.borrow_mut().0.clear();
        }
    });
}

// What the UI thread sends.

enum Command {
    LoadUrl(String),
    LoadHtml {
        html: String,
        base_url: Option<String>,
    },
    Reload,
    Stop,
    GoBack,
    GoForward,
    GoToHistory(i32),
    Evaluate {
        script: String,
        reply: u64,
    },
    Focus(bool),
    OpenDevtools,
    SetZoom(f64),
    /// The box's size in device pixels, and device pixels a point.
    Size {
        width: u32,
        height: u32,
        scale: f64,
    },
    Shown(bool),
    Pointer(Pointer),
    Key {
        press: bool,
        keyval: u32,
        keycode: u16,
        state: u32,
        time: u32,
    },
    Ime(Ime),
    Decide {
        decision: u64,
        allowed: bool,
    },
    /// An item of the `<select>` popup chosen, or none: it closed.
    Choose(Option<u32>),
    /// zgx's clipboard text, for a paste (`BRIDGED`).
    Clipboard(String),
}
/// Pointer input, in the page's device pixels.
enum Pointer {
    Move {
        x: f64,
        y: f64,
        state: u32,
    },
    Button {
        press: bool,
        x: f64,
        y: f64,
        button: u32,
        state: u32,
    },
    Scroll {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
        state: u32,
    },
    Cross {
        enter: bool,
        x: f64,
        y: f64,
    },
}
/// A `<select>`'s popup: its items, and the element's box in the page's
/// device pixels.
struct OptionMenu {
    items: Vec<OptionItem>,
    at: (f32, f32, f32, f32),
}
struct OptionItem {
    label: String,
    group: bool,
    enabled: bool,
    selected: bool,
}
enum Ime {
    Preedit(String, u32),
    Commit(String),
}
enum ImChange {
    Editing(bool),
    Area(f32, f32, f32, f32),
}

// The UI thread's side.

thread_local! {
    static UI_PAGES: RefCell<HashMap<u64, Weak<UiPage>>> = RefCell::new(HashMap::new());
    /// Keys pages left alone, for the app's shortcuts (`take_unhandled_keys`).
    static UNHANDLED: RefCell<Vec<(Weak<RefCell<Scene>>, KeyPress)>> = const { RefCell::new(Vec::new()) };
    /// zgx focus to move to or from pages' boxes (`sync_focus`).
    static FOCUS: RefCell<Vec<(NodeId, bool)>> = const { RefCell::new(Vec::new()) };
    /// Windows whose page moved its caret or began or ended editing
    /// (`ime_moved`).
    static IME_MOVED: RefCell<Vec<Weak<RefCell<Scene>>>> = const { RefCell::new(Vec::new()) };
}
fn ui_page(id: u64) -> Option<Rc<UiPage>> {
    UI_PAGES.with(|pages| pages.borrow().get(&id).and_then(Weak::upgrade))
}

/// What gets a script's result.
type Reply = Box<dyn FnOnce(ScriptResult)>;
/// A page, on the UI thread: what its controller, box and painter share.
struct UiPage {
    id: u64,
    web: WeakWebView,
    gpu: zgx::gpu_view::GpuView,
    schemes: Vec<SchemeHandler>,
    replies: RefCell<HashMap<u64, Reply>>,
    next_reply: Cell<u64>,
    /// The box showing it, and its scene.
    node: Cell<Option<NodeId>>,
    scene: RefCell<Weak<RefCell<Scene>>>,
    /// Device pixels a point, as last drawn.
    scale: Rc<Cell<f32>>,
    shown: Cell<usize>,
    /// Keys sent, by their time (GDK's), until handled or not.
    keys: RefCell<VecDeque<(u32, KeyPress)>>,
    last_key: Cell<u32>,
    /// An editable has the page's focus, and where its caret is (device pixels).
    editing: Cell<bool>,
    caret: Cell<Option<(f32, f32, f32, f32)>>,
    /// The window's clipboard, for Wayland's bridge (`BRIDGED`).
    clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
    /// Where the page's accessible tree is (another process's object), as
    /// the box's author ID embedding it (`accesskit_atspi_common::EMBED`),
    /// and the box, to project anew.
    embedding: RefCell<Option<String>>,
    embeddings: Cell<u64>,
    handle: RefCell<Option<zgx::compose::NodeHandle>>,
    /// The `<select>` popup to show, and a count its box follows (made
    /// with the first box, in its runtime).
    options: RefCell<Option<Rc<OptionMenu>>>,
    options_shown: RefCell<Option<zgx::reactive::Signal<u64>>>,
}
impl UiPage {
    /// The page's accessible tree is at `path` on `bus`: the box embeds it,
    /// asking it again for each report (a reload leaves it where it was,
    /// but WebKit's socket may have taken it back).
    fn embed(&self, bus: String, path: String) {
        let count = self.embeddings.get() + 1;
        self.embeddings.set(count);
        *self.embedding.borrow_mut() = Some(format!("zgx-atspi-embed\n{bus}\n{path}\n{count}"));
        if let Some(handle) = &*self.handle.borrow() {
            // Projected again: its node's revision moves.
            handle.touch_semantics();
        }
    }
    fn show_options(&self, menu: Option<OptionMenu>) {
        *self.options.borrow_mut() = menu.map(Rc::new);
        if let Some(shown) = &*self.options_shown.borrow() {
            shown.update(|count| *count += 1);
        }
    }
    fn send(&self, command: Command) {
        let id = self.id;
        on_gtk(move |gtk| {
            if let Some(page) = gtk.page(id) {
                page.run(command);
            }
        });
    }
    fn set_cursor(&self, cursor: zgx::cursor::Cursor) {
        if let (Some(node), Some(scene)) = (self.node.get(), self.scene.borrow().upgrade())
            && let Ok(mut scene) = scene.try_borrow_mut()
            && scene.contains(node)
        {
            scene.set_cursor(node, Some(cursor));
        }
    }
    fn unhandled(&self, time: u32) {
        let mut keys = self.keys.borrow_mut();
        if let Some(at) = keys.iter().position(|(t, _)| *t == time) {
            let (_, press) = keys.remove(at).expect("found");
            let scene = self.scene.borrow().clone();
            UNHANDLED.with(|unhandled| unhandled.borrow_mut().push((scene, press)));
        }
    }
    fn ime_changed(&self, change: ImChange) {
        let changed = match change {
            ImChange::Editing(editing) => self.editing.replace(editing) != editing,
            ImChange::Area(x, y, w, h) => {
                self.caret.replace(Some((x, y, w, h))) != Some((x, y, w, h))
            }
        };
        if changed {
            let scene = self.scene.borrow().clone();
            IME_MOVED.with(|moved| moved.borrow_mut().push(scene));
        }
    }
    /// A point of the window in the page's device pixels.
    fn local(&self, x: f32, y: f32) -> Option<(f64, f64)> {
        let (node, scene) = (self.node.get()?, self.scene.borrow().upgrade()?);
        let scene = scene.try_borrow().ok()?;
        let bounds = scene.bounds(node);
        let shown = scene.window_rect(node, bounds);
        let scale = self.scale.get() as f64;
        let sx = bounds.width / shown.width.max(0.001);
        let sy = bounds.height / shown.height.max(0.001);
        Some((
            ((x - shown.x) * sx) as f64 * scale,
            ((y - shown.y) * sy) as f64 * scale,
        ))
    }
    /// Its box's input.
    fn input(&self, event: &mut EventContext) {
        let state = modifier_state(event.pointer_modifiers());
        match event.event.clone() {
            InputEvent::PointerMove { x, y } => {
                if let Some((x, y)) = self.local(x, y) {
                    self.send(Command::Pointer(Pointer::Move { x, y, state }));
                    // Where it is over the page, in points (`WebView::pointer`).
                    let scale = self.scale.get().max(0.01) as f64;
                    let at = ((x / scale) as f32, (y / scale) as f32);
                    if let Some(web) = self.web.upgrade() {
                        zgx::native_view::post(move || web.set_pointer(Some(at)));
                    }
                }
            }
            InputEvent::PointerDown { x, y, button } => {
                event.focus();
                event.capture_pointer();
                if let Some((x, y)) = self.local(x, y) {
                    let button = gdk_button(button);
                    self.send(Command::Pointer(Pointer::Button {
                        press: true,
                        x,
                        y,
                        button,
                        state,
                    }));
                }
            }
            InputEvent::PointerUp { x, y, button } => {
                event.release_pointer();
                if let Some((x, y)) = self.local(x, y) {
                    let button = gdk_button(button);
                    self.send(Command::Pointer(Pointer::Button {
                        press: false,
                        x,
                        y,
                        button,
                        state,
                    }));
                }
            }
            InputEvent::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                if let Some((x, y)) = self.local(x, y) {
                    // WebKit scrolls a smooth step's unit by 40 of the page's pixels.
                    let (dx, dy) = (delta_x as f64 / 40., delta_y as f64 / 40.);
                    self.send(Command::Pointer(Pointer::Scroll {
                        x,
                        y,
                        dx,
                        dy,
                        state,
                    }));
                    event.stop_propagation();
                    event.prevent_default();
                }
            }
            InputEvent::PointerEnter => self.send(Command::Pointer(Pointer::Cross {
                enter: true,
                x: 0.,
                y: 0.,
            })),
            InputEvent::PointerLeave => {
                self.send(Command::Pointer(Pointer::Cross {
                    enter: false,
                    x: -1.,
                    y: -1.,
                }));
                if let Some(web) = self.web.upgrade() {
                    zgx::native_view::post(move || web.set_pointer(None));
                }
            }
            InputEvent::Focus => self.send(Command::Focus(true)),
            InputEvent::Blur => self.send(Command::Focus(false)),
            _ => {}
        }
    }
}
fn gdk_button(button: PointerButton) -> u32 {
    match button {
        PointerButton::Primary => 1,
        PointerButton::Middle => 2,
        PointerButton::Secondary => 3,
        PointerButton::Other(n) => u32::from(n),
    }
}
fn modifier_state(modifiers: Modifiers) -> u32 {
    let mut state = 0;
    if modifiers.shift {
        state |= gtk::GDK_SHIFT_MASK;
    }
    if modifiers.control {
        state |= gtk::GDK_CONTROL_MASK;
    }
    if modifiers.alt {
        state |= gtk::GDK_MOD1_MASK;
    }
    if modifiers.meta {
        state |= gtk::GDK_SUPER_MASK | gtk::GDK_META_MASK;
    }
    state
}

/// The page's controller's backend.
pub(crate) struct GtkWebView(Rc<UiPage>);
impl WebViewBackend for GtkWebView {
    fn run(&self, command: WebViewCommand) {
        let page = &self.0;
        let command = match command {
            WebViewCommand::LoadUrl(url) => Command::LoadUrl(url),
            WebViewCommand::LoadHtml { html, base_url } => Command::LoadHtml { html, base_url },
            WebViewCommand::Reload => Command::Reload,
            WebViewCommand::Stop => Command::Stop,
            WebViewCommand::GoBack => Command::GoBack,
            WebViewCommand::GoForward => Command::GoForward,
            WebViewCommand::GoToHistory(n) => Command::GoToHistory(n),
            WebViewCommand::Evaluate { script, reply } => {
                let id = page.next_reply.get() + 1;
                page.next_reply.set(id);
                if let Some(reply) = reply {
                    page.replies.borrow_mut().insert(id, reply);
                }
                Command::Evaluate { script, reply: id }
            }
            WebViewCommand::Focus(focus) => {
                // Through zgx's focus, which the box's focus follows.
                if let Some(node) = page.node.get() {
                    FOCUS.with(|requests| requests.borrow_mut().push((node, focus)));
                    zgx::native_view::wake();
                    return;
                }
                Command::Focus(focus)
            }
            WebViewCommand::OpenDevtools => Command::OpenDevtools,
            WebViewCommand::SetZoom(zoom) => Command::SetZoom(zoom),
        };
        page.send(command);
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
impl Drop for GtkWebView {
    fn drop(&mut self) {
        let id = self.0.id;
        UI_PAGES.with(|pages| pages.borrow_mut().remove(&id));
        on_gtk(move |gtk| gtk.close(id));
    }
}

/// The page behind `web`, made now if it has none.
fn backend(
    web: &WebView,
    clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
) -> Rc<UiPage> {
    if let Some(backend) = web.backend()
        && let Some(page) = backend.as_any().downcast_ref::<GtkWebView>()
    {
        return page.0.clone();
    }
    let options = web.options();
    let frame = Arc::new(Mutex::new(Frame::default()));
    let scale = Rc::new(Cell::new(1.));
    let schemes: Vec<SchemeHandler> = {
        let mut taken = std::collections::HashSet::new();
        options
            .schemes
            .iter()
            .filter(|handler| {
                let usable = zgx::web_view::is_app_scheme(&handler.scheme) && taken.insert(handler.scheme.clone());
                if !usable {
                    eprintln!(
                        "zgx: web view scheme `{}` left out: not a scheme an app can serve, or given twice",
                        handler.scheme
                    );
                }
                usable
            })
            .cloned()
            .collect()
    };
    let page = Rc::new(UiPage {
        id: web.id(),
        web: web.downgrade(),
        gpu: zgx_gpu::viewport::view(Painter {
            id: web.id(),
            frame: frame.clone(),
            scale: scale.clone(),
            asked: None,
            gpu: None,
        }),
        schemes: schemes.clone(),
        replies: RefCell::new(HashMap::new()),
        next_reply: Cell::new(0),
        node: Cell::new(None),
        scene: RefCell::new(Weak::new()),
        scale,
        shown: Cell::new(0),
        keys: RefCell::new(VecDeque::new()),
        last_key: Cell::new(0),
        editing: Cell::new(false),
        caret: Cell::new(None),
        clipboard,
        options: RefCell::new(None),
        options_shown: RefCell::new(None),
        embedding: RefCell::new(None),
        embeddings: Cell::new(0),
        handle: RefCell::new(None),
    });
    let spec = Spec {
        id: web.id(),
        profile: options.profile.clone(),
        transparent: options.transparent,
        inspectable: options.inspectable,
        user_agent: options.user_agent.clone(),
        messages: options.messages,
        new_windows: options.new_windows,
        scripts: options
            .scripts
            .iter()
            .map(|s| (s.source.clone(), s.main_frame_only))
            .collect(),
        schemes: schemes.iter().map(|h| h.scheme.clone()).collect(),
        frame,
    };
    on_gtk(move |gtk| gtk.make(spec));
    UI_PAGES.with(|pages| pages.borrow_mut().insert(web.id(), Rc::downgrade(&page)));
    web.attach(Rc::new(GtkWebView(page.clone())));
    page
}

/// The service making pages' boxes, where WebKitGTK runs (`None` where it
/// can't: pages are placeholders there).
pub(crate) fn page_views(clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>) -> PageViews {
    PageViews::new(move |web| {
        if !started() {
            return None;
        }
        let page = backend(web, clipboard.clone());
        Some(component(move |cx| {
            let shown = page
                .options_shown
                .borrow_mut()
                .get_or_insert_with(|| cx.runtime().signal(0))
                .clone();
            let (input, mounted, menu) = (page.clone(), page.clone(), page.clone());
            div()
                .focusable(true)
                .overflow_hidden()
                .on_event(move |event| input.input(event))
                .accessible_label("Web page")
                .on_mount(move |handle| {
                    let page = mounted;
                    page.node.set(Some(handle.node()));
                    *page.handle.borrow_mut() = Some(handle.clone());
                    *page.scene.borrow_mut() = handle
                        .scene()
                        .as_ref()
                        .map_or_else(Weak::new, Rc::downgrade);
                    if page.shown.replace(page.shown.get() + 1) == 0 {
                        page.send(Command::Shown(true));
                    }
                    handle.retain(Shown(page.clone(), handle.node()));
                })
                .child(
                    // Unnamed: the page's own tree stands for it (`embedding`).
                    gpu_view("", &page.gpu)
                        .absolute()
                        .left(0.)
                        .top(0.)
                        .w_full()
                        .h_full(),
                )
                .child(switch(
                    move || shown.get(),
                    move |_, cx| option_menu(&menu, cx),
                ))
        }))
    })
}
/// The page's `<select>` popup, as a zgx menu under the element.
fn option_menu(page: &Rc<UiPage>, cx: &mut zgx::compose::Context) -> View {
    let Some(menu) = page.options.borrow().clone() else {
        return div();
    };
    let open = cx.runtime().signal(true);
    let chosen = Rc::new(Cell::new(false));
    // Closed with nothing chosen (Escape, a press outside): WebKit hears so
    // after the click that closed it, which chooses first if it was an item.
    let closed = {
        let (open, chosen, page) = (open.clone(), chosen.clone(), page.clone());
        cx.runtime().effect(move || {
            if !open.get() {
                let (chosen, page) = (chosen.clone(), page.clone());
                zgx::native_view::post(move || {
                    if !chosen.get() {
                        page.send(Command::Choose(None));
                    }
                });
            }
        })
    };
    cx.retain(closed);
    let scale = page.scale.get().max(0.01);
    let (x, y, w, h) = menu.at;
    let anchor = div()
        .absolute()
        .left(x / scale)
        .top(y / scale)
        .w(w / scale)
        .h(h / scale);
    let items = menu.items.iter().enumerate().map(|(i, item)| {
        let entry = menu_item(item.label.clone()).disabled(item.group || !item.enabled);
        let selected = item.selected;
        let entry = entry.checked_when(move || selected);
        let (chosen, page) = (chosen.clone(), page.clone());
        entry.on_click(move || {
            chosen.set(true);
            page.send(Command::Choose(Some(i as u32)));
        })
    });
    zgx::compose::menu("Options", open, anchor).children(items)
}
/// A box showing a page: the page shows while one does.
struct Shown(Rc<UiPage>, NodeId);
impl Drop for Shown {
    fn drop(&mut self) {
        let page = &self.0;
        if page.node.get() == Some(self.1) {
            page.node.set(None);
        }
        if page.shown.replace(page.shown.get().saturating_sub(1)) == 1 {
            page.send(Command::Shown(false));
        }
    }
}

// The window's hooks (`NativeHost`).

/// Whether `node` is a page's box, and its author ID embedding the page's
/// accessible tree once WebKit has named it (`accesskit_atspi_common::EMBED`).
pub(crate) fn embedding(node: NodeId) -> Option<Option<String>> {
    if !live() {
        return None;
    }
    UI_PAGES.with(|pages| {
        pages
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .find(|page| page.node.get() == Some(node))
            .map(|page| page.embedding.borrow().clone())
    })
}
/// The page whose box has zgx's focus.
fn focused_page(ui: &Ui) -> Option<Rc<UiPage>> {
    if !live() {
        return None;
    }
    let focused = ui.input.focused()?;
    UI_PAGES.with(|pages| {
        pages
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .find(|page| page.node.get() == Some(focused))
    })
}

/// Send a key to the page whose box has focus, before zgx hears it: `true`
/// if it went there. A key the app reserves (`KeyBinding::reserved`) stays
/// the app's; one the page leaves alone comes back (`take_unhandled_keys`).
pub(crate) fn page_key(
    ui: &Ui,
    event: &winit::event::KeyEvent,
    modifiers: Modifiers,
    key: Option<&zgx::input::Key>,
) -> bool {
    let Some(page) = focused_page(ui) else {
        return false;
    };
    let press = event.state == winit::event::ElementState::Pressed;
    if press
        && let Some(key) = key
        && ui.input.reserves_key(&ui.scene.borrow(), key, modifiers)
    {
        return false;
    }
    let Some(keyval) = keyval(&event.logical_key) else {
        return false;
    };
    use winit::platform::scancode::PhysicalKeyExtScancode;
    let keycode = event
        .physical_key
        .to_scancode()
        .map_or(0, |code| code as u16 + 8);
    // The event's time names it when WebKit sends it back unhandled.
    let time = now_ms().max(page.last_key.get() + 1);
    page.last_key.set(time);
    if press {
        let mut keys = page.keys.borrow_mut();
        keys.push_back((time, (event.clone(), modifiers)));
        while keys.len() > 64 {
            keys.pop_front();
        }
    }
    let state = modifier_state(modifiers);
    // A paste where GTK's clipboard reaches nothing: zgx's text first.
    let paste = (modifiers.control && (keyval == 0x76 || keyval == 0x56))
        || (modifiers.shift && keyval == 0xff63);
    if press && paste && BRIDGED.load(std::sync::atomic::Ordering::Relaxed) {
        let text = page
            .clipboard
            .borrow_mut()
            .as_mut()
            .and_then(|clipboard| clipboard.get_text().ok());
        if let Some(text) = text {
            page.send(Command::Clipboard(text));
        }
    }
    page.send(Command::Key {
        press,
        keyval,
        keycode,
        state,
        time,
    });
    true
}
/// A key sent to a page, with the modifiers held as it was pressed: WebKit
/// answers after its web process has seen it, by when they may be up (on a
/// loaded machine, Ctrl+K came back as K, and no binding took it).
pub(crate) type KeyPress = (winit::event::KeyEvent, Modifiers);
/// Keys pages in `ui`'s window left alone since last asked, for the app's
/// shortcuts.
pub(crate) fn take_unhandled_keys(ui: &Ui) -> Vec<KeyPress> {
    if !live() {
        return Vec::new();
    }
    UNHANDLED.with(|unhandled| {
        let mut unhandled = unhandled.borrow_mut();
        if unhandled.is_empty() {
            return Vec::new();
        }
        let ours =
            |scene: &Weak<RefCell<Scene>>| std::ptr::eq(scene.as_ptr(), Rc::as_ptr(&ui.scene));
        let (taken, kept) = std::mem::take(&mut *unhandled)
            .into_iter()
            .partition(|(scene, _)| ours(scene));
        *unhandled = kept;
        taken
            .into_iter()
            .map(|(_, press)| press)
            .collect::<Vec<_>>()
    })
}
/// Whether a page in `ui`'s window moved its caret or began or ended editing
/// since last asked: WebKit says so apart from the frame it draws, maybe
/// after it, so the window tells its input method then, not at its next
/// frame (an idle window draws none).
pub(crate) fn ime_moved(ui: &Ui) -> bool {
    if !live() {
        return false;
    }
    IME_MOVED.with(|moved| {
        let mut moved = moved.borrow_mut();
        let count = moved.len();
        moved.retain(|scene| {
            scene.strong_count() > 0 && !std::ptr::eq(scene.as_ptr(), Rc::as_ptr(&ui.scene))
        });
        moved.len() != count
    })
}
/// Move zgx's focus to or from pages' boxes as their controllers asked.
pub(crate) fn sync_focus(ui: &Ui) {
    if !live() {
        return;
    }
    let requests = FOCUS.with(|requests| std::mem::take(&mut *requests.borrow_mut()));
    for (node, focus) in requests {
        if !ui.scene.borrow().contains(node) {
            continue;
        }
        if focus {
            ui.input.focus(&ui.scene, Some(node));
        } else if ui.input.focused() == Some(node) {
            ui.input.focus(&ui.scene, None);
        }
    }
}
/// The focused page's box, while an editable in it has the page's focus:
/// what an input method composes into.
pub(crate) fn ime_page(ui: &Ui) -> Option<NodeId> {
    focused_page(ui)
        .filter(|page| page.editing.get())
        .and_then(|page| page.node.get())
}
/// Where the focused page's caret shows, in the window's points.
pub(crate) fn ime_area(ui: &Ui) -> Option<Rect> {
    let page = focused_page(ui).filter(|page| page.editing.get())?;
    let (x, y, w, h) = page.caret.get()?;
    let node = page.node.get()?;
    let scene = ui.scene.borrow();
    let bounds = scene.bounds(node);
    let scale = page.scale.get().max(0.01);
    let rect = Rect::new(
        bounds.x + x / scale,
        bounds.y + y / scale,
        w / scale,
        h / scale,
    );
    Some(scene.window_rect(node, rect))
}
/// Give an input method's event to the page composing (`ime_page`): `true`
/// if it took it.
pub(crate) fn page_ime(ui: &Ui, target: Option<NodeId>, event: &winit::event::Ime) -> bool {
    let Some(page) = focused_page(ui).filter(|page| target.is_some() && page.node.get() == target)
    else {
        return false;
    };
    let ime = match event {
        winit::event::Ime::Enabled => return true,
        winit::event::Ime::Disabled => Ime::Preedit(String::new(), 0),
        winit::event::Ime::Preedit(text, cursor) => {
            let at = cursor.map_or(text.chars().count(), |(start, _)| {
                text[..start.min(text.len())].chars().count()
            });
            Ime::Preedit(text.clone(), at as u32)
        }
        winit::event::Ime::Commit(text) => Ime::Commit(text.clone()),
    };
    page.send(Command::Ime(ime));
    true
}

/// The GDK key value of a key as the layout made it.
fn keyval(key: &winit::keyboard::Key) -> Option<u32> {
    use winit::keyboard::{Key, NamedKey};
    Some(match key {
        Key::Character(text) => {
            let c = text.chars().next()?;
            // SAFETY: a pure GDK function, on any thread.
            gtk::api().map(|api| unsafe { (api.gdk_unicode_to_keyval)(c as u32) })?
        }
        Key::Named(named) => match named {
            NamedKey::Space => 0x20,
            NamedKey::Enter => 0xff0d,
            NamedKey::Tab => 0xff09,
            NamedKey::Backspace => 0xff08,
            NamedKey::Escape => 0xff1b,
            NamedKey::Delete => 0xffff,
            NamedKey::Insert => 0xff63,
            NamedKey::Home => 0xff50,
            NamedKey::ArrowLeft => 0xff51,
            NamedKey::ArrowUp => 0xff52,
            NamedKey::ArrowRight => 0xff53,
            NamedKey::ArrowDown => 0xff54,
            NamedKey::PageUp => 0xff55,
            NamedKey::PageDown => 0xff56,
            NamedKey::End => 0xff57,
            NamedKey::ContextMenu => 0xff67,
            NamedKey::PrintScreen => 0xff61,
            NamedKey::Pause => 0xff13,
            NamedKey::ScrollLock => 0xff14,
            NamedKey::NumLock => 0xff7f,
            NamedKey::CapsLock => 0xffe5,
            NamedKey::Shift => 0xffe1,
            NamedKey::Control => 0xffe3,
            NamedKey::Alt => 0xffe9,
            NamedKey::AltGraph => 0xfe03,
            NamedKey::Super | NamedKey::Meta => 0xffeb,
            NamedKey::F1 => 0xffbe,
            NamedKey::F2 => 0xffbf,
            NamedKey::F3 => 0xffc0,
            NamedKey::F4 => 0xffc1,
            NamedKey::F5 => 0xffc2,
            NamedKey::F6 => 0xffc3,
            NamedKey::F7 => 0xffc4,
            NamedKey::F8 => 0xffc5,
            NamedKey::F9 => 0xffc6,
            NamedKey::F10 => 0xffc7,
            NamedKey::F11 => 0xffc8,
            NamedKey::F12 => 0xffc9,
            _ => return None,
        },
        _ => return None,
    })
}

// The painter.

/// Draws a page's box: its frame's damage uploaded into a texture of the
/// page's size, then drawn over the box's texture.
struct Painter {
    id: u64,
    frame: Arc<Mutex<Frame>>,
    scale: Rc<Cell<f32>>,
    /// The size and scale the page was last asked for.
    asked: Option<((u32, u32), f32)>,
    gpu: Option<Blit>,
}
struct Blit {
    pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: Option<(wgpu::Texture, wgpu::BindGroup, (u32, u32))>,
}
impl Viewport for Painter {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        self.scale.set(frame.scale);
        let asked = (frame.size, frame.scale);
        if self.asked != Some(asked) {
            self.asked = Some(asked);
            let id = self.id;
            let (width, height, scale) = (frame.size.0, frame.size.1, frame.scale as f64);
            on_gtk(move |gtk| {
                if let Some(page) = gtk.page(id) {
                    page.run(Command::Size {
                        width,
                        height,
                        scale,
                    });
                }
            });
        }
        let blit = match &mut self.gpu {
            Some(blit) if blit.format == frame.format => blit,
            _ => self.gpu.insert(Blit::new(frame.device, frame.format)),
        };
        {
            let mut shared = self.frame.lock().unwrap();
            shared.told = false;
            let size = (shared.width, shared.height);
            if size.0 > 0 && size.1 > 0 {
                let fresh = blit.texture.as_ref().is_none_or(|(_, _, at)| *at != size);
                if fresh {
                    let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("web page"),
                        size: wgpu::Extent3d {
                            width: size.0,
                            height: size.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    let bind = frame.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("web page"),
                        layout: &blit.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&blit.sampler),
                            },
                        ],
                    });
                    blit.texture = Some((texture, bind, size));
                }
                let damage = if fresh {
                    Some((0, 0, size.0, size.1))
                } else {
                    shared.damage
                };
                shared.damage = None;
                if let (Some((x, y, w, h)), Some((texture, ..))) = (damage, &blit.texture)
                    && w > 0
                    && h > 0
                {
                    let row = size.0 * 4;
                    frame.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x, y, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &shared.pixels[(y * row + x * 4) as usize..],
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(row),
                            rows_per_image: None,
                        },
                        wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("web page"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        if let Some((_, bind, _)) = &blit.texture {
            pass.set_pipeline(&blit.pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
    fn device_changed(&mut self) {
        // A new texture, uploaded whole.
        self.gpu = None;
    }
}
impl Blit {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("web page"),
            source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("web page"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("web page"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("web page"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("web page"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            format,
            layout,
            sampler,
            texture: None,
        }
    }
}
/// The page's texture over the whole box (one triangle covering it).
const BLIT: &str = r#"
@group(0) @binding(0) var page: texture_2d<f32>;
@group(0) @binding(1) var linear: sampler;
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}
@fragment fn fragment(in: Out) -> @location(0) vec4<f32> {
    return textureSample(page, linear, in.uv);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Damage gathered between two uploads is the box around all of it.
    #[test]
    fn damage_unites_into_one_box() {
        assert_eq!(union(None, (4, 5, 6, 7)), (4, 5, 6, 7));
        assert_eq!(union(Some((0, 0, 10, 10)), (20, 5, 5, 20)), (0, 0, 25, 25));
        assert_eq!(union(Some((10, 10, 5, 5)), (12, 12, 1, 1)), (10, 10, 5, 5));
    }

    /// zgx's modifiers as GDK's state: Meta is GDK's Super and Meta both,
    /// as a Super key press sets them.
    #[test]
    fn modifiers_are_gdk_state() {
        let all = Modifiers {
            shift: true,
            control: true,
            alt: true,
            meta: true,
        };
        assert_eq!(
            modifier_state(all),
            gtk::GDK_SHIFT_MASK
                | gtk::GDK_CONTROL_MASK
                | gtk::GDK_MOD1_MASK
                | gtk::GDK_SUPER_MASK
                | gtk::GDK_META_MASK
        );
        assert_eq!(modifier_state(Modifiers::default()), 0);
    }

    /// Keys named by the layout are GDK's key values (gdkkeysyms.h).
    #[test]
    fn named_keys_are_gdk_key_values() {
        use winit::keyboard::{Key, NamedKey};
        for (key, value) in [
            (NamedKey::Enter, 0xff0d),
            (NamedKey::Tab, 0xff09),
            (NamedKey::Backspace, 0xff08),
            (NamedKey::Escape, 0xff1b),
            (NamedKey::ArrowLeft, 0xff51),
            (NamedKey::PageDown, 0xff56),
            (NamedKey::F12, 0xffc9),
            (NamedKey::Control, 0xffe3),
            (NamedKey::Space, 0x20),
        ] {
            assert_eq!(keyval(&Key::Named(key)), Some(value), "{key:?}");
        }
        assert_eq!(keyval(&Key::Named(NamedKey::BrowserBack)), None);
    }

    /// A named profile keeps its data under the user's XDG directories and
    /// the program's name, its name made safe for a path.
    #[test]
    fn a_named_profile_has_directories_of_its_own() {
        let (data, cache) = profile_directories("work/../me");
        assert!(data.ends_with("/web-profiles/work____me"), "{data}");
        assert!(cache.ends_with("/web-profiles/work____me"), "{cache}");
        assert_ne!(data, cache);
        assert!(!data.contains("/../"));
    }

    /// Each page's message channel carries a secret of its own, which the
    /// script it injects holds and no page script can read.
    #[test]
    fn each_page_has_its_own_secret() {
        let (a, b) = (nonce(1), nonce(1));
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        let script = CHANNEL_SCRIPT.replace("NONCE", &format!("'{a}'"));
        assert!(script.contains(&format!("n: '{a}'")));
        assert!(!script.contains("NONCE"));
    }
}
