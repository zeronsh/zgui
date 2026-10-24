use zgx::{compose::prelude::*, input::InputEvent, semantics::ScrollAxis, widgets::Ui};
fn wheel(ui: &mut Ui, x: f32, y: f32, delta_x: f32, delta_y: f32) {
    ui.dispatch(InputEvent::Scroll {
        x,
        y,
        delta_x,
        delta_y,
    });
}
#[test]
fn horizontal_scroll_follows_intrinsic_extent_padding_and_resize() {
    let mut ui = Ui::new(600., 400.);
    let offset = ui.signal(0.);
    let width = ui.signal(220.);
    let read_width = width.clone();
    let expanded = ui.signal(false);
    let read_expanded = expanded.clone();
    let view = ui.mount(
        scroll_x(offset.clone())
            .h(100.)
            .p(10.)
            .reactive_style(move || Styles::new().w(read_width.get()))
            .child(div().w(150.).h(40.))
            .child(switch(
                move || read_expanded.get(),
                |expanded, _| div().w(if expanded { 350. } else { 150. }).h(40.),
            )),
    );
    ui.prepare_frame();
    offset.set(f32::INFINITY);
    assert_eq!(offset.get(), 100.);
    expanded.set(true);
    ui.prepare_frame();
    offset.set(10000.);
    assert_eq!(offset.get(), 300.);
    width.set(320.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 200.);
    expanded.set(false);
    ui.prepare_frame();
    assert_eq!(offset.get(), 0.);
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().scroll_axis,
        Some(ScrollAxis::Horizontal)
    );
}
#[test]
fn mixed_axis_wheels_route_to_the_matching_viewport() {
    let mut ui = Ui::new(600., 400.);
    let x = ui.signal(0.);
    let y = ui.signal(0.);
    ui.mount(
        scroll(y.clone())
            .w(300.)
            .h(180.)
            .child(
                scroll_x(x.clone())
                    .w(250.)
                    .h(100.)
                    .child(div().w(500.).h(80.)),
            )
            .child(div().h(400.)),
    );
    ui.prepare_frame();
    wheel(&mut ui, 20., 20., 60., 0.);
    assert_eq!((x.get(), y.get()), (60., 0.));
    wheel(&mut ui, 20., 20., 0., 50.);
    assert_eq!((x.get(), y.get()), (60., 50.));
    wheel(&mut ui, 20., 20., f32::NAN, 0.);
    assert_eq!((x.get(), y.get()), (60., 50.));
}
#[test]
fn mixed_axis_batched_focus_reveals_without_rebuilding_or_layout() {
    let mut ui = Ui::new(600., 400.);
    let x = ui.signal(0.);
    let y = ui.signal(0.);
    let view = ui.mount(
        scroll(y.clone())
            .w(300.)
            .h(140.)
            .p(10.)
            .child(div().h(200.))
            .child(
                scroll_x(x.clone())
                    .w(260.)
                    .h(100.)
                    .p(10.)
                    .child(div().w(300.).h(80.))
                    .child(button().w(60.).h(80.).id("target")),
            ),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let target = view.find("target").unwrap();
    ui.runtime.batch(|| {
        ui.input.focus(&ui.scene, Some(target));
    });
    assert_eq!((x.get(), y.get()), (120., 170.));
    let bounds = ui.scene.borrow().bounds(target);
    assert_eq!((bounds.x, bounds.y), (200., 50.));
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    x.set(0.);
    ui.prepare_frame();
    assert_eq!(x.get(), 0.);
    assert_eq!(view.find("target"), Some(target));
}
#[test]
fn horizontal_disabled_cleanup_and_oversized_focus() {
    let mut ui = Ui::new(600., 300.);
    let offset = ui.signal(0.);
    let disabled = ui.signal(true);
    let read = disabled.clone();
    let view = ui.mount(
        scroll_x(offset.clone())
            .w(100.)
            .h(100.)
            .disabled_when(move || read.get())
            .child(div().w(150.))
            .child(button().w(200.).h(80.).id("large"))
            .child(div().w(200.)),
    );
    ui.prepare_frame();
    wheel(&mut ui, 20., 20., 50., 0.);
    assert_eq!(offset.get(), 0.);
    let large = view.find("large").unwrap();
    assert!(!ui.input.focus(&ui.scene, Some(large)));
    disabled.set(false);
    ui.input.focus(&ui.scene, Some(large));
    assert_eq!(offset.get(), 150.);
    offset.set(200.);
    ui.input.focus(&ui.scene, None);
    ui.input.focus(&ui.scene, Some(large));
    assert_eq!(offset.get(), 200.);
    view.unmount();
    assert!(!ui.input.has_listeners(view.node()));
    offset.set(900.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 900.);
}
