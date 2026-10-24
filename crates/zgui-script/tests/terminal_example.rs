//! The terminal example's interface (zgui-desktop/examples/terminal), run
//! headless: its commands split, close and move panes as its menus would.
mod common;

use common::Harness;
use std::{cell::RefCell, rc::Rc};
use zgui::scene::Rect;
use zgui_script::{ScriptHost, Value};

const APP: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zgui-desktop/examples/terminal/app.luau"
);

struct App {
    h: Harness,
    actions: Rc<RefCell<Vec<String>>>,
    /// The last layout the script asked to save.
    saved: Rc<RefCell<Value>>,
    /// Where a drag has the pointer.
    pointer: (f32, f32),
}
impl App {
    fn new() -> Self {
        Self::with_saved(Value::Nil)
    }
    /// The app as a launch finds it, with the layout a run saved.
    fn with_saved(layout: Value) -> Self {
        Self::with_host(layout, |_| {})
    }
    /// The app with what `setup` adds to its host (plugins).
    fn with_host(layout: Value, setup: impl FnOnce(&ScriptHost) + 'static) -> Self {
        Self::with_window(layout, setup, |view| view)
    }
    /// And with what `wrap` provides around it, as a window does (a fake
    /// platform for file dialogs).
    fn with_window(
        layout: Value,
        setup: impl FnOnce(&ScriptHost) + 'static,
        wrap: impl FnOnce(zgui::compose::View) -> zgui::compose::View,
    ) -> Self {
        let actions: Rc<RefCell<Vec<String>>> = Rc::default();
        let saved: Rc<RefCell<Value>> = Rc::default();
        let (log, keep) = (actions.clone(), saved.clone());
        let h = Harness::wrapped(
            move |runtime| {
                let host = ScriptHost::new(runtime, APP);
                for name in [
                    "zoom_window",
                    "close_window",
                    "set_title",
                    "end_session",
                    "keep_sessions",
                    "plugins_rescan",
                    "plugin_decide",
                ] {
                    let log = log.clone();
                    host.action(name, move |value| {
                        let arg = match value {
                            Value::Str(s) => format!(" {s}"),
                            Value::List(ids) => format!(" {ids:?}"),
                            Value::Map(_) => format!(" {}", value.to_json()),
                            _ => String::new(),
                        };
                        log.borrow_mut().push(format!("{name}{arg}"));
                    });
                }
                host.action("save_layout", move |layout| *keep.borrow_mut() = layout);
                host.shared("launch_dir", "/tmp");
                host.shared("home", "/Users/someone");
                host.shared("saved_layout", layout);
                setup(&host);
                host
            },
            wrap,
        );
        assert_eq!(h.host.error().get(), None);
        let mut app = Self {
            h,
            actions,
            saved,
            pointer: (0., 0.),
        };
        // The test window is 400 points wide: panes get all of it.
        app.run("toggle_sidebar");
        app
    }
    /// ⌘K, then the palette's row for `kind`, chosen by its title as typed
    /// and Return.
    fn open_panel(&mut self, kind: &str) {
        self.open_panel_with(kind, Default::default());
    }
    /// The same in a tab of its own (⇧Return): the whole window.
    fn open_panel_in_tab(&mut self, kind: &str) {
        self.open_panel_with(
            kind,
            zgui::input::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
    }
    fn open_panel_with(&mut self, kind: &str, modifiers: zgui::input::Modifiers) {
        self.run("open_panel");
        let title = match kind {
            "panel:notes/notes" => "Notes",
            "panel:hacker-news/front-page" => "Hacker News",
            "panel:editor/editor" => "Editor",
            other => other,
        };
        self.h
            .ui
            .dispatch(zgui::input::InputEvent::Text(title.into()));
        self.settle();
        assert!(self.h.find(&format!("open:{kind}")).is_some(), "no {kind}");
        self.h.key_with(zgui::input::Key::Enter, modifiers);
        self.settle();
    }
    fn run(&mut self, id: &str) {
        assert!(self.h.host.run_command(id), "no command `{id}`");
        self.settle();
    }
    /// Half a second of display frames: panes finish easing into place.
    fn settle(&mut self) {
        self.h.settle();
        for _ in 0..40 {
            self.h.frame();
        }
    }
    /// Each mounted terminal's box.
    fn panes(&mut self) -> Vec<Rect> {
        self.h.ui.prepare_frame();
        let scene = self.h.ui.scene.borrow();
        let mut rects: Vec<Rect> = self
            .h
            .natives
            .nodes()
            .into_iter()
            .map(|n| scene.bounds(n))
            .collect();
        rects.sort_by(|a, b| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap());
        rects
    }
}

#[test]
fn panes_split_beside_and_below_and_close_back_to_one() {
    let mut app = App::new();
    let one = app.panes();
    assert_eq!(one.len(), 1);
    assert!(one[0].width > 300. && one[0].height > 200., "{one:?}");

    app.run("split_right");
    let two = app.panes();
    assert_eq!(two.len(), 2, "{two:?}");
    assert!(
        two[0].x < two[1].x && (two[0].y - two[1].y).abs() < 1.,
        "{two:?}"
    );
    assert!(
        (two[0].width - two[1].width).abs() < 2.,
        "even halves: {two:?}"
    );

    app.run("split_down");
    let three = app.panes();
    assert_eq!(three.len(), 3, "{three:?}");
    // The right half is now two panes, one above the other.
    assert!(
        (three[1].x - three[2].x).abs() < 1. && three[1].y < three[2].y,
        "{three:?}"
    );

    app.run("close_pane");
    assert_eq!(app.panes().len(), 2);
    app.run("close_pane");
    assert_eq!(app.panes().len(), 1);
    assert_eq!(
        app.panes()[0].width,
        one[0].width,
        "one pane fills the tab again"
    );
}

#[test]
fn tabs_open_switch_and_closing_the_last_pane_closes_the_window() {
    let mut app = App::new();
    app.run("new_tab");
    app.run("split_right");
    assert_eq!(app.panes().len(), 2, "the new tab shows");
    app.run("tab_1");
    assert_eq!(app.panes().len(), 1, "the first tab has one pane");
    app.run("next_tab");
    assert_eq!(app.panes().len(), 2);
    // Close everything: the second tab's panes, then the first tab's.
    app.run("close_tab");
    assert_eq!(app.panes().len(), 1);
    app.run("close_pane");
    assert!(app.actions.borrow().iter().any(|a| a == "close_window"));
}

#[test]
fn pane_navigation_runs_without_changing_the_layout() {
    let mut app = App::new();
    app.run("split_right");
    app.run("split_down");
    let before = app.panes();
    for id in [
        "focus_left",
        "focus_right",
        "focus_up",
        "focus_down",
        "previous_pane",
        "next_pane",
        "zoom_pane",
    ] {
        app.run(id);
        assert_eq!(app.h.host.error().get(), None, "{id}");
    }
    // Zoomed: the focused pane alone; again: the split back.
    assert_eq!(app.panes().len(), 1);
    app.run("zoom_pane");
    assert_eq!(app.panes(), before);
}

#[test]
fn dragging_a_pane_by_its_grip_previews_and_moves_it() {
    use zgui::input::{InputEvent, PointerButton};
    let mut app = App::new();
    app.run("split_right");
    let before = app.panes();
    let (left, right) = (before[0], before[1]);
    // The pointer near the left pane's top shows its grip.
    let node = app
        .h
        .natives
        .nodes()
        .into_iter()
        .find(|n| app.h.ui.scene.borrow().bounds(*n) == left)
        .unwrap();
    let Some(zgui::native_view::NativeContent::Terminal(term)) = app.h.natives.content(node) else {
        panic!("no terminal")
    };
    term.set_pointer(Some((left.width / 2., 10.)));
    app.settle();
    // The grip: centered, 6 points below the pane's top.
    let (gx, gy) = (left.x + left.width / 2., left.y + 1. + 6. + 11.);
    app.h.ui.dispatch(InputEvent::PointerMove { x: gx, y: gy });
    app.h.ui.dispatch(InputEvent::PointerDown {
        x: gx,
        y: gy,
        button: PointerButton::Primary,
    });
    // Over the right pane, nearest its bottom edge.
    let (tx, ty) = (right.x + right.width / 2., right.y + right.height - 20.);
    for step in 1..=8 {
        let t = step as f32 / 8.;
        app.h.ui.dispatch(InputEvent::PointerMove {
            x: gx + (tx - gx) * t,
            y: gy + (ty - gy) * t,
        });
    }
    app.settle();
    // The preview: the left pane moved under the right one, so the two
    // stack, each the full width.
    let preview = app.panes();
    assert_eq!(preview.len(), 2, "{preview:?}");
    assert!(
        preview.iter().all(|r| (r.x - left.x).abs() < 1.)
            && preview.iter().all(|r| r.width > left.width + right.width),
        "previewed stacked: {preview:?}"
    );
    app.h.ui.dispatch(InputEvent::PointerUp {
        x: tx,
        y: ty,
        button: PointerButton::Primary,
    });
    app.settle();
    // Dropped: kept as previewed, the dragged pane below the other.
    assert_eq!(app.panes(), preview);
    assert!(preview[0].y < preview[1].y && preview[0].height > 100.);
    assert_eq!(app.h.host.error().get(), None);
}

impl App {
    /// Every text the window shows.
    fn texts(&mut self) -> Vec<String> {
        use zgui::scene::NodeKind;
        self.h.ui.prepare_frame();
        let scene = self.h.ui.scene.borrow();
        let mut out = Vec::new();
        let mut stack = vec![scene.root()];
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n) {
                out.push(text.to_string());
            }
            stack.extend(scene.children(n).iter().copied());
        }
        out
    }
}

