use zgx::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    scene::NodeId,
    widgets::Ui,
};

fn settle(ui: &mut Ui) {
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
}

/// How many selection highlights are drawn inside `node`.
fn highlights(ui: &Ui, node: NodeId) -> usize {
    let scene = ui.scene.borrow();
    let mut stack = vec![node];
    let mut count = 0;
    while let Some(id) = stack.pop() {
        if let Some(highlight) = scene.text_highlight(id) {
            count += highlight.rects.len();
        }
        stack.extend(scene.children(id).iter().copied());
    }
    count
}

fn click(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
}

#[test]
fn an_unfocused_field_draws_no_selection_until_it_has_the_focus_again() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("https://example.com/".to_string());
    let view = ui.mount(
        column()
            .child(
                text_input("Address", value)
                    .select_on_focus(true)
                    .id("field")
                    .w(300.)
                    .h(30.),
            )
            .child(button().id("other").size(80., 30.).child(text("Other"))),
    );
    let (field, other) = (view.find("field").unwrap(), view.find("other").unwrap());
    settle(&mut ui);
    assert_eq!(highlights(&ui, field), 0);

    click(&mut ui, 20., 10.);
    settle(&mut ui);
    assert_eq!(ui.input.focused(), Some(field));
    assert_eq!(highlights(&ui, field), 1, "focusing it selects the address");

    // Its window went to the back: still the focused field, and its selection still shows.
    ui.input.dispatch(&ui.scene, InputEvent::Blur);
    settle(&mut ui);
    assert_eq!(highlights(&ui, field), 1);
    ui.input.dispatch_to(&ui.scene, field, InputEvent::Focus);

    // The focus went somewhere else: nothing drawn, though the selection is kept.
    assert!(ui.input.focus(&ui.scene, Some(other)));
    settle(&mut ui);
    assert_eq!(
        highlights(&ui, field),
        0,
        "an unfocused field draws no selection"
    );
    assert_eq!(
        ui.editor(field).map(|e| e.selection()),
        Some((0, 20)),
        "the selection is kept"
    );

    click(&mut ui, 20., 10.);
    settle(&mut ui);
    assert_eq!(highlights(&ui, field), 1, "it shows again with the focus");
}
