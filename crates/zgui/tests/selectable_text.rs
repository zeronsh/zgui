//! Display text selection (`View::selectable`): the headless core's
//! approximate layout puts each grapheme in a cell 0.6 of the text size
//! wide, so at size 10 a character is 6 wide and a line 20 tall here.
use std::{cell::Cell, rc::Rc};
use zgui::{
    compose::prelude::*,
    cursor::Cursor,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeKind,
    widgets::Ui,
};

fn document(ui: &mut Ui, children: Vec<View>) {
    ui.mount(
        column()
            .w(300.)
            .gap(4.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .children(children),
    );
    ui.prepare_frame();
}
fn press(ui: &mut Ui, x: f32, y: f32, shift: bool) {
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
fn release(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch_with_modifiers(
        InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        },
        Modifiers::default(),
    );
}
fn move_to(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch_with_modifiers(InputEvent::PointerMove { x, y }, Modifiers::default());
}
fn drag(ui: &mut Ui, from: (f32, f32), to: (f32, f32)) {
    press(ui, from.0, from.1, false);
    move_to(ui, to.0, to.1);
    release(ui, to.0, to.1);
}
fn shortcut(key: &str) -> InputEvent {
    InputEvent::KeyDown {
        key: Key::Character(key.into()),
        modifiers: Modifiers {
            meta: cfg!(target_os = "macos"),
            control: !cfg!(target_os = "macos"),
            ..Modifiers::default()
        },
        repeat: false,
    }
}
/// The text nodes in the scene, in document order.
fn texts(ui: &Ui) -> Vec<zgui::scene::NodeId> {
    let scene = ui.scene.borrow();
    let mut found = Vec::new();
    let mut stack = vec![scene.root()];
    while let Some(id) = stack.pop() {
        if matches!(
            scene.kind(id),
            NodeKind::Text { .. } | NodeKind::RichText { .. }
        ) {
            found.push(id);
        }
        stack.extend(scene.children(id).iter().rev());
    }
    found
}
fn highlighted(ui: &Ui) -> Vec<usize> {
    let scene = ui.scene.borrow();
    texts(ui)
        .iter()
        .enumerate()
        .filter(|(_, id)| scene.text_highlight(**id).is_some())
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn a_drag_selects_across_text_nodes_in_document_order() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![text("Hello world"), text("Second paragraph"), text("Third")],
    );
    drag(&mut ui, (12., 10.), (30., 34.));
    assert_eq!(ui.selected_text(), "llo world\nSecon");
    assert_eq!(highlighted(&ui), [0, 1]);
    // Backwards, from the same anchor, past the end of the document.
    press(&mut ui, 30., 34., false);
    move_to(&mut ui, 12., 10.);
    assert_eq!(ui.selected_text(), "llo world\nSecon");
    move_to(&mut ui, 100., 290.);
    release(&mut ui, 100., 290.);
    assert_eq!(ui.selected_text(), "d paragraph\nThird");
    assert_eq!(highlighted(&ui), [1, 2]);
    // A press that does not move collapses it.
    press(&mut ui, 5., 10., false);
    release(&mut ui, 5., 10.);
    assert!(!ui.has_text_selection());
    assert!(highlighted(&ui).is_empty());
}

#[test]
fn double_and_triple_presses_select_words_and_paragraphs_and_drag_by_them() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![text("alpha beta gamma"), text("one\ntwo three")],
    );
    for _ in 0..2 {
        press(&mut ui, 40., 10., false);
        release(&mut ui, 40., 10.);
    }
    assert_eq!(ui.selected_text(), "beta");
    // Dragging after a double press extends in whole words. (Moving away
    // ends the chain of presses.)
    move_to(&mut ui, 200., 200.);
    press(&mut ui, 40., 10., false);
    release(&mut ui, 40., 10.);
    press(&mut ui, 40., 10., false);
    move_to(&mut ui, 80., 10.);
    assert_eq!(ui.selected_text(), "beta gamma");
    move_to(&mut ui, 4., 10.);
    assert_eq!(ui.selected_text(), "alpha beta");
    release(&mut ui, 4., 10.);
    // A triple press selects the line between hard breaks.
    move_to(&mut ui, 200., 200.);
    for _ in 0..3 {
        press(&mut ui, 10., 54., false);
        release(&mut ui, 10., 54.);
    }
    assert_eq!(ui.selected_text(), "two three");
}

