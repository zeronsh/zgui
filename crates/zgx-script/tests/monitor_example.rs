//! The resource monitor's interface (zgx-desktop/examples/zgx_monitor),
//! run headless on made-up samples: panels split, browse, close, and move
//! by dragging, as the terminal example's panes do.
mod common;

use common::Harness;
use std::rc::Rc;
use zgx::{
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{NodeKind, Rect},
};
use zgx_script::{ScriptHost, Value};

const APP: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zgx-desktop/examples/zgx_monitor/app.luau"
);

fn map(fields: &[(&str, Value)]) -> Value {
    let mut fields: Vec<(Rc<str>, Value)> = fields
        .iter()
        .map(|(k, v)| ((*k).into(), v.clone()))
        .collect();
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Map(fields.into())
}
fn num(n: f64) -> Value {
    Value::Num(n)
}

/// Processes 100 (the launched one) to 104, busiest last.
fn procs(tick: f64) -> Value {
    Value::List(
        (0..5)
            .map(|i| {
                let cpu = i as f64 * 10. + tick % 3.;
                map(&[
                    ("pid", num(100. + i as f64)),
                    ("name", Value::Str(format!("proc{i}").into())),
                    ("cpu", num(cpu)),
                    ("gpu", num(i as f64)),
                    ("mem", num(50. + i as f64 * 100.)),
                    ("smooth", num(cpu)),
                ])
            })
            .collect(),
    )
}

fn history(name: &str, n: usize) -> Value {
    let series = |scale: f64| Value::List((0..n).map(|i| num((i % 7) as f64 * scale)).collect());
    map(&[
        ("name", Value::Str(name.into())),
        ("cpu", series(3.)),
        ("gpu", series(1.)),
        ("mem", series(40.)),
    ])
}

struct App {
    h: Harness,
    tick: f64,
}
impl App {
    fn new() -> Self {
        let h = Harness::new(|runtime| {
            let host = ScriptHost::new(runtime, APP);
            host.shared("launched", 100.);
            host.shared("procs", procs(0.));
            host.action("close_window", |_| {});
            host
        });
        h.ui.scene.borrow_mut().resize(1200., 760.);
        assert_eq!(h.host.error().get(), None);
        let mut app = Self { h, tick: 0. };
        app.sample();
        app
    }
    /// A sample, as the host publishes one.
    fn sample(&mut self) {
        self.tick += 1.;
        self.h
            .host
            .shared("procs", Value::Nil)
            .set(procs(self.tick));
        self.h.host.shared("summary", Value::Nil).set(map(&[
            ("count", num(5.)),
            ("cpu", num(100.)),
            ("gpu", num(7.)),
        ]));
        let watch = self.h.host.shared("watch", Value::Nil).get();
        for pid in watch.list().iter().filter_map(Value::as_f64) {
            self.h
                .host
                .shared(&format!("history:{pid}"), Value::Nil)
                .set(history(&format!("proc{}", pid - 100.), 40));
        }
        self.settle();
    }
    fn run(&mut self, id: &str) {
        assert!(self.h.host.run_command(id), "no command `{id}`");
        self.settle();
    }
    fn settle(&mut self) {
        self.h.settle();
        for _ in 0..40 {
            self.h.frame();
        }
        assert_eq!(self.h.host.error().get(), None);
    }
    fn key(&mut self, key: Key) {
        self.h.ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        self.settle();
    }
    /// Every text the window shows.
    fn texts(&mut self) -> Vec<String> {
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
    /// The panels' boxes and ids, in reading order.
    fn panels(&mut self) -> Vec<(Rect, String)> {
        let mut found = Vec::new();
        for n in 1..40 {
            let id = format!("p{n}");
            if let Some(node) = self.h.find(&format!("panel-{id}")) {
                found.push((self.h.ui.scene.borrow().bounds(node), id));
            }
        }
        found.sort_by(|a, b| (a.0.y, a.0.x).partial_cmp(&(b.0.y, b.0.x)).unwrap());
        found
    }
    fn drag(&mut self, from: (f32, f32), to: (f32, f32), release: bool) {
        self.h.ui.dispatch(InputEvent::PointerMove {
            x: from.0,
            y: from.1,
        });
        self.h.ui.dispatch(InputEvent::PointerDown {
            x: from.0,
            y: from.1,
            button: PointerButton::Primary,
        });
        for step in 1..=10 {
            let t = step as f32 / 10.;
            self.h.ui.dispatch(InputEvent::PointerMove {
                x: from.0 + (to.0 - from.0) * t,
                y: from.1 + (to.1 - from.1) * t,
            });
        }
        self.settle();
        if release {
            self.h.ui.dispatch(InputEvent::PointerUp {
                x: to.0,
                y: to.1,
                button: PointerButton::Primary,
            });
            self.settle();
        }
    }
}

#[test]
fn it_opens_on_the_launched_process_and_lists_every_process() {
    let mut app = App::new();
    let texts = app.texts();
    for name in ["proc0", "proc1", "proc4"] {
        assert!(texts.iter().any(|t| t == name), "{name} in {texts:?}");
    }
    assert_eq!(app.panels().len(), 1, "one panel");
    assert!(texts.iter().any(|t| t.contains("5 processes")), "{texts:?}");
}

impl App {
    fn panel_names(&mut self) -> Vec<String> {
        let ids: Vec<String> = self.panels().into_iter().map(|(_, id)| id).collect();
        ids.iter()
            .map(|id| {
                let node = self.h.find(&format!("title-{id}")).unwrap();
                let scene = self.h.ui.scene.borrow();
                let mut stack = vec![node];
                while let Some(n) = stack.pop() {
                    if let NodeKind::Text { text, .. } = scene.kind(n) {
                        return text.to_string();
                    }
                    stack.extend(scene.children(n).iter().copied());
                }
                String::new()
            })
            .collect()
    }
    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2., r.y + r.height / 2.)
    }
    /// The sidebar row showing `name`.
    fn row(&mut self, name: &str) -> Rect {
        self.h.ui.prepare_frame();
        let scene = self.h.ui.scene.borrow();
        let mut stack = vec![scene.root()];
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n)
                && &**text == name
            {
                let b = scene.bounds(n);
                if b.x < 300. {
                    return b;
                }
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no row {name}")
    }
}

