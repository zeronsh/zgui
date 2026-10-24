//! A WebView2 page behind a `zgui::web_view::WebView` controller (Windows).
//!
//! Each page is a composition controller: its visual goes in the window's
//! DirectComposition tree (dcomp.rs, which also forwards the pointer to it),
//! and keyboard focus goes to the hidden window WebView2 keeps for it. Making
//! the environment and the page is asynchronous; commands wait in order until
//! the page is ready. WebView2's callbacks arrive on the UI thread from its
//! message loop; each reads what it needs and posts the state change
//! (`zgui::native_view::post`), applied at the event loop's next safe point.
//!
//! The runtime is the system's (Windows 11 ships it): without one, web views
//! stay placeholders and say why once (`available`).
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    rc::Rc,
    sync::Mutex,
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::DirectComposition::IDCompositionVisual,
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, IStream},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetFocus, SetFocus},
            Shell::SHCreateMemStream,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, HCURSOR, HWND_MESSAGE, IsChild, PostMessageW,
                RegisterClassW, SetCursor, WM_APP, WNDCLASSW,
            },
        },
    },
    core::{HRESULT, HSTRING, Interface, PCWSTR, PWSTR},
};
use zgui::web_view::{
    NavigationKind, NavigationRequest, SchemeRequest, SchemeResponder, SchemeResponse,
    ScriptResult, WeakWebView, WebProfile, WebView, WebViewBackend, WebViewCommand, WebViewEvent,
};

// The loader linked in (webview2-com-sys ships it): no DLL beside the app.
// SAFETY: WebView2Loader.h's declarations (`STDAPI`, so "system"): string
// arguments are NUL-terminated or null, `options` and `handler` the COM
// interfaces' raw pointers, `version` a string the loader allocates with
// CoTaskMemAlloc.
#[link(name = "WebView2LoaderStatic", kind = "static")]
unsafe extern "system" {
    #[link_name = "CreateCoreWebView2EnvironmentWithOptions"]
    fn create_environment(
        browser_folder: PCWSTR,
        user_data_folder: PCWSTR,
        options: *mut c_void,
        handler: *mut c_void,
    ) -> HRESULT;
    #[link_name = "GetAvailableCoreWebView2BrowserVersionString"]
    fn browser_version(browser_folder: PCWSTR, version: *mut PWSTR) -> HRESULT;
}

/// Defines `window.zgui.postMessage(value)`, sending `value` as JSON, and
/// WebKit's `window.webkit.messageHandlers.zgui` for pages written for it.
const CHANNEL_SCRIPT: &str = "(() => { const view = window.chrome && window.chrome.webview; if (!view || window.zgui) return; const post = (value) => view.postMessage(JSON.stringify(value === undefined ? null : value)); Object.defineProperty(window, 'zgui', { value: Object.freeze({ postMessage: post }) }); if (!window.webkit) { Object.defineProperty(window, 'webkit', { value: Object.freeze({ messageHandlers: Object.freeze({ zgui: Object.freeze({ postMessage: (value) => typeof value === 'string' ? view.postMessage(value) : post(value) }) }) }) }); } })();";

/// Whether the WebView2 runtime is installed (said once when it is not).
pub(crate) fn available() -> bool {
    thread_local! {
        static AVAILABLE: std::cell::OnceCell<bool> = const { std::cell::OnceCell::new() };
    }
    AVAILABLE.with(|available| {
        *available.get_or_init(|| {
            let mut version = PWSTR::null();
            // SAFETY: the loader writes a string it allocated, freed here.
            let result = unsafe { browser_version(PCWSTR::null(), &mut version) };
            let found = result.is_ok() && !version.is_null();
            if !version.is_null() {
                // SAFETY: allocated by the loader with CoTaskMemAlloc.
                unsafe { CoTaskMemFree(Some(version.0 as *const c_void)) };
            }
            if !found {
                eprintln!(
                    "zgui: web views need the WebView2 runtime, which is not installed ({}): they show placeholders",
                    windows::core::Error::from_hresult(result).message()
                );
            }
            found
        })
    })
}

fn take(get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut value = PWSTR::null();
    match get(&mut value) {
        Ok(()) if !value.is_null() => take_pwstr(value),
        _ => String::new(),
    }
}
fn flag(get: impl FnOnce(*mut windows::core::BOOL) -> windows::core::Result<()>) -> bool {
    let mut value = windows::core::BOOL(0);
    get(&mut value).is_ok() && value.as_bool()
}

// Environments: one per set of app schemes (WebView2 registers schemes
// with its environment, and an environment's options are fixed by the
// first to use its data folder).
enum Environment {
    Pending(Vec<Box<dyn FnOnce(Option<ICoreWebView2Environment>)>>),
    Ready(ICoreWebView2Environment),
    Failed,
}
thread_local! {
    static ENVIRONMENTS: RefCell<HashMap<Vec<String>, Environment>> = RefCell::new(HashMap::new());
}

/// Where pages keep their profiles: the app's local data, by its name.
fn data_folder(schemes: &[String]) -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "zgui".into());
    let mut folder = base.join(app).join("WebView2");
    if !schemes.is_empty() {
        // A data folder takes one environment's options: another set of
        // schemes keeps its profiles apart.
        let hash = schemes
            .join(",")
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        folder.set_file_name(format!("WebView2-{hash:016x}"));
    }
    folder
}

