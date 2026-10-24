use std::f32::consts::FRAC_PI_2;
use zgx::{
    compose::prelude::*,
    input::InputEvent,
    scene::{Color, LocalTransform},
    widgets::Ui,
};

#[test]
fn fluent_transforms_set_only_their_components_last_write_winning() {
    let mut ui = Ui::new(200., 160.);
    let view = ui.mount(
        div()
            .w(60.)
            .h(40.)
            .bg(Color(10, 20, 30, 255))
            .scale(3.)
            .rotate(0.5)
            .scale_xy(2., 0.5)
            .transform_origin(0., 1.),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    assert_eq!(
        scene.local_transform(view.node()),
        LocalTransform {
            scale: (2., 0.5),
            rotate: 0.5,
            origin: (0., 1.),
            ..LocalTransform::IDENTITY
        }
    );
    assert!(scene.is_layer(view.node()));
    drop(scene);
    let plain = ui.mount(div().w(10.).h(10.).skew(0.25, 0.));
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().local_transform(plain.node()),
        LocalTransform {
            skew: (0.25, 0.),
            ..LocalTransform::IDENTITY
        }
    );
}

#[test]
fn reactive_rotation_composites_only_and_equal_values_are_idle() {
    let mut ui = Ui::new(200., 160.);
    let angle = ui.signal(0.);
    let tick = ui.signal(0);
    let read_angle = angle.clone();
    let read_tick = tick.clone();
    let view = ui.mount(
        div()
            .w(60.)
            .h(40.)
            .bg(Color(10, 20, 30, 255))
            .scale(2.)
            .child(text("label"))
            .reactive_style(move || {
                read_tick.get();
                Styles::new().rotate(read_angle.get())
            }),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    for step in 1..=3 {
        angle.set(step as f32 * 0.1);
        ui.prepare_frame();
        let resolved = ui.scene.borrow().local_transform(view.node());
        // The base scale stays under the dynamic rotation.
        assert_eq!(resolved.scale, (2., 2.));
        assert_eq!(resolved.rotate, step as f32 * 0.1);
        let frame = ui.scene.borrow_mut().flush();
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 0);
        assert_eq!(frame.composite_nodes, 1);
        assert!(!frame.damage.is_empty());
    }
    // Re-running with the same resolved rotation mutates nothing.
    tick.set(1);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
    angle.set(f32::NAN);
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    tick.set(2);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
    assert_eq!(ui.scene.borrow().local_transform(view.node()).rotate, 0.);
}

#[test]
fn hover_scale_applies_and_restores() {
    let mut ui = Ui::new(200., 160.);
    let view = ui.mount(
        div()
            .w(60.)
            .h(40.)
            .bg(Color(10, 20, 30, 255))
            .rotate(FRAC_PI_2)
            .hover(|s| s.scale(1.5)),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    // Rotated a quarter turn about (30, 20): it spans x 10..50, y -10..50, so
    // (30, 45) is on it although outside its laid-out box.
    ui.dispatch(InputEvent::PointerMove { x: 30., y: 45. });
    ui.prepare_frame();
    let hovered = ui.scene.borrow().local_transform(view.node());
    assert_eq!((hovered.scale, hovered.rotate), ((1.5, 1.5), FRAC_PI_2));
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(frame.layout_nodes, 0);
    ui.dispatch(InputEvent::PointerLeave);
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().local_transform(view.node()).scale,
        (1., 1.)
    );
}
