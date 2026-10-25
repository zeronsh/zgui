//! The example apps' common interactions, each to its frame (and drawn, if
//! there is a drawing): mail (opening a message, a search keystroke,
//! archiving), the dashboard (a feed tick, sorting, a filter keystroke),
//! Sketch (dragging a shape, panning, zooming) and the code editor (a
//! keystroke, scrolling a page, a find-all keystroke). Each prints its time
//! and instructions retired (macOS) per interaction and, with
//! `ZGX_PROFILE=1`, where they went. `apps_bench` runs them drawn on a GPU
//! with text shaped; CI's instruction gate runs them inside
//! `list_state_bench` (no new binary to link), small and undrawn but for
//! their canvases' CPU work.
//!
//! `APP=mail` (`dashboard`, `sketch`, `code`) runs one app; `REPS` sets the
//! interactions measured (20); `ROWS` the dashboard's rows (100,000),
//! `SHAPES` Sketch's shapes (500), `LINES` the code editor's file (100,000).
use crate::bench_limits;
use std::time::Instant;
use zgx::{
    input::{InputEvent, Key, PointerButton},
    profile::Phase,
    scene::Rect,
};
use zgx_script::{ScriptHost, testing::TestApp};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");

/// A size from the environment, else `default`.
fn size(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// This process's instructions retired (macOS; 0 elsewhere).
fn instructions() -> u64 {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Usage {
            uuid: [u8; 16],
            fields: [u64; 40],
        }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut Usage) -> i32;
        }
        let mut usage = Usage {
            uuid: [0; 16],
            fields: [0; 40],
        };
        // SAFETY: RUSAGE_INFO_V4 into a buffer as large as it; this process.
        unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut usage) };
        // `ri_instructions`.
        usage.fields[29]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

/// What draws an app's frames and shapes its text: a GPU offscreen
/// (`apps_bench`), or its canvases' CPU work alone (the gate's run, its
/// text laid out by the fallback layout, as the other script benches' is).
pub trait Drawing {
    /// Draw `damage` of `scene`'s frame.
    fn draw(&mut self, scene: &zgx::scene::Scene, damage: &[Rect]);
    /// Wait for what was drawn to be done.
    fn finish(&mut self) {}
}
/// Makes an app's drawing for its scene of `size` (points), if any.
pub type MakeDrawing<'a> =
    &'a dyn Fn(&mut zgx::scene::Scene, (f32, f32)) -> Option<Box<dyn Drawing>>;

/// An app, run headless as a window runs it: a frame laid out, its damage
/// found and drawn (if there is a drawing).
struct Bench {
    app: TestApp,
    drawing: Option<Box<dyn Drawing>>,
    reps: usize,
}
impl Bench {
    fn new(script: &str, size: (f32, f32), args: &[String], make: MakeDrawing) -> Self {
        Self::with(script, size, args, make, |_| {})
    }
    /// `new`, with `setup` run on the app before its first frame.
    fn with(
        script: &str,
        size: (f32, f32),
        args: &[String],
        make: MakeDrawing,
        setup: impl FnOnce(&mut TestApp),
    ) -> Self {
        let path = format!("{EXAMPLES}/{script}/app.luau");
        let mut app = TestApp::with_desktop(|runtime| {
            ScriptHost::new(runtime, path)
                .limits(bench_limits::limits())
                .args(args.iter().cloned())
        });
        app.ui.scene.borrow_mut().resize(size.0, size.1);
        // As a desktop window with no screen reader on: names and values
        // made from state wait for one.
        app.ui.set_assistive_technology(false);
        setup(&mut app);
        let drawing = make(&mut app.ui.scene.borrow_mut(), size);
        let reps = self::size("REPS", 20);
        let mut bench = Self { app, drawing, reps };
        bench.frames(3);
        assert_eq!(bench.app.error(), None, "{script} loads");
        bench
    }
    /// A frame: what the app changed, laid out, its damage drawn.
    fn frame(&mut self) {
        self.app.frame();
        let report = self.app.ui.scene.borrow_mut().flush();
        if let Some(drawing) = &mut self.drawing {
            drawing.draw(&self.app.ui.scene.borrow(), &report.damage);
        }
        zgx::profile::frame();
    }
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }
    /// `interaction` (the `k`th), each followed by its frame, `reps` times
    /// after two to warm up: its time, instructions and phases, printed.
    fn measure(&mut self, name: &str, mut interaction: impl FnMut(&mut Self, usize)) {
        for k in 0..2 {
            interaction(self, k);
            self.frame();
        }
        if let Some(drawing) = &mut self.drawing {
            drawing.finish();
        }
        zgx::profile::take();
        let (start, before) = (Instant::now(), instructions());
        for k in 0..self.reps {
            interaction(self, k + 2);
            self.frame();
        }
        if let Some(drawing) = &mut self.drawing {
            drawing.finish();
        }
        let reps = self.reps as f64;
        let ms = start.elapsed().as_secs_f64() * 1e3 / reps;
        let millions = (instructions() - before) as f64 / 1e6 / reps;
        let mut line = format!("{name}: {ms:.2} ms, {millions:.2} M instructions");
        let phases = zgx::profile::take();
        if !phases.is_empty() {
            let counted: u64 = phases.iter().map(|p| p.1).sum();
            for (phase, count, _) in &phases {
                if *count > 0 && !matches!(phase, Phase::Submit | Phase::Present) {
                    line += &format!(" | {} {:.2}", phase.name(), *count as f64 / 1e6 / reps);
                }
            }
            let other = millions - counted as f64 / 1e6 / reps;
            line += &format!(" | other {other:.2}");
        }
        println!("{line}");
        assert_eq!(self.app.error(), None, "{name}");
    }
    fn click_at(&mut self, (x, y): (f32, f32)) {
        let button = PointerButton::Primary;
        self.app
            .ui
            .dispatch(InputEvent::PointerDown { x, y, button });
        self.app.ui.dispatch(InputEvent::PointerUp { x, y, button });
    }
    fn bounds(&mut self, id: &str) -> Rect {
        self.app.bounds(id)
    }
}

