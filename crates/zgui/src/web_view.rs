//! Embedded web pages: a [`WebView`] controller and the [`web_view`] /
//! [`web_view_src`] components showing it.
//!
//! ```no_run
//! use zgui::{compose::prelude::*, web_view::{WebView, WebViewOptions}};
//! fn page(cx: &mut Context) -> View {
//!     let web = WebView::new(&cx.runtime(), WebViewOptions::default());
//!     web.load_url("https://example.com");
//!     let title = web.clone();
//!     column()
//!         .grow()
//!         .child(text_signal(move || title.title()))
//!         .child(web_view(&web).grow().rounded(12.))
//! }
//! ```
//!
//! The page is the platform's own view (WKWebView on macOS; WebView2 on
//! Windows, with `zgui-desktop`'s `webview2` feature), laid out, clipped,
//! moved and faded as the node is; zgui content painted after it (menus,
//! popovers, a composer) draws above it, and glass over it blurs the page on
//! macOS (see [`crate::native_view`]). Without a platform backend (headless,
//! the software renderer, other platforms for now) the node renders a
//! labelled placeholder box.
//!
//! The controller outlives the node: unmounting hides the page and keeps it
//! loaded; mounting it again (in any window) shows it where it is placed.
//! Commands issued before the page exists are queued. Navigation state is
//! reactive ([`WebView::state`] and its accessors) and platform events reach
//! [`WebView::on_event`] subscribers on the UI thread at a safe point.
use crate::{
    compose::{View, component, div, text},
    input::InputEvent,
    native_view::{NativeContent, NativeViews},
    reactive::{Runtime, Signal},
    style::Styled,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

/// Where a page keeps cookies, storage and caches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum WebProfile {
    /// The app's default persistent store.
    #[default]
    Default,
    /// Nothing persists; each view gets its own store.
    Ephemeral,
    /// A persistent store of its own, shared by every view naming it (macOS
    /// 14 and later; earlier systems fall back to an ephemeral store).
    Named(String),
}

/// A script run in each page before the page's own (see
/// [`WebViewOptions::scripts`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserScript {
    /// The JavaScript.
    pub source: String,
    /// Only in the top frame, not in iframes.
    pub main_frame_only: bool,
}

/// A request for a URL in a scheme the app serves (see [`SchemeHandler`]).
/// Any page in the view can make one (an `<img>`, an `<iframe>`, a
/// `fetch` on a site the view shows): a handler whose requests do more than
/// read checks who asked (`main_document_url`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemeRequest {
    /// The [`WebView::id`] of the page asking.
    pub web_view: u64,
    /// The URL asked for.
    pub url: String,
    /// The top page the request is made for (empty when the platform does
    /// not say): a site that embeds or fetches the scheme's URLs shows here.
    pub main_document_url: String,
    /// The HTTP method (`GET`, `POST`).
    pub method: String,
    /// The request's headers.
    pub headers: Vec<(String, String)>,
    /// The request's body.
    pub body: Vec<u8>,
}

/// The answer to a [`SchemeRequest`], as an HTTP response.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemeResponse {
    /// The HTTP status (200 for OK).
    pub status: u16,
    /// The response's headers (`Content-Type`).
    pub headers: Vec<(String, String)>,
    /// The response's body.
    pub body: Vec<u8>,
}

/// Answers one [`SchemeRequest`], from any thread, once. Dropping it
/// unanswered fails the request; an answer after the page gave up on the
/// request (it navigated away, or closed) is dropped.
pub struct SchemeResponder(Option<Box<dyn FnOnce(Option<SchemeResponse>) + Send>>);
impl SchemeResponder {
    /// `deliver` gets the response, or `None` if the responder was dropped.
    pub fn new(deliver: impl FnOnce(Option<SchemeResponse>) + Send + 'static) -> Self {
        Self(Some(Box::new(deliver)))
    }
    /// Answer the request with `response`.
    pub fn respond(mut self, response: SchemeResponse) {
        if let Some(deliver) = self.0.take() {
            deliver(Some(response));
        }
    }
}
impl Drop for SchemeResponder {
    fn drop(&mut self) {
        if let Some(deliver) = self.0.take() {
            deliver(None);
        }
    }
}
impl std::fmt::Debug for SchemeResponder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SchemeResponder")
    }
}

