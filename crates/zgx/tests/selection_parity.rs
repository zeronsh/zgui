//! Text selection and editing as Chrome does them, case by case: where a
//! click, a key or a gesture puts the caret and the selection.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    widgets::{EditorHandle, Ui},
};

fn editor_in(build: impl FnOnce(&Ui) -> View) -> (Ui, EditorHandle) {
    let mut ui = Ui::new(600., 400.);
    let view = build(&ui);
    let handle = ui.mount(view);
    ui.prepare_frame();
    let node = handle.find("ed").expect("an editor `ed`");
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    (ui, editor)
}
fn area(ui: &Ui, text: &str, width: f32) -> View {
    text_area("Notes", ui.signal(text.to_owned()))
        .id("ed")
        .size(width, 160.)
        .text_size(10.)
        .line_height(20.)
        .text_wrap(true)
}
fn press(ui: &mut Ui, x: f32, y: f32, clicks: usize) {
    for _ in 0..clicks {
        ui.dispatch_with_modifiers(
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            },
            Modifiers::default(),
        );
        ui.dispatch_with_modifiers(
            InputEvent::PointerUp {
                x,
                y,
                button: PointerButton::Primary,
            },
            Modifiers::default(),
        );
    }
}
fn key(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
}
fn selection(editor: &EditorHandle) -> (usize, usize) {
    let s = editor.editor.borrow().selection();
    (s.anchor.min(s.focus), s.anchor.max(s.focus))
}
fn selected(editor: &EditorHandle) -> String {
    let (a, b) = selection(editor);
    editor.editor.borrow().text()[a..b].to_owned()
}
/// Where the caret goes for a click at `x` in `text`'s first line.
fn caret_at(text: &str, x: f32) -> usize {
    let (mut ui, editor) = editor_in(|ui| area(ui, text, 300.));
    press(&mut ui, x, 10., 1);
    editor.editor.borrow().selection().focus
}
/// The word a double click at the start of `word` in `text` selects.
fn word_at(text: &str, word: &str) -> String {
    let at = text.find(word).unwrap();
    // A click inside the word's first character: the first x whose caret
    // lands past its start (past the character's middle).
    let x = (0..500)
        .map(|x| x as f32)
        .find(|x| caret_at(text, *x) > at)
        .expect("the word is on the first line");
    let (mut ui, editor) = editor_in(|ui| area(ui, text, 500.));
    press(&mut ui, x, 10., 2);
    selected(&editor)
}

// 1. A click in an editor inside a zoomed node lands where it shows.
#[test]
fn a_click_in_a_zoomed_editor_puts_the_caret_where_it_shows() {
    let text = "abcdefghijklmnop";
    for x in [12., 33., 51.] {
        let plain = caret_at(text, x);
        let (mut ui, editor) = editor_in(|ui| column().zoom(2.).child(area(ui, text, 300.)));
        press(&mut ui, x * 2., 20., 1);
        assert_eq!(
            editor.editor.borrow().selection().focus,
            plain,
            "a click at {x} shown at {}",
            x * 2.
        );
    }
}

// 2. Double click: words as Chrome's (ICU) word breaks make them.
#[test]
fn a_double_click_selects_the_word_chrome_does() {
    assert_eq!(word_at("say don't go", "don't"), "don't");
    assert_eq!(word_at("pi is 3.14 ok", "3.14"), "3.14");
    assert_eq!(word_at("a 1,000 b", "1,000"), "1,000");
    assert_eq!(word_at("call foo_bar now", "foo_bar"), "foo_bar");
    assert_eq!(word_at("see example.com now", "example.com"), "example.com");
}

// 3. Double click in Chinese or Japanese: a dictionary word, as Chrome's.
#[test]
fn a_double_click_in_chinese_selects_a_dictionary_word() {
    assert_eq!(word_at("我喜欢编程语言", "编程"), "编程");
}

// 4. Ctrl+arrow on Windows stops at the start of the next word; Linux
// (Chrome there, GTK) stops at its end, as macOS's Option does.
#[cfg(not(target_os = "macos"))]
#[test]
fn control_right_stops_where_the_platform_does() {
    let text = "one two three";
    let (mut ui, editor) = editor_in(|ui| area(ui, text, 300.));
    let word = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    key(&mut ui, Key::ArrowRight, word);
    let want = if cfg!(windows) { 4 } else { 3 };
    assert_eq!(editor.editor.borrow().selection().focus, want);
}

// 5. Ctrl+Up/Down on Windows and Linux move by paragraphs, as Option
// does on macOS.
#[cfg(not(target_os = "macos"))]
#[test]
fn control_up_and_down_move_by_paragraphs() {
    let text = "first paragraph\nsecond one here\nthird";
    let (mut ui, editor) = editor_in(|ui| area(ui, text, 300.));
    let at = |e: &EditorHandle| e.editor.borrow().selection().focus;
    editor.editor.borrow_mut().set_selection(20, 20);
    editor.refresh();
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    key(&mut ui, Key::ArrowDown, control);
    assert_eq!(at(&editor), 32, "the start of the next paragraph");
    key(&mut ui, Key::ArrowUp, control);
    assert_eq!(at(&editor), 16, "the start of the paragraph");
}

// 6. macOS: Control-A and Control-E go to the paragraph's edges, not the
// wrapped row's (NSTextView's moveToBeginningOfParagraph).
#[cfg(target_os = "macos")]
#[test]
fn control_a_and_e_go_to_the_paragraphs_edges_on_macos() {
    let text = "a long paragraph that wraps onto a second row in a narrow field";
    let (mut ui, editor) = editor_in(|ui| area(ui, text, 80.));
    let at = |e: &EditorHandle| e.editor.borrow().selection().focus;
    // In the middle, on a later row.
    editor.editor.borrow_mut().set_selection(40, 40);
    editor.refresh();
    ui.prepare_frame();
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    key(&mut ui, Key::Character("a".into()), control);
    assert_eq!(at(&editor), 0, "the paragraph's start");
    key(&mut ui, Key::Character("e".into()), control);
    assert_eq!(at(&editor), text.len(), "the paragraph's end");
}