#[test]
fn shift_press_extends_from_the_anchor() {
    let mut ui = Ui::new(400., 300.);
    document(&mut ui, vec![text("abcdefghij"), text("klmnopqrst")]);
    drag(&mut ui, (12., 10.), (24., 10.));
    assert_eq!(ui.selected_text(), "cd");
    press(&mut ui, 30., 34., true);
    release(&mut ui, 30., 34.);
    assert_eq!(ui.selected_text(), "cdefghij\nklmno");
}

#[test]
fn select_all_and_presses_elsewhere() {
    let mut ui = Ui::new(400., 300.);
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    ui.mount(
        column().children([
            column()
                .w(300.)
                .text_size(10.)
                .line_height(20.)
                .selectable(true)
                .children([
                    text("First"),
                    text("Kept out").selectable(false),
                    text("Last"),
                ]),
            button()
                .child(text("Act"))
                .on_click(move || counted.set(counted.get() + 1)),
            text("Outside"),
        ]),
    );
    ui.prepare_frame();
    // Nothing selected or pressed yet: select-all has no region.
    assert!(!ui.dispatch(shortcut("a")).default_prevented);
    press(&mut ui, 4., 10., false);
    release(&mut ui, 4., 10.);
    assert!(ui.dispatch(shortcut("a")).default_prevented);
    // What is kept out leaves a gap: a blank line.
    assert_eq!(ui.selected_text(), "First\n\nLast");
    // A button elsewhere acts and keeps the selection; plain text ends it.
    let act = ui.scene.borrow().bounds(texts(&ui)[3]);
    press(&mut ui, act.x + 2., act.y + 2., false);
    release(&mut ui, act.x + 2., act.y + 2.);
    assert_eq!(clicks.get(), 1);
    assert_eq!(ui.selected_text(), "First\n\nLast");
    let outside = ui.scene.borrow().bounds(texts(&ui)[4]);
    press(&mut ui, outside.x + 2., outside.y + 2., false);
    assert!(!ui.has_text_selection());
    assert!(!ui.dispatch(shortcut("a")).default_prevented);
}

#[test]
fn links_and_buttons_inside_click_and_a_drag_from_one_selects() {
    let mut ui = Ui::new(400., 300.);
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    document(
        &mut ui,
        vec![
            text("Before"),
            button()
                .child(text("Press"))
                .on_click(move || counted.set(counted.get() + 1)),
            text("After"),
        ],
    );
    let label = ui.scene.borrow().bounds(texts(&ui)[1]);
    let (x, y) = (label.x + 1., label.y + label.height / 2.);
    press(&mut ui, x, y, false);
    release(&mut ui, x, y);
    assert_eq!(clicks.get(), 1);
    assert!(!ui.has_text_selection());
    // A drag from it selects, and it does not activate.
    press(&mut ui, x, y, false);
    move_to(&mut ui, x + 2., y);
    assert!(!ui.has_text_selection(), "within the press's slop");
    move_to(&mut ui, 200., 290.);
    release(&mut ui, 200., 290.);
    assert_eq!(clicks.get(), 1);
    assert_eq!(ui.selected_text(), "Press\nAfter");
}

#[test]
fn editors_inside_a_selectable_region_select_their_own_text() {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal("editable".to_owned());
    document(
        &mut ui,
        vec![text("Label"), text_input("Field", value).w(200.)],
    );
    let field = ui.scene.borrow().bounds(texts(&ui)[1]);
    drag(
        &mut ui,
        (field.x + 2., field.y + 5.),
        (field.x + 30., field.y + 5.),
    );
    assert!(!ui.has_text_selection());
    let editor = ui.focused_editor().expect("the press focused the editor");
    let (anchor, focus) = editor.selection();
    assert_ne!(anchor, focus);
    // Select-all goes to the focused editor.
    assert!(ui.dispatch(shortcut("a")).default_prevented);
    assert!(!ui.has_text_selection());
}