/// Serves a URL scheme of the app's own (`myapp://…`) to pages: `serve` is
/// called on the UI thread for each request and may answer later, from
/// another thread, through its responder.
///
/// The scheme is lowercase letters, digits, `+`, `-` and `.`, starting with
/// a letter, and not one the platform serves itself (`http`, `https`,
/// `file`, `data`, `blob`, `about`…): a web view leaves out a scheme it
/// cannot register, with a warning, rather than fail. `serve` lives as long
/// as the page: hold the page in it through a [`WeakWebView`], or the two
/// keep each other alive.
#[derive(Clone)]
pub struct SchemeHandler {
    /// The scheme served, without `:` (`myapp`).
    pub scheme: String,
    /// Called for each request in the scheme.
    pub serve: Rc<dyn Fn(SchemeRequest, SchemeResponder)>,
}
impl SchemeHandler {
    /// Serve `scheme` with `serve`.
    pub fn new(
        scheme: impl Into<String>,
        serve: impl Fn(SchemeRequest, SchemeResponder) + 'static,
    ) -> Self {
        Self {
            scheme: scheme.into(),
            serve: Rc::new(serve),
        }
    }
}
impl std::fmt::Debug for SchemeHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemeHandler")
            .field("scheme", &self.scheme)
            .finish()
    }
}
impl PartialEq for SchemeHandler {
    fn eq(&self, other: &Self) -> bool {
        self.scheme == other.scheme && Rc::ptr_eq(&self.serve, &other.serve)
    }
}

/// A navigation about to start, for [`WebView::set_navigation_policy`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationRequest {
    /// Where it goes.
    pub url: String,
    /// The top frame navigates (not an iframe).
    pub main_frame: bool,
    /// What started it: a link, a form, history, a reload, or anything else
    /// (a script, a redirect, the app).
    pub kind: NavigationKind,
    /// The page of the frame that started it (empty for the app's own).
    pub source_url: String,
}
/// What started a navigation ([`NavigationRequest::kind`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationKind {
    /// A link was followed.
    Link,
    /// A form was submitted.
    FormSubmission,
    /// Back or forward through history.
    BackForward,
    /// The page reloaded.
    Reload,
    /// A form was submitted again (a reload or history step after a POST).
    FormResubmission,
    /// Anything else: a script, a redirect, the app.
    Other,
}
/// Whether `scheme` can be one an app serves (see [`SchemeHandler`]): its
/// syntax, and not a scheme every platform's web view serves itself.
pub fn is_app_scheme(scheme: &str) -> bool {
    const BUILT_IN: &[&str] = &[
        "http",
        "https",
        "file",
        "data",
        "blob",
        "about",
        "javascript",
        "ws",
        "wss",
        "ftp",
    ];
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
        && !BUILT_IN.contains(&scheme)
}

