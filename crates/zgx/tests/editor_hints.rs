//! Editor placeholders, the selection API, select-on-focus and window drag
//! regions.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    scene::{Color, NodeId, NodeKind},
    widgets::{Ui, WindowGesture},
};

fn descendants(ui: &Ui, root: NodeId) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let mut result = vec![root];
    let mut cursor = 0;
    while cursor < result.len() {
        result.extend_from_slice(scene.children(result[cursor]));
        cursor += 1;
    }
    result
}

/// The text the editor paints, and its colour.
fn painted(ui: &Ui, root: NodeId) -> Option<(String, Color)> {
    let scene = ui.scene.borrow();
    descendants(ui, root)
        .into_iter()
        .find_map(|id| match scene.kind(id) {
            NodeKind::Text { text, color, .. } => Some((text.to_string(), *color)),
            _ => None,
        })
}

fn down(x: f32, y: f32) -> InputEvent {
    InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    }
}
fn up(x: f32, y: f32) -> InputEvent {
    InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    }
}

#[test]
fn a_placeholder_shows_while_empty_and_is_never_the_value() {
    let mut ui = Ui::new(500., 300.);
    let value = ui.signal(String::new());
    let plain = ui.signal(String::new());
    let view = ui.mount(
        column()
            .child(
                text_input("Search", value.clone())
                    .id("search")
                    .placeholder("Search or enter address")
                    .text_color(rgb(0xffffff)),
            )
            .child(text_input("Plain", plain.clone()).id("plain")),
    );
    ui.prepare_frame();
    let search = view.find("search").unwrap();
    let other = view.find("plain").unwrap();
    let muted = ui
        .theme()
        .root()
        .color_untracked(zgx::theme::tok::TEXT_TERTIARY);
    assert_eq!(
        painted(&ui, search),
        Some(("Search or enter address".into(), muted))
    );
    assert_eq!(value.get(), "");
    let semantics = ui.semantics.borrow().get(search).cloned().unwrap();
    assert_eq!(
        semantics.placeholder.as_deref(),
        Some("Search or enter address")
    );
    assert_eq!(semantics.value.as_deref(), Some(""));
    assert_eq!(semantics.label, "Search");
    // No extra node, and the same size as an input without one.
    assert_eq!(
        descendants(&ui, search).len(),
        descendants(&ui, other).len()
    );
    let (a, b) = {
        let scene = ui.scene.borrow();
        (scene.bounds(search), scene.bounds(other))
    };
    assert_eq!((a.width, a.height), (b.width, b.height));

    // Typing replaces it with the text, in the text's colour.
    ui.input.focus(&ui.scene, Some(search));
    ui.dispatch(InputEvent::Text("zgx".into()));
    assert_eq!(value.get(), "zgx");
    assert_eq!(
        painted(&ui, search),
        Some(("zgx".into(), Color(255, 255, 255, 255)))
    );
    // Emptied again: back.
    value.set(String::new());
    assert_eq!(painted(&ui, search).unwrap().0, "Search or enter address");
}

#[test]
fn placeholder_colour_and_text_follow_their_sources() {
    let mut ui = Ui::new(500., 300.);
    let value = ui.signal(String::new());
    let hint = ui.signal(String::from("one"));
    let read = hint.clone();
    let view = ui.mount(
        text_area("Notes", value)
            .id("notes")
            .placeholder_when(move || read.get())
            .placeholder_color(Color(1, 2, 3, 255)),
    );
    ui.prepare_frame();
    let notes = view.find("notes").unwrap();
    assert_eq!(
        painted(&ui, notes),
        Some(("one".into(), Color(1, 2, 3, 255)))
    );
    hint.set("two".into());
    assert_eq!(painted(&ui, notes).unwrap().0, "two");
    assert_eq!(
        ui.semantics
            .borrow()
            .get(notes)
            .unwrap()
            .placeholder
            .as_deref(),
        Some("two")
    );
    hint.set(String::new());
    assert_eq!(painted(&ui, notes).unwrap().0, "");
    assert_eq!(ui.semantics.borrow().get(notes).unwrap().placeholder, None);
}