/// The toolbar button named `label`'s bounds.
fn button(bench: &Bench, label: &str) -> Rect {
    let semantics = bench.app.ui.semantics.borrow();
    let scene = bench.app.ui.scene.borrow();
    let mut pending = vec![scene.root()];
    while let Some(id) = pending.pop() {
        if semantics
            .get(id)
            .is_some_and(|n| n.role == zgx::semantics::Role::Button && n.label == label)
        {
            return scene.bounds(id);
        }
        pending.extend(scene.children(id).iter().copied());
    }
    panic!("no {label} button")
}

fn mail(make: MakeDrawing) {
    let mut b = Bench::new("mail", (1180., 760.), &[], make);
    b.measure("mail: open a message", |b, k| {
        b.app.click_row("messages", k % 2 + 1);
    });
    b.measure("mail: search keystroke", |b, k| {
        if k % 2 == 0 {
            b.app.type_text("search", "r");
        } else {
            b.app.key(Key::Backspace);
        }
    });
    b.app.click_row("messages", 1);
    b.frames(30);
    b.measure("mail: archive", |b, _| {
        let archive = button(b, "Archive");
        b.click_at((archive.x + 4., archive.y + archive.height / 2.));
    });
}

fn dashboard(make: MakeDrawing) {
    let rows = size("ROWS", 100_000).to_string();
    // The live feed's timer ticks by the wall clock, as often as the run is
    // slow (under cachegrind, many times as often): paused before the first
    // frame, each tick is the one this measures.
    let mut b = Bench::with("dashboard", (1280., 800.), &[rows], make, |app| {
        app.click("live")
    });
    b.app.click("sort-price");
    b.frames(2);
    b.measure("dashboard: feed tick, sorted by price", |b, _| {
        assert!(b.app.host.run_command("refresh"));
    });
    b.measure("dashboard: sort", |b, k| {
        b.app.click(if k % 2 == 0 {
            "sort-volume"
        } else {
            "sort-symbol"
        });
    });
    b.measure("dashboard: filter keystroke", |b, k| {
        if k % 2 == 0 {
            b.app.type_text("search", "B");
        } else {
            b.app.key(Key::Backspace);
        }
    });
}