/// How a web view is made: its background, profile, user agent, page
/// messages and windows, scripts and schemes.
#[derive(Clone, Debug, PartialEq)]
pub struct WebViewOptions {
    /// Draw no page background of the view's own: the node's (and what lies
    /// under it) shows where the page is transparent.
    pub transparent: bool,
    /// Where it keeps cookies and site data.
    pub profile: WebProfile,
    /// Allow Web Inspector (Safari's Develop menu, [`WebView::open_devtools`]).
    /// Defaults on in debug builds.
    pub inspectable: bool,
    /// The user agent string pages see; `None` keeps the platform's.
    pub user_agent: Option<String>,
    /// Pages may post to the app: `window.webkit.messageHandlers.zgui
    /// .postMessage(value)` arrives as [`WebViewEvent::Message`] (JSON).
    pub messages: bool,
    /// Pages may open windows (`window.open`, target=_blank): each request
    /// arrives as [`WebViewEvent::NewWindowRequested`] and opens nothing.
    pub new_windows: bool,
    /// Scripts injected at document start, in order, before the page's own.
    pub scripts: Vec<UserScript>,
    /// Schemes the app serves itself. On macOS each page served this way is
    /// its own origin and, like `https`, a secure context.
    pub schemes: Vec<SchemeHandler>,
}
impl Default for WebViewOptions {
    fn default() -> Self {
        Self {
            transparent: false,
            profile: WebProfile::Default,
            inspectable: cfg!(debug_assertions),
            user_agent: None,
            messages: true,
            new_windows: true,
            scripts: Vec::new(),
            schemes: Vec::new(),
        }
    }
}

/// A page in a web view's history.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryItem {
    /// Its URL.
    pub url: String,
    /// Its title.
    pub title: String,
}

/// A page's navigation state, kept by the controller.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WebViewState {
    /// The page's URL.
    pub url: String,
    /// The page's title.
    pub title: String,
    /// Whether a page is loading.
    pub loading: bool,
    /// Estimated load progress, 0 to 1.
    pub progress: f64,
    /// Whether there is a page to go back to.
    pub can_go_back: bool,
    /// Whether there is a page to go forward to.
    pub can_go_forward: bool,
    /// The pages back and forward from this one, nearest first: what a
    /// browser's held back and forward buttons list.
    pub back_list: Vec<HistoryItem>,
    /// The pages forward from this one, nearest first.
    pub forward_list: Vec<HistoryItem>,
    /// The page has keyboard focus.
    pub focused: bool,
    /// The last navigation's failure, until the next starts.
    pub error: Option<String>,
}

/// What happened in a page, as the platform reports it (see
/// [`WebView::on_event`]).
#[derive(Clone, Debug, PartialEq)]
pub enum WebViewEvent {
    /// A navigation started.
    NavigationStarted {
        /// Where to.
        url: String,
    },
    /// The new page's content started arriving: its URL is final.
    NavigationCommitted {
        /// The final URL.
        url: String,
    },
    /// A navigation finished loading.
    NavigationFinished {
        /// The page's URL.
        url: String,
    },
    /// A navigation failed.
    NavigationFailed {
        /// Where it was going.
        url: String,
        /// Why it failed.
        error: String,
    },
    /// The page's title changed.
    TitleChanged {
        /// The new title.
        title: String,
    },
    /// The page's URL changed (a redirect, or a same-page navigation).
    UrlChanged {
        /// The new URL.
        url: String,
    },
    /// A page message (see [`WebViewOptions::messages`]), as JSON, with
    /// who sent it. Every page in the view, and every frame in it (an ad's
    /// iframe), can post one: check `origin` (and `main_frame`) before
    /// acting on a message that does anything.
    Message {
        /// The message, as JSON.
        body: String,
        /// The sending frame's origin: `https://example.com`,
        /// `myapp://settings` (a port only when it is not the scheme's own).
        origin: String,
        /// Sent by the top frame, not an iframe.
        main_frame: bool,
    },
    /// A page asked to open a window (see [`WebViewOptions::new_windows`]);
    /// nothing opened.
    NewWindowRequested {
        /// What it asked to open.
        url: String,
    },
    /// The page's process ended (crashed or was killed); reload to recover.
    ProcessTerminated,
}

/// A JavaScript evaluation's result: the value as JSON, or the error.
pub type ScriptResult = Result<String, String>;