#[test]
fn a_long_placeholder_does_not_size_the_input() {
    let mut ui = Ui::new(800., 300.);
    let (a, b) = (ui.signal(String::new()), ui.signal(String::new()));
    let view = ui.mount(
        row()
            .child(
                text_input("A", a)
                    .id("a")
                    .placeholder("a placeholder far wider than the input it sits in, by far"),
            )
            .child(text_input("B", b).id("b")),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let (a, b) = (
        scene.bounds(view.find("a").unwrap()),
        scene.bounds(view.find("b").unwrap()),
    );
    assert_eq!((a.width, a.height), (b.width, b.height));
}

#[test]
fn selection_api_reads_and_writes_byte_offsets() {
    let mut ui = Ui::new(500., 300.);
    let value = ui.signal(String::from("héllo wörld"));
    let view = ui.mount(text_input("Text", value).id("text"));
    ui.prepare_frame();
    let node = view.find("text").unwrap();
    let editor = ui.editor(node).unwrap();
    editor.select_all();
    assert_eq!(editor.selection(), (0, "héllo wörld".len()));
    editor.set_selection(7, 1);
    assert_eq!(editor.selection(), (7, 1));
    // Inside `é` (bytes 1..3): back to its start; past the end: the end.
    editor.set_selection(2, 99);
    assert_eq!(editor.selection(), (1, "héllo wörld".len()));
    assert_eq!(
        ui.semantics.borrow().get(node).unwrap().text_selection,
        Some((1, "héllo wörld".len()))
    );
    // Typing replaces the selection (`héllo ` is seven bytes).
    editor.set_selection(0, 7);
    ui.input.focus(&ui.scene, Some(node));
    ui.dispatch(InputEvent::Text("hi ".into()));
    assert_eq!(editor.editor.borrow().text(), "hi wörld");
}

#[test]
fn select_on_focus_selects_on_the_first_click_and_places_the_caret_on_the_next() {
    let mut ui = Ui::new(500., 300.);
    let value = ui.signal(String::from("https://example.com/a/long/path"));
    let view = ui.mount(
        column()
            .child(
                text_input("Address", value.clone())
                    .id("address")
                    .w(400.)
                    .select_on_focus(true),
            )
            .child(button().id("other").child(text("Other")).on_click(|| {})),
    );
    ui.prepare_frame();
    let node = view.find("address").unwrap();
    let editor = ui.editor(node).unwrap();
    let end = value.get().len();
    let at = ui.scene.borrow().bounds(node);
    let (x, y) = (at.x + 40., at.y + at.height / 2.);
    let shift = Modifiers::default();
    // First click: focus, everything selected, and moving with the button
    // down does not shrink it to a range from the press.
    ui.dispatch_with_modifiers(down(x, y), shift);
    assert_eq!(ui.input.focused(), Some(node));
    assert_eq!(editor.selection(), (0, end));
    ui.dispatch_with_modifiers(InputEvent::PointerMove { x: x + 3., y }, shift);
    ui.dispatch_with_modifiers(up(x + 3., y), shift);
    assert_eq!(editor.selection(), (0, end));
    // Second click (not a double click): a caret where it lands.
    ui.dispatch(down(x, y));
    ui.dispatch(up(x, y));
    let (anchor, focus) = editor.selection();
    assert_eq!(anchor, focus);
    assert!(anchor > 0 && anchor < end, "{anchor}");

    // The window going to the back and coming forward keeps that caret.
    ui.dispatch(InputEvent::Blur);
    ui.input.dispatch_to(&ui.scene, node, InputEvent::Focus);
    assert_eq!(editor.selection(), (anchor, anchor));

    // Focus leaving and coming back (Tab, a program) selects all again.
    let other = view.find("other").unwrap();
    ui.input.focus(&ui.scene, Some(other));
    ui.input.focus(&ui.scene, Some(node));
    assert_eq!(editor.selection(), (0, end));
}

#[test]
fn a_drag_region_moves_the_window_except_from_what_takes_input() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(String::new());
    let view = ui.mount(
        row()
            .id("titlebar")
            .size(400., 40.)
            .window_drag()
            .child(div().size(100., 40.))
            .child(button().id("close").size(40., 40.).on_click(|| {}))
            .child(text_input("Find", value).id("find").w(120.)),
    );
    ui.prepare_frame();
    let none = Modifiers::default();
    // Empty space, a plain child: drag.
    ui.dispatch_with_modifiers(down(50., 20.), none);
    assert_eq!(ui.take_window_gesture(), Some(WindowGesture::Drag));
    assert_eq!(ui.take_window_gesture(), None, "taken once");
    ui.dispatch_with_modifiers(up(50., 20.), none);
    // The second press of a double click.
    ui.dispatch_with_modifiers(down(50., 20.), none);
    assert_eq!(ui.take_window_gesture(), Some(WindowGesture::DoubleClick));
    ui.dispatch_with_modifiers(up(50., 20.), none);

    // A button and an input keep their presses.
    let close = view.find("close").unwrap();
    let at = ui.scene.borrow().bounds(close);
    ui.dispatch(down(at.x + 5., 20.));
    assert_eq!(ui.take_window_gesture(), None);
    ui.dispatch(up(at.x + 5., 20.));
    let find = view.find("find").unwrap();
    let at = ui.scene.borrow().bounds(find);
    ui.dispatch(down(at.x + 5., at.y + 5.));
    assert_eq!(ui.take_window_gesture(), None);
    assert_eq!(ui.input.focused(), Some(find));
    ui.dispatch(up(at.x + 5., at.y + 5.));
    // A press to move the window leaves focus where it was.
    ui.dispatch(down(50., 20.));
    assert_eq!(ui.take_window_gesture(), Some(WindowGesture::Drag));
    assert_eq!(ui.input.focused(), Some(find));
}

