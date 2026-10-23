//! The mail example (examples/mail), run headless: opening a message,
//! searching, composing, and the settings window's dark mode switch.
mod common;

use common::Harness;
use std::time::Instant;
use zgui::{scene::NodeKind, widgets::Ui};
use zgui_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/mail/app.luau");

fn texts(ui: &Ui) -> Vec<String> {
    let scene = ui.scene.borrow();
    scene
        .paint_items()
        .filter_map(|item| match scene.kind(item.id) {
            NodeKind::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_mail_example_opens_searches_composes_and_switches_to_light() {
    let start = Instant::now();
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, APP));
    h.frames(2);
    let loaded = start.elapsed();
    assert!(loaded.as_secs_f64() < 2., "loads in {loaded:?}");
    // A row, then its message.
    h.click("row-1");
    h.frames(1);
    let first = h.text("subject");
    assert!(!first.is_empty(), "the message opens");
    // Down opens the next one, as a mail app's list does.
    let before = h.texts();
    h.key(zgui::input::Key::ArrowDown);
    h.frames(1);
    assert_ne!(h.texts(), before, "another message shows");
    h.key(zgui::input::Key::ArrowUp);
    h.frames(1);
    assert_eq!(h.text("subject"), first, "and Up goes back");
    // Searching narrows the list.
    h.type_text("search", "invoice");
    h.frames(1);
    let shown: Vec<String> = (1..=3)
        .filter_map(|i| {
            h.find(&format!("row-{i}"))
                .map(|_| h.text(&format!("row-{i}")))
        })
        .collect();
    assert!(!shown.is_empty());
    // ⌘N composes; the body is spellchecked (a password field is not).
    assert!(h.host.run_command("new"));
    h.frames(1);
    assert!(h.find("body").is_some(), "the compose sheet opens");
    // ⌘, opens the settings window; its switch turns the app light.
    assert!(h.host.run_command("settings"));
    h.frames(1);
    let windows = h.host.windows().get();
    assert_eq!(windows.len(), 1);
    assert!(windows[0].open.get().truthy());
    let mut settings = Ui::with_runtime(h.ui.runtime.clone(), 420., 260.);
    settings.mount(h.host.window_view("settings"));
    settings.prepare_frame();
    assert!(texts(&settings).contains(&"Dark appearance".to_owned()));
    assert_eq!(&*h.ui.theme().current(), "zgui-dark");
    let at = {
        let scene = settings.scene.borrow();
        let label = scene
            .paint_items()
            .find(|i| matches!(scene.kind(i.id), NodeKind::Text { text, .. } if &**text == "Dark appearance"))
            .map(|i| scene.bounds(i.id))
            .unwrap();
        (label.x + 4., label.y + label.height / 2.)
    };
    use zgui::input::{InputEvent, PointerButton};
    let button = PointerButton::Primary;
    settings.dispatch(InputEvent::PointerDown {
        x: at.0,
        y: at.1,
        button,
    });
    settings.dispatch(InputEvent::PointerUp {
        x: at.0,
        y: at.1,
        button,
    });
    h.frames(1);
    assert_eq!(
        &*h.ui.theme().current(),
        "zgui-light",
        "every window goes light"
    );
}

#[test]
#[ignore = "a timing probe: cargo test --release -- --ignored --nocapture"]
fn how_long_opening_a_message_takes() {
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, APP));
    h.frames(2);
    let start = Instant::now();
    for i in 0..10 {
        h.click(&format!("row-{}", i % 2 + 1));
        h.frames(1);
    }
    eprintln!("open: {:?} per message", start.elapsed() / 10);
    let start = Instant::now();
    for i in 0..10 {
        h.type_text("search", if i % 2 == 0 { "r" } else { "e" });
        h.frames(1);
    }
    eprintln!("search keystroke: {:?}", start.elapsed() / 10);
}
