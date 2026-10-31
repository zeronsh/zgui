//! Testing a scripted app without a window: [`TestApp`] mounts a host
//! headless, with the task executor and frame clock a window has, and drives
//! it as a person would (clicks, typing, keys) while a test reads what it
//! shows.
//!
//! ```ignore
//! use zgx_script::{ScriptHost, testing::TestApp};
//!
//! #[test]
//! fn incrementing_counts() {
//!     let mut app = TestApp::new(|runtime| ScriptHost::new(runtime, "ui/app.luau"));
//!     app.click("inc");
//!     assert_eq!(app.text("label"), "Count: 1");
//!     // Async host calls and fetches settle in their own time:
//!     assert!(app.wait_until(|app| app.text("user") == "Ada"));
//!     // A hot path runs no script:
//!     let calls = app.calls();
//!     app.frames(30);
//!     assert_eq!(app.calls(), calls);
//! }
//! ```
//!
//! Host actions, the fetcher (`ScriptHost::fetcher`) and shared state are
//! set up on the host before it is returned; [`TestApp::wrapped`] provides
//! services as a window does. [`TestApp::with_desktop`] gives the app a
//! clipboard and file dialogs ([`FakeDesktop`]) that answer as the test
//! says a person would: open and save run as they do in a window.
use crate::ScriptHost;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    compose::{TaskRunner, View, ViewHandle, provide},
    frame::{Frame, FrameClock},
    input::{InputEvent, Key, Modifiers},
    reactive::Runtime,
    scene::{NodeId, NodeKind, Rect},
    task::LocalExecutor,
    widgets::Ui,
};

/// A desktop's clipboard and file dialogs, for [`TestApp::with_desktop`]:
/// each dialog answers with what the test queued for it ([`pick`],
/// [`save_to`]), or as a cancelled one does when nothing is queued, and
/// keeps what it was asked ([`asked`]).
///
/// [`pick`]: FakeDesktop::pick
/// [`save_to`]: FakeDesktop::save_to
/// [`asked`]: FakeDesktop::asked
#[derive(Default)]
pub struct FakeDesktop {
    clipboard: RefCell<String>,
    image: RefCell<Option<std::sync::Arc<zgx::image::ImageData>>>,
    files: RefCell<Vec<std::path::PathBuf>>,
    opens: RefCell<std::collections::VecDeque<Vec<std::path::PathBuf>>>,
    saves: RefCell<std::collections::VecDeque<std::path::PathBuf>>,
    asked: RefCell<Vec<zgx::platform::FilePicker>>,
    notifications: RefCell<Vec<zgx::platform::Notification>>,
    badge: RefCell<Option<String>>,
    deny: std::cell::Cell<bool>,
    clicks: Clicks,
}
/// Notifications clicked, not yet heard, and who waits for the next.
type Clicks = std::sync::Arc<
    std::sync::Mutex<(std::collections::VecDeque<String>, Option<std::task::Waker>)>,