#[test]
fn text_is_not_selectable_unless_asked_and_costs_nothing() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .children([text("Plain"), text("Text")]),
    );
    ui.prepare_frame();
    let listeners = texts(&ui)
        .iter()
        .map(|id| ui.input.has_listeners(*id))
        .collect::<Vec<_>>();
    drag(&mut ui, (2., 5.), (20., 30.));
    assert!(!ui.has_text_selection());
    assert!(!ui.scene.borrow().has_text_highlights());
    assert_eq!(ui.scene.borrow().cursor_at(2., 5.), None);
    assert!(listeners.iter().all(|listens| !listens));
}

#[test]
fn the_cursor_is_an_i_beam_over_selectable_text_only() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![
            text("Text"),
            row().child(text("Pointer")).cursor(Cursor::Pointer),
        ],
    );
    let [first, second] = texts(&ui)[..] else {
        panic!()
    };
    let scene = ui.scene.borrow();
    let (a, b) = (scene.bounds(first), scene.bounds(second));
    assert_eq!(scene.cursor_at(a.x + 2., a.y + 2.), Some(Cursor::Text));
    assert_eq!(scene.cursor_at(b.x + 2., b.y + 2.), Some(Cursor::Pointer));
    // Blank room in the region is not text.
    assert_eq!(scene.cursor_at(290., 290.), None);
}

#[test]
fn rich_text_spans_select_as_one_paragraph() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![
            rich_text()
                .child(text_span("Bold ").font_bold())
                .child(text_span("and plain"))
                .into(),
            text("Next"),
        ],
    );
    drag(&mut ui, (32., 10.), (12., 34.));
    assert_eq!(ui.selected_text(), "and plain\nNe");
}

#[test]
fn a_selection_change_repaints_only_the_text_it_changes() {
    let mut ui = Ui::new(400., 300.);
    document(&mut ui, vec![text("abcdefghij"), text("klmnopqrst")]);
    press(&mut ui, 0., 10., false);
    move_to(&mut ui, 30., 34.);
    ui.prepare_frame();
    let _ = ui.scene.borrow_mut().flush();
    // Extending within the second line leaves the first alone.
    move_to(&mut ui, 42., 34.);
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert!(!report.damage.is_empty());
    assert!(
        report.damage.iter().all(|rect| rect.y >= 24.),
        "{:?}",
        report.damage
    );
    // An unchanged selection repaints nothing.
    move_to(&mut ui, 42., 34.);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().damage.is_empty());
}

#[test]
fn removing_selected_text_drops_its_part_and_unmounting_ends_it() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let visible = shown.clone();
    let handle = ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Kept"))
            .child(switch(
                move || visible.get(),
                |shown, _| if shown { text("Gone") } else { column() },
            ))
            .child(text("Last")),
    );
    ui.prepare_frame();
    drag(&mut ui, (0., 10.), (100., 290.));
    assert_eq!(ui.selected_text(), "Kept\nGone\nLast");
    shown.set(false);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "Kept\nLast");
    handle.unmount();
    ui.prepare_frame();
    assert!(!ui.has_text_selection());
    assert!(!ui.scene.borrow().has_text_highlights());
}

#[test]
fn the_inactive_window_colour_and_copy_text_spacing() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .gap(16.)
            .selectable(true)
            .child(row().gap(8.).children([text("Name"), text("Value")]))
            .child(text("Paragraph")),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    assert_eq!(ui.selected_text(), "Name\tValue\n\nParagraph");
    let first = texts(&ui)[0];
    let active = ui.scene.borrow().text_highlight(first).unwrap().color;
    ui.set_window_active(false);
    let inactive = ui.scene.borrow().text_highlight(first).unwrap().color;
    assert_ne!(active, inactive);
    ui.set_window_active(true);
    assert_eq!(
        ui.scene.borrow().text_highlight(first).unwrap().color,
        active
    );
}
