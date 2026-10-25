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
    input::{InputEvent, Key, Modifiers, PointerButton},
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
    opens: RefCell<std::collections::VecDeque<Vec<std::path::PathBuf>>>,
    saves: RefCell<std::collections::VecDeque<std::path::PathBuf>>,
    asked: RefCell<Vec<zgx::platform::FilePicker>>,
}
impl FakeDesktop {
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
}
impl zgx::platform::Platform for FakeDesktop {
    fn clipboard_text(&self) -> Option<String> {
        Some(self.clipboard.borrow().clone())
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        *self.clipboard.borrow_mut() = text;
        true
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
        let desktop = Rc::new(FakeDesktop::default());
        let service = zgx::platform::PlatformService(desktop.clone());
        let mut app = Self::wrapped(host, move |view| provide(service, view));
        app.desktop = Some(desktop);
        app
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
        let mut ui = Ui::new(400., 300.);
        let host = host(&ui.runtime);
        if let Err(error) = host.reload() {
            panic!("{error}");
        }
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
        app
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

    /// The node with `id`, after layout.
    pub fn find(&mut self, id: &str) -> Option<NodeId> {
        self.ui.prepare_frame();
        self.handle.find(id)
    }
    fn node(&mut self, id: &str) -> NodeId {
        self.find(id).unwrap_or_else(|| panic!("no node `{id}`"))
    }
    /// Where node `id` is, in points.
    pub fn bounds(&mut self, id: &str) -> Rect {
        let node = self.node(id);
        self.ui.scene.borrow().bounds(node)
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
        let scene = self.ui.scene.borrow();
        // A list holds a viewport, which holds the content, which holds
        // the rows.
        let content = scene
            .children(list_node)
            .first()
            .and_then(|viewport| scene.children(*viewport).first().copied())
            .unwrap_or_else(|| panic!("`{list}` is not a list"));
        let mut rows: Vec<NodeId> = scene.children(content).to_vec();
        rows.sort_by(|a, b| scene.bounds(*a).y.total_cmp(&scene.bounds(*b).y));
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
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n) {
                return text.to_string();
            }
            stack.extend(scene.children(n).iter().rev());
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
        let b = self.bounds(id);
        self.click_at(b);
    }
    fn click_at(&mut self, b: Rect) {
        let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
        self.ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        self.ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
        self.settle();
    }
    /// Click field `id` (giving it focus) and type `text` into it.
    pub fn type_text(&mut self, id: &str, text: &str) {
        self.click(id);
        self.ui.dispatch(InputEvent::Text(text.into()));
        self.settle();
    }
    /// Press and release `key`, with `modifiers`, wherever focus is.
    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.ui.dispatch(InputEvent::KeyDown {
            key: key.clone(),
            modifiers,
            repeat: false,
        });
        self.ui.dispatch(InputEvent::KeyUp { key, modifiers });
        self.settle();
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