#[test]
fn a_hover_styled_child_of_a_drag_region_still_moves_the_window() {
    let mut ui = Ui::new(400., 200.);
    ui.mount(
        row()
            .size(400., 40.)
            .window_drag()
            .child(div().size(100., 40.).hover(|s| s.bg(rgb(0x333333)))),
    );
    ui.prepare_frame();
    ui.dispatch(InputEvent::PointerMove { x: 50., y: 20. });
    ui.dispatch(down(50., 20.));
    assert_eq!(ui.take_window_gesture(), Some(WindowGesture::Drag));
    assert_eq!(ui.input.captured(), None);
}

#[test]
fn a_secondary_click_on_a_drag_region_asks_for_the_window_menu() {
    let mut ui = Ui::new(400., 200.);
    let view = ui.mount(
        row()
            .size(400., 40.)
            .window_drag()
            .child(div().size(100., 40.))
            .child(button().id("close").size(40., 40.).on_click(|| {})),
    );
    ui.prepare_frame();
    let secondary = |down: bool, x: f32| {
        if down {
            InputEvent::PointerDown {
                x,
                y: 20.,
                button: PointerButton::Secondary,
            }
        } else {
            InputEvent::PointerUp {
                x,
                y: 20.,
                button: PointerButton::Secondary,
            }
        }
    };
    // On release, as a title bar shows it.
    ui.dispatch(secondary(true, 50.));
    assert_eq!(ui.take_window_gesture(), None);
    ui.dispatch(secondary(false, 50.));
    assert_eq!(ui.take_window_gesture(), Some(WindowGesture::Menu));
    // Not over what takes input.
    let close = view.find("close").unwrap();
    let at = ui.scene.borrow().bounds(close);
    ui.dispatch(secondary(true, at.x + 5.));
    ui.dispatch(secondary(false, at.x + 5.));
    assert_eq!(ui.take_window_gesture(), None);
}
