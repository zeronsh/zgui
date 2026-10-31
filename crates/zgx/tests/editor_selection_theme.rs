//! An editor's selection takes the theme's selection colour as it changes
//! (a theme switch, the system's accent), not only the one it was drawn in.
use zgx::{
    compose::prelude::*,
    scene::{Color, NodeId},
    theme::tok,
    widgets::Ui,
};

/// The colours of the selections drawn inside `node`.
fn rects(ui: &Ui, node: NodeId) -> Vec<Color> {
    let scene = ui.scene.borrow();
    let mut stack = vec![node];
    let mut colors = Vec::new();
    while let Some(id) = stack.pop() {
        if let Some(highlight) = scene.text_highlight(id) {
            colors.push(highlight.color);
        }
        stack.extend(scene.children(id).iter().copied());
    }
    colors
}

#[test]
fn a_showing_selection_takes_a_new_selection_colour() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("select me".to_string());
    let view = ui.mount(column().child(text_input("Field", value).id("field").w(300.).h(30.)));
    let field = view.find("field").unwrap();
    ui.prepare_frame();
    let editor = ui.editor(field).unwrap();
    editor.set_selection(0, 9);
    ui.input.focus(&ui.scene, Some(field));
    ui.prepare_frame();
    let before = ui.theme().root().color_untracked(tok::SELECTION);
    assert!(
        rects(&ui, field).contains(&before),
        "the selection is drawn"
    );
    let accent = Color(255, 0, 0, 0x66);
    ui.theme()
        .set_system_tokens([(tok::SELECTION.0, accent.into())]);
    ui.prepare_frame();
    let colors = rects(&ui, field);
    assert!(
        colors.contains(&accent),
        "the selection took the new colour: {colors:?}"
    );
    assert!(
        !colors.contains(&before),
        "and kept none of the old: {colors:?}"
    );
}

#[test]
fn a_selection_over_lines_is_one_shape_on_one_node() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("one\n\nthree".to_string());
    let view = ui.mount(column().child(text_area("Notes", value).id("notes").size(300., 120.)));
    let notes = view.find("notes").unwrap();
    ui.prepare_frame();
    let editor = ui.editor(notes).unwrap();
    ui.input.focus(&ui.scene, Some(notes));
    editor.set_selection(0, 10);
    ui.prepare_frame();
    let mut carrying = Vec::new();
    {
        let scene = ui.scene.borrow();
        let mut stack = vec![notes];
        while let Some(id) = stack.pop() {
            if let Some(highlight) = scene.text_highlight(id) {
                carrying.push((highlight.rects.len(), highlight.radius));
            }
            stack.extend(scene.children(id).iter().copied());
        }
    }
    assert_eq!(carrying, [(3, 3.)], "three lines, an empty one among them");
    editor.set_selection(2, 2);
    ui.prepare_frame();
    assert!(rects(&ui, notes).is_empty(), "no selection, no highlight");
}

#[test]
fn a_selection_running_past_the_edge_is_not_rounded_off_there() {
    let mut ui = Ui::new(400., 200.);
    let lines: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
    let all = lines.join("\n");
    let len = all.len();
    let value = ui.signal(all);
    let view = ui.mount(column().child(text_area("Notes", value).id("notes").size(300., 100.)));
    let notes = view.find("notes").unwrap();
    ui.prepare_frame();
    let editor = ui.editor(notes).unwrap();
    ui.input.focus(&ui.scene, Some(notes));
    editor.set_selection(0, len);
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let mut stack = vec![notes];
    let mut carried = None;
    while let Some(id) = stack.pop() {
        if let Some(highlight) = scene.text_highlight(id) {
            carried = Some(highlight.clone());
        }
        stack.extend(scene.children(id).iter().copied());
    }
    // The caret at the end, the view is scrolled there.
    let highlight = carried.expect("a selection");
    let top = highlight.rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
    assert!(top < -1., "the line above the view: {top}");
}