/// Call `ready` with the environment for `schemes`, made now if need be.
fn with_environment(
    schemes: Vec<String>,
    ready: impl FnOnce(Option<ICoreWebView2Environment>) + 'static,
) {
    let mut ready: Option<Box<dyn FnOnce(Option<ICoreWebView2Environment>)>> =
        Some(Box::new(ready));
    let (found, start) = ENVIRONMENTS.with(|environments| {
        let mut environments = environments.borrow_mut();
        match environments.get_mut(&schemes) {
            Some(Environment::Ready(environment)) => (Some(Some(environment.clone())), false),
            Some(Environment::Failed) => (Some(None), false),
            Some(Environment::Pending(waiting)) => {
                waiting.extend(ready.take());
                (None, false)
            }
            None => {
                let waiting = ready.take().into_iter().collect();
                environments.insert(schemes.clone(), Environment::Pending(waiting));
                (None, true)
            }
        }
    });
    if let (Some(found), Some(ready)) = (found, ready) {
        ready(found);
        return;
    }
    if !start {
        return;
    }
    // SAFETY: COM on the UI thread, which WebView2 needs (an STA).
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let options = CoreWebView2EnvironmentOptions::default();
    // SAFETY: options of our own, read by the loader during the call.
    unsafe {
        let registrations = schemes
            .iter()
            .map(|scheme| {
                let registration = CoreWebView2CustomSchemeRegistration::new(scheme.clone());
                // Its own origin and, like `https`, a secure context; any page may load it.
                registration.set_treat_as_secure(true);
                registration.set_has_authority_component(true);
                registration.set_allowed_origins(vec!["*".into()]);
                Some(registration.into())
            })
            .collect();
        options.set_scheme_registrations(registrations);
    }
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let key = schemes.clone();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
        move |result, environment| {
            if let Err(error) = &result {
                eprintln!(
                    "zgui: WebView2 environment unavailable: {}",
                    error.message()
                );
            }
            let environment = environment.filter(|_| result.is_ok());
            let waiting = ENVIRONMENTS.with(|environments| {
                let mut environments = environments.borrow_mut();
                let state = match &environment {
                    Some(environment) => Environment::Ready(environment.clone()),
                    None => Environment::Failed,
                };
                match environments.insert(key.clone(), state) {
                    Some(Environment::Pending(waiting)) => waiting,
                    _ => Vec::new(),
                }
            });
            for ready in waiting {
                ready(environment.clone());
            }
            Ok(())
        },
    ));
    let folder = HSTRING::from(data_folder(&schemes).as_os_str());
    // SAFETY: on the UI thread, in its STA; `folder`, `options` and
    // `handler` are locals alive across the call, and the loader takes
    // references of its own to the interfaces it keeps (COM's rule).
    let result = unsafe {
        create_environment(
            PCWSTR::null(),
            PCWSTR(folder.as_ptr()),
            options.as_raw(),
            handler.as_raw(),
        )
    };
    if result.is_err() {
        eprintln!(
            "zgui: WebView2 environment unavailable: {}",
            windows::core::Error::from_hresult(result).message()
        );
        let waiting = ENVIRONMENTS.with(|environments| {
            match environments
                .borrow_mut()
                .insert(schemes, Environment::Failed)
            {
                Some(Environment::Pending(waiting)) => waiting,
                _ => Vec::new(),
            }
        });
        for ready in waiting {
            ready(None);
        }
    }
}

/// A profile name WebView2 takes (letters, digits and a few marks, at most
/// 64), stable for a name.
fn profile_name(name: &str) -> String {
    let plain = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if plain {
        return name.into();
    }
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("zgui-{hash:016x}")
}

/// Requests for app schemes waiting on their answers, by id (UI thread).
struct Served {
    args: ICoreWebView2WebResourceRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    environment: ICoreWebView2Environment,
}
thread_local! {
    static SERVED: RefCell<HashMap<u64, Served>> = RefCell::new(HashMap::new());
    static NEXT_SERVED: Cell<u64> = const { Cell::new(0) };
}
/// Answers from any thread, until the UI thread hands them to WebView2.
static ANSWERS: Mutex<Vec<(u64, Option<SchemeResponse>)>> = Mutex::new(Vec::new());

/// A message-only window of the UI thread's that hands answers over when
/// posted to (`deliver_answers`), whatever window asked: one closed since
/// still gets its requests completed. Made with the first request (UI thread).
fn answers_window() -> isize {
    thread_local! {
        static WINDOW: std::cell::OnceCell<isize> = const { std::cell::OnceCell::new() };
    }
    /// # Safety
    ///
    /// Called by Windows alone, as the answers window's class procedure.
    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        if message == WM_APP {
            deliver_answers();
            return windows::Win32::Foundation::LRESULT(0);
        }
        // SAFETY: the default procedure, for our own window.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
    WINDOW.with(|window| {
        *window.get_or_init(|| {
            let class = windows::core::w!("zgui.webview2.answers");
            // SAFETY: registers a class of ours and makes a message-only
            // window of it, which lives as long as the thread.
            unsafe {
                let instance = GetModuleHandleW(None).unwrap_or_default();
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    ..Default::default()
                });
                CreateWindowExW(
                    Default::default(),
                    class,
                    None,
                    Default::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(instance.into()),
                    None,
                )
                .map_or(0, |hwnd| hwnd.0 as isize)
            }
        })
    })
}
/// Hand WebView2 the scheme answers that arrived (UI thread).
fn deliver_answers() {
    let answers = std::mem::take(&mut *ANSWERS.lock().unwrap_or_else(|e| e.into_inner()));
    for (id, response) in answers {
        let Some(served) = SERVED.with(|served| served.borrow_mut().remove(&id)) else {
            continue;
        };
        // SAFETY: WebView2 objects on the UI thread.
        unsafe {
            if let Some(response) = response {
                let reason = HSTRING::from(reason_phrase(response.status));
                let headers: String = response
                    .headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect();
                let headers = HSTRING::from(headers);
                let stream: Option<IStream> = SHCreateMemStream(Some(&response.body));
                if let Ok(answer) = served.environment.CreateWebResourceResponse(
                    stream.as_ref(),
                    i32::from(response.status),
                    &reason,
                    &headers,
                ) {
                    let _ = served.args.SetResponse(&answer);
                }
            }
            // Unanswered: the request goes on, and fails as a network error.
            let _ = served.deferral.Complete();
        }
    }
}
fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "",
    }
}

/// What a page's controller has, once made.
struct Ready {
    composition: ICoreWebView2CompositionController,
    controller: ICoreWebView2Controller,
    view: ICoreWebView2,
    environment: ICoreWebView2Environment,
}

/// A page: its WebView2 controller (once made) and what zgui gave it.
pub(crate) struct Page {
    web: WeakWebView,
    ready: RefCell<Option<Ready>>,
    /// Commands from before the page was ready, in order.
    queued: RefCell<Vec<WebViewCommand>>,
    /// Ready and its scripts in: commands run at once.
    started: Cell<bool>,
    /// The page could not be made.
    failed: Cell<bool>,
    /// The visual it shows in, its bounds in the window (physical pixels:
    /// left, top, right, bottom) and whether it is shown.
    target: RefCell<Option<IDCompositionVisual>>,
    bounds: Cell<(i32, i32, i32, i32)>,
    visible: Cell<bool>,
    parent: Cell<HWND>,
    scale: Cell<f32>,
    focused: Cell<bool>,
    /// The pointer is over it (dcomp.rs): its cursor shows at once.
    pub hovered: Cell<bool>,
    cursor: Cell<Option<HCURSOR>>,
    /// Loading, as WebView2 reports no such state.
    loading: Cell<bool>,
    /// The last HTML shown and its base: WebView2 serves the base URL's
    /// request with it (none: `NavigateToString`).
    html: RefCell<Option<(String, Option<String>)>>,
    /// The page served for a base URL (set while loading `html` there).
    serving: RefCell<Option<String>>,
    /// The URL of the last top-frame navigation that started.
    navigating: RefCell<String>,
    /// The folder of HTML shown with a file base (see `served_url`).
    files: RefCell<Option<std::path::PathBuf>>,
    /// The app schemes it serves (its options' that WebView2 can take).
    schemes: RefCell<Vec<String>>,
    /// Commits the tree of the window it shows in, for changes made outside
    /// that window's frames (the page becoming ready, or going).
    commit: RefCell<Option<Rc<dyn Fn()>>>,
}

