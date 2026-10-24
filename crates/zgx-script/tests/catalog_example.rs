//! The component catalogue (examples/catalog), run headless: each section
//! works from the pointer and the keyboard, in both directions and themes.
mod common;

use common::Harness;
use zgx::input::{InputEvent, Key, Modifiers, PointerButton};
use zgx_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/catalog/app.luau");

fn catalog() -> Harness {
    let h = Harness::new(|runtime| ScriptHost::new(runtime, APP));
    // Tall enough for every section without scrolling.
    h.ui.scene.borrow_mut().resize(980., 3000.);
    h.frames(2);
    h
}

#[test]
fn the_catalogue_runs_in_both_directions_and_themes() {
    let mut h = catalog();
    assert_eq!(h.error(), None);
    h.click("rtl");
    h.click("dark");
    h.frames(2);
    assert_eq!(h.error(), None);
    assert_eq!(&*h.ui.theme().current(), "zgx-light");
}

#[test]
fn its_controls_answer_the_keyboard() {
    let mut h = catalog();
    // The segmented control: focus its chosen option, then arrow along.
    let range = h.find("range").unwrap();
    let week = h.ui.scene.borrow().children(range)[1];
    h.ui.input.focus(&h.ui.scene, Some(week));
    h.key(Key::ArrowRight);
    assert!(h.texts().contains(&"Showing a month".to_owned()));
    // The stepper.
    let copies = h.find("copies").unwrap();
    let field = h.ui.scene.borrow().children(copies)[0];
    h.ui.input.focus(&h.ui.scene, Some(field));
    h.key(Key::PageUp);
    assert!(h.texts().contains(&"12 copies".to_owned()));
    // Indeterminate progress, and back.
    h.click("busy");
    assert!(h.texts().contains(&"Determinate".to_owned()));
    assert_eq!(h.error(), None);
}

#[test]
fn its_overlays_open_and_close_as_described() {
    let mut h = catalog();
    h.click("view-menu");
    h.frames(2);
    let shortcut = if cfg!(target_os = "macos") {
        "⌥Z"
    } else {
        "Alt+Z"
    };
    assert!(
        h.texts().contains(&shortcut.to_owned()),
        "the shortcut shows"
    );
    h.click("size-large");
    h.click("view-menu");
    h.frames(2);
    let large = h.find("size-large").unwrap();
    assert_eq!(
        h.ui.semantics.borrow().get(large).unwrap().checked,
        Some(true),
        "the radio item chosen"
    );
    h.key(Key::Escape);
    // The dialog a click outside doesn't close.
    h.click("open-dialog");
    h.frames(2);
    assert!(h.texts().contains(&"Discard changes?".to_owned()));
    let button = PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown {
        x: 5.,
        y: 5.,
        button,
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x: 5.,
        y: 5.,
        button,
    });
    h.frames(2);
    assert!(
        h.texts().contains(&"Discard changes?".to_owned()),
        "still open"
    );
    h.key(Key::Escape);
    h.frames(2);
    // Shift+F10 opens the file's context menu.
    let file = h.find("file").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(file));
    h.key_with(
        Key::Function(10),
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    h.frames(2);
    assert!(h.texts().contains(&"Rename".to_owned()));
    assert_eq!(h.error(), None);
}