#[test]
fn splitting_closing_and_browsing_with_keys() {
    let mut app = App::new();
    assert_eq!(app.panel_names(), ["proc0"]);
    app.run("split_right");
    let two = app.panels();
    assert_eq!(two.len(), 2);
    assert!(
        two[0].0.x < two[1].0.x && (two[0].0.width - two[1].0.width).abs() < 2.,
        "{two:?}"
    );
    // The new panel shows the same process, to compare against.
    assert_eq!(app.panel_names(), ["proc0", "proc0"]);
    // ↓ from the filter field moves the new (active) panel down the list,
    // busiest first after the launched process.
    app.key(Key::ArrowDown);
    assert_eq!(app.panel_names(), ["proc0", "proc4"]);
    app.key(Key::ArrowDown);
    assert_eq!(app.panel_names(), ["proc0", "proc3"]);
    app.key(Key::ArrowUp);
    assert_eq!(app.panel_names(), ["proc0", "proc4"]);
    // ← to the first panel, ↓ moves it instead.
    app.key(Key::ArrowLeft);
    app.key(Key::ArrowDown);
    assert_eq!(app.panel_names(), ["proc4", "proc4"]);
    app.run("split_down");
    assert_eq!(app.panels().len(), 3);
    app.run("close_panel");
    app.run("close_panel");
    assert_eq!(app.panels().len(), 1);
    assert!(
        app.panels()[0].0.width > 800.,
        "one panel fills the area again"
    );
}

#[test]
fn typing_filters_the_list() {
    let mut app = App::new();
    app.h.ui.dispatch(InputEvent::Text("proc3".into()));
    app.settle();
    let texts = app.texts();
    assert!(texts.iter().any(|t| t == "proc3"));
    assert!(!texts.iter().any(|t| t == "proc4"), "{texts:?}");
    app.key(Key::Escape);
    assert!(app.texts().iter().any(|t| t == "proc4"));
}

