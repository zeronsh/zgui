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

/// The platform's word keys (Option on macOS, Control elsewhere) move by
/// words and delete back a word, through the editor's keys.
#[test]
fn word_keys_move_and_delete_by_words() {
    let (mut ui, value) = field("one two three");
    let word = Modifiers {
        alt: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Modifiers::default()
    };
    let at = |ui: &Ui| ui.focused_editor().unwrap().editor.borrow().selection();
    key(&mut ui, Key::ArrowLeft, word);
    assert_eq!(at(&ui).focus, 8, "the start of the last word");
    key(
        &mut ui,
        Key::ArrowLeft,
        Modifiers {
            shift: true,
            ..word
        },
    );
    let selection = at(&ui);
    assert_eq!(
        (selection.anchor, selection.focus),
        (8, 4),
        "Shift extends a word"
    );
    key(&mut ui, Key::ArrowRight, word);
    key(&mut ui, Key::ArrowRight, word);
    assert_eq!(at(&ui).focus, 13, "to the end of the last");
    key(&mut ui, Key::Backspace, word);
    assert_eq!(value.get(), "one two ", "a word deleted back");
}

/// ⌘-arrows go to the line's and the text's ends on macOS.
#[cfg(target_os = "macos")]
#[test]
fn command_arrows_go_to_the_line_and_text_ends_on_macos() {
    let (mut ui, _) = field("first line\nsecond line");
    let at = |ui: &Ui| {
        ui.focused_editor()
            .unwrap()
            .editor
            .borrow()
            .selection()
            .focus
    };
    let meta = Modifiers {
        meta: true,
        ..Modifiers::default()
    };
    key(&mut ui, Key::ArrowUp, meta);
    assert_eq!(at(&ui), 0, "the text's start");
    key(&mut ui, Key::ArrowRight, meta);
    assert_eq!(at(&ui), 10, "the line's end");
    key(&mut ui, Key::ArrowDown, meta);
    assert_eq!(at(&ui), 22, "the text's end");
    key(&mut ui, Key::ArrowLeft, meta);
    assert_eq!(at(&ui), 11, "the line's start");
}

/// A single character pasted is a step of its own, not part of the typing
/// before it, as AppKit undoes a paste.
#[test]
fn a_one_character_paste_undoes_apart_from_typing() {
    let (mut ui, value) = field("");
    for c in "ab".chars() {
        ui.dispatch(InputEvent::Text(c.to_string()));
    }
    ui.focused_editor().unwrap().paste("c");
    assert_eq!(value.get(), "abc");
    key(&mut ui, Key::Character("z".into()), primary());
    assert_eq!(value.get(), "ab", "the paste undone alone");
}