/// What a controller asks of its platform view.
pub enum WebViewCommand {
    /// Load a URL.
    LoadUrl(String),
    /// Show HTML (see [`WebView::load_html`]).
    LoadHtml {
        /// The document.
        html: String,
        /// What its relative URLs and origin resolve against.
        base_url: Option<String>,
    },
    /// Reload the page.
    Reload,
    /// Stop loading.
    Stop,
    /// Go back a page.
    GoBack,
    /// Go forward a page.
    GoForward,
    /// Go `n` pages through history: negative back, positive forward.
    GoToHistory(i32),
    /// Evaluate JavaScript in the page.
    Evaluate {
        /// The script.
        script: String,
        /// What gets its result, if anything.
        reply: Option<Box<dyn FnOnce(ScriptResult)>>,
    },
    /// Give the page keyboard focus, or take it back (`false`).
    Focus(bool),
    /// Open Web Inspector.
    OpenDevtools,
    /// Page zoom (1 is 100%).
    SetZoom(f64),
}
impl std::fmt::Debug for WebViewCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadUrl(url) => f.debug_tuple("LoadUrl").field(url).finish(),
            Self::LoadHtml { base_url, .. } => f
                .debug_struct("LoadHtml")
                .field("base_url", base_url)
                .finish(),
            Self::Reload => f.write_str("Reload"),
            Self::Stop => f.write_str("Stop"),
            Self::GoBack => f.write_str("GoBack"),
            Self::GoForward => f.write_str("GoForward"),
            Self::GoToHistory(n) => f.debug_tuple("GoToHistory").field(n).finish(),
            Self::Evaluate { script, .. } => {
                f.debug_struct("Evaluate").field("script", script).finish()
            }
            Self::Focus(focus) => f.debug_tuple("Focus").field(focus).finish(),
            Self::OpenDevtools => f.write_str("OpenDevtools"),
            Self::SetZoom(zoom) => f.debug_tuple("SetZoom").field(zoom).finish(),
        }
    }
}

/// A platform's page (implemented by the desktop host).
pub trait WebViewBackend {
    /// Carry out `command`.
    fn run(&self, command: WebViewCommand);
    /// The backend as `Any`, for the host to downcast.
    fn as_any(&self) -> &dyn Any;
}

