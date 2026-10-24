//! The sketch example (examples/sketch), run headless: drawing, selecting,
//! moving with snapping, resizing by a handle, the inspector, undo and redo,
//! the keys, panning and zooming, and saving and opening through the file
//! dialogs.
mod common;

use common::Harness;
use std::time::Duration;
use zgx::input::{InputEvent, Key, Modifiers, PointerButton};
use zgx_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sketch/app.luau");
/// The platform's command key: Command on macOS, Control elsewhere.
fn command() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

/// Where the world's origin starts on the board.
const ORIGIN: (f32, f32) = (80., 60.);

fn sketch() -> Harness {
    let h = Harness::with_desktop(|runtime| ScriptHost::new(runtime, APP));
    h.ui.scene.borrow_mut().resize(1240., 800.);
    h.frames(2);
    h
}
/// A world point, on the window (at the starting view).
fn at(h: &mut Harness, x: f32, y: f32) -> (f32, f32) {
    let board = h.bounds("board");
    (board.x + ORIGIN.0 + x, board.y + ORIGIN.1 + y)
}
fn drag_with(h: &mut Harness, from: (f32, f32), to: (f32, f32), modifiers: Modifiers) {
    let button = PointerButton::Primary;
    h.ui.dispatch_with_modifiers(
        InputEvent::PointerDown {
            x: from.0,
            y: from.1,
            button,
        },
        modifiers,
    );
    for step in 1..=4 {
        let t = step as f32 / 4.;
        h.ui.dispatch_with_modifiers(
            InputEvent::PointerMove {
                x: from.0 + (to.0 - from.0) * t,
                y: from.1 + (to.1 - from.1) * t,
            },
            modifiers,
        );
        h.frame();
    }
    h.ui.dispatch_with_modifiers(
        InputEvent::PointerUp {
            x: to.0,
            y: to.1,
            button,
        },
        modifiers,
    );
    h.settle();
    h.frame();
}
/// Drag between world points.
fn drag(h: &mut Harness, from: (f32, f32), to: (f32, f32)) {
    let (from, to) = (at(h, from.0, from.1), at(h, to.0, to.1));
    drag_with(h, from, to, Modifiers::default());
}
fn click_at(h: &mut Harness, x: f32, y: f32) {
    let p = at(h, x, y);
    drag_with(h, p, p, Modifiers::default());
}
fn press(h: &mut Harness, key: &str) {
    h.key(Key::Character(key.into()));
}
/// The inspector's fields for the one shape selected.
fn field(h: &mut Harness, key: &str) -> String {
    h.text(&format!("field-{key}"))
}
fn rect(h: &mut Harness) -> [String; 4] {
    ["x", "y", "w", "h"].map(|k| field(h, k))
}
/// How many shapes the drawing has (nothing selected, the inspector says).
fn count(h: &mut Harness) -> String {
    h.key(Key::Escape);
    h.texts()
        .into_iter()
        .find(|t| t.ends_with("shape") || t.ends_with("shapes"))
        .unwrap_or_default()
}

/// A rectangle drawn from (96, 96) to (216, 176): the grid's lines.
fn draw_rect(h: &mut Harness) {
    h.click("tool-rect");
    drag(h, (96., 96.), (216., 176.));
}

#[test]
fn a_drag_draws_a_shape_the_inspector_shows() {
    let mut h = sketch();
    assert_eq!(h.error(), None);
    draw_rect(&mut h);
    assert_eq!(rect(&mut h), ["96", "96", "120", "80"]);
    assert_eq!(field(&mut h, "name"), "Rectangle 1");
    // A click, not a drag, makes one of the usual size; the tool goes back
    // to selecting.
    h.click("tool-ellipse");
    click_at(&mut h, 400., 300.);
    assert_eq!(rect(&mut h), ["400", "304", "120", "80"], "on the grid");
    assert_eq!(field(&mut h, "name"), "Ellipse 2");
    assert_eq!(count(&mut h), "2 shapes");
    assert_eq!(h.text("title"), "Untitled •");
    assert_eq!(h.error(), None);
}