#[test]
fn a_tab_shows_an_icon_per_pane_and_counts_past_three() {
    let mut app = App::new();
    app.run("split_right");
    app.run("split_down");
    assert!(
        !app.texts().iter().any(|t| t.starts_with('+')),
        "three panes, three icons"
    );
    app.run("split_down");
    assert_eq!(app.panes().len(), 4);
    assert!(
        app.texts().iter().any(|t| t == "+2"),
        "two icons and +2: {:?}",
        app.texts()
    );
}

#[test]
fn dropping_a_pane_on_the_tab_bar_opens_it_as_a_tab_where_previewed() {
    use zgui::input::{InputEvent, PointerButton};
    let mut app = App::new();
    app.run("split_right");
    let left = app.panes()[0];
    let node = app
        .h
        .natives
        .nodes()
        .into_iter()
        .find(|n| app.h.ui.scene.borrow().bounds(*n) == left)
        .unwrap();
    let Some(zgui::native_view::NativeContent::Terminal(term)) = app.h.natives.content(node) else {
        panic!("no terminal")
    };
    term.set_pointer(Some((left.width / 2., 10.)));
    app.settle();
    let named = |app: &mut App| app.texts().iter().filter(|t| *t == "Terminal").count();
    let (gx, gy) = (left.x + left.width / 2., left.y + 1. + 6. + 11.);
    app.h.ui.dispatch(InputEvent::PointerMove { x: gx, y: gy });
    app.h.ui.dispatch(InputEvent::PointerDown {
        x: gx,
        y: gy,
        button: PointerButton::Primary,
    });
    app.h
        .ui
        .dispatch(InputEvent::PointerMove { x: gx, y: gy + 20. });
    app.settle();
    // Dragging: the tab's name and the preview's.
    let dragging = named(&mut app);
    // Up to the empty titlebar, right of the tabs: a slot opens at the end.
    let (tx, ty) = (300., 20.);
    for step in 1..=8 {
        let t = step as f32 / 8.;
        app.h.ui.dispatch(InputEvent::PointerMove {
            x: gx + (tx - gx) * t,
            y: gy + (ty - gy) * t,
        });
    }
    app.settle();
    assert_eq!(
        named(&mut app),
        dragging + 1,
        "a slot previews the new tab: {:?}",
        app.texts()
    );
    app.h.ui.dispatch(InputEvent::PointerUp {
        x: tx,
        y: ty,
        button: PointerButton::Primary,
    });
    app.settle();
    assert_eq!(app.panes().len(), 1, "the new tab, with the pane");
    app.run("tab_1");
    assert_eq!(app.panes().len(), 1, "the first tab, without it");
    assert_eq!(app.h.host.error().get(), None);
}

