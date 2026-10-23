//! Testing a scripted app without a window: [`TestApp`] mounts a host
//! headless, with the task executor and frame clock a window has, and drives
//! it as a person would (clicks, typing, keys) while a test reads what it
//! shows.
//!
//! ```ignore
//! use zgui_script::{ScriptHost, testing::TestApp};
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
//! services as a window does (a fake `zgui::platform::PlatformService` for
//! the clipboard and file dialogs).
use crate::ScriptHost;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use zgui::{
    compose::{TaskRunner, View, ViewHandle, provide},
    frame::{Frame, FrameClock},
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Runtime,
    scene::{NodeId, NodeKind, Rect},
    task::LocalExecutor,
    widgets::Ui,
};

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
    pub natives: zgui::native_view::NativeViews,
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
    /// With the window's layout inspector, as a desktop window provides it.
    pub fn inspected(
        host: impl FnOnce(&Runtime) -> ScriptHost,
        inspector: &zgui::inspect::Inspector,
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
        let natives = zgui::native_view::NativeViews::new();
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
        let scene = self.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n) {
                return text.to_string();
            }
            stack.extend(scene.children(n).iter().rev());
        }
        panic!("no text in `{id}`: {:?}", scene.kind(node))
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
    /// Node `id`'s opacity.
    pub fn opacity(&mut self, id: &str) -> f32 {
        let node = self.node(id);
        self.ui.scene.borrow().effects(node).opacity
    }

    /// Press and release the primary button at node `id`'s middle.
    pub fn click(&mut self, id: &str) {
        let b = self.bounds(id);
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
