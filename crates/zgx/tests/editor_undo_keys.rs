//! Editors undo typing and deleting in runs, and take the platform's line
//! deletion and (on macOS) Control keys.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};

fn field(text: &str) -> (Ui, zgx::reactive::Signal<String>) {
    field_sized(text, 300.)
}
fn field_sized(text: &str, width: f32) -> (Ui, zgx::reactive::Signal<String>) {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(text.to_owned());
    let view = ui.mount(text_area("Notes", value.clone()).size(width, 120.));
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
    // Control+A to the paragraph's start, Control+K to its end.
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

/// Control-A and Control-E go to a wrapped paragraph's edges, not its row's,
/// as NSTextView's `moveToBeginningOfParagraph:` does; Control-K kills to
/// the paragraph's end, kills in a row adding up, and Control-Y yanks them
/// back; Control-T swaps the characters around the caret.
#[cfg(target_os = "macos")]
#[test]
fn macos_control_keys_go_by_paragraphs_kill_yank_and_transpose() {
    let text = "a long first paragraph that wraps onto more rows than one\nnext";
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let at = |ui: &Ui| ui.focused_editor().unwrap().editor.borrow().selection();
    // In a narrow field, from a later row of the first paragraph.
    let (mut ui, value) = field_sized(text, 90.);
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(40, 40);
    editor.refresh();
    ui.prepare_frame();
    key(&mut ui, Key::Character("a".into()), control);
    assert_eq!(at(&ui).focus, 0, "the paragraph's start, rows above");
    key(&mut ui, Key::Character("e".into()), control);
    assert_eq!(at(&ui).focus, 57, "the paragraph's end, rows below");
    let shift = Modifiers {
        shift: true,
        ..control
    };
    key(&mut ui, Key::Character("a".into()), shift);
    let selection = at(&ui);
    assert_eq!(
        (selection.anchor, selection.focus),
        (57, 0),
        "Shift extends"
    );

    // Two kills in a row: the paragraph's rest, then its break.
    editor.editor.borrow_mut().set_selection(7, 7);
    editor.refresh();
    key(&mut ui, Key::Character("k".into()), control);
    key(&mut ui, Key::Character("k".into()), control);
    assert_eq!(value.get(), "a long next");
    key(&mut ui, Key::Character("e".into()), control);
    key(&mut ui, Key::Character("y".into()), control);
    let yanked = format!("a long next{}", &text[7..58]);
    assert_eq!(value.get(), yanked, "both kills yanked back together");
    // A kill after other edits (the yank) starts the buffer again.
    editor.editor.borrow_mut().set_selection(1, 1);
    editor.refresh();
    key(&mut ui, Key::Character("k".into()), control);
    assert_eq!(value.get(), "a\n");
    key(&mut ui, Key::Character("y".into()), control);
    assert_eq!(value.get(), yanked, "only the last kill yanked");

    // Control-T at the end swaps the two before it, undone in one step.
    let (mut ui, value) = field("ab");
    key(&mut ui, Key::Character("t".into()), control);
    assert_eq!(value.get(), "ba");
    let meta = Modifiers {
        meta: true,
        ..Modifiers::default()
    };
    key(&mut ui, Key::Character("z".into()), meta);
    assert_eq!(value.get(), "ab");

    // A password's kill deletes it and keeps nothing to yank.
    let mut ui = Ui::new(400., 200.);
    let secret = ui.signal("hunter2".to_owned());
    let view = ui.mount(
        text_input("Password", secret.clone())
            .secure(true)
            .size(200., 30.),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    key(&mut ui, Key::Character("k".into()), control);
    assert_eq!(secret.get(), "");
    key(&mut ui, Key::Character("y".into()), control);
    assert!(!secret.get().contains("hunter2"), "{}", secret.get());
}

/// Control-Up and Control-Down go to paragraphs' starts on Windows and
/// Linux (Chrome there, and Windows' edit controls), not a row up or down,
/// Shift extending.
#[cfg(not(target_os = "macos"))]
#[test]
fn control_arrows_go_to_paragraph_starts_off_macos() {
    let text = "a long first paragraph that wraps onto more rows than one\nnext\nlast";
    let (mut ui, _) = field_sized(text, 90.);
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(3, 3);
    editor.refresh();
    ui.prepare_frame();
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let at = |ui: &Ui| ui.focused_editor().unwrap().editor.borrow().selection();
    key(&mut ui, Key::ArrowDown, control);
    assert_eq!(at(&ui).focus, 58, "past the first paragraph's rows");
    key(&mut ui, Key::ArrowDown, control);
    assert_eq!(at(&ui).focus, 63, "the last paragraph's start");
    key(&mut ui, Key::ArrowDown, control);
    assert_eq!(at(&ui).focus, 67, "then the text's end");
    key(&mut ui, Key::ArrowUp, control);
    assert_eq!(at(&ui).focus, 63, "back to its paragraph's start");
    key(&mut ui, Key::ArrowUp, control);
    assert_eq!(at(&ui).focus, 58, "then the one before's");
    let shift = Modifiers {
        shift: true,
        ..control
    };
    key(&mut ui, Key::ArrowUp, shift);
    let selection = at(&ui);
    assert_eq!(
        (selection.anchor, selection.focus),
        (58, 0),
        "Shift extends"
    );
}

/// The word step on: Windows' (Control-Right, Control-Delete) goes past the
/// spaces to the next word's start, then a line's end, then over its break;
/// macOS's Option and Linux's Control stop at each word's end.
#[test]
fn the_word_step_on_stops_where_the_platform_does() {
    let (mut ui, value) = field("one two  three\nfour");
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    let word = Modifiers {
        alt: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Modifiers::default()
    };
    let stops: Vec<usize> = (0..4)
        .map(|_| {
            key(&mut ui, Key::ArrowRight, word);
            editor.editor.borrow().selection().focus
        })
        .collect();
    let want = if cfg!(windows) {
        [4, 9, 14, 15]
    } else {
        [3, 7, 14, 19]
    };
    assert_eq!(stops, want);
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    key(&mut ui, Key::Delete, word);
    let want = if cfg!(windows) {
        "two  three\nfour"
    } else {
        " two  three\nfour"
    };
    assert_eq!(value.get(), want);
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