impl App {
    /// The tabs' names in the strip, left to right, where they show.
    fn tab_names(&mut self) -> Vec<Rect> {
        use zgui::scene::NodeKind;
        self.h.ui.prepare_frame();
        let scene = self.h.ui.scene.borrow();
        let mut out = Vec::new();
        let mut stack = vec![scene.root()];
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n)
                && &**text == "Terminal"
                && scene.bounds(n).y < 40.
            {
                out.push(scene.bounds(n));
            }
            stack.extend(scene.children(n).iter().copied());
        }
        out.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        out
    }
    /// Press at `from` and drag to `to`.
    fn drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        use zgui::input::{InputEvent, PointerButton};
        let (x, y) = from;
        self.h.ui.dispatch(InputEvent::PointerMove { x, y });
        self.h.ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        self.pointer = from;
        self.drag_to(to);
    }
    /// Move the pointer to `to` in eight steps, and settle there.
    fn drag_to(&mut self, (tx, ty): (f32, f32)) {
        let (x, y) = self.pointer;
        for step in 1..=8 {
            let t = step as f32 / 8.;
            self.h.ui.dispatch(zgui::input::InputEvent::PointerMove {
                x: x + (tx - x) * t,
                y: y + (ty - y) * t,
            });
        }
        self.pointer = (tx, ty);
        self.settle();
    }
    fn release(&mut self, (x, y): (f32, f32)) {
        use zgui::input::{InputEvent, PointerButton};
        self.h.ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
        self.settle();
    }
}

