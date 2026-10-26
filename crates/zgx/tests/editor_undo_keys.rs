//! Editors undo typing and deleting in runs, and take the platform's line
//! deletion and (on macOS) Control keys.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};

fn field(text: &str) -> (Ui, zgx::reactive::Signal<String>) {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(text.to_owned());
    let view = ui.mount(text_area("Notes", value.clone()).size(300., 120.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    (ui, value)
}
fn key(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
}
fn primary() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Modifiers::default()
    }
}

#[test]
fn typed_words_undo_as_one_run() {
    let (mut ui, value) = field("");
    for c in "two words".chars() {
        ui.dispatch(InputEvent::Text(c.to_string()));
    }
    assert_eq!(value.get(), "two words");
    key(&mut ui, Key::Character("z".into()), primary());
    assert_eq!(value.get(), "", "one undo takes the run of typing");
    key(
        &mut ui,
        Key::Character("z".into()),
        Modifiers {
            shift: true,
            ..primary()
        },
    );
    assert_eq!(value.get(), "two words");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_line_deletion_and_control_keys() {
    let (mut ui, value) = field("first line\nsecond line");
    let meta = Modifiers {
        meta: true,
        ..Modifiers::default()
    };
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    // The caret is at the end: ⌘⌫ deletes to the line's start.
    key(&mut ui, Key::Backspace, meta);
    assert_eq!(value.get(), "first line\n");
    key(&mut ui, Key::Character("z".into()), meta);
    assert_eq!(value.get(), "first line\nsecond line");
    // Control+A to the line's start, Control+K to its end.
    key(&mut ui, Key::Character("a".into()), control);
    key(&mut ui, Key::Character("f".into()), control);
    key(&mut ui, Key::Character("k".into()), control);
    assert_eq!(value.get(), "first line\ns");
    key(&mut ui, Key::Character("h".into()), control);
    assert_eq!(value.get(), "first line\n");
}

/// Option-Up and Option-Down go by paragraphs on macOS, Shift extending, as
/// AppKit's text views do.
#[cfg(target_os = "macos")]
#[test]
fn option_arrows_move_by_paragraphs_on_macos() {
    let (mut ui, value) = field("one\ntwo three\nfour");
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(8, 8);
    editor.refresh();
    let option = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    let focus = |ui: &Ui| ui.focused_editor().unwrap().editor.borrow().selection();
    key(&mut ui, Key::ArrowUp, option);
    assert_eq!(focus(&ui).focus, 4, "the paragraph's start");
    key(&mut ui, Key::ArrowUp, option);
    assert_eq!(focus(&ui).focus, 0, "then the one before's");
    key(&mut ui, Key::ArrowDown, option);
    key(
        &mut ui,
        Key::ArrowDown,
        Modifiers {
            shift: true,
            ..option
        },
    );
    let selection = focus(&ui);
    assert_eq!(
        (selection.anchor, selection.focus),
        (3, 13),
        "Shift extends"
    );
    assert_eq!(value.get(), "one\ntwo three\nfour", "nothing typed");
}