>;
// A fake desktop is Send and unwind-safe, as published releases' was: losing
// either breaks a test that moves or catches across one.
const _: () = {
    const fn holds<T: Send + std::panic::UnwindSafe>() {}
    holds::<FakeDesktop>();
};
impl FakeDesktop {
    /// The notifications shown so far, in order.
    pub fn notifications(&self) -> Vec<zgx::platform::Notification> {
        self.notifications.borrow().clone()
    }
    /// The app icon's badge, if it has one.
    pub fn badge(&self) -> Option<String> {
        self.badge.borrow().clone()
    }
    /// Refuse notifications from now on, as a person who turned them off.
    pub fn deny_notifications(&self) {
        self.deny.set(true);
    }
    /// Click the notification tagged `tag`, as the person would.
    pub fn click_notification(&self, tag: &str) {
        let waker = {
            let mut clicks = self.clicks.lock().unwrap();
            clicks.0.push_back(tag.to_string());
            clicks.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    /// The next open dialog picks `files`.
    pub fn pick(&self, files: impl IntoIterator<Item = impl Into<std::path::PathBuf>>) {
        let files = files.into_iter().map(Into::into).collect();
        self.opens.borrow_mut().push_back(files);
    }
    /// The next save dialog saves to `path`.
    pub fn save_to(&self, path: impl Into<std::path::PathBuf>) {
        self.saves.borrow_mut().push_back(path.into());
    }
    /// What the dialogs shown so far were asked to show, in order.
    pub fn asked(&self) -> Vec<zgx::platform::FilePicker> {
        self.asked.borrow().clone()
    }
    /// The clipboard's text.
    pub fn clipboard(&self) -> String {
        self.clipboard.borrow().clone()
    }
    /// Put `text` on the clipboard, as another app copies it.
    pub fn copy(&self, text: impl Into<String>) {
        *self.clipboard.borrow_mut() = text.into();
    }
    /// The clipboard's image: its width, height and RGBA pixels.
    pub fn clipboard_image(&self) -> Option<(u32, u32, Vec<u8>)> {
        let image = self.image.borrow();
        let image = image.as_ref()?;
        Some((image.width(), image.height(), image.pixels().to_vec()))
    }
    /// Put an image (`width` by `height` RGBA `pixels`) on the clipboard, as
    /// another app copies one.
    pub fn copy_image(&self, width: u32, height: u32, pixels: Vec<u8>) {
        let image = zgx::image::ImageData::new(width, height, pixels).expect("a valid image");
        *self.image.borrow_mut() = Some(std::sync::Arc::new(image));
    }
    /// Put `files` on the clipboard, as a file manager copies them.
    pub fn copy_files(&self, files: Vec<std::path::PathBuf>) {
        *self.files.borrow_mut() = files;
    }
}
impl zgx::platform::Platform for FakeDesktop {
    fn clipboard_files(&self) -> Vec<std::path::PathBuf> {
        self.files.borrow().clone()
    }
    fn clipboard_text(&self) -> Option<String> {
        Some(self.clipboard.borrow().clone())
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        *self.clipboard.borrow_mut() = text;
        true
    }
    fn clipboard_image(&self) -> zgx::platform::Pending<Option<zgx::image::ImageData>> {
        // A copy sharing the pixels.
        let image = (self.image.borrow().as_ref())
            .map(|image| image.transformed(zgx::affine::Affine::IDENTITY));
        Box::pin(async move { image })
    }
    fn set_clipboard_image(
        &self,
        image: std::sync::Arc<zgx::image::ImageData>,
    ) -> zgx::platform::Pending<bool> {
        *self.image.borrow_mut() = Some(image);
        Box::pin(async { true })
    }
    fn open_files(
        &self,
        picker: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Vec<std::path::PathBuf>, String>> {
        self.asked.borrow_mut().push(picker);
        let files = self.opens.borrow_mut().pop_front().unwrap_or_default();
        Box::pin(async move { Ok(files) })
    }
    fn save_file(
        &self,
        picker: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Option<std::path::PathBuf>, String>> {
        self.asked.borrow_mut().push(picker);
        let path = self.saves.borrow_mut().pop_front();
        Box::pin(async move { Ok(path) })
    }
    fn notify(
        &self,
        notification: zgx::platform::Notification,
    ) -> zgx::platform::Pending<Result<(), zgx::platform::NotifyError>> {
        if self.deny.get() {
            return Box::pin(async {
                Err(zgx::platform::NotifyError::Denied(
                    "notifications are off for this app".into(),
                ))
            });
        }
        self.notifications.borrow_mut().push(notification);
        Box::pin(async { Ok(()) })
    }
    fn set_badge(&self, badge: Option<&str>) -> bool {
        *self.badge.borrow_mut() = badge.map(str::to_string);
        true
    }
    fn next_notification_click(&self) -> zgx::platform::Pending<Option<String>> {
        let clicks = self.clicks.clone();
        Box::pin(std::future::poll_fn(move |cx| {
            let mut clicks = clicks.lock().unwrap();
            match clicks.0.pop_front() {
                Some(tag) => std::task::Poll::Ready(Some(tag)),
                None => {
                    clicks.1 = Some(cx.waker().clone());
                    std::task::Poll::Pending
                }
            }
        }))
    }
}

/// A target that is an id alone, as tests have always named nodes.
fn is_plain_id(target: &str) -> bool {
    !target.is_empty()
        && target
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        && !target.starts_with("role:")
}

/// A scripted app mounted headless, 400 by 300 points.
pub struct TestApp {
    /// Its document.
    pub ui: Ui,
    /// The script host running it.
    pub host: ScriptHost,
    /// The mounted script view.
    pub handle: ViewHandle,
    /// The executor its tasks run on; the test advances it.
    pub executor: Rc<RefCell<LocalExecutor>>,
    /// The frame clock its animations wait on; the test delivers frames.
    pub clock: FrameClock,
    /// Display frames delivered so far.
    pub frame: Cell<u64>,
    /// When it started, the time frames count from.
    pub start: Instant,
    /// Native views (web pages, terminals) the script placed: headless, they
    /// show placeholders.
    pub natives: zgx::native_view::NativeViews,
    /// Its clipboard and file dialogs, made with [`TestApp::with_desktop`].
    pub desktop: Option<Rc<FakeDesktop>>,
}

impl TestApp {
    /// The host `host` makes, loaded and mounted. A script that fails to
    /// load panics, with its error.
    pub fn new(host: impl FnOnce(&Runtime) -> ScriptHost) -> Self {
        Self::wrapped(host, |view| view)
    }
    /// An inline script, as `app`.
    pub fn inline(code: &str) -> Self {
        Self::new(|runtime| ScriptHost::inline(runtime, "app", code))
    }
    /// With a clipboard and file dialogs ([`FakeDesktop`], in `desktop`):
    /// `ui.open_files`, `ui.save_file`, `ui.read_file` and `ui.write_file`
    /// work as in a window, the dialogs answering as the test queues.
    pub fn with_desktop(host: impl FnOnce(&Runtime) -> ScriptHost) -> Self {
        Self::try_with_desktop(host).unwrap_or_else(|error| panic!("{error}"))
    }
    /// [`TestApp::with_desktop`], or the script's error when it fails to load.
    pub fn try_with_desktop(host: impl FnOnce(&Runtime) -> ScriptHost) -> Result<Self, String> {
        let desktop = Rc::new(FakeDesktop::default());
        let service = zgx::platform::PlatformService(desktop.clone());
        let mut app = Self::try_wrapped(host, move |view| provide(service, view))?;
        app.desktop = Some(desktop);
        Ok(app)
    }
    /// With the window's layout inspector, as a desktop window provides it.
    pub fn inspected(
        host: impl FnOnce(&Runtime) -> ScriptHost,
        inspector: &zgx::inspect::Inspector,
    ) -> Self {
        let inspector = inspector.clone();
        Self::wrapped(host, move |view| provide(inspector, view))
    }
    /// With `wrap` around the host's view: services a window provides
    /// (`compose::provide`), such as a fake platform.
    pub fn wrapped(
        host: impl FnOnce(&Runtime) -> ScriptHost,
        wrap: impl FnOnce(View) -> View,
    ) -> Self {
        Self::try_wrapped(host, wrap).unwrap_or_else(|error| panic!("{error}"))
    }
    /// [`TestApp::new`], or the script's error when it fails to load.
    pub fn try_new(host: impl FnOnce(&Runtime) -> ScriptHost) -> Result<Self, String> {
        Self::try_wrapped(host, |view| view)
    }
    /// [`TestApp::wrapped`], or the script's error when it fails to load.
    pub fn try_wrapped(
        host: impl FnOnce(&Runtime) -> ScriptHost,
        wrap: impl FnOnce(View) -> View,
    ) -> Result<Self, String> {
        let mut ui = Ui::new(400., 300.);
        let host = host(&ui.runtime);
        host.reload().map_err(|error| error.to_string())?;
        let executor = Rc::new(RefCell::new(LocalExecutor::new()));
        let clock = FrameClock::new();
        let natives = zgx::native_view::NativeViews::new();
        let handle = ui.mount(provide(
            natives.clone(),
            provide(
                TaskRunner::from_executor(executor.clone()),
                provide(clock.clone(), wrap(host.view())),
            ),
        ));
        let app = Self {
            ui,
            host,
            handle,
            executor,
            clock,
            frame: Cell::new(0),
            start: Instant::now(),
            natives,
            desktop: None,
        };
        app.settle();
        // A window lays out before its first frame.
        app.ui.prepare_frame();
        app.settle();
        Ok(app)
    }

    /// Run what is ready (handlers' tasks, settled host calls) until
    /// nothing is.
    pub fn settle(&self) {
        while self.executor.borrow_mut().tick() > 0 {}
    }
    /// Deliver one 60 Hz display frame and lay out, as a window does.
    pub fn frame(&self) {
        let index = self.frame.get() + 1;
        self.frame.set(index);
        let interval = Duration::from_secs_f64(1. / 60.);
        self.clock.deliver(Frame {
            index,
            time: self.start + interval * index as u32,
            interval,
        });
        self.settle();
        self.ui.prepare_frame();
        self.settle();
        // The frame is done, as a renderer's present ends one.
        zgx::profile::frame();
    }
    /// `n` frames: an animation's worth.
    pub fn frames(&self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }
    /// Settle and lay out until `done` holds, or five seconds pass (host
    /// calls, fetches and timers run in real time); whether it held.
    pub fn wait_until(&mut self, mut done: impl FnMut(&mut Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.settle();
            self.ui.prepare_frame();
            if done(self) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The node `target` names, after layout: an id (`"send"`, the first
    /// mounted node with it), or a target as agents name one (`"\"Send\""`,
    /// `"role:button \"Send\""`, `"#list row 3"`, `"@12"`; see
    /// [`crate::driver`]). None if nothing, or more than one, matches.
    pub fn find(&mut self, target: &str) -> Option<NodeId> {
        self.ui.prepare_frame();
        if is_plain_id(target) {
            return self.handle.find(target);
        }
        crate::driver::find(&self.ui, &self.handle, target).ok()
    }
    /// [`TestApp::find`], or why it found none: nothing matched, or several
    /// did (each described).
    pub fn target(&mut self, target: &str) -> Result<NodeId, String> {
        self.ui.prepare_frame();
        if is_plain_id(target) {
            return self
                .handle
                .find(target)
                .ok_or_else(|| format!("no node `{target}`"));
        }
        crate::driver::find(&self.ui, &self.handle, target)
    }
    /// The interface as `zgx inspect tree` shows it: a node a line.
    pub fn tree(&mut self) -> String {
        self.ui.prepare_frame();
        let (lines, hidden) =
            crate::driver::tree(&self.ui, &self.handle, None, &Default::default());
        crate::driver::tree_text(&lines, hidden)
    }
    fn node(&mut self, target: &str) -> NodeId {
        self.target(target).unwrap_or_else(|e| panic!("{e}"))
    }
    /// Where node `id` is, in points.
    pub fn bounds(&mut self, id: &str) -> Rect {
        let node = self.node(id);
        self.ui.scene.borrow().bounds(node)
    }
    /// Where node `id` shows in the window: `bounds`, through whatever
    /// transforms and zooms it lies under, as a pointer reaches it.
    fn on_screen(&mut self, id: &str) -> Rect {
        let node = self.node(id);
        self.ui.scene.borrow().transformed_bounds(node)
    }
    /// The text of node `id`, or of the first text inside it.
    pub fn text(&mut self, id: &str) -> String {
        let node = self.node(id);
        self.text_in(node)
    }
    /// Row `n` (from 1) of the list `list` among those mounted, top to
    /// bottom: rows by where they are, whatever names they have (a keyed
    /// list's rows are named by their items).
    pub fn row(&mut self, list: &str, n: usize) -> NodeId {
        let list_node = self.node(list);
        let rows = crate::driver::rows(&self.ui.scene.borrow(), list_node);
        *rows
            .get(n.wrapping_sub(1))
            .unwrap_or_else(|| panic!("`{list}` has {} rows mounted, not {n}", rows.len()))
    }
    /// The text of row `n` of the list `list` (the first text in it).
    pub fn row_text(&mut self, list: &str, n: usize) -> String {
        let row = self.row(list, n);
        self.text_in(row)
    }
    /// Press and release the primary button at the middle of row `n` of the
    /// list `list`.
    pub fn click_row(&mut self, list: &str, n: usize) {
        let row = self.row(list, n);
        let bounds = self.ui.scene.borrow().bounds(row);
        self.click_at(bounds);
    }
    fn text_in(&self, node: NodeId) -> String {
        let scene = self.ui.scene.borrow();
        let mut stack = vec![node];
        let mut empty = false;
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n) {
                // An editor's selection lies in an empty text of its own.
                if !text.is_empty() {
                    return text.to_string();
                }
                empty = true;
            }
            stack.extend(scene.children(n).iter().rev());
        }
        if empty {
            return String::new();
        }
        panic!("no text in {node:?}: {:?}", scene.kind(node))
    }
    /// Every text the app shows, in tree order.
    pub fn texts(&mut self) -> Vec<String> {
        self.ui.prepare_frame();
        let scene = self.ui.scene.borrow();
        let mut out = Vec::new();
        let mut stack = vec![scene.root()];
        while let Some(n) = stack.pop() {
            match scene.kind(n) {
                NodeKind::Text { text, .. } => out.push(text.to_string()),
                // A paragraph of spans (`ui.rich`): its text, whole.
                NodeKind::RichText { text, .. } => out.push(text.text().to_owned()),
                _ => {}
            }
            stack.extend(scene.children(n).iter().rev());
        }
        out
    }
    /// Whether node `id` is there and shows: neither it nor what holds it
    /// is transparent (a closed modal's content is kept, unseen).
    pub fn shown(&mut self, id: &str) -> bool {
        let Some(node) = self.find(id) else {
            return false;
        };
        let scene = self.ui.scene.borrow();
        std::iter::once(node)
            .chain(scene.ancestors(node))
            .all(|n| scene.effects(n).opacity > 0.)
    }
    /// Node `id`'s opacity.
    pub fn opacity(&mut self, id: &str) -> f32 {
        let node = self.node(id);
        self.ui.scene.borrow().effects(node).opacity
    }

    /// Press and release the primary button at node `id`'s middle.
    pub fn click(&mut self, id: &str) {
        let b = self.on_screen(id);
        self.click_at(b);
    }
    fn click_at(&mut self, b: Rect) {
        crate::driver::click_at(&mut self.ui, b);
        self.settle();
    }
    /// Click field `id` (giving it focus) and type `text` into it.
    pub fn type_text(&mut self, id: &str, text: &str) {
        self.click(id);
        self.ui.dispatch(InputEvent::Text(text.into()));
        self.settle();
    }
    /// Click field `id` and replace its whole text with `text`, as a person
    /// selecting all and typing does.
    pub fn fill(&mut self, id: &str, text: &str) {
        self.click(id);
        self.keys("cmd+a");
        self.key(Key::Backspace);
        self.ui.dispatch(InputEvent::Text(text.into()));
        self.settle();
    }
    /// Move the pointer to node `id`'s middle, as a mouse does: what it
    /// leaves hears it go, and a tooltip waits its delay.
    pub fn hover(&mut self, id: &str) {
        let b = self.on_screen(id);
        crate::driver::hover_at(&mut self.ui, (b.x + b.width / 2., b.y + b.height / 2.));
        self.settle();
    }
    /// Scroll the viewport under node `id`'s middle by `dy` points (down is
    /// positive), as a wheel or a trackpad does.
    pub fn scroll(&mut self, id: &str, dy: f32) {
        let node = self.node(id);
        crate::driver::scroll(&mut self.ui, node, dy);
        self.settle();
    }
    /// Drag node `from` onto node `to` with the primary button: pressed
    /// near `from`'s top left, past the drag threshold, then released over
    /// `to`'s middle (its upper half: a reorderable row drops above it).
    pub fn drag(&mut self, from: &str, to: &str) {
        let (a, b) = (self.on_screen(from), self.on_screen(to));
        crate::driver::drag(&mut self.ui, a, b);
        self.settle();
    }
    /// Press and release `key`, with `modifiers`, wherever focus is.
    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        crate::driver::press(&mut self.ui, key, modifiers);
        self.settle();
    }
    /// Press keys written as agents write them: `"Enter"`, `"cmd+k"`,
    /// `"alt+ArrowDown"` (`cmd` is Command on macOS, Control elsewhere).
    pub fn keys(&mut self, keys: &str) {
        let (key, modifiers) = crate::driver::parse_keys(keys).unwrap_or_else(|e| panic!("{e}"));
        self.key_with(key, modifiers);
    }
    /// Press and release `key`.
    pub fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    /// The script error showing (a failed load, a handler's error), if any.
    pub fn error(&self) -> Option<String> {
        self.host.error().get()
    }
    /// Calls into scripts so far (`ScriptHost::script_calls`): hold a hot
    /// path to none.
    pub fn calls(&self) -> u64 {
        self.host.script_calls()
    }
}