/// The `WebViewBackend` of a page (its `as_any` finds the page).
pub(crate) struct Backend(pub Rc<Page>);
impl WebViewBackend for Backend {
    fn run(&self, command: WebViewCommand) {
        // Queued until its scripts are in: then in order.
        if self.0.started.get() {
            self.0.run(command);
        } else if !self.0.failed.get() {
            self.0.queued.borrow_mut().push(command);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        // The controller goes now (with the page's processes). Taken out
        // first: focus moving and the controller closing send messages and
        // events, whose handlers read `ready`.
        let ready = self.0.ready.borrow_mut().take();
        if let Some(ready) = ready {
            // SAFETY: WebView2 objects on the UI thread.
            unsafe {
                if self.0.has_focus() {
                    let _ = SetFocus(Some(self.0.parent.get()));
                }
                let _ = ready.composition.SetRootVisualTarget(None);
                let _ = ready.controller.Close();
            }
            self.0.committed();
        }
    }
}

/// The page behind `web`, if it has one of ours.
pub(crate) fn page_of(web: &WebView) -> Option<Rc<Page>> {
    let backend = web.backend()?;
    backend
        .as_any()
        .downcast_ref::<Backend>()
        .map(|backend| backend.0.clone())
}

/// The page behind `web`, made now (for window `parent`) if it has none.
pub(crate) fn backend(web: &WebView, parent: HWND, scale: f32) -> Option<Rc<Page>> {
    if let Some(page) = page_of(web) {
        page.scale.set(scale);
        return Some(page);
    }
    let page = Rc::new(Page {
        web: web.downgrade(),
        ready: RefCell::new(None),
        queued: RefCell::new(Vec::new()),
        started: Cell::new(false),
        failed: Cell::new(false),
        target: RefCell::new(None),
        bounds: Cell::new((0, 0, 0, 0)),
        visible: Cell::new(false),
        parent: Cell::new(parent),
        scale: Cell::new(scale),
        focused: Cell::new(false),
        hovered: Cell::new(false),
        cursor: Cell::new(None),
        loading: Cell::new(false),
        html: RefCell::new(None),
        serving: RefCell::new(None),
        navigating: RefCell::new(String::new()),
        files: RefCell::new(None),
        schemes: RefCell::new(Vec::new()),
        commit: RefCell::new(None),
    });
    web.attach(Rc::new(Backend(page.clone())));
    let mut schemes: Vec<String> = Vec::new();
    for handler in &web.options().schemes {
        let scheme = handler.scheme.clone();
        if zgui::web_view::is_app_scheme(&scheme) && !schemes.contains(&scheme) {
            schemes.push(scheme);
        } else {
            eprintln!(
                "zgui: web view scheme `{scheme}` left out: not a scheme an app can serve, or given twice"
            );
        }
    }
    schemes.sort();
    page.schemes.replace(schemes.clone());
    let weak = Rc::downgrade(&page);
    with_environment(schemes, move |environment| {
        let Some(page) = weak.upgrade() else {
            return;
        };
        match environment {
            Some(environment) => Page::create(&page, environment),
            None => page.fail("WebView2 is unavailable"),
        }
    });
    Some(page)
}

impl Page {
    fn post(&self, delivery: impl FnOnce(&WebView) + 'static) {
        let web = self.web.clone();
        zgui::native_view::post(move || {
            if let Some(web) = web.upgrade() {
                delivery(&web);
            }
        });
    }
    fn fail(&self, error: &str) {
        self.failed.set(true);
        self.queued.borrow_mut().clear();
        let error = error.to_owned();
        self.post(move |web| web.update_state(|state| state.error = Some(error)));
    }
    /// Make the controller in `environment`, for the page's profile.
    fn create(page: &Rc<Page>, environment: ICoreWebView2Environment) {
        let Some(web) = page.web.upgrade() else {
            return;
        };
        let weak = Rc::downgrade(page);
        let made = environment.clone();
        let handler = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(
            move |result, composition| {
                let Some(page) = weak.upgrade() else {
                    // Gone before it was ready.
                    if let Some(composition) = composition
                        && let Ok(controller) = composition.cast::<ICoreWebView2Controller>()
                    {
                        // SAFETY: closes a controller nothing holds.
                        let _ = unsafe { controller.Close() };
                    }
                    return Ok(());
                };
                match (result, composition) {
                    (Ok(()), Some(composition)) => page.ready(composition, made),
                    (result, _) => {
                        let message = result.err().map(|e| e.message()).unwrap_or_default();
                        eprintln!("zgui: a web view could not be made: {message}");
                        page.fail("The page could not be made.");
                    }
                }
                Ok(())
            },
        ));
        let parent = page.parent.get();
        // SAFETY: WebView2 calls on the UI thread; it keeps the handler.
        let result = unsafe {
            match environment.cast::<ICoreWebView2Environment10>() {
                Ok(environment) => (|| {
                    let options = environment.CreateCoreWebView2ControllerOptions()?;
                    match &web.options().profile {
                        WebProfile::Default => {}
                        WebProfile::Ephemeral => {
                            // Each ephemeral view gets a store of its own.
                            options.SetIsInPrivateModeEnabled(true)?;
                            options.SetProfileName(&HSTRING::from(format!(
                                "zgui-ephemeral-{}",
                                web.id()
                            )))?;
                        }
                        WebProfile::Named(name) => {
                            options.SetProfileName(&HSTRING::from(profile_name(name)))?;
                        }
                    }
                    environment.CreateCoreWebView2CompositionControllerWithOptions(
                        parent, &options, &handler,
                    )
                })(),
                Err(_) => environment
                    .cast::<ICoreWebView2Environment3>()
                    .and_then(|environment| {
                        environment.CreateCoreWebView2CompositionController(parent, &handler)
                    }),
            }
        };
        if let Err(error) = result {
            eprintln!("zgui: a web view could not be made: {}", error.message());
            page.fail("The page could not be made.");
        }
    }
    /// The controller is made: set it up, show it where zgui placed it, and
    /// run what was asked of it meanwhile (once its scripts are in).
    fn ready(
        self: &Rc<Self>,
        composition: ICoreWebView2CompositionController,
        environment: ICoreWebView2Environment,
    ) {
        let Some(web) = self.web.upgrade() else {
            return;
        };
        let options = web.options().clone();
        // SAFETY: WebView2 objects on the UI thread throughout.
        let setup = unsafe {
            (|| -> windows::core::Result<Ready> {
                let controller: ICoreWebView2Controller = composition.cast()?;
                let view = controller.CoreWebView2()?;
                let settings = view.Settings()?;
                settings.SetAreDevToolsEnabled(options.inspectable)?;
                // WebKit shows no link status bar.
                settings.SetIsStatusBarEnabled(false)?;
                if let Some(agent) = &options.user_agent
                    && let Ok(settings) = settings.cast::<ICoreWebView2Settings2>()
                {
                    settings.SetUserAgent(&HSTRING::from(agent))?;
                }
                if options.transparent
                    && let Ok(controller) = controller.cast::<ICoreWebView2Controller2>()
                {
                    controller.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                        A: 0,
                        R: 0,
                        G: 0,
                        B: 0,
                    })?;
                }
                // zgui's scale, not the monitor's as WebView2 sees it.
                if let Ok(controller) = controller.cast::<ICoreWebView2Controller3>() {
                    controller.SetShouldDetectMonitorScaleChanges(false)?;
                    controller.SetRasterizationScale(f64::from(self.scale.get()))?;
                }
                Ok(Ready {
                    composition,
                    controller,
                    view,
                    environment,
                })
            })()
        };
        let ready = match setup {
            Ok(ready) => ready,
            Err(error) => {
                eprintln!("zgui: a web view could not be set up: {}", error.message());
                self.fail("The page could not be made.");
                return;
            }
        };
        self.subscribe(&ready, &options);
        // SAFETY: as above.
        unsafe {
            for scheme in self.schemes.borrow().iter() {
                let filter = HSTRING::from(format!("{scheme}:*"));
                let added = ready.view.cast::<ICoreWebView2_22>().and_then(|view| {
                    view.AddWebResourceRequestedFilterWithRequestSourceKinds(
                        &filter,
                        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                    )
                });
                if added.is_err() {
                    let _ = ready.view.AddWebResourceRequestedFilter(
                        &filter,
                        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    );
                }
            }
        }
        // Scripts before the page's own: in order, and in before the first navigation.
        let mut scripts: Vec<String> = options
            .scripts
            .iter()
            .map(|script| {
                if script.main_frame_only {
                    // WebView2 injects into every frame. Stopped at its top in
                    // iframes, the source stays a script of its own in the top
                    // frame: its top-level `const`, `let` and `class` are
                    // global, as WebKit's are (a block or an eval would scope them).
                    format!(
                        "if (window !== window.top) throw 'zgui: a main-frame-only script';\n{}",
                        script.source
                    )
                } else {
                    script.source.clone()
                }
            })
            .collect();
        if options.messages {
            scripts.push(CHANNEL_SCRIPT.into());
        }
        let view = ready.view.clone();
        *self.ready.borrow_mut() = Some(ready);
        self.apply_target();
        self.committed();
        let remaining = Rc::new(Cell::new(scripts.len()));
        let weak = Rc::downgrade(self);
        let start = Rc::new(move || {
            if let Some(page) = weak.upgrade() {
                let queued = std::mem::take(&mut *page.queued.borrow_mut());
                page.started.set(true);
                for command in queued {
                    page.run(command);
                }
            }
        });
        if scripts.is_empty() {
            start();
            return;
        }
        let added = {
            let remaining = remaining.clone();
            move || {
                remaining.set(remaining.get() - 1);
                if remaining.get() == 0 {
                    start();
                }
            }
        };
        let added = Rc::new(added);
        for script in scripts {
            let done = added.clone();
            let handler = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
                move |_, _| {
                    done();
                    Ok(())
                },
            ));
            // SAFETY: as above.
            if unsafe { view.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(script), &handler) }
                .is_err()
            {
                added();
            }
        }
    }
    /// Follow the page's events.
    fn subscribe(self: &Rc<Self>, ready: &Ready, options: &zgui::web_view::WebViewOptions) {
        let mut token = 0i64;
        let weak = Rc::downgrade(self);
        let on = move |f: &dyn Fn(&Rc<Page>)| {
            if let Some(page) = weak.upgrade() {
                f(&page);
            }
        };
        let view = &ready.view;
        // SAFETY: WebView2 calls on the UI thread; it keeps the handlers.
        unsafe {
            let page = on.clone();
            let _ = view.add_NavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        page(&|page| page.navigation_starting(&args, true));
                    }
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_FrameNavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        page(&|page| page.navigation_starting(&args, false));
                    }
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_ContentLoading(
                &ContentLoadingEventHandler::create(Box::new(move |_, args| {
                    let error_page = args.is_some_and(|args| flag(|v| args.IsErrorPage(v)));
                    page(&|page| {
                        if !error_page {
                            let url = page.url();
                            page.post(move |web| {
                                web.update_state(|state| state.progress = state.progress.max(0.5));
                                web.emit(WebViewEvent::NavigationCommitted { url });
                            });
                        }
                        page.changed();
                    });
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        page(&|page| page.navigation_completed(&args));
                    }
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_SourceChanged(
                &SourceChangedEventHandler::create(Box::new(move |_, _| {
                    page(&|page| page.changed());
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_DocumentTitleChanged(
                &DocumentTitleChangedEventHandler::create(Box::new(move |_, _| {
                    page(&|page| page.changed());
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_HistoryChanged(
                &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                    page(&|page| page.changed());
                    Ok(())
                })),
                &mut token,
            );
            // As WebKit with no delegate for them: pages save no files and
            // get no permission (camera, location, notifications, the
            // clipboard) in the app's name.
            let _ = view.add_PermissionRequested(
                &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                    if let Some(args) = args {
                        let _ = args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
                    }
                    Ok(())
                })),
                &mut token,
            );
            if let Ok(view) = view.cast::<ICoreWebView2_4>() {
                let _ = view.add_DownloadStarting(
                    &DownloadStartingEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            let _ = args.SetCancel(true);
                        }
                        Ok(())
                    })),
                    &mut token,
                );
            }
            let new_windows = options.new_windows;
            let page = on.clone();
            let _ = view.add_NewWindowRequested(
                &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    // Nothing opens: the app hears of it.
                    let _ = args.SetHandled(true);
                    if new_windows {
                        let url = take(|v| args.Uri(v));
                        page(&|page| {
                            let url = url.clone();
                            page.post(move |web| {
                                web.emit(WebViewEvent::NewWindowRequested { url })
                            });
                        });
                    }
                    Ok(())
                })),
                &mut token,
            );
            if options.messages {
                let page = on.clone();
                let _ = view.add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut text = PWSTR::null();
                        let body = match args.TryGetWebMessageAsString(&mut text) {
                            Ok(()) if !text.is_null() => take_pwstr(text),
                            _ => take(|v| args.WebMessageAsJson(v)),
                        };
                        let origin = origin_of(&take(|v| args.Source(v)));
                        page(&|page| {
                            let (body, origin) = (body.clone(), origin.clone());
                            page.post(move |web| {
                                web.emit(WebViewEvent::Message {
                                    body,
                                    origin,
                                    main_frame: true,
                                })
                            });
                        });
                        Ok(())
                    })),
                    &mut token,
                );
            }
            let page = on.clone();
            let _ = view.add_ProcessFailed(
                &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    if let Some(args) = args {
                        let _ = args.ProcessFailedKind(&mut kind);
                    }
                    let ended = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE;
                    if ended {
                        page(&|page| {
                            page.loading.set(false);
                            page.post(|web| {
                                web.update_state(|state| {
                                    state.loading = false;
                                    state.error = Some("The page stopped responding.".into());
                                });
                                web.emit(WebViewEvent::ProcessTerminated);
                            });
                        });
                    }
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = view.add_WebResourceRequested(
                &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        page(&|page| page.serve(&args));
                    }
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = ready.controller.add_GotFocus(
                &FocusChangedEventHandler::create(Box::new(move |_, _| {
                    page(&|page| page.focus_changed(true));
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = ready.controller.add_LostFocus(
                &FocusChangedEventHandler::create(Box::new(move |_, _| {
                    page(&|page| page.focus_changed(false));
                    Ok(())
                })),
                &mut token,
            );
            let page = on.clone();
            let _ = ready.composition.add_CursorChanged(
                &CursorChangedEventHandler::create(Box::new(move |sender, _| {
                    let mut cursor = HCURSOR::default();
                    if let Some(sender) = sender {
                        let _ = sender.Cursor(&mut cursor);
                    }
                    page(&|page| {
                        page.cursor.set((!cursor.is_invalid()).then_some(cursor));
                        if page.hovered.get() && !cursor.is_invalid() {
                            // SAFETY: a cursor the page handed us.
                            SetCursor(Some(cursor));
                        }
                    });
                    Ok(())
                })),
                &mut token,
            );
        }
    }
    fn focus_changed(&self, focused: bool) {
        self.focused.set(focused);
        // The host agrees zgui's focus at its next update (`sync_focus`).
        zgui::native_view::post(|| {});
    }
    fn url(&self) -> String {
        let source = match &*self.ready.borrow() {
            // SAFETY: WebView2 on the UI thread.
            Some(ready) => take(|v| unsafe { ready.view.Source(v) }),
            None => String::new(),
        };
        // WebView2 reports none for some (a `data:` URL): where it went.
        if source.is_empty() {
            self.navigating.borrow().clone()
        } else {
            source
        }
    }
    fn navigation_starting(
        &self,
        args: &ICoreWebView2NavigationStartingEventArgs,
        main_frame: bool,
    ) {
        // SAFETY: event arguments, on the UI thread.
        let (url, user, kind) = unsafe {
            let mut kind = COREWEBVIEW2_NAVIGATION_KIND_NEW_DOCUMENT;
            if let Ok(args) = args.cast::<ICoreWebView2NavigationStartingEventArgs3>() {
                let _ = args.NavigationKind(&mut kind);
            }
            (
                take(|v| args.Uri(v)),
                flag(|v| args.IsUserInitiated(v)),
                kind,
            )
        };
        let request = NavigationRequest {
            url: url.clone(),
            main_frame,
            kind: if kind == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD {
                NavigationKind::BackForward
            } else if kind == COREWEBVIEW2_NAVIGATION_KIND_RELOAD {
                NavigationKind::Reload
            } else if user {
                NavigationKind::Link
            } else {
                NavigationKind::Other
            },
            source_url: if main_frame {
                self.url()
            } else {
                String::new()
            },
        };
        let allowed = self.web.upgrade().is_none_or(|web| web.allows(&request));
        if !allowed {
            // SAFETY: as above.
            let _ = unsafe { args.SetCancel(true) };
            return;
        }
        if main_frame {
            self.loading.set(true);
            self.navigating.replace(url.clone());
            self.post(move |web| {
                web.update_state(|state| {
                    state.error = None;
                    state.loading = true;
                    state.progress = 0.1;
                });
                web.emit(WebViewEvent::NavigationStarted { url });
            });
        }
    }
    fn navigation_completed(&self, args: &ICoreWebView2NavigationCompletedEventArgs) {
        // SAFETY: event arguments, on the UI thread.
        let (success, status) = unsafe {
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
            let _ = args.WebErrorStatus(&mut status);
            (flag(|v| args.IsSuccess(v)), status)
        };
        self.loading.set(false);
        let url = self.url();
        if success {
            self.post(move |web| {
                web.update_state(|state| {
                    state.loading = false;
                    state.progress = 1.;
                });
                web.emit(WebViewEvent::NavigationFinished { url });
            });
        } else if status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
            // A new navigation replaced this one (or the policy cancelled it).
            self.post(|web| web.update_state(|state| state.loading = false));
        } else {
            // The URL that failed, not the page still showing.
            let url = self.navigating.borrow().clone();
            let error = error_message(status);
            self.post(move |web| {
                web.update_state(|state| {
                    state.loading = false;
                    state.error = Some(error.clone());
                });
                web.emit(WebViewEvent::NavigationFailed { url, error });
            });
        }
        self.changed();
    }
    /// Report the page's navigation state as it is now.
    fn changed(&self) {
        let ready = self.ready.borrow();
        let Some(ready) = &*ready else {
            return;
        };
        // SAFETY: WebView2 on the UI thread.
        let (url, title, back, forward) = unsafe {
            (
                self.url(),
                take(|v| ready.view.DocumentTitle(v)),
                flag(|v| ready.view.CanGoBack(v)),
                flag(|v| ready.view.CanGoForward(v)),
            )
        };
        let loading = self.loading.get();
        self.post(move |web| {
            let (old_url, old_title) = web
                .state()
                .with_untracked(|state| (state.url.clone(), state.title.clone()));
            // An untitled page shows its URL as its title in WebView2.
            let title = if title == url { String::new() } else { title };
            web.update_state(|state| {
                if !url.is_empty() {
                    state.url = url.clone();
                }
                state.title = title.clone();
                state.loading = loading;
                state.can_go_back = back;
                state.can_go_forward = forward;
            });
            if !url.is_empty() && url != old_url {
                web.emit(WebViewEvent::UrlChanged { url });
            }
            if title != old_title {
                web.emit(WebViewEvent::TitleChanged { title });
            }
        });
    }
    /// A request WebView2 asks the app for: HTML given for a base URL, or an
    /// app scheme's.
    fn serve(&self, args: &ICoreWebView2WebResourceRequestedEventArgs) {
        let Some(web) = self.web.upgrade() else {
            return;
        };
        let environment = match &*self.ready.borrow() {
            Some(ready) => ready.environment.clone(),
            None => return,
        };
        // SAFETY: event arguments, on the UI thread.
        let request = unsafe {
            let Ok(request) = args.Request() else {
                return;
            };
            request
        };
        // SAFETY: as above.
        let url = take(|v| unsafe { request.Uri(v) });
        let local = match self.html_for(&url) {
            Some(html) => Some((html.into_bytes(), "text/html; charset=utf-8")),
            None if url.starts_with(FILE_ORIGIN) => {
                // The HTML's folder: a file of it, or not found.
                Some(self.serve_file(&url).unwrap_or((Vec::new(), "")))
            }
            None => None,
        };
        if let Some((body, kind)) = local {
            let (status, reason) = if kind.is_empty() {
                (404, windows::core::w!("Not Found"))
            } else {
                (200, windows::core::w!("OK"))
            };
            let headers = HSTRING::from(format!("Content-Type: {kind}"));
            // SAFETY: as above.
            unsafe {
                let stream: Option<IStream> = SHCreateMemStream(Some(&body));
                if let Ok(answer) =
                    environment.CreateWebResourceResponse(stream.as_ref(), status, reason, &headers)
                {
                    let _ = args.SetResponse(&answer);
                }
            }
            return;
        }
        let scheme = url
            .split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !self.schemes.borrow().contains(&scheme) {
            return;
        }
        let Some(handler) = web
            .options()
            .schemes
            .iter()
            .find(|handler| handler.scheme == scheme)
            .cloned()
        else {
            return;
        };
        // SAFETY: as above.
        let (method, headers, body, deferral) = unsafe {
            let method = take(|v| request.Method(v));
            let mut headers = Vec::new();
            if let Ok(iterator) = request.Headers().and_then(|headers| headers.GetIterator()) {
                while flag(|v| iterator.HasCurrentHeader(v)) {
                    let (mut name, mut value) = (PWSTR::null(), PWSTR::null());
                    if iterator.GetCurrentHeader(&mut name, &mut value).is_ok() {
                        headers.push((take_pwstr(name), take_pwstr(value)));
                    }
                    if !flag(|v| iterator.MoveNext(v)) {
                        break;
                    }
                }
            }
            let mut body = Vec::new();
            if let Ok(stream) = request.Content() {
                let mut chunk = [0u8; 16 * 1024];
                loop {
                    let mut read = 0u32;
                    let result = stream.Read(
                        chunk.as_mut_ptr() as *mut c_void,
                        chunk.len() as u32,
                        Some(&mut read),
                    );
                    if result.is_err() || read == 0 {
                        break;
                    }
                    body.extend_from_slice(&chunk[..read as usize]);
                }
            }
            let Ok(deferral) = args.GetDeferral() else {
                return;
            };
            (method, headers, body, deferral)
        };
        let id = NEXT_SERVED.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        SERVED.with(|served| {
            served.borrow_mut().insert(
                id,
                Served {
                    args: args.clone(),
                    deferral,
                    environment,
                },
            )
        });
        let answers = answers_window();
        let responder = SchemeResponder::new(move |response| {
            ANSWERS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((id, response));
            // SAFETY: posts to the UI thread's answers window.
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(answers as *mut c_void)),
                    WM_APP,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        });
        let request = SchemeRequest {
            web_view: web.id(),
            url,
            main_document_url: self.url(),
            method: if method.is_empty() {
                "GET".into()
            } else {
                method
            },
            headers,
            body,
        };
        (handler.serve)(request, responder);
    }
    /// The HTML to answer a request for `url` with, while showing it there.
    fn html_for(&self, url: &str) -> Option<String> {
        let serving = self.serving.borrow();
        if serving.as_deref() != Some(normalized(url).as_str()) {
            return None;
        }
        self.html.borrow().as_ref().map(|(html, _)| html.clone())
    }
    fn has_focus(&self) -> bool {
        let parent = self.parent.get();
        // SAFETY: reads which window has focus.
        self.focused.get()
            && unsafe {
                let focus = GetFocus();
                !focus.is_invalid() && focus != parent && IsChild(parent, focus).as_bool()
            }
    }
    /// The page's keyboard focus as WebView2 last reported it.
    pub fn focused(&self) -> bool {
        self.focused.get()
    }
    pub fn cursor(&self) -> Option<HCURSOR> {
        self.cursor.get()
    }
    /// Show the page in `visual` (the innermost of its cuts) of window
    /// `parent`, whose tree `commit` commits. Within a frame: its commit shows it.
    pub fn mount(&self, visual: &IDCompositionVisual, parent: HWND, commit: Rc<dyn Fn()>) {
        *self.target.borrow_mut() = Some(visual.clone());
        *self.commit.borrow_mut() = Some(commit);
        if self.parent.replace(parent) != parent
            && let Some(ready) = &*self.ready.borrow()
        {
            // SAFETY: WebView2 on the UI thread.
            let _ = unsafe { ready.controller.SetParentWindow(parent) };
        }
        self.apply_target();
    }
    /// Take the page out of `visual`, if it shows there.
    pub fn unmount(&self, visual: &IDCompositionVisual) {
        let mut target = self.target.borrow_mut();
        if target.as_ref() != Some(visual) {
            return;
        }
        *target = None;
        drop(target);
        self.set_visible(false);
        if let Some(ready) = &*self.ready.borrow() {
            // SAFETY: WebView2 on the UI thread.
            let _ = unsafe { ready.composition.SetRootVisualTarget(None) };
        }
    }
    pub fn set_visible(&self, visible: bool) {
        if self.visible.replace(visible) == visible {
            return;
        }
        if !visible && self.has_focus() {
            // SAFETY: gives focus back to the page's window.
            let _ = unsafe { SetFocus(Some(self.parent.get())) };
        }
        if let Some(ready) = &*self.ready.borrow() {
            // SAFETY: WebView2 on the UI thread.
            let _ = unsafe { ready.controller.SetIsVisible(visible) };
        }
    }
    /// The page's box in its window, in physical pixels, and the window's scale.
    pub fn set_bounds(&self, bounds: (i32, i32, i32, i32), scale: f32) {
        let rescaled = self.scale.replace(scale) != scale;
        let moved = self.bounds.replace(bounds) != bounds;
        let ready = self.ready.borrow();
        let Some(ready) = &*ready else {
            return;
        };
        if rescaled && let Ok(controller) = ready.controller.cast::<ICoreWebView2Controller3>() {
            // SAFETY: WebView2 on the UI thread.
            let _ = unsafe { controller.SetRasterizationScale(f64::from(scale)) };
        }
        if moved {
            Self::put_bounds(ready, bounds);
        }
    }
    /// Its window moved on screen (WebView2 places popups and reports
    /// accessibility bounds by it).
    pub fn parent_moved(&self) {
        if let Some(ready) = &*self.ready.borrow() {
            // SAFETY: WebView2 on the UI thread.
            let _ = unsafe { ready.controller.NotifyParentWindowPositionChanged() };
        }
    }
    fn put_bounds(ready: &Ready, (left, top, right, bottom): (i32, i32, i32, i32)) {
        // Relative to the window (popups and accessibility place by it);
        // the page's visual is placed by its cut's offset.
        // SAFETY: WebView2 on the UI thread.
        let _ = unsafe {
            ready.controller.SetBounds(RECT {
                left,
                top,
                right,
                bottom,
            })
        };
    }
    /// Show a change made outside a frame of the page's window.
    fn committed(&self) {
        let commit = self.commit.borrow().clone();
        if let Some(commit) = commit {
            commit();
        }
    }
    /// Put the made controller where zgui placed the page.
    fn apply_target(&self) {
        let ready = self.ready.borrow();
        let Some(ready) = &*ready else {
            return;
        };
        let target = self.target.borrow();
        // SAFETY: WebView2 on the UI thread.
        unsafe {
            if let Ok(controller) = ready.controller.cast::<ICoreWebView2Controller3>() {
                let _ = controller.SetRasterizationScale(f64::from(self.scale.get()));
            }
            let target = target
                .as_ref()
                .and_then(|visual| visual.cast::<windows::core::IUnknown>().ok());
            let _ = ready.composition.SetRootVisualTarget(target.as_ref());
            Self::put_bounds(ready, self.bounds.get());
            let _ = ready
                .controller
                .SetIsVisible(target.is_some() && self.visible.get());
            drop(target);
        }
    }
    /// A mouse message for the page, at `(x, y)` from its top left
    /// (physical pixels). WebView2's event kinds are the messages.
    pub fn send_mouse(&self, message: u32, wparam: WPARAM, x: i32, y: i32) {
        let ready = self.ready.borrow();
        let Some(ready) = &*ready else {
            return;
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONUP,
        };
        let data = match message {
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL | WM_XBUTTONDOWN | WM_XBUTTONUP | WM_XBUTTONDBLCLK => {
                ((wparam.0 >> 16) & 0xffff) as i16 as i32 as u32
            }
            _ => 0,
        };
        let keys = (wparam.0 & 0xffff) as i32;
        // SAFETY: WebView2 on the UI thread.
        let _ = unsafe {
            ready.composition.SendMouseInput(
                COREWEBVIEW2_MOUSE_EVENT_KIND(message as i32),
                COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys),
                data,
                POINT { x, y },
            )
        };
    }
    fn run(&self, command: WebViewCommand) {
        let ready = self.ready.borrow();
        let Some(ready) = &*ready else {
            return;
        };
        let view = &ready.view;
        // SAFETY: WebView2 calls on the UI thread.
        unsafe {
            match command {
                WebViewCommand::LoadUrl(url) => {
                    self.html.replace(None);
                    self.serving.replace(None);
                    self.files.replace(None);
                    let target = if std::path::Path::new(&url).is_absolute() && !url.contains("://")
                    {
                        // A path: show the file.
                        format!("file:///{}", url.replace('\\', "/").trim_start_matches('/'))
                    } else {
                        url.clone()
                    };
                    if view.Navigate(&HSTRING::from(target)).is_err() {
                        self.post(move |web| {
                            web.update_state(|state| state.error = Some("Not a URL".into()));
                            web.emit(WebViewEvent::NavigationFailed {
                                url,
                                error: "Not a URL".into(),
                            });
                        });
                    }
                }
                WebViewCommand::LoadHtml { html, base_url } => {
                    self.show_html(view, html, base_url);
                }
                WebViewCommand::Reload => {
                    // Still on the HTML's page (not a link followed from it).
                    let html = self.html.borrow().clone().filter(|(_, base)| {
                        let shown = match base {
                            Some(_) => self.serving.borrow().clone().unwrap_or_default(),
                            None => "about:blank".into(),
                        };
                        normalized(&take(|v| view.Source(v))) == shown
                    });
                    match html {
                        Some((html, base)) => self.show_html(view, html, base),
                        None => {
                            let _ = view.Reload();
                        }
                    }
                }
                WebViewCommand::Stop => {
                    let _ = view.Stop();
                }
                WebViewCommand::GoBack => {
                    let _ = view.GoBack();
                }
                WebViewCommand::GoForward => {
                    let _ = view.GoForward();
                }
                WebViewCommand::GoToHistory(n) => {
                    // WebView2 lists no history: the page's own goes there.
                    let _ = view.ExecuteScript(
                        &HSTRING::from(format!("history.go({n})")),
                        None::<&ICoreWebView2ExecuteScriptCompletedHandler>,
                    );
                }
                WebViewCommand::Evaluate { script, reply } => {
                    self.evaluate(view, script, reply);
                }
                WebViewCommand::Focus(focus) => {
                    if focus && !self.has_focus() {
                        let _ = ready
                            .controller
                            .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                    } else if !focus && self.has_focus() {
                        let _ = SetFocus(Some(self.parent.get()));
                    }
                }
                WebViewCommand::OpenDevtools => {
                    let _ = view.OpenDevToolsWindow();
                }
                WebViewCommand::SetZoom(zoom) => {
                    if zoom > 0. {
                        let _ = ready.controller.SetZoomFactor(zoom);
                    }
                }
            }
        }
    }
    /// Show `html`: from a string (about:blank), or served for `base`.
    fn show_html(&self, view: &ICoreWebView2, html: String, base: Option<String>) {
        // SAFETY: WebView2 calls on the UI thread.
        unsafe {
            match &base {
                Some(base) => {
                    // Its requests come back to `serve`, answered with the HTML,
                    // at the URL as WebView2 writes it.
                    let served = self.served_url(view, base);
                    let filter = HSTRING::from(served.as_str());
                    let previous = self.serving.replace(Some(served.clone()));
                    if previous.as_deref() != Some(served.as_str()) {
                        if let Some(previous) = previous {
                            let _ = view.RemoveWebResourceRequestedFilter(
                                &HSTRING::from(previous),
                                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
                            );
                        }
                        let _ = view.AddWebResourceRequestedFilter(
                            &filter,
                            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
                        );
                    }
                    self.html.replace(Some((html, Some(base.clone()))));
                    let _ = view.Navigate(&filter);
                }
                None => {
                    self.serving.replace(None);
                    self.files.replace(None);
                    let _ = view.NavigateToString(&HSTRING::from(html.as_str()));
                    self.html.replace(Some((html, None)));
                }
            }
        }
    }
    /// Where HTML given for `base` is served: `base` as WebView2 writes it,
    /// or, for a file (WebView2 asks the app for no `file:` request), its
    /// name on a host of the page's own whose other paths are the files in
    /// its folder (`serve_file`), so relative URLs find its neighbours.
    fn served_url(&self, view: &ICoreWebView2, base: &str) -> String {
        self.files.replace(None);
        if !base.to_ascii_lowercase().starts_with("file:") {
            return normalized(base);
        }
        let path = percent_decoded(base["file:".len()..].trim_start_matches('/'));
        let path = std::path::PathBuf::from(path.replace('/', "\\"));
        let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
            return normalized(base);
        };
        self.files.replace(Some(folder.to_path_buf()));
        let filter = HSTRING::from(format!("{FILE_ORIGIN}/*"));
        // SAFETY: WebView2 on the UI thread.
        let _ = unsafe {
            view.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
        };
        format!("{FILE_ORIGIN}/{}", name.to_string_lossy())
    }
    /// A file of the HTML's folder, for a request on its host: its bytes and
    /// type, or none (outside the folder, or missing).
    fn serve_file(&self, url: &str) -> Option<(Vec<u8>, &'static str)> {
        let folder = self.files.borrow().clone()?;
        let path = url.strip_prefix(FILE_ORIGIN)?.split(['?', '#']).next()?;
        let relative = percent_decoded(path.trim_start_matches('/'));
        let mut file = folder.clone();
        for part in relative.split(['/', '\\']) {
            match part {
                "" | "." => {}
                // Never above the folder.
                ".." => return None,
                part => file.push(part),
            }
        }
        let bytes = std::fs::read(&file).ok()?;
        let kind = match file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .as_deref()
        {
            Some("html" | "htm") => "text/html; charset=utf-8",
            Some("css") => "text/css",
            Some("js" | "mjs") => "text/javascript",
            Some("json") => "application/json",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("ico") => "image/x-icon",
            Some("woff2") => "font/woff2",
            Some("woff") => "font/woff",
            Some("ttf") => "font/ttf",
            Some("wasm") => "application/wasm",
            Some("txt") => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        };
        Some((bytes, kind))
    }
    fn evaluate(
        &self,
        view: &ICoreWebView2,
        script: String,
        reply: Option<Box<dyn FnOnce(ScriptResult)>>,
    ) {
        let web = self.web.clone();
        let deliver = move |result: ScriptResult| {
            if let Some(reply) = reply {
                zgui::native_view::post(move || {
                    // The reply runs even if the controller went.
                    let _ = web.upgrade();
                    reply(result);
                });
            }
        };
        let script = HSTRING::from(script);
        // SAFETY: WebView2 calls on the UI thread; it keeps the handler.
        unsafe {
            // With the exception's message where the runtime reports it.
            if let Ok(view) = view.cast::<ICoreWebView2_21>() {
                let handler = ExecuteScriptWithResultCompletedHandler::create(Box::new(
                    move |result, outcome| {
                        let outcome = match (result, outcome) {
                            (Ok(()), Some(outcome)) => {
                                if flag(|v| outcome.Succeeded(v)) {
                                    Ok(take(|v| outcome.ResultAsJson(v)))
                                } else {
                                    let message = outcome
                                        .Exception()
                                        .map(|exception| take(|v| exception.Message(v)))
                                        .unwrap_or_default();
                                    Err(if message.is_empty() {
                                        "The script threw an exception.".into()
                                    } else {
                                        message
                                    })
                                }
                            }
                            (Err(error), _) => Err(error.message()),
                            (Ok(()), None) => Ok("null".into()),
                        };
                        deliver(outcome);
                        Ok(())
                    },
                ));
                let _ = view.ExecuteScriptWithResult(&script, &handler);
                return;
            }
            let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
                deliver(match result {
                    Ok(()) => Ok(if json.is_empty() { "null".into() } else { json }),
                    Err(error) => Err(error.message()),
                });
                Ok(())
            }));
            let _ = view.ExecuteScript(&script, &handler);
        }
    }
}