type Listener = Rc<RefCell<dyn FnMut(&WebViewEvent)>>;
type Policy = Rc<dyn Fn(&NavigationRequest) -> bool>;
struct Inner {
    id: u64,
    options: WebViewOptions,
    state: Signal<WebViewState>,
    /// The pointer over the page, in points from its top left.
    pointer: Signal<Option<(f32, f32)>>,
    attached: Signal<bool>,
    backend: RefCell<Option<Rc<dyn WebViewBackend>>>,
    /// Closed (`close`): no page again, commands go nowhere.
    closed: Cell<bool>,
    queued: RefCell<Vec<WebViewCommand>>,
    listeners: RefCell<Vec<(u64, Listener)>>,
    next_listener: Cell<u64>,
    /// The last `set_src`, so an unchanged reactive source navigates once.
    src: RefCell<Option<String>>,
    policy: RefCell<Option<Policy>>,
}
/// A web page's controller; clones share it.
#[derive(Clone)]
pub struct WebView(Rc<Inner>);
/// A weak controller reference, for platform callbacks.
#[derive(Clone)]
pub struct WeakWebView(Weak<Inner>);
impl WeakWebView {
    /// The controller, if it still exists.
    pub fn upgrade(&self) -> Option<WebView> {
        self.0.upgrade().map(WebView)
    }
}
/// Keeps an event subscription; dropping it unsubscribes.
pub struct WebViewSubscription {
    web: Weak<Inner>,
    id: u64,
}
impl Drop for WebViewSubscription {
    fn drop(&mut self) {
        if let Some(inner) = self.web.upgrade()
            && let Ok(mut listeners) = inner.listeners.try_borrow_mut()
        {
            listeners.retain(|(id, _)| *id != self.id);
        }
    }
}
thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}
impl PartialEq for WebView {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl std::fmt::Debug for WebView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebView").field("id", &self.0.id).finish()
    }
}
impl WebView {
    /// A controller for a page made with `options`. Commands queue until a
    /// platform page attaches (`attach`).
    pub fn new(runtime: &Runtime, options: WebViewOptions) -> Self {
        let id = NEXT_ID.with(|next| {
            let id = next.get() + 1;
            next.set(id);
            id
        });
        Self(Rc::new(Inner {
            id,
            options,
            state: runtime.signal(WebViewState::default()),
            pointer: runtime.signal(None),
            attached: runtime.signal(false),
            backend: RefCell::new(None),
            closed: Cell::new(false),
            queued: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
            next_listener: Cell::new(0),
            src: RefCell::new(None),
            policy: RefCell::new(None),
        }))
    }
    /// Unique per controller in this thread.
    pub fn id(&self) -> u64 {
        self.0.id
    }
    /// The options it was made with.
    pub fn options(&self) -> &WebViewOptions {
        &self.0.options
    }
    /// A reference that doesn't keep the controller alive.
    pub fn downgrade(&self) -> WeakWebView {
        WeakWebView(Rc::downgrade(&self.0))
    }
    /// The navigation state, reactive.
    pub fn state(&self) -> Signal<WebViewState> {
        self.0.state.clone()
    }
    /// The page's URL (reactive, as `state` is).
    pub fn url(&self) -> String {
        self.0.state.with(|state| state.url.clone())
    }
    /// The page's title (reactive).
    pub fn title(&self) -> String {
        self.0.state.with(|state| state.title.clone())
    }
    /// Whether a page is loading (reactive).
    pub fn loading(&self) -> bool {
        self.0.state.with(|state| state.loading)
    }
    /// Estimated load progress, 0 to 1 (reactive).
    pub fn progress(&self) -> f64 {
        self.0.state.with(|state| state.progress)
    }
    /// Whether there is a page to go back to (reactive).
    pub fn can_go_back(&self) -> bool {
        self.0.state.with(|state| state.can_go_back)
    }
    /// Whether there is a page to go forward to (reactive).
    pub fn can_go_forward(&self) -> bool {
        self.0.state.with(|state| state.can_go_forward)
    }
    /// Whether the page has keyboard focus (reactive).
    pub fn focused(&self) -> bool {
        self.0.state.with(|state| state.focused)
    }
    /// Where the pointer is over the page, in points from its top left;
    /// `None` outside it (reactive, its own signal: following it does not
    /// rerun what reads the rest of the state). Over the page zgui hears no
    /// pointer moves (its nodes had their leave), so this is how to follow
    /// the pointer there, as `Terminal::pointer` is over a terminal.
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.0.pointer.get()
    }
    /// Report the pointer (the platform's report; call from a delivery).
    pub fn set_pointer(&self, pointer: Option<(f32, f32)>) {
        if self.0.pointer.with_untracked(|p| *p != pointer) {
            self.0.pointer.set(pointer);
        }
    }
    /// Whether a report would change anything: something follows the
    /// pointer, or it has one to clear.
    pub(crate) fn wants_pointer(&self) -> bool {
        self.0.pointer.is_observed() || self.0.pointer.with_untracked(Option::is_some)
    }
    /// Whether a platform page exists for it (reactive).
    pub fn is_attached(&self) -> bool {
        self.0.attached.get()
    }
    fn run(&self, command: WebViewCommand) {
        if self.0.closed.get() {
            return;
        }
        let backend = self.0.backend.borrow().clone();
        match backend {
            Some(backend) => backend.run(command),
            None => self.0.queued.borrow_mut().push(command),
        }
    }
    /// End the page now: its platform page goes (with the process and the
    /// timers behind it), however many handles to this controller remain,
    /// and none is made for it again. Handles still kept (a script's, until
    /// it collects them) do nothing.
    pub fn close(&self) {
        if self.0.closed.replace(true) {
            return;
        }
        self.0.queued.borrow_mut().clear();
        drop(self.detach());
    }
    /// Whether it was closed (`close`).
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    /// Navigate to `url`. The state shows it at once; the page confirms it.
    pub fn load_url(&self, url: impl Into<String>) {
        let url = url.into();
        *self.0.src.borrow_mut() = Some(url.clone());
        // Shown at once (placeholders, address bars), confirmed by the page.
        self.0.state.update(|state| {
            state.url = url.clone();
            state.error = None;
        });
        self.run(WebViewCommand::LoadUrl(url));
    }
    /// Navigate to `url` unless it is what the last `set_src` (or
    /// `load_url`) asked for: a reactive source's equal writes do nothing.
    pub fn set_src(&self, url: &str) {
        if self.0.src.borrow().as_deref() == Some(url) {
            return;
        }
        self.load_url(url);
    }
    /// Show `html`; relative URLs and the page's origin resolve against
    /// `base_url` (none: `about:blank`, with no access to other origins).
    pub fn load_html(&self, html: impl Into<String>, base_url: Option<&str>) {
        *self.0.src.borrow_mut() = None;
        self.run(WebViewCommand::LoadHtml {
            html: html.into(),
            base_url: base_url.map(Into::into),
        });
    }
    /// Reload the page.
    pub fn reload(&self) {
        self.run(WebViewCommand::Reload);
    }
    /// Stop loading.
    pub fn stop(&self) {
        self.run(WebViewCommand::Stop);
    }
    /// Go back a page.
    pub fn go_back(&self) {
        self.run(WebViewCommand::GoBack);
    }
    /// Go forward a page.
    pub fn go_forward(&self) {
        self.run(WebViewCommand::GoForward);
    }
    /// Go `n` pages through history (`-1` is back, `1` forward); see
    /// [`WebViewState::back_list`].
    pub fn go_to_history(&self, n: i32) {
        self.run(WebViewCommand::GoToHistory(n));
    }
    /// The pages back from this one, nearest first (reactive).
    pub fn back_list(&self) -> Vec<HistoryItem> {
        self.0.state.with(|state| state.back_list.clone())
    }
    /// The pages forward from this one, nearest first (reactive).
    pub fn forward_list(&self) -> Vec<HistoryItem> {
        self.0.state.with(|state| state.forward_list.clone())
    }
    /// Evaluate `script` in the page; `reply` gets its result as JSON (an
    /// `undefined` result is `null`), on the UI thread at a safe point.
    pub fn evaluate_js(
        &self,
        script: impl Into<String>,
        reply: impl FnOnce(ScriptResult) + 'static,
    ) {
        self.run(WebViewCommand::Evaluate {
            script: script.into(),
            reply: Some(Box::new(reply)),
        });
    }
    /// Evaluate `script`, ignoring its result.
    pub fn run_js(&self, script: impl Into<String>) {
        self.run(WebViewCommand::Evaluate {
            script: script.into(),
            reply: None,
        });
    }
    /// Give the page keyboard focus.
    pub fn focus(&self) {
        self.run(WebViewCommand::Focus(true));
    }
    /// Take keyboard focus back from the page (to the window's zgui content).
    pub fn blur(&self) {
        self.run(WebViewCommand::Focus(false));
    }
    /// Open Web Inspector, when [`WebViewOptions::inspectable`].
    pub fn open_devtools(&self) {
        self.run(WebViewCommand::OpenDevtools);
    }
    /// Zoom the page (1 is 100%).
    pub fn set_zoom(&self, zoom: f64) {
        self.run(WebViewCommand::SetZoom(zoom));
    }
    /// Decide each navigation before it starts: `false` cancels it. Called on
    /// the UI thread from inside the platform's callback, so it must answer
    /// at once and not call back into this controller.
    pub fn set_navigation_policy(&self, policy: impl Fn(&NavigationRequest) -> bool + 'static) {
        *self.0.policy.borrow_mut() = Some(Rc::new(policy));
    }
    /// Whether the navigation policy lets `request` go ahead (the platform's
    /// question; with no policy, everything may).
    pub fn allows(&self, request: &NavigationRequest) -> bool {
        let policy = self.0.policy.borrow().clone();
        policy.is_none_or(|policy| policy(request))
    }
    /// Call `listener` with each platform event, until the subscription drops.
    pub fn on_event(&self, listener: impl FnMut(&WebViewEvent) + 'static) -> WebViewSubscription {
        let id = self.0.next_listener.get() + 1;
        self.0.next_listener.set(id);
        self.0
            .listeners
            .borrow_mut()
            .push((id, Rc::new(RefCell::new(listener))));
        WebViewSubscription {
            web: Rc::downgrade(&self.0),
            id,
        }
    }

    // The platform side.

    /// Attach the platform page; queued commands run now, in order.
    pub fn attach(&self, backend: Rc<dyn WebViewBackend>) {
        if self.0.closed.get() {
            return;
        }
        *self.0.backend.borrow_mut() = Some(backend.clone());
        let queued = std::mem::take(&mut *self.0.queued.borrow_mut());
        for command in queued {
            backend.run(command);
        }
        self.0.attached.set(true);
    }
    /// Drop the platform page (it goes once nothing else holds it).
    pub fn detach(&self) -> Option<Rc<dyn WebViewBackend>> {
        let backend = self.0.backend.borrow_mut().take();
        self.0.attached.set(false);
        backend
    }
    /// The platform page, while one is attached.
    pub fn backend(&self) -> Option<Rc<dyn WebViewBackend>> {
        self.0.backend.borrow().clone()
    }
    /// Update the state (the platform's report). Call from a delivery
    /// ([`crate::native_view::post`]), not from inside a platform callback.
    pub fn update_state(&self, change: impl FnOnce(&mut WebViewState)) {
        self.0.state.update(change);
    }
    /// Deliver `event` to subscribers. Call from a delivery, as above.
    pub fn emit(&self, event: WebViewEvent) {
        let listeners: Vec<Listener> = self
            .0
            .listeners
            .borrow()
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        for listener in listeners {
            if let Ok(mut listener) = listener.try_borrow_mut() {
                listener(&event);
            }
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        // Pages go with their controller; the backend's own drop removes the view.
        self.backend.get_mut().take();
    }
}