#[test]
fn moves_snap_to_the_others_edges_unless_snapping_is_off() {
    let mut h = sketch();
    draw_rect(&mut h);
    h.click("tool-rect");
    drag(&mut h, (304., 304.), (368., 352.));
    assert_eq!(rect(&mut h), ["304", "304", "64", "48"]);
    // Moved to 3 points right of the first's left edge: it snaps onto it.
    drag(&mut h, (320., 320.), (320. - 205., 320.));
    assert_eq!(field(&mut h, "x"), "96");
    h.click("snapping");
    drag(&mut h, (112., 320.), (112. + 3., 320.));
    assert_eq!(
        field(&mut h, "x"),
        "99",
        "without snapping it goes where it's put"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn a_handle_resizes_and_undo_puts_it_back() {
    let mut h = sketch();
    draw_rect(&mut h);
    // The bottom right handle, dragged out and snapped to the grid.
    drag(&mut h, (216., 176.), (256., 200.));
    assert_eq!(rect(&mut h), ["96", "96", "160", "104"]);
    h.key_with(Key::Character("z".into()), command());
    assert_eq!(rect(&mut h), ["96", "96", "120", "80"]);
    h.click("redo");
    assert_eq!(rect(&mut h), ["96", "96", "160", "104"]);
    h.click("undo");
    h.click("undo");
    assert_eq!(count(&mut h), "0 shapes", "the drawing undone too");
    assert_eq!(h.error(), None);
}

#[test]
fn the_inspector_edits_what_is_selected() {
    let mut h = sketch();
    draw_rect(&mut h);
    let retype = |h: &mut Harness, id: &str, text: &str| {
        h.click(id);
        for _ in 0..8 {
            h.key(Key::Backspace);
        }
        h.ui.dispatch(InputEvent::Text(text.into()));
        h.settle();
    };
    // Typing applies nothing until Enter, or leaving the field.
    retype(&mut h, "field-w", "200");
    assert_eq!(field(&mut h, "w"), "200", "the field shows what is typed");
    h.click("field-h");
    assert_eq!(field(&mut h, "w"), "200", "applied on leaving it");
    retype(&mut h, "field-h", "50");
    h.key(Key::Enter);
    assert_eq!(rect(&mut h), ["96", "96", "200", "50"]);
    // What isn't a number goes back to the shape's.
    retype(&mut h, "field-x", "left");
    h.key(Key::Enter);
    assert_eq!(field(&mut h, "x"), "96");
    h.click("fill-#33b679");
    assert_eq!(field(&mut h, "fill"), "#33b679");
    // Each applied field is one undo.
    h.click("undo");
    assert_eq!(field(&mut h, "fill"), "#4c8dff");
    h.click("undo");
    assert_eq!(field(&mut h, "h"), "80");
    h.click("undo");
    assert_eq!(field(&mut h, "w"), "120");
    assert_eq!(h.error(), None);
}

#[test]
fn keys_pick_tools_nudge_select_and_delete() {
    let mut h = sketch();
    // Nothing focused: the board's keys reach the app.
    press(&mut h, "r");
    click_at(&mut h, 96., 96.);
    assert_eq!(field(&mut h, "name"), "Rectangle 1");
    h.key(Key::ArrowRight);
    h.key_with(
        Key::ArrowDown,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(rect(&mut h)[..2], ["97", "106"]);
    press(&mut h, "o");
    click_at(&mut h, 400., 96.);
    // Everything, by a rectangle around it, then deleted.
    h.key(Key::Escape);
    drag(&mut h, (60., 60.), (560., 240.));
    assert!(h.texts().contains(&"2 shapes".to_owned()), "both selected");
    h.key(Key::Backspace);
    assert_eq!(count(&mut h), "0 shapes");
    // In a field, keys are the field's.
    h.click("undo");
    h.key_with(Key::Character("a".into()), command());
    assert!(h.texts().contains(&"2 shapes".to_owned()), "⌘A selects all");
    assert_eq!(h.error(), None);
}

#[test]
fn the_wheel_pans_and_zooms_about_the_pointer() {
    let mut h = sketch();
    draw_rect(&mut h);
    h.key(Key::Escape);
    assert_eq!(h.text("zoom"), "100%");
    let (x, y) = at(&mut h, 100., 100.);
    h.ui.dispatch(InputEvent::Pinch { x, y, delta: 0.25 });
    h.frame();
    assert_eq!(h.text("zoom"), "125%");
    // What was under the pointer still is: the shape's corner at (100,
    // 100) is there to click.
    let button = PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown { x, y, button });
    h.ui.dispatch(InputEvent::PointerUp { x, y, button });
    h.settle();
    assert_eq!(field(&mut h, "x"), "96", "selected where it was");
    // ⌘ and the wheel zoom back out; the wheel alone pans.
    h.ui.dispatch_with_modifiers(
        InputEvent::Scroll {
            x,
            y,
            delta_x: 0.,
            delta_y: 22.314,
        },
        Modifiers {
            meta: true,
            ..Default::default()
        },
    );
    h.frame();
    assert_eq!(h.text("zoom"), "100%");
    h.key(Key::Escape);
    h.ui.dispatch(InputEvent::Scroll {
        x,
        y,
        delta_x: 300.,
        delta_y: 0.,
    });
    h.frame();
    h.ui.dispatch(InputEvent::PointerDown { x, y, button });
    h.ui.dispatch(InputEvent::PointerUp { x, y, button });
    h.settle();
    assert!(
        h.texts().contains(&"Nothing selected".to_owned()),
        "panned away from it"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn a_drawing_saves_and_opens_through_the_dialogs() {
    let dir = std::env::temp_dir().join(format!("zgx-sketch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("drawing.json");
    let mut h = sketch();
    let desktop = h.desktop.clone().unwrap();
    draw_rect(&mut h);
    desktop.save_to(&path);
    assert!(h.host.run_command("save"));
    assert!(
        h.wait_until(|h| h.text("title") == "drawing.json"),
        "saved: {}",
        h.text("title")
    );
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("\"kind\": \"rect\""), "{saved}");
    assert_eq!(
        desktop.asked()[0].file_name.as_deref(),
        Some("Drawing.json")
    );

    assert!(h.host.run_command("new"));
    h.settle();
    assert_eq!(count(&mut h), "0 shapes");
    assert_eq!(h.text("title"), "Untitled");
    desktop.pick([&path]);
    assert!(h.host.run_command("open"));
    assert!(h.wait_until(|h| h.text("title") == "drawing.json"));
    assert_eq!(count(&mut h), "1 shape");
    click_at(&mut h, 150., 130.);
    assert_eq!(rect(&mut h), ["96", "96", "120", "80"]);
    // What was opened is its own history: nothing to undo.
    h.click("undo");
    assert_eq!(field(&mut h, "w"), "120");
    // A file that isn't a drawing says so, and changes nothing.
    std::fs::write(dir.join("other.json"), "{ not json").unwrap();
    desktop.pick([dir.join("other.json")]);
    assert!(h.host.run_command("open"));
    assert!(h.wait_until(|h| h.text("status") == "Not a Sketch drawing"));
    assert_eq!(count(&mut h), "1 shape");
    let _ = std::fs::remove_dir_all(&dir);
    std::thread::sleep(Duration::from_millis(1));
    assert_eq!(h.error(), None);
}

#[test]
#[ignore = "a timing probe: cargo test --release -- --ignored --nocapture"]
fn how_long_a_busy_drawing_takes() {
    let dir = std::env::temp_dir().join(format!("zgx-sketch-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("busy.json");
    let shapes: Vec<String> = (0..500)
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
    let mut h = sketch();
    let desktop = h.desktop.clone().unwrap();
    desktop.pick([&path]);
    let start = std::time::Instant::now();
    assert!(h.host.run_command("open"));
    assert!(h.wait_until(|h| h.text("title") == "busy.json"));
    eprintln!("open 500 shapes: {:?}", start.elapsed());
    let button = PointerButton::Primary;
    let time = |h: &mut Harness, what: &str, from: (f32, f32), steps: usize| {
        h.ui.dispatch(InputEvent::PointerDown {
            x: from.0,
            y: from.1,
            button,
        });
        h.frame();
        let calls = h.calls();
        let start = std::time::Instant::now();
        for i in 1..=steps {
            h.ui.dispatch(InputEvent::PointerMove {
                x: from.0 + i as f32 * 2.,
                y: from.1 + i as f32,
            });
            h.frame();
        }
        eprintln!(
            "{what}: {:?} a step, {} script calls",
            start.elapsed() / steps as u32,
            (h.calls() - calls) / steps as u64
        );
        h.ui.dispatch(InputEvent::PointerUp {
            x: from.0,
            y: from.1,
            button,
        });
        h.settle();
    };
    let p = at(&mut h, 16., 16.);
    time(&mut h, "move one shape", p, 40);
    let corner = at(&mut h, 32., 32.);
    time(&mut h, "resize by a handle", corner, 40);
    h.key(Key::Escape);
    let empty = at(&mut h, -60., -50.);
    time(&mut h, "select by rectangle", empty, 40);
    h.key_with(Key::Character("a".into()), command());
    let p = at(&mut h, 56., 56.);
    time(&mut h, "move all 500", p, 40);
    let calls = h.calls();
    let start = std::time::Instant::now();
    for _ in 0..40 {
        h.ui.dispatch(InputEvent::Scroll {
            x: p.0,
            y: p.1,
            delta_x: 3.,
            delta_y: 2.,
        });
        h.frame();
    }
    eprintln!(
        "pan: {:?} a step, {} script calls",
        start.elapsed() / 40,
        (h.calls() - calls) / 40
    );
    eprintln!("error: {:?}", h.error());
    let _ = std::fs::remove_dir_all(&dir);
}