fn sketch(make: MakeDrawing) {
    let dir = std::env::temp_dir().join(format!("zgx-apps-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("busy.json");
    let shapes: Vec<String> = (0..size("SHAPES", 500))
        .map(|i| {
            let (x, y) = ((i % 25) * 40, (i / 25) * 40);
            format!(
                r##"{{"id":"s{}","kind":"{}","name":"S{i}","x":{x},"y":{y},"w":32,"h":32,"fill":"#4c8dff","stroke":"#202124","stroke_width":1,"radius":4}}"##,
                i + 1,
                if i % 2 == 0 { "rect" } else { "ellipse" }
            )
        })
        .collect();
    std::fs::write(
        &path,
        format!(r#"{{"version":1,"shapes":[{}]}}"#, shapes.join(",")),
    )
    .unwrap();
    let mut b = Bench::new("sketch", (1240., 800.), &[], make);
    b.app.desktop.clone().unwrap().pick([&path]);
    assert!(b.app.host.run_command("open"));
    assert!(b.app.wait_until(|h| h.text("title") == "busy.json"));
    b.frames(2);
    // The board's origin: world (0, 0) on the window, at the starting view.
    let board = b.bounds("board");
    let at = |x: f32, y: f32| (board.x + 80. + x, board.y + 60. + y);
    let (x, y) = at(16., 16.);
    let button = PointerButton::Primary;
    b.app.ui.dispatch(InputEvent::PointerDown { x, y, button });
    b.frame();
    b.measure("sketch: drag a shape", |b, k| {
        let k = k as f32;
        b.app.ui.dispatch(InputEvent::PointerMove {
            x: x + k * 2.,
            y: y + k,
        });
    });
    b.app.ui.dispatch(InputEvent::PointerUp { x, y, button });
    b.app.key(Key::Escape);
    b.frame();
    let (x, y) = at(400., 300.);
    b.measure("sketch: pan", |b, k| {
        let step = if (k / 10) % 2 == 0 { 12. } else { -12. };
        b.app.ui.dispatch(InputEvent::Scroll {
            x,
            y,
            delta_x: step,
            delta_y: step / 2.,
        });
    });
    b.measure("sketch: zoom", |b, k| {
        let delta = if k % 2 == 0 { 0.05 } else { -0.05 };
        b.app.ui.dispatch(InputEvent::Pinch { x, y, delta });
    });
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Rust file of `lines` lines, as the code editor's sample makes them.
fn source(lines: usize) -> String {
    let body = [
        "/// A function, its docs and its body.",
        "pub fn compute_N(input: &[u8], scale: f32) -> Result<Vec<u32>, Error> {",
        "    let name = \"item N\"; // a comment",
        "    let total: u32 = input.iter().map(|b| *b as u32 * N).sum();",
        "    /* a block comment",
        "       running on */",
        "    if total > 0xff { return Err(Error::Overflow(total)); }",
        "    Ok(vec![total; scale as usize])",
        "}",
        "",
    ];
    (0..lines)
        .map(|i| body[i % body.len()].replace('N', &i.to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn code(make: MakeDrawing) {
    let lines = size("LINES", 100_000);
    let path = std::env::temp_dir().join(format!("zgx-apps-bench-{}.rs", std::process::id()));
    std::fs::write(&path, source(lines)).unwrap();
    let mut b = Bench::new("code_editor", (1100., 760.), &[], make);
    b.app.desktop.clone().unwrap().pick([&path]);
    assert!(b.app.host.run_command("open"));
    assert!(
        b.app
            .wait_until(|h| h.text("status-message").starts_with("Opened"))
    );
    b.frames(3);
    let editor = b.app.find("editor").expect("the editor");
    b.app.ui.input.focus(&b.app.ui.scene, Some(editor));
    let handle = b.app.ui.editor(editor).unwrap();
    let middle = handle.editor.borrow().len() / 2;
    handle.set_selection(middle, middle);
    b.frames(2);
    b.measure("code: keystroke", |b, k| {
        let text = if k % 2 == 0 { "x" } else { "y" };
        b.app.ui.dispatch(InputEvent::Text(text.into()));
    });
    let (x, y) = {
        let r = b.bounds("editor");
        (r.x + r.width / 2., r.y + r.height / 2.)
    };
    b.measure("code: scroll a page", |b, k| {
        let delta_y = if (k / 10) % 2 == 0 { 600. } else { -600. };
        b.app.ui.dispatch(InputEvent::Scroll {
            x,
            y,
            delta_x: 0.,
            delta_y,
        });
    });
    assert!(b.app.host.run_command("find"));
    b.frames(2);
    b.app.type_text("find", "Overflo");
    b.frames(2);
    b.measure("code: find-all keystroke", |b, k| {
        if k % 2 == 0 {
            b.app.ui.dispatch(InputEvent::Text("w".into()));
        } else {
            b.app.key(Key::Backspace);
        }
    });
    let _ = std::fs::remove_file(&path);
}

/// The apps `APP` names (all of them if none), drawn by what `make` makes.
pub fn run(make: MakeDrawing) {
    let only = std::env::var("APP").ok();
    for (name, run) in [
        ("mail", mail as fn(MakeDrawing)),
        ("dashboard", dashboard),
        ("sketch", sketch),
        ("code", code),
    ] {
        if only.as_deref().is_none_or(|only| only == name) {
            run(make);
        }
    }
}