/// Where HTML given with a file base is served, with its folder's files
/// (see `served_url`).
const FILE_ORIGIN: &str = "https://zgui-file.invalid";

/// `text` with its `%XX` escapes decoded (UTF-8).
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                i += 3;
            }
            (byte, ..) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `url` as WebView2 writes a request's: scheme and host lowercase, a path
/// (`/` at least) after an authority, no fragment.
fn normalized(url: &str) -> String {
    let url = url.split('#').next().unwrap_or_default();
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, rest) = rest.split_at(end);
    let slash = if rest.starts_with('/') { "" } else { "/" };
    format!(
        "{}://{}{slash}{rest}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    )
}

/// The origin of `url`: `https://example.com`, `myapp://settings` (a port
/// only when it is in the URL).
fn origin_of(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.split(':').next().unwrap_or_default().to_owned();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        host.to_ascii_lowercase()
    )
}

/// What a navigation's failure says.
fn error_message(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> String {
    let message = match status {
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CLIENT_CERTIFICATE_CONTAINS_ERRORS
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID => {
            "The site's certificate is not valid."
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE => "The server could not be reached.",
        COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => "The request timed out.",
        COREWEBVIEW2_WEB_ERROR_STATUS_ERROR_HTTP_INVALID_SERVER_RESPONSE => {
            "The server's response was not valid."
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET => "The connection was lost.",
        COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED => {
            "The internet connection appears to be offline."
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT => "Could not connect to the server.",
        COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => {
            "A server with the specified hostname could not be found."
        }
        COREWEBVIEW2_WEB_ERROR_STATUS_REDIRECT_FAILED => "The redirect failed.",
        _ => "The page could not be loaded.",
    };
    message.into()
}
