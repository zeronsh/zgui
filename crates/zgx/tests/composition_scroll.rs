use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, input::InputEvent, semantics::Role, widgets::Ui};
fn wheel(ui: &mut Ui, x: f32, y: f32, delta_y: f32) {
    ui.dispatch(InputEvent::Scroll {
        x,
        y,
        delta_x: 0.,
        delta_y,
    });
}
#[test]
fn ordinary_scroll_measures_dynamic_children_and_reclamps_on_resize() {
    let mut ui = Ui::new(600., 400.);
    let offset = ui.signal(0.);
    let expanded = ui.signal(false);
    let read_expanded = expanded.clone();
    let height = ui.signal(120.);
    let read_height = height.clone();
    let view = ui.mount(
        scroll(offset.clone())
            .w(220.)
            .p(10.)
            .reactive_style(move || Styles::new().h(read_height.get()))
            .child(div().h(150.))
            .child(switch(
                move || read_expanded.get(),
                |expanded, _| {
                    if expanded {
                        div().h(250.)
                    } else {
                        div().h(50.)
                    }
                },
            )),
    );
    ui.prepare_frame();
    offset.set(10000.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 100.);
    expanded.set(true);
    ui.prepare_frame();
    offset.set(10000.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 300.);
    height.set(220.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 200.);
    expanded.set(false);
    ui.prepare_frame();
    assert_eq!(offset.get(), 0.);
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().role,
        Role::ScrollView
    );
}
#[test]
fn nested_scroll_consumes_motion_until_edge_then_bubbles() {
    let mut ui = Ui::new(500., 400.);
    let outer = ui.signal(0.);
    let inner = ui.signal(0.);
    ui.mount(
        scroll(outer.clone())
            .w(300.)
            .h(160.)
            .child(
                scroll(inner.clone())
                    .w(200.)
                    .h(100.)
                    .child(div().w(200.).h(300.)),
            )
            .child(div().h(400.)),
    );
    ui.prepare_frame();
    wheel(&mut ui, 20., 20., 100.);
    assert_eq!((inner.get(), outer.get()), (100., 0.));
    wheel(&mut ui, 20., 20., 100.);
    assert_eq!((inner.get(), outer.get()), (200., 0.));
    // Each a gesture of its own: one latches to the view it scrolls.
    let pause = || std::thread::sleep(std::time::Duration::from_millis(350));
    pause();
    wheel(&mut ui, 20., 20., 40.);
    assert_eq!((inner.get(), outer.get()), (200., 40.));
    pause();
    wheel(&mut ui, 20., 20., -50.);
    assert_eq!((inner.get(), outer.get()), (150., 40.));
}
#[test]
fn offset_updates_are_compositor_only_and_children_retain_local_state() {
    let mut ui = Ui::new(500., 300.);
    let offset = ui.signal(0.);
    let mounts = Rc::new(Cell::new(0));
    let observed = mounts.clone();
    let view = ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .child(component(move |cx| {
                observed.set(observed.get() + 1);
                let count = cx.state(42);
                column()
                    .h(500.)
                    .child(text_signal(move || count.get().to_string()).id("local"))
            })),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let child = view.find("local").unwrap();
    for n in [25., 100., 350., 0.] {
        offset.set(n);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(view.find("local"), Some(child));
    }
    assert_eq!(mounts.get(), 1);
}
#[test]
fn disabled_scroll_blocks_input_and_unmount_releases_observers() {
    let mut ui = Ui::new(500., 300.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .disabled(true)
            .child(div().h(500.)),
    );
    ui.prepare_frame();
    assert!(ui.semantics.borrow().get(view.node()).unwrap().disabled);
    wheel(&mut ui, 20., 20., 50.);
    assert_eq!(offset.get(), 0.);
    offset.set(f32::INFINITY);
    ui.prepare_frame();
    assert_eq!(offset.get(), 400.);
    offset.set(f32::NAN);
    assert_eq!(offset.get(), 0.);
    view.unmount();
    assert!(!ui.input.has_listeners(view.node()));
    assert!(ui.semantics.borrow().get(view.node()).is_none());
    offset.set(900.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 900.);
}
