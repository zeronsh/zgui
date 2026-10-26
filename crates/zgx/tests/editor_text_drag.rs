//! Selected text drags as AppKit's text views and browsers drag it: within
//! an editor it moves (one undo step), into another it moves too (Option on
//! macOS, Control elsewhere, copies), out of the window as plain text, and a
//! press on the selection that does not move puts the caret there.
use zgx::{
    compose::prelude::*,
    input::{DragItem, DraggedText, InputEvent, Modifiers, PointerButton},
    widgets::{EditorHandle, Ui},
};

/// Characters 6 points wide, 10 in from the field's edge; lines 20 high.
fn x_of(char: usize) -> f32 {
    10. + char as f32 * 6. + 1.
}
fn field(ui: &Ui, id: &str, text: &str) -> View {
    text_area(id, ui.signal(text.to_owned()))
        .id(id)
        .size(300., 60.)
        .p(10.)
        .text_size(10.)
        .line_height(20.)
}
fn editor(ui: &Ui, view: &zgx::compose::ViewHandle, id: &str) -> EditorHandle {
    ui.editor(view.find(id).unwrap()).unwrap()
}
fn pointer(ui: &mut Ui, event: InputEvent, modifiers: Modifiers) {
    ui.dispatch_with_modifiers(event, modifiers);
    ui.prepare_frame();
}
fn drag(ui: &mut Ui, from: (f32, f32), to: (f32, f32), modifiers: Modifiers) {
    let button = PointerButton::Primary;
    pointer(
        ui,
        InputEvent::PointerDown {
            x: from.0,
            y: from.1,
            button,
        },
        Modifiers::default(),
    );
    pointer(
        ui,
        InputEvent::PointerMove {
            x: from.0 + 8.,
            y: from.1,
        },
        Modifiers::default(),
    );
    pointer(ui, InputEvent::PointerMove { x: to.0, y: to.1 }, modifiers);
    pointer(
        ui,
        InputEvent::PointerUp {
            x: to.0,
            y: to.1,
            button,
        },
        modifiers,
    );
}
fn select(editor: &EditorHandle, range: std::ops::Range<usize>) {
    editor
        .editor
        .borrow_mut()
        .set_selection(range.start, range.end);
    editor.refresh();
}

#[test]
fn a_selection_dragged_within_an_editor_moves_in_one_undo_step() {
    let mut ui = Ui::new(400., 300.);
    let a = field(&ui, "a", "one two three");
    let view = ui.mount(column().child(a));
    ui.prepare_frame();
    let a = editor(&ui, &view, "a");
    select(&a, 4..7);
    drag(
        &mut ui,
        (x_of(5), 20.),
        (x_of(13), 20.),
        Modifiers::default(),
    );
    assert_eq!(a.value().get(), "one  threetwo");
    assert_eq!(a.copy(), "two", "the moved text selected");
    assert!(a.editor.borrow_mut().undo());
    assert_eq!(
        a.editor.borrow().text(),
        "one two three",
        "one step puts it back"
    );
}

#[test]
fn a_press_on_the_selection_let_go_puts_the_caret_there() {
    let mut ui = Ui::new(400., 300.);
    let a = field(&ui, "a", "one two three");
    let view = ui.mount(column().child(a));
    ui.prepare_frame();
    let a = editor(&ui, &view, "a");
    select(&a, 4..7);
    let button = PointerButton::Primary;
    pointer(
        &mut ui,
        InputEvent::PointerDown {
            x: x_of(5),
            y: 20.,
            button,
        },
        Modifiers::default(),
    );
    assert_eq!(a.selection(), (4, 7), "the selection stays while pressed");
    pointer(
        &mut ui,
        InputEvent::PointerUp {
            x: x_of(5),
            y: 20.,
            button,
        },
        Modifiers::default(),
    );
    assert_eq!(a.selection(), (5, 5));
    assert_eq!(a.value().get(), "one two three");
}

#[test]
fn text_dragged_into_another_editor_moves_unless_copied_and_leaves_as_text() {
    for copy in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let (a, b) = (field(&ui, "a", "alpha beta"), field(&ui, "b", "x"));
        let view = ui.mount(column().child(a).child(b));
        ui.prepare_frame();
        let (a, b) = (editor(&ui, &view, "a"), editor(&ui, &view, "b"));
        select(&a, 6..10);
        let button = PointerButton::Primary;
        pointer(
            &mut ui,
            InputEvent::PointerDown {
                x: x_of(7),
                y: 20.,
                button,
            },
            Modifiers::default(),
        );
        pointer(
            &mut ui,
            InputEvent::PointerMove {
                x: x_of(7) + 8.,
                y: 20.,
            },
            Modifiers::default(),
        );
        // As it starts: what other apps would take, plain text.
        let state = ui.input.drag_state().expect("dragging");
        assert_eq!(
            state.payload.downcast_ref::<DraggedText>().unwrap().text(),
            "beta"
        );
        assert_eq!(
            state.export.as_deref(),
            Some(&[DragItem::Text("beta".into())][..])
        );
        let modifiers = Modifiers {
            alt: copy && cfg!(target_os = "macos"),
            control: copy && !cfg!(target_os = "macos"),
            ..Modifiers::default()
        };
        // Onto the second field (60 high, below the first), past its "x".
        pointer(
            &mut ui,
            InputEvent::PointerMove { x: x_of(3), y: 80. },
            modifiers,
        );
        pointer(
            &mut ui,
            InputEvent::PointerUp {
                x: x_of(3),
                y: 80.,
                button,
            },
            modifiers,
        );
        assert_eq!(b.value().get(), "xbeta", "copy {copy}");
        assert_eq!(b.copy(), "beta", "the dropped text selected");
        let left = if copy { "alpha beta" } else { "alpha " };
        assert_eq!(a.value().get(), left, "copy {copy}");
    }
}

#[test]
fn a_read_only_editor_takes_no_drop_and_a_password_drags_nothing() {
    let mut ui = Ui::new(400., 300.);
    let (a, b) = (
        field(&ui, "a", "alpha beta"),
        field(&ui, "b", "x").read_only(true),
    );
    let view = ui.mount(column().child(a).child(b));
    ui.prepare_frame();
    let (a, b) = (editor(&ui, &view, "a"), editor(&ui, &view, "b"));
    select(&a, 6..10);
    drag(
        &mut ui,
        (x_of(7), 20.),
        (x_of(3), 80.),
        Modifiers::default(),
    );
    assert_eq!(b.value().get(), "x");
    assert_eq!(a.value().get(), "alpha beta", "not taken: it stays");
    let mut ui = Ui::new(400., 300.);
    let secret = ui.signal(String::from("hunter2"));
    let view = ui.mount(
        column().child(
            text_input("Password", secret)
                .secure(true)
                .id("p")
                .size(300., 40.),
        ),
    );
    ui.prepare_frame();
    let p = editor(&ui, &view, "p");
    select(&p, 0..7);
    let button = PointerButton::Primary;
    pointer(
        &mut ui,
        InputEvent::PointerDown {
            x: 20.,
            y: 20.,
            button,
        },
        Modifiers::default(),
    );
    pointer(
        &mut ui,
        InputEvent::PointerMove { x: 60., y: 20. },
        Modifiers::default(),
    );
    assert!(!ui.input.is_dragging(), "a password never leaves its field");
}
