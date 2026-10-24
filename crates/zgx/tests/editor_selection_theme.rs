//! An editor's selection takes the theme's selection colour as it changes
//! (a theme switch, the system's accent), not only the one it was drawn in.
use zgx::{
    compose::prelude::*,
    scene::{Color, NodeId, NodeKind},
    theme::tok,
    widgets::Ui,
};

/// The colours of the rects drawn inside `node`.
fn rects(ui: &Ui, node: NodeId) -> Vec<Color> {
    let scene = ui.scene.borrow();
    let mut stack = vec![node];
    let mut colors = Vec::new();
    while let Some(id) = stack.pop() {
        if let NodeKind::Rect(c) = scene.kind(id) {
            colors.push(*c);
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
