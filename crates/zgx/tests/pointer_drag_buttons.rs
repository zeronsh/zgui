use zgx::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    widgets::Ui,
};
fn down(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
}
fn up(ui: &mut Ui, x: f32, y: f32, button: PointerButton) {
    ui.dispatch(InputEvent::PointerUp { x, y, button });
}
#[test]
fn editor_selection_survives_unrelated_release_and_cancels_on_primary_release() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("abcdefghij".to_owned());
    let editor = ui.text_input(ui.root(), "Editor", value.clone(), 240., false);
    ui.prepare_frame();
    down(&mut ui, 12., 15.);
    ui.dispatch(InputEvent::PointerMove { x: 40., y: 15. });
    let first = editor.editor.borrow().selection();
    assert!(first.focus > first.anchor);
    for button in [
        PointerButton::Secondary,
        PointerButton::Middle,
        PointerButton::Other(8),
    ] {
        up(&mut ui, 40., 15., button);
        assert_eq!(ui.input.captured(), Some(editor.node));
    }
    ui.dispatch(InputEvent::PointerMove { x: 90., y: 15. });
    let next = editor.editor.borrow().selection();
    assert_eq!(next.anchor, first.anchor);
    assert!(next.focus > first.focus);
    up(&mut ui, 90., 15., PointerButton::Primary);
    assert_eq!(ui.input.captured(), None);
    ui.dispatch(InputEvent::PointerMove { x: 12., y: 15. });
    assert_eq!(editor.editor.borrow().selection(), next);
    down(&mut ui, 12., 15.);
    ui.dispatch(InputEvent::PointerCancel);
    assert_eq!(ui.input.captured(), None);
    let cancelled = editor.editor.borrow().selection();
    ui.dispatch(InputEvent::PointerMove { x: 90., y: 15. });
    assert_eq!(editor.editor.borrow().selection(), cancelled);
    down(&mut ui, 12., 15.);
    ui.set_disabled(editor.node, true);
    assert_eq!(ui.input.captured(), None);
    assert_eq!(value.get(), "abcdefghij");
}
#[test]
fn legacy_and_declarative_slider_drag_survive_unrelated_release() {
    for declarative in [false, true] {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(0.);
        let node = if declarative {
            ui.mount(slider("Slider", value.clone(), 0.0..=100.).size(200., 40.))
                .node()
        } else {
            ui.slider(ui.root(), "Slider", value.clone(), 0.0..=100., 200.)
        };
        ui.prepare_frame();
        down(&mut ui, 30., 15.);
        let first = value.get();
        up(&mut ui, 30., 15., PointerButton::Secondary);
        assert_eq!(ui.input.captured(), Some(node), "declarative={declarative}");
        assert_eq!(
            ui.input.focused(),
            Some(node),
            "pointer press focuses slider"
        );
        ui.dispatch(InputEvent::PointerMove { x: 170., y: 15. });
        assert!(value.get() > first);
        up(&mut ui, 170., 15., PointerButton::Primary);
        assert_eq!(ui.input.captured(), None);
        let released = value.get();
        ui.dispatch(InputEvent::PointerMove { x: 50., y: 15. });
        assert_eq!(value.get(), released);
        down(&mut ui, 30., 15.);
        ui.dispatch(InputEvent::PointerCancel);
        assert_eq!(ui.input.captured(), None);
        let cancelled = value.get();
        ui.dispatch(InputEvent::PointerMove { x: 170., y: 15. });
        assert_eq!(value.get(), cancelled);
        down(&mut ui, 30., 15.);
        ui.set_disabled(node, true);
        assert_eq!(ui.input.captured(), None);
        let disabled = value.get();
        ui.dispatch(InputEvent::PointerMove { x: 170., y: 15. });
        assert_eq!(value.get(), disabled);
    }
}
