use zgui::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    widgets::{EditorHandle, Ui},
};

fn fixture(text: &str, width: f32, wrap: bool) -> (Ui, EditorHandle) {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        text_area("Document", ui.signal(text.to_owned()))
            .size(width, 160.)
            .p(10.)
            .text_size(10.)
            .line_height(20.)
            .text_wrap(wrap),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    (ui, editor)
}
fn down(ui: &mut Ui, x: f32, y: f32, shift: bool) {
    ui.dispatch_with_modifiers(
        InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        },
        Modifiers {
            shift,
            ..Modifiers::default()
        },
    );
}
fn up(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch_with_modifiers(
        InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        },
        Modifiers::default(),
    );
}
fn click(ui: &mut Ui, x: f32, y: f32, shift: bool) {
    down(ui, x, y, shift);
    up(ui, x, y);
}
fn move_to(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch_with_modifiers(InputEvent::PointerMove { x, y }, Modifiers::default());
}

#[test]
fn shift_click_and_drag_keep_existing_anchor_while_legacy_clicks_collapse() {
    let (mut ui, editor) = fixture("abcdefghij", 200., false);
    click(&mut ui, 22., 20., false);
    assert_eq!(editor.editor.borrow().selection().focus, 2);
    down(&mut ui, 58., 20., true);
    assert_eq!(editor.editor.borrow().selection().anchor, 2);
    assert_eq!(editor.editor.borrow().selection().focus, 8);
    move_to(&mut ui, 10., 20.);
    assert_eq!(editor.editor.borrow().selection().anchor, 2);
    assert_eq!(editor.editor.borrow().selection().focus, 0);
    up(&mut ui, 10., 20.);
    for _ in 0..3 {
        ui.dispatch(InputEvent::PointerDown {
            x: 40.,
            y: 20.,
            button: PointerButton::Primary,
        });
        ui.dispatch(InputEvent::PointerUp {
            x: 40.,
            y: 20.,
            button: PointerButton::Primary,
        });
        let selection = editor.editor.borrow().selection();
        assert_eq!(selection.anchor, 5);
        assert_eq!(selection.focus, 5);
    }
}

#[test]
fn double_click_drag_extends_whole_words_and_reverses_around_original_word() {
    let (mut ui, editor) = fixture("one two three", 200., false);
    click(&mut ui, 40., 20., false);
    down(&mut ui, 40., 20., false);
    assert_eq!(editor.copy(), "two");
    move_to(&mut ui, 76., 20.);
    assert_eq!(editor.copy(), "two three");
    assert_eq!(editor.editor.borrow().selection().anchor, 4);
    move_to(&mut ui, 16., 20.);
    assert_eq!(editor.copy(), "one two");
    assert_eq!(editor.editor.borrow().selection().anchor, 7);
    assert_eq!(editor.editor.borrow().selection().focus, 0);
    up(&mut ui, 16., 20.);
    move_to(&mut ui, 88., 20.);
    assert_eq!(
        editor.copy(),
        "one two",
        "released drag must stop selecting"
    );
}

#[test]
fn double_click_selects_complete_unicode_graphemes_and_separator_segments() {
    for (text, x, expected) in [
        ("cafe\u{301} world", 22., "cafe\u{301}"),
        ("👩\u{200d}💻 ok", 10., "👩\u{200d}💻"),
        ("hello   world", 46., "   "),
        ("hello! world", 40., "!"),
    ] {
        let (mut ui, editor) = fixture(text, 200., false);
        click(&mut ui, x, 20., false);
        click(&mut ui, x, 20., false);
        assert_eq!(editor.copy(), expected, "{text}");
        assert_eq!(editor.value().get(), text);
    }
}

#[test]
fn triple_click_drag_selects_visual_lines_without_consuming_next_wrap_row() {
    let (mut ui, editor) = fixture("abcdefghijklmnopqrst", 50., true);
    click(&mut ui, 22., 40., false);
    click(&mut ui, 22., 40., false);
    down(&mut ui, 22., 40., false);
    assert_eq!(editor.copy(), "fghij");
    move_to(&mut ui, 22., 60.);
    assert_eq!(editor.copy(), "fghijklmno");
    move_to(&mut ui, 22., 20.);
    assert_eq!(editor.copy(), "abcdefghij");
    assert_eq!(editor.editor.borrow().selection().anchor, 10);
    assert_eq!(editor.editor.borrow().selection().focus, 0);
    up(&mut ui, 22., 20.);
    let (mut ui, editor) = fixture("first\nsecond\nthird", 200., false);
    for _ in 0..3 {
        click(&mut ui, 22., 40., false);
    }
    assert_eq!(editor.copy(), "second", "hard newline is excluded");
}

#[test]
fn pointer_selection_is_read_only_safe_cancels_composition_and_preserves_history() {
    let (mut ui, editor) = fixture("one two", 200., false);
    editor.paste("prefix ");
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.dispatch(InputEvent::ImePreedit {
        text: "中".into(),
        cursor: Some((3, 3)),
    });
    let epoch = editor.editor.borrow().composition_cancel_revision();
    click(&mut ui, 16., 20., false);
    assert!(editor.editor.borrow().preedit().is_none());
    assert_ne!(editor.editor.borrow().composition_cancel_revision(), epoch);
    editor.set_read_only(true);
    click(&mut ui, 16., 20., false);
    assert_eq!(editor.copy(), "prefix");
    assert_eq!(editor.value().get(), "prefix one two");
    editor.set_read_only(false);
    assert!(editor.editor.borrow_mut().undo());
    assert_eq!(editor.editor.borrow().text(), "one two");
    assert!(editor.editor.borrow_mut().redo());
    assert_eq!(editor.editor.borrow().text(), "prefix one two");
}

#[test]
fn word_selection_uses_the_clicked_glyph_on_both_sides_of_caret_midpoints() {
    for (x, expected) in [(38., "hello"), (44., " "), (50., "world")] {
        let (mut ui, editor) = fixture("hello world", 200., false);
        click(&mut ui, x, 20., false);
        click(&mut ui, x, 20., false);
        assert_eq!(editor.copy(), expected, "pointer x={x}");
    }
}

#[test]
fn single_line_triple_click_selects_all_and_cancel_stops_unit_drag() {
    let mut ui = Ui::new(400., 200.);
    let view = ui.mount(
        text_input("Single", ui.signal("one two three".into()))
            .size(200., 40.)
            .p(10.)
            .text_size(10.)
            .line_height(20.),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    for _ in 0..2 {
        click(&mut ui, 40., 20., false);
    }
    down(&mut ui, 40., 20., false);
    assert_eq!(editor.copy(), "one two three");
    ui.dispatch_with_modifiers(InputEvent::PointerCancel, Modifiers::default());
    assert!(ui.input.captured().is_none());
    move_to(&mut ui, 10., 20.);
    assert_eq!(editor.copy(), "one two three");
}