#[test]
fn the_active_tab_dragged_down_shows_the_next_and_comes_back_in_the_strip() {
    let mut app = App::new();
    app.run("split_right");
    app.run("new_tab");
    let strip = app.tab_names();
    assert_eq!(strip.len(), 2, "{strip:?}");
    assert_eq!(app.panes().len(), 1, "the second tab shows");
    let grab = (strip[1].x + 4., strip[1].y + strip[1].height / 2.);
    // Down into the panes: it leaves the strip, and the first tab shows.
    app.drag(grab, (200., 200.));
    assert_eq!(app.panes().len(), 2, "the first tab shows");
    let out = app.tab_names();
    assert!(
        out.len() == 1 || out[1].width < 1.,
        "it folds out of the strip: {out:?}"
    );
    // Back up to the strip, past the first tab: it shows again.
    let back = (strip[1].x + strip[1].width, grab.1);
    app.drag_to(back);
    assert_eq!(app.panes().len(), 1, "the second tab again");
    app.release(back);
    assert_eq!(app.tab_names(), strip, "back where it was");
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn the_active_tab_dragged_onto_the_next_tabs_pane_joins_it() {
    let mut app = App::new();
    app.run("split_right");
    app.run("new_tab");
    let strip = app.tab_names();
    let grab = (strip[1].x + 4., strip[1].y + strip[1].height / 2.);
    app.drag(grab, (200., 200.));
    let first = app.panes();
    assert_eq!(first.len(), 2);
    // Near the bottom of the left pane: the tab goes under it.
    let target = (
        first[0].x + first[0].width / 2.,
        first[0].y + first[0].height - 20.,
    );
    app.drag_to(target);
    app.release(target);
    assert_eq!(app.panes().len(), 3, "its pane joined the first tab");
    assert_eq!(app.tab_names().len(), 1, "one tab left");
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn a_browser_pane_opens_beside_with_its_address_bar_focused_and_closes() {
    use zgui::native_view::NativeContent;
    let mut app = App::new();
    app.run("new_browser");
    app.h.ui.prepare_frame();
    let contents: Vec<_> = app
        .h
        .natives
        .nodes()
        .into_iter()
        .filter_map(|n| app.h.natives.content(n))
        .collect();
    assert_eq!(contents.len(), 2, "a terminal and a page");
    assert!(contents.iter().any(|c| matches!(c, NativeContent::Web(_))));
    let texts = app.texts();
    assert!(
        texts.iter().any(|t| t == "New Tab"),
        "the tab is named for the page: {texts:?}"
    );
    assert!(texts.iter().any(|t| t == "Search or enter address"));
    // The address bar has the keyboard.
    let focused = app.h.ui.input.focused().expect("something focused");
    assert!(
        !app.h.natives.contains(focused),
        "zgui's address input, not the terminal or the page, has focus"
    );
    // Commands that would ask a terminal act on the page instead.
    for id in [
        "copy",
        "paste",
        "bigger",
        "smaller",
        "actual_size",
        "reload_page",
        "open_location",
    ] {
        app.run(id);
        assert_eq!(app.h.host.error().get(), None, "{id}");
    }
    app.run("close_pane");
    app.h.ui.prepare_frame();
    let left: Vec<_> = app
        .h
        .natives
        .nodes()
        .into_iter()
        .filter_map(|n| app.h.natives.content(n))
        .collect();
    assert_eq!(left.len(), 1);
    assert!(matches!(left[0], NativeContent::Terminal(_)));
}

#[test]
fn resizing_the_window_moves_the_panes_without_running_script() {
    // The panes' frames are percentages and points zgui resolves itself:
    // the script works them out when the split changes, not when the area
    // does (a window drag, the sidebar easing open).
    let mut app = App::new();
    app.run("split_right");
    app.run("split_down");
    let before = app.panes();
    let calls = app.h.host.script_calls();
    app.h.ui.scene.borrow_mut().resize(600., 300.);
    app.settle();
    let after = app.panes();
    assert_eq!(app.h.host.script_calls(), calls, "a resize ran script");
    assert_eq!(after.len(), 3);
    // Each pane kept its place in the split, wider by its share of 200.
    let grew: f32 = after
        .iter()
        .zip(&before)
        .map(|(a, b)| a.width - b.width)
        .sum();
    assert!((grew - 300.).abs() < 2., "{before:?} -> {after:?}");
    assert!(
        (after[0].width - after[1].width).abs() < 2. && after[1].x > after[0].x + after[0].width,
        "{after:?}"
    );
}

#[test]
fn agents_in_tabs_not_shown_are_listed_and_a_click_opens_their_tab() {
    // Two tabs, the second shown; an agent works in the first, which no
    // window shows: the session server reports it all the same.
    let layout = Value::from_json(
        r#"{"version":1,"active":"t3","next_id":7,"kinds":[],"pages":[],"zoom":[],"ratios":{},
            "focus":{"t1":"p6","t3":"p4"},"dirs":{},
            "tabs":[{"id":"t1","root":{"id":"s5","kind":"split","axis":"row",
                        "a":{"id":"p2","kind":"pane"},"b":{"id":"p6","kind":"pane"}}},
                    {"id":"t3","root":{"id":"p4","kind":"pane"}}]}"#,
    )
    .unwrap();
    let mut app = App::with_saved(layout);
    app.run("toggle_sidebar");
    assert_eq!(app.panes().len(), 1, "the second tab, one pane");
    let agents = app.h.host.shared("agents", Value::Nil);
    agents.set(
        Value::from_json(
            r#"{"sessions":{
                "p2":{"process":"claude","idle":false,"title":"◐ Fix the build",
                      "pwd":"file:///Users/someone/zgui",
                      "status":{"state":"working","since_ms":1000,"reported":false}},
                "p4":{"process":"zsh","idle":true,"title":"","pwd":"file:///Users/someone",
                      "status":{"state":"idle","since_ms":1000,"reported":false}}}}"#,
        )
        .unwrap(),
    );
    app.settle();
    let texts = app.texts();
    assert!(texts.iter().any(|t| t == "Fix the build"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Working · zgui"), "{texts:?}");
    assert!(app.h.find("agent:p4").is_none(), "a shell is no agent");
    // Its row opens its tab, with its pane focused.
    app.h.click("agent:p2");
    app.settle();
    assert_eq!(app.panes().len(), 2, "the first tab, two panes");
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn the_agents_sidebar_opens_beside_the_panes() {
    let mut app = App::new();
    let wide = app.panes();
    assert!(!app.texts().iter().any(|t| t == "Agents"));
    app.run("toggle_sidebar");
    let texts = app.texts();
    assert!(texts.iter().any(|t| t == "Agents"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "No agents running"), "{texts:?}");
    let narrow = app.panes();
    assert!(
        narrow[0].x >= wide[0].x + 200. && narrow[0].width < wide[0].width - 200.,
        "{wide:?} -> {narrow:?}"
    );
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn a_long_address_stays_inside_its_browser_pane() {
    use zgui::input::InputEvent;
    let mut app = App::new();
    app.run("new_browser");
    // Type a very long address into the focused address bar.
    let long = format!("example.com/?{}", "solution=3893d11c553cebd6&".repeat(12));
    app.h.ui.dispatch(InputEvent::Text(long));
    app.settle();
    // The browser is the right half; its card ends 6 points in from the
    // window's 400-point edge.
    let right = 400. - 6.;
    let field = app.h.ui.input.focused().unwrap();
    let scene = app.h.ui.scene.borrow();
    // The address input and the bar row holding it end inside the pane.
    let mut node = Some(field);
    for _ in 0..3 {
        let n = node.unwrap();
        let b = scene.bounds(n);
        assert!(b.x + b.width <= right + 0.5, "{b:?} overflows the pane");
        node = scene.parent(n);
    }
    drop(scene);
    assert_eq!(app.h.host.error().get(), None);
}

impl App {
    /// The shown terminals.
    fn terminals(&mut self) -> Vec<zgui::terminal::Terminal> {
        self.h.ui.prepare_frame();
        self.h
            .natives
            .nodes()
            .into_iter()
            .filter_map(|n| match self.h.natives.content(n) {
                Some(zgui::native_view::NativeContent::Terminal(term)) => Some(term),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn a_new_tab_starts_where_its_terminal_is_unless_that_is_the_root() {
    let mut app = App::new();
    let start = |app: &mut App| {
        let terms = app.terminals();
        assert_eq!(terms.len(), 1);
        terms[0].options().working_directory
    };
    let first = app.terminals()[0].clone();
    first.update_state(|s| s.pwd = "file://mac/Users/someone/src".into());
    app.run("new_tab");
    assert_eq!(start(&mut app), Some("/Users/someone/src".into()));
    // A shell at the root (one started from the Finder by an older build):
    // the next starts where the app was launched instead.
    app.run("tab_1");
    first.update_state(|s| s.pwd = "file://mac/".into());
    app.run("new_tab");
    assert_eq!(start(&mut app), Some("/tmp".into()));
}

#[test]
fn the_layout_is_saved_and_a_launch_opens_it_again() {
    let mut app = App::new();
    app.run("split_right");
    app.run("split_down");
    app.run("new_tab");
    app.run("tab_1");
    let before = app.panes();
    let saved = app.saved.borrow().clone();
    assert!(matches!(saved, Value::Map(_)), "saved: {saved:?}");
    // The JSON on disk, read back by the next launch.
    let saved = Value::from_json(&saved.to_json()).unwrap();
    let mut again = App::with_saved(saved);
    again.settle();
    assert_eq!(again.panes(), before, "the same panes, in the same places");
    // It keeps the restored panes' sessions (and only those).
    let log = again.actions.borrow().join("\n");
    assert!(log.contains("keep_sessions"), "{log}");
    again.run("tab_2");
    assert_eq!(again.panes().len(), 1, "and the second tab");
    // Closing a pane ends its session.
    again.run("close_pane");
    assert!(
        again
            .actions
            .borrow()
            .iter()
            .any(|a| a.starts_with("end_session")),
        "{:?}",
        again.actions.borrow()
    );
}

#[test]
fn clicking_a_blank_page_focuses_its_address_bar_without_an_error() {
    use zgui::input::{InputEvent, PointerButton};
    use zgui::scene::NodeKind;
    let mut app = App::new();
    app.run("new_browser");
    // The page has no address yet: its start says where to begin.
    app.h.ui.prepare_frame();
    let hint = {
        let scene = app.h.ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if matches!(scene.kind(n), NodeKind::Text { text, .. } if &**text == "Search or enter an address above")
            {
                found = Some(scene.bounds(n));
            }
            stack.extend(scene.children(n).iter().copied());
        }
        found.expect("the blank page's hint")
    };
    let (x, y) = (hint.x + hint.width / 2., hint.y + hint.height / 2.);
    app.h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    app.h.ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
    app.settle();
    assert_eq!(app.h.host.error().get(), None);
    let focused = app.h.ui.input.focused().expect("something focused");
    assert!(
        !app.h.natives.contains(focused),
        "zgui's address input has focus"
    );
}

/// Every fill and text colour in the scene.
fn colors(ui: &zgui::widgets::Ui) -> Vec<zgui::scene::Color> {
    use zgui::scene::NodeKind;
    let scene = ui.scene.borrow();
    let mut out = Vec::new();
    let mut stack = vec![scene.root()];
    while let Some(id) = stack.pop() {
        match scene.kind(id) {
            NodeKind::Panel { quad, .. } => out.push(quad.fill),
            NodeKind::Text { color, .. } => out.push(*color),
            _ => {}
        }
        stack.extend(scene.children(id).iter().copied());
    }
    out
}
/// The window's pixels, drawn as a window would (and written to
/// `target/theme-snapshots/<name>.ppm` to look at).
fn snapshot(ui: &zgui::widgets::Ui, name: &str) -> Vec<u8> {
    let mut gpu = zgui_gpu::GpuRenderer::new(400, 300).expect("a GPU");
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.prepare_frame();
    let full = [zgui::scene::Rect::new(0., 0., 400., 300.)];
    ui.scene.borrow_mut().flush();
    gpu.render(&ui.scene.borrow(), &full).unwrap();
    let pixels = gpu.readback().unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/theme-snapshots");
    if std::fs::create_dir_all(&dir).is_ok() {
        let mut ppm = b"P6 400 300 255\n".to_vec();
        for p in pixels.chunks(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        let _ = std::fs::write(dir.join(format!("{name}.ppm")), ppm);
    }
    pixels
}

#[test]
fn any_theme_styles_it_the_light_theme_too() {
    use zgui::scene::Color;
    let mut app = App::new();
    app.settle();
    // Its own theme: exactly the palette theme.luau used to hold.
    assert_eq!(&*app.h.ui.theme().current(), "terminal-glass");
    let glass = colors(&app.h.ui);
    assert!(
        glass.contains(&Color(0x13, 0x14, 0x18, 0x73)),
        "the glass tint"
    );
    assert!(glass.contains(&Color(0x0a, 0x0b, 0x0e, 0x4d)), "the pane");
    assert!(glass.contains(&Color(255, 255, 255, 0xeb)), "the text");
    let dark = snapshot(&app.h.ui, "terminal-glass");
    // The built-in light theme knows nothing of the terminal, yet styles it:
    // its own tokens default to standard ones.
    app.h.ui.theme().set(zgui::theme::LIGHT).unwrap();
    app.settle();
    let light = colors(&app.h.ui);
    assert!(
        light.contains(&Color(0xf5, 0xf6, 0xf8, 0xb3)),
        "light glass"
    );
    assert!(
        light.contains(&Color(0xec, 0xee, 0xf2, 255)),
        "panes on the sunken surface"
    );
    assert!(light.contains(&Color(0x1a, 0x1f, 0x29, 255)), "dark text");
    assert!(
        !light.contains(&Color(255, 255, 255, 0xeb)),
        "no glass text left"
    );
    let lit = snapshot(&app.h.ui, "terminal-light");
    // Beside the panes, only the window's tint shows: it is light now.
    let at = |pixels: &[u8]| {
        let i = (150 * 400 + 2) * 4;
        pixels[i] as u32 + pixels[i + 1] as u32 + pixels[i + 2] as u32
    };
    assert!(at(&lit) > at(&dark) + 200, "{} vs {}", at(&lit), at(&dark));
    // Back to its own theme, as it was.
    app.h.ui.theme().set("terminal-glass").unwrap();
    app.settle();
    assert_eq!(colors(&app.h.ui).len(), glass.len());
    assert!(colors(&app.h.ui).contains(&Color(0x0a, 0x0b, 0x0e, 0x4d)));
}

/// A plugin with a panel, as terminal.rs would find it and load it.
fn notes_plugin(status: &str) -> (Value, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("zterm-notes-{}-{status}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("init.luau");
    std::fs::write(
        &entry,
        r#"
local Notes = ui.component(function(props)
    return ui.text { id = `notes-{props.slot.pane}`, `notes for {props.slot.pane}` }
end)
ui.contribute("panel:notes", Notes {})
"#,
    )
    .unwrap();
    let listed = Value::from_json(&format!(
        r#"[{{ "id": "notes", "name": "Notes", "version": "1.0", "author": "Ada", "description": "A notepad per pane",
             "status": "{status}", "error": null,
             "permissions": [{{ "id": "clipboard", "text": "Read and change your clipboard" }}],
             "panels": [{{ "id": "notes", "kind": "panel:notes/notes", "title": "Notes", "description": "A notepad" }}] }}]"#
    ))
    .unwrap();
    (listed, entry)
}
fn with_notes(status: &'static str) -> App {
    let (listed, entry) = notes_plugin(status);
    App::with_host(Value::Nil, move |host| {
        host.shared("plugins", listed);
        if status == "enabled" {
            host.set_plugins(vec![zgui_script::Plugin {
                name: "notes".into(),
                entry,
                grants: vec![zgui_script::Capability::Clipboard],
            }]);
        }
    })
}

#[test]
fn command_k_opens_terminals_browsers_and_plugin_panels_beside() {
    use zgui::input::Key;
    let mut app = with_notes("enabled");
    let keys: Vec<_> = app
        .h
        .host
        .commands()
        .get()
        .into_iter()
        .filter(|c| c.id == "open_panel" || c.id == "clear")
        .map(|c| (c.id, c.key))
        .collect();
    assert_eq!(
        keys,
        [
            ("open_panel".to_string(), Some("cmd+k".to_string())),
            ("clear".to_string(), Some("cmd+shift+k".to_string()))
        ]
    );
    app.run("open_panel");
    assert!(
        app.actions.borrow().iter().any(|a| a == "plugins_rescan"),
        "opening looks for new plugins"
    );
    for kind in ["terminal", "browser", "panel:notes/notes", "plugins"] {
        assert!(app.h.find(&format!("open:{kind}")).is_some(), "{kind}");
    }
    // The search field has the keyboard: typing filters.
    let focused = app.h.ui.input.focused().expect("something focused");
    assert_eq!(Some(focused), app.h.find("palette-query"));
    app.h
        .ui
        .dispatch(zgui::input::InputEvent::Text("note".into()));
    app.settle();
    assert!(app.h.find("open:browser").is_none());
    app.h.key(Key::Enter);
    app.settle();
    assert!(app.h.find("palette-query").is_none(), "it closes");
    // The panel opened beside the terminal, its pane in props.slot.
    let panes = app.panes();
    assert_eq!(panes.len(), 1, "one terminal: {panes:?}");
    let notes = app
        .h
        .texts()
        .into_iter()
        .find(|t| t.starts_with("notes for p"));
    assert!(notes.is_some(), "{:?}", app.h.texts());
    assert!(panes[0].width < 250., "it split: {panes:?}");
    assert_eq!(app.h.host.error().get(), None);
    // A second, in a tab of its own (⇧Return), is another instance.
    app.run("open_panel");
    app.h
        .ui
        .dispatch(zgui::input::InputEvent::Text("notes".into()));
    app.settle();
    app.h.key_with(
        Key::Enter,
        zgui::input::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    app.settle();
    let tabs = match app.saved.borrow().field("tabs") {
        Some(Value::List(tabs)) => tabs.len(),
        _ => 0,
    };
    assert_eq!(tabs, 2);
    // Closing a panel ends nothing.
    app.run("close_pane");
    assert!(
        !app.actions
            .borrow()
            .iter()
            .any(|a| a.starts_with("end_session")),
        "{:?}",
        app.actions.borrow()
    );
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn escape_puts_the_palette_away_and_the_browser_button_is_gone() {
    use zgui::input::Key;
    let mut app = App::new();
    app.run("open_panel");
    assert!(app.h.find("palette-query").is_some());
    app.h.key(Key::Escape);
    app.settle();
    assert!(app.h.find("palette-query").is_none());
    // Opening a browser is ⌘K's (or ⇧⌘B's) now: the titlebar has the two
    // splits and the new tab, no browser.
    let tools = app.h.texts();
    assert!(!tools.iter().any(|t| t == "Browser"), "{tools:?}");
    app.run("open_panel");
    app.h.click("open:browser");
    let focused = app.h.ui.input.focused().expect("something focused");
    assert!(
        !app.h.natives.contains(focused),
        "the address bar has focus"
    );
}

#[test]
fn a_plugin_found_asks_first_and_only_a_click_answers() {
    use zgui::input::Key;
    let mut app = with_notes("pending");
    assert!(app.h.find("consent-allow").is_some(), "the sheet is up");
    let texts = app.h.texts();
    assert!(
        texts.iter().any(|t| t == "Allow “Notes” to run?"),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t == "Read and change your clipboard"));
    // Return (meant for a terminal) answers nothing.
    app.h.key(Key::Enter);
    app.settle();
    assert!(
        !app.actions
            .borrow()
            .iter()
            .any(|a| a.starts_with("plugin_decide"))
    );
    app.h.click("consent-allow");
    assert!(
        app.actions
            .borrow()
            .iter()
            .any(|a| a == r#"plugin_decide {"allowed":true,"id":"notes"}"#),
        "{:?}",
        app.actions.borrow()
    );
    // Not Now puts it off for this run.
    let mut app = with_notes("pending");
    app.h.click("consent-later");
    assert!(app.h.find("consent-allow").is_none());
    // A pending plugin's panel is not offered.
    app.run("open_panel");
    assert!(app.h.find("open:panel:notes/notes").is_none());
}

#[test]
fn a_saved_panel_pane_opens_again_without_a_shell() {
    let mut app = with_notes("enabled");
    app.open_panel("panel:notes/notes");
    let saved = app.saved.borrow().clone();
    let (listed, entry) = notes_plugin("enabled");
    let mut again = App::with_host(saved, move |host| {
        host.shared("plugins", listed);
        host.set_plugins(vec![zgui_script::Plugin {
            name: "notes".into(),
            entry,
            grants: vec![],
        }]);
    });
    assert_eq!(again.panes().len(), 1, "one terminal, one panel");
    let panel = again
        .h
        .texts()
        .into_iter()
        .find_map(|t| t.strip_prefix("notes for ").map(str::to_owned))
        .expect("the panel again");
    let kept = again
        .actions
        .borrow()
        .iter()
        .find(|a| a.starts_with("keep_sessions"))
        .cloned()
        .unwrap_or_default();
    assert!(kept.contains("p"), "{kept}");
    assert!(
        !kept.contains(&format!("\"{panel}\"")),
        "no shell for the panel: {kept}"
    );
}

/// The example plugins zterm ships (examples/terminal-plugins), as the
/// user would allow them.
fn example_plugin(name: &str, grants: Vec<zgui_script::Capability>) -> zgui_script::Plugin {
    zgui_script::Plugin {
        name: name.into(),
        entry: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../zgui-desktop/examples/terminal-plugins")
            .join(name)
            .join("init.luau"),
        grants,
    }
}
fn listed(id: &str, panel: &str, title: &str) -> String {
    format!(
        r#"{{ "id": "{id}", "name": "{title}", "version": "1", "author": "", "description": "", "status": "enabled",
             "error": null, "permissions": [], "panels": [{{ "id": "{panel}", "kind": "panel:{id}/{panel}", "title": "{title}", "description": "" }}] }}"#
    )
}

/// What the example plugins fetch, answered from here.
async fn hn_fetch(request: zgui_script::Request) -> Result<zgui_script::Response, String> {
    {
        let url = request.url;
        let body = if url.ends_with("/topstories.json") {
            "[7, 8]".to_string()
        } else if url.contains("/v0/item/") {
            let id = if url.contains("/7.") { 7 } else { 8 };
            format!(
                r#"{{ "id": {id}, "title": "Story {id}", "url": "https://example.com/{id}", "by": "ada", "score": 10, "descendants": 2, "time": 0, "kids": [81, 82] }}"#
            )
        } else if url.contains("/api/v1/items/8") {
            r#"{ "id": 8, "children": [
                { "id": 82, "author": "bob", "text": "<p>Second, <i>said</i> bob", "created_at_i": 0, "children": [] },
                { "id": 81, "author": "cy", "text": "First &amp; best<p>See <a href=\"https://x.org\">x.org</a>", "created_at_i": 0,
                  "children": [{ "id": 83, "author": "ada", "text": "A reply", "created_at_i": 0, "children": [] }] }
            ] }"#
                .to_string()
        } else if url.contains("/user/") {
            r#"{ "id": "ada", "karma": 1234, "created": 0, "about": "Hello" }"#.to_string()
        } else {
            return Err(format!("unexpected {url}"));
        };
        Ok(zgui_script::Response {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.into_bytes(),
        })
    }
}

#[test]
fn the_hacker_news_plugin_navigates_within_its_pane() {
    use zgui_script::Capability;
    let mut app = App::with_host(Value::Nil, |host| {
        host.shared(
            "plugins",
            Value::from_json(&format!(
                "[{}]",
                listed("hacker-news", "front-page", "Hacker News")
            ))
            .unwrap(),
        );
        host.set_plugins(vec![example_plugin(
            "hacker-news",
            vec![Capability::Fetch, Capability::Web],
        )]);
        host.fetcher(hn_fetch);
    });
    app.open_panel_in_tab("panel:hacker-news/front-page");
    assert!(
        app.h.wait_until(|h| h.find("story-8").is_some()),
        "{:?}",
        app.h.texts()
    );
    assert_eq!(app.h.host.error().get(), None);
    // A story: its discussion, in HN's order, threaded.
    app.h.click("story-8");
    assert!(
        app.h
            .wait_until(|h| h.texts().iter().any(|t| t == "A reply")),
        "{:?}",
        app.h.texts()
    );
    let texts = app.h.texts();
    let first = texts.iter().position(|t| t.starts_with("First & best"));
    let second = texts.iter().position(|t| t.starts_with("Second, said bob"));
    assert!(first < second && first.is_some(), "{texts:?}");
    assert!(texts.iter().any(|t| t == "See x.org"), "{texts:?}");
    // The article, in the pane; back closes it.
    // The story has eased in.
    app.settle();
    app.h.click("hn-primary");
    app.settle();
    let pages = |app: &mut App| {
        app.h.ui.prepare_frame();
        app.h
            .natives
            .nodes()
            .into_iter()
            .filter(|n| {
                matches!(
                    app.h.natives.content(*n),
                    Some(zgui::native_view::NativeContent::Web(_))
                )
            })
            .count()
    };
    assert_eq!(pages(&mut app), 1, "the article shows");
    app.h.click("hn-back");
    app.settle();
    assert_eq!(pages(&mut app), 0);
    assert!(
        app.h.texts().iter().any(|t| t == "A reply"),
        "back at the story"
    );
    app.h.click("hn-back");
    app.settle();
    assert!(app.h.find("story-7").is_some(), "back at the feed");
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn the_editor_plugin_opens_edits_and_saves_a_picked_file() {
    use zgui::input::{Key, Modifiers};
    use zgui::platform::{FilePicker, Pending, Platform, PlatformService};
    use zgui_script::Capability;
    struct Fake(std::path::PathBuf);
    impl Platform for Fake {
        fn clipboard_text(&self) -> Option<String> {
            None
        }
        fn set_clipboard_text(&self, _: String) -> bool {
            false
        }
        fn open_files(&self, _: FilePicker) -> Pending<Result<Vec<std::path::PathBuf>, String>> {
            let path = self.0.clone();
            Box::pin(async move { Ok(vec![path]) })
        }
        fn save_file(&self, _: FilePicker) -> Pending<Result<Option<std::path::PathBuf>, String>> {
            let path = self.0.clone();
            Box::pin(async move { Ok(Some(path)) })
        }
    }
    let dir = std::env::temp_dir().join(format!("zterm-editor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("hello.rs");
    std::fs::write(&file, "fn main() {\n}\n").unwrap();
    let fake = std::rc::Rc::new(Fake(file.clone()));
    let mut app = App::with_window(
        Value::Nil,
        |host| {
            host.shared(
                "plugins",
                Value::from_json(&format!("[{}]", listed("editor", "editor", "Editor"))).unwrap(),
            );
            host.set_plugins(vec![example_plugin("editor", vec![Capability::Files])]);
        },
        move |view| zgui::compose::provide(PlatformService(fake), view),
    );
    app.open_panel("panel:editor/editor");
    app.h.click("editor-open");
    assert!(
        app.h
            .wait_until(|h| h.texts().iter().any(|t| t == "hello.rs")),
        "{:?}",
        app.h.texts()
    );
    let texts = app.h.texts();
    assert!(texts.iter().any(|t| t == "Rust"), "{texts:?}");
    assert!(
        texts.iter().any(|t| t == "1"),
        "its lines are numbered: {texts:?}"
    );
    // Typed into, it is edited; ⌘S (Ctrl+S off macOS) saves it.
    app.h
        .ui
        .dispatch(zgui::input::InputEvent::Text("// hi\n".into()));
    app.settle();
    assert!(app.h.texts().iter().any(|t| t == "Edited"));
    app.h.key_with(
        Key::Character("s".into()),
        Modifiers {
            meta: cfg!(target_os = "macos"),
            control: !cfg!(target_os = "macos"),
            ..Default::default()
        },
    );
    assert!(
        app.h
            .wait_until(|h| h.texts().iter().any(|t| t == "Saved hello.rs")),
        "{:?}",
        app.h.texts()
    );
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(
        saved.contains("// hi") && saved.contains("fn main()"),
        "{saved}"
    );
    // A reload (the plugin edited, the pane rebuilt) keeps the file open.
    app.h.host.reload().unwrap();
    app.settle();
    assert!(
        app.h.texts().iter().any(|t| t == "hello.rs"),
        "{:?}",
        app.h.texts()
    );
    assert_eq!(app.h.host.error().get(), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_example_plugins_run_in_their_panels() {
    use zgui_script::Capability;
    let run = |grants: Vec<Capability>| {
        App::with_host(Value::Nil, move |host| {
            host.shared(
                "plugins",
                Value::from_json(&format!(
                    "[{}, {}]",
                    listed("notes", "notes", "Notes"),
                    listed("hacker-news", "front-page", "Hacker News")
                ))
                .unwrap(),
            );
            host.set_plugins(vec![
                example_plugin("notes", vec![]),
                example_plugin("hacker-news", grants),
            ]);
            host.fetcher(hn_fetch);
        })
    };
    let mut app = run(vec![Capability::Fetch, Capability::Web]);
    app.open_panel("panel:notes/notes");
    assert_eq!(app.h.host.error().get(), None);
    assert!(app.h.texts().iter().any(|t| t == "Notes for this pane"));
    app.open_panel("panel:hacker-news/front-page");
    assert!(
        app.h.wait_until(|h| h.find("story-8").is_some()),
        "{:?}",
        app.h.texts()
    );
    assert_eq!(app.h.host.error().get(), None);

    // Not allowed the network, it cannot fetch.
    let mut app = run(vec![Capability::Web]);
    app.open_panel("panel:hacker-news/front-page");
    app.settle();
    let texts = app.h.texts();
    assert!(
        texts.iter().any(|t| t.starts_with("Could not load")),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t.contains("Story")));
}

#[test]
fn a_panel_is_where_it_was_left_after_another_tab_shows() {
    use zgui_script::Capability;
    let fetches = std::rc::Rc::new(std::cell::Cell::new(0));
    let counted = fetches.clone();
    let mut app = App::with_host(Value::Nil, move |host| {
        host.shared(
            "plugins",
            Value::from_json(&format!(
                "[{}]",
                listed("hacker-news", "front-page", "Hacker News")
            ))
            .unwrap(),
        );
        host.set_plugins(vec![example_plugin(
            "hacker-news",
            vec![Capability::Fetch, Capability::Web],
        )]);
        host.fetcher(move |request| {
            counted.set(counted.get() + 1);
            hn_fetch(request)
        });
    });
    app.open_panel_in_tab("panel:hacker-news/front-page");
    assert!(app.h.wait_until(|h| h.find("story-8").is_some()));
    app.settle();
    app.h.click("story-8");
    assert!(
        app.h
            .wait_until(|h| h.texts().iter().any(|t| t == "A reply"))
    );
    app.settle();
    let back = app.h.find("hn-back").expect("the story's back button");
    let fetched = fetches.get();
    // Another tab, then this one again.
    app.run("previous_tab");
    assert!(
        !app.h.ui.scene.borrow().layout_visible(back),
        "hidden while another tab shows"
    );
    app.run("next_tab");
    assert_eq!(
        app.h.find("hn-back"),
        Some(back),
        "the same panel, not one built again"
    );
    assert!(app.h.texts().iter().any(|t| t == "A reply"));
    assert_eq!(fetches.get(), fetched, "nothing fetched again");
    assert_eq!(app.h.host.error().get(), None);
}

#[test]
fn a_panels_bar_keeps_its_clicks_while_the_grip_shows() {
    use zgui_script::Capability;
    let mut app = App::with_host(Value::Nil, |host| {
        host.shared(
            "plugins",
            Value::from_json(&format!(
                "[{}]",
                listed("hacker-news", "front-page", "Hacker News")
            ))
            .unwrap(),
        );
        host.set_plugins(vec![example_plugin(
            "hacker-news",
            vec![Capability::Fetch, Capability::Web],
        )]);
        host.fetcher(hn_fetch);
    });
    app.open_panel_in_tab("panel:hacker-news/front-page");
    assert!(app.h.wait_until(|h| h.find("story-8").is_some()));
    app.settle();
    app.h.click("story-8");
    app.settle();
    // The pointer at the top of the pane: the grip shows, across from the
    // bar's back button.
    let back = app.h.bounds("hn-back");
    app.h.ui.dispatch(zgui::input::InputEvent::PointerMove {
        x: back.x + back.width / 2.,
        y: back.y + back.height / 2.,
    });
    app.settle();
    app.h.click("hn-back");
    app.settle();
    assert!(app.h.find("story-7").is_some(), "back at the feed");
}
