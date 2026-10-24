use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    scene::{Color, Transform},
    widgets::Ui,
};

#[test]
fn reactive_invalid_translation_is_idle_and_sparse_removal_restores_base() {
    let mut ui = Ui::new(200., 160.);
    let tick = ui.signal(0);
    let apply = ui.signal(true);
    let read_tick = tick.clone();
    let read_apply = apply.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let view = ui.mount(
        div()
            .w(60.)
            .h(40.)
            .bg(Color(10, 20, 30, 255))
            .translate(20., 30.)
            .reactive_style(move || {
                read_tick.get();
                count.set(count.get() + 1);
                if read_apply.get() {
                    Styles::new().translate(f32::NAN, f32::INFINITY)
                } else {
                    Styles::new()
                }
            }),
    );
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().transform(view.node()),
        Transform::default()
    );
    ui.scene.borrow_mut().flush();
    let initial_calls = calls.get();
    for value in 1..=3 {
        tick.set(value);
        ui.prepare_frame();
        assert_eq!(calls.get(), initial_calls + value);
        assert_eq!(
            ui.scene.borrow().transform(view.node()),
            Transform::default()
        );
        let frame = ui.scene.borrow_mut().flush();
        assert!(
            frame.is_idle(),
            "equal normalized translation must be idle: {frame:?}"
        );
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.composite_nodes, 0);
        assert!(frame.damage.is_empty());
    }
    apply.set(false);
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().transform(view.node()),
        Transform { x: 20., y: 30. }
    );
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(frame.layout_nodes, 0);
    assert_eq!(frame.composite_nodes, 1);
    assert!(!frame.damage.is_empty());
    apply.set(true);
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().transform(view.node()),
        Transform::default()
    );
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(frame.layout_nodes, 0);
    assert!(!frame.damage.is_empty());
    assert!(ui.scene.borrow_mut().flush().is_idle());
}