/// Show `web`'s page in a node laid out like any other: give it a size (or
/// let it grow), round it, clip it, move and fade it.
pub fn web_view(web: &WebView) -> View {
    native(NativeContent::Web(web.clone()), Some(web.clone()), None)
}
/// [`web_view`] navigating to `src` whenever its tracked reads change it.
pub fn web_view_src(web: &WebView, src: impl FnMut() -> String + 'static) -> View {
    native(
        NativeContent::Web(web.clone()),
        Some(web.clone()),
        Some(Box::new(src)),
    )
}
/// A node showing `content` (see [`crate::native_view`]). On macOS the
/// desktop host mounts a `Retained<NSView>` given as
/// `NativeContent::Platform(Rc::new(view))`.
pub fn native_view(content: NativeContent) -> View {
    let web = match &content {
        NativeContent::Web(web) => Some(web.clone()),
        _ => None,
    };
    native(content, web, None)
}
/// What takes keyboard focus with a native node.
enum Focusable {
    Web(WebView),
    Terminal(crate::terminal::Terminal),
}
impl Focusable {
    fn of(content: &NativeContent) -> Option<Self> {
        match content {
            NativeContent::Web(web) => Some(Self::Web(web.clone())),
            NativeContent::Terminal(term) => Some(Self::Terminal(term.clone())),
            NativeContent::Platform(_) => None,
        }
    }
    fn focused(&self) -> bool {
        match self {
            Self::Web(web) => web.focused(),
            Self::Terminal(term) => term.focused(),
        }
    }
    fn focus(&self, on: bool) {
        match (self, on) {
            (Self::Web(web), true) => web.focus(),
            (Self::Web(web), false) => web.blur(),
            (Self::Terminal(term), true) => term.focus(),
            (Self::Terminal(term), false) => term.blur(),
        }
    }
}
fn native(
    content: NativeContent,
    web: Option<WebView>,
    src: Option<Box<dyn FnMut() -> String>>,
) -> View {
    component(move |cx| {
        let natives = cx.try_service::<NativeViews>();
        let supported = natives.as_ref().is_some_and(|natives| natives.supported());
        if let (Some(web), Some(mut src)) = (web.clone(), src) {
            let effect = cx.runtime().effect(move || {
                let url = src();
                web.set_src(&url);
            });
            cx.retain(effect);
        }
        let focus = Focusable::of(&content);
        let mut root = div().focusable(focus.is_some()).on_event(move |event| {
            // The view's keyboard focus follows the node's: Tab into it,
            // or focus moved away from it in zgui.
            let Some(focus) = &focus else { return };
            match event.event {
                InputEvent::Focus if !focus.focused() => focus.focus(true),
                InputEvent::Blur if focus.focused() => focus.focus(false),
                _ => {}
            }
        });
        if !supported {
            let label = content.clone();
            root = root.child(placeholder(label));
        }
        let registered = content.clone();
        root.on_mount(move |handle| {
            if let Some(natives) = natives {
                if supported
                    && let Some(scene) = handle.scene()
                    && let Ok(mut scene) = scene.try_borrow_mut()
                {
                    scene.set_platform_painted(handle.node(), true);
                }
                handle.retain(natives.register(handle.node(), registered));
            }
        })
    })
}
/// What a native node shows without a platform backend.
fn placeholder(content: NativeContent) -> View {
    let label = match &content {
        NativeContent::Web(web) => {
            let web = web.clone();
            crate::compose::text_signal(move || {
                let url = web.url();
                if url.is_empty() {
                    "Web view".into()
                } else {
                    format!("Web view · {url}")
                }
            })
        }
        NativeContent::Terminal(term) => {
            let term = term.clone();
            crate::compose::text_signal(move || {
                let title = term.title();
                if title.is_empty() {
                    "Terminal".into()
                } else {
                    format!("Terminal · {title}")
                }
            })
        }
        NativeContent::Platform(_) => text(content.label()),
    };
    div()
        .absolute()
        .left(0.)
        .top(0.)
        .w_full()
        .h_full()
        .bg(crate::style::rgba(0x8080_8020))
        .border(1.)
        .border_color(crate::style::rgba(0x8080_8060))
        .items_center()
        .justify_center()
        .overflow_hidden()
        .child(
            label
                .text_size(12.)
                .text_color(crate::style::rgba(0xa0a0_a0ff)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_scheme_is_its_own_and_well_formed() {
        assert!(is_app_scheme("myapp"));
        assert!(is_app_scheme("zgui-ui.v2+x"));
        for taken in ["https", "http", "file", "blob", "data", "about"] {
            assert!(!is_app_scheme(taken), "{taken} is the web view's");
        }
        for malformed in ["", "MyApp", "2app", "my app", "my:app", "app/x"] {
            assert!(!is_app_scheme(malformed), "{malformed:?}");
        }
    }

    #[test]
    fn a_policy_decides_and_none_allows() {
        let runtime = crate::reactive::Runtime::new();
        let web = WebView::new(&runtime, WebViewOptions::default());
        let request = |url: &str| NavigationRequest {
            url: url.into(),
            main_frame: true,
            kind: NavigationKind::Link,
            source_url: String::new(),
        };
        assert!(web.allows(&request("https://a.test/")));
        web.set_navigation_policy(|r| !r.url.starts_with("file:"));
        assert!(web.allows(&request("https://a.test/")));
        assert!(!web.allows(&request("file:///etc/passwd")));
    }
}