#[test]
fn a_process_dragged_to_a_panels_edge_opens_beside_it_and_to_its_middle_shows_there() {
    let mut app = App::new();
    let one = app.panels()[0].0;
    // Onto the right edge: previewed as a panel of its own, then dropped.
    let from = App::center(app.row("proc2"));
    let edge = (one.x + one.width - 30., one.y + one.height / 2.);
    app.drag(from, edge, false);
    let preview = app.panels();
    assert_eq!(preview.len(), 2, "previewed: {preview:?}");
    app.h.ui.dispatch(InputEvent::PointerUp {
        x: edge.0,
        y: edge.1,
        button: PointerButton::Primary,
    });
    app.settle();
    assert_eq!(app.panels(), preview, "kept as previewed");
    assert_eq!(app.panel_names(), ["proc0", "proc2"]);
    // Onto the first panel's middle: it shows there.
    let first = app.panels()[0].0;
    let from = App::center(app.row("proc3"));
    app.drag(from, App::center(first), true);
    assert_eq!(app.panel_names(), ["proc3", "proc2"]);
}

#[test]
fn a_panel_dragged_by_its_header_moves_below_another_or_swaps() {
    let mut app = App::new();
    app.run("split_right");
    app.key(Key::ArrowDown);
    assert_eq!(app.panel_names(), ["proc0", "proc4"]);
    let panels = app.panels();
    let (left, right) = (panels[0].0, panels[1].0);
    // The left panel's header onto the right one's middle: they swap.
    let header = (left.x + 60., left.y + 20.);
    app.drag(header, App::center(right), true);
    assert_eq!(app.panel_names(), ["proc4", "proc0"]);
    // Onto the other's bottom edge: stacked, each the full width.
    let panels = app.panels();
    let (left, right) = (panels[0].0, panels[1].0);
    let header = (left.x + 60., left.y + 20.);
    app.drag(
        header,
        (right.x + right.width / 2., right.y + right.height - 20.),
        true,
    );
    let stacked = app.panels();
    assert!(
        stacked[0].0.y < stacked[1].0.y && stacked[0].0.width > left.width + right.width,
        "{stacked:?}"
    );
    assert_eq!(app.panel_names(), ["proc0", "proc4"]);
}

#[test]
fn the_sidebar_closes_and_opens_and_the_keys_still_browse() {
    let mut app = App::new();
    let open = app.panels()[0].0;
    app.run("toggle_sidebar");
    let closed = app.panels()[0].0;
    assert!(
        closed.x < 20. && closed.width > open.width + 200.,
        "the panel takes the sidebar's room: {open:?} → {closed:?}"
    );
    // Closed, ↓ still moves the panel through the list.
    app.key(Key::ArrowDown);
    assert_eq!(app.panel_names(), ["proc4"]);
    app.run("toggle_sidebar");
    assert_eq!(app.panels()[0].0, open, "back where it was");
}

/// A machine's worth of processes, sampled for a while: each sample
/// replaces the list, so the script holds about as much at the end as after
/// the first (it once kept the last 64 lists and ran out of memory).
#[test]
fn sampling_many_processes_holds_steady_memory() {
    let app = App::new();
    let many = |tick: f64| {
        Value::List(
            (0..700)
                .map(|i| {
                    let cpu = (i as f64 * 7. + tick * 13.) % 50.;
                    map(&[
                        ("pid", num(100. + i as f64)),
                        ("name", Value::Str(format!("proc{i}").into())),
                        ("cpu", num(cpu)),
                        ("gpu", num(0.)),
                        ("mem", num(50. + i as f64)),
                        ("smooth", num(cpu)),
                    ])
                })
                .collect(),
        )
    };
    let sample = |tick: f64| {
        app.h.host.shared("procs", Value::Nil).set(many(tick));
        app.h.settle();
        app.h.frames(30);
    };
    sample(0.);
    let first = app.h.host.script_memory();
    for tick in 1..200 {
        sample(tick as f64);
    }
    assert_eq!(app.h.host.error().get(), None);
    let last = app.h.host.script_memory();
    assert!(
        last < first + (2 << 20),
        "{} KB after one sample, {} KB after 200",
        first / 1024,
        last / 1024
    );
}
