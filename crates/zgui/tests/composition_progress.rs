use zgui::{compose::prelude::*, scene::NodeKind, semantics::Role, widgets::Ui};

#[test]
fn progress_values_composite_without_layout_and_follow_allocated_size() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(0.25);
    let width = ui.signal(200.);
    let read_width = width.clone();
    let mounted = ui.mount(
        progress("Download", value.clone())
            .h(24.)
            .p(4.)
            .text_color(rgb(0x11aaee))
            .reactive_style(move || Styles::new().w(read_width.get())),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let root = mounted.node();
    let viewport = ui.scene.borrow().children(root)[0];
    let fill = ui.scene.borrow().children(viewport)[0];
    assert_eq!(ui.scene.borrow().bounds(viewport).width, 192.);
    assert_eq!(ui.scene.borrow().bounds(viewport).height, 16.);
    assert_eq!(ui.scene.borrow().transform(fill).x, -144.);
    assert!(matches!(ui.scene.borrow().kind(fill), NodeKind::Rect(c) if *c == rgb(0x11aaee)));
    value.set(0.75);
    ui.prepare_frame();
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(frame.layout_nodes, 0, "value updates must not relayout");
    assert!(!frame.damage.is_empty());
    assert_eq!(ui.scene.borrow().transform(fill).x, -48.);
    value.set(0.75);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
    width.set(400.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(viewport).width, 392.);
    assert_eq!(ui.scene.borrow().transform(fill).x, -98.);
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().numeric_value,
        Some(0.75)
    );
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().role,
        Role::Progress
    );
    assert!(!ui.input.focus(&ui.scene, Some(root)));
    mounted.unmount();
    value.set(0.5);
    width.set(120.);
    ui.prepare_frame();
    assert!(!ui.scene.borrow().contains(fill));
    assert!(ui.semantics.borrow().get(root).is_none());
}

#[test]
fn progress_normalizes_external_values_and_retains_disabled_semantics() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(f32::NAN);
    let mounted = ui.mount(progress("Download", value.clone()).disabled(true));
    // (NaN is indeterminate, below.)
    for (input, expected) in [(f32::INFINITY, 1.), (f32::NEG_INFINITY, 0.), (1.5, 1.)] {
        value.set(input);
        assert_eq!(value.get(), expected);
        assert_eq!(
            ui.semantics
                .borrow()
                .get(mounted.node())
                .unwrap()
                .numeric_value,
            Some(expected as f64)
        );
    }
    assert!(ui.semantics.borrow().get(mounted.node()).unwrap().disabled);
    let bad = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.mount(progress("Invalid", value).child(text("extra")));
    }));
    assert!(bad.is_err());
    assert_eq!(ui.scene.borrow().children(ui.root()), &[mounted.node()]);
}

#[test]
fn an_indeterminate_bar_sweeps_a_stretch_and_tells_no_value() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(f32::NAN);
    let mounted = ui.mount(progress("Syncing", value.clone()).w(200.).h(8.).p(0.));
    ui.prepare_frame();
    let root = mounted.node();
    let viewport = ui.scene.borrow().children(root)[0];
    let fill = ui.scene.borrow().children(viewport)[0];
    let stretch = ui.scene.borrow().bounds(fill).width;
    assert!(
        (stretch - 60.).abs() < 0.01,
        "a third of the track: {stretch}"
    );
    assert!(
        ui.scene.borrow().transform(fill).x < 0.,
        "starting off the track"
    );
    assert!(value.get().is_nan(), "left indeterminate");
    assert_eq!(ui.semantics.borrow().get(root).unwrap().numeric_value, None);
    // A value makes it determinate again.
    value.set(0.5);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(fill).width, 200.);
    assert_eq!(ui.scene.borrow().transform(fill).x, -100.);
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().numeric_value,
        Some(0.5)
    );
}
