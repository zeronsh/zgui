use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeKind,
    semantics::Role,
    widgets::Ui,
};

fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}
fn down(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
}
fn up(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
}
#[test]
fn slider_tracks_allocated_size_padding_and_captured_drag() {
    let mut ui = Ui::new(700., 300.);
    let value = ui.signal(0.);
    let width = ui.signal(240.);
    let width_read = width.clone();
    let view = ui.mount(
        slider("Volume", value.clone(), 0. ..=100.)
            .h(60.)
            .p(20.)
            .reactive_style(move || Styles::new().w(width_read.get())),
    );
    ui.prepare_frame();
    let root = view.node();
    let rail = ui.scene.borrow().children(root)[0];
    let thumb = ui.scene.borrow().children(root)[2];
    let bounds = ui.scene.borrow().bounds(rail);
    assert_eq!((bounds.x, bounds.width), (25., 190.));
    down(&mut ui, bounds.x + bounds.width / 2., 30.);
    assert!((value.get() - 50.).abs() < 0.001);
    ui.dispatch(InputEvent::PointerMove { x: 690., y: 200. });
    assert_eq!(value.get(), 100.);
    up(&mut ui, 690., 200.);
    width.set(440.);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(rail);
    assert_eq!((bounds.x, bounds.width), (25., 390.));
    down(&mut ui, bounds.x + bounds.width / 4., 30.);
    up(&mut ui, bounds.x + bounds.width / 4., 30.);
    assert!((value.get() - 25.).abs() < 0.001);
    ui.prepare_frame();
    let knob = ui.scene.borrow().bounds(thumb);
    assert!((knob.x + knob.width / 2. - (bounds.x + bounds.width / 4.)).abs() < 0.001);
}
#[test]
fn slider_full_float_range_semantics_keyboard_and_cleanup() {
    let mut ui = Ui::new(500., 100.);
    let value = ui.signal(0.);
    let view = ui.mount(slider("Balance", value.clone(), -f32::MAX..=f32::MAX));
    let root = view.node();
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(root)));
    key(&mut ui, Key::ArrowRight);
    assert!(value.get().is_finite() && value.get() > 0.);
    key(&mut ui, Key::Home);
    assert_eq!(value.get(), -f32::MAX);
    key(&mut ui, Key::End);
    assert_eq!(value.get(), f32::MAX);
    value.set(f32::NAN);
    assert_eq!(value.get(), -f32::MAX);
    value.set(f32::INFINITY);
    assert_eq!(value.get(), f32::MAX);
    let sem = ui.semantics.borrow().get(root).unwrap().clone();
    assert_eq!(sem.role, Role::Slider);
    assert_eq!(sem.label, "Balance");
    assert_eq!(sem.numeric_value, Some(f32::MAX as f64));
    view.unmount();
    assert!(!ui.input.has_listeners(root));
    assert!(ui.semantics.borrow().get(root).is_none());
    value.set(0.);
    assert!(!ui.scene.borrow().contains(root));
}
#[test]
fn slider_disabled_and_reactive_thumb_styles_preserve_model_ownership() {
    let mut ui = Ui::new(500., 100.);
    let value = ui.signal(25.);
    let disabled = ui.signal(false);
    let read_disabled = disabled.clone();
    let view = ui.mount(
        column().text_color(rgb(0x12ab34)).child(
            slider("Volume", value.clone(), 0. ..=100.)
                .id("slider")
                .disabled_when(move || read_disabled.get())
                .disabled_style(|s| s.text_color(rgb(0x987654))),
        ),
    );
    let root = view.find("slider").unwrap();
    ui.prepare_frame();
    let thumb = ui.scene.borrow().children(root)[2];
    assert_eq!(
        ui.scene.borrow().kind(thumb),
        &NodeKind::Rect(rgb(0x12ab34))
    );
    disabled.set(true);
    ui.prepare_frame();
    assert!(ui.semantics.borrow().get(root).unwrap().disabled);
    assert_eq!(
        ui.scene.borrow().kind(thumb),
        &NodeKind::Rect(rgb(0x987654))
    );
    down(&mut ui, 200., 15.);
    up(&mut ui, 200., 15.);
    assert_eq!(value.get(), 25.);
    value.set(75.);
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().numeric_value,
        Some(75.)
    );
    disabled.set(false);
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(root)));
    key(&mut ui, Key::End);
    assert_eq!(value.get(), 100.);
}

#[test]
fn slider_initial_disabled_semantics_match_input() {
    let mut ui = Ui::new(500., 100.);
    let value = ui.signal(25.);
    let view = ui.mount(slider("Disabled", value.clone(), 0. ..=100.).disabled(true));
    ui.prepare_frame();
    let root = view.node();
    assert!(ui.semantics.borrow().get(root).unwrap().disabled);
    assert!(!ui.input.focus(&ui.scene, Some(root)));
    down(&mut ui, 200., 15.);
    up(&mut ui, 200., 15.);
    assert_eq!(value.get(), 25.);
}

#[test]
fn slider_value_only_updates_avoid_layout() {
    let mut ui = Ui::new(500., 100.);
    let value = ui.signal(25.);
    ui.mount(slider("Volume", value.clone(), 0.0..=100.));
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    for next in [75., 0., 100., 50.] {
        value.set(next);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        assert_eq!(frame.layout_nodes, 0);
        assert!(!frame.damage.is_empty());
    }
}

#[test]
fn a_slider_steps_by_its_step_pages_by_ten_and_drags_onto_steps() {
    let mut ui = Ui::new(400., 100.);
    let value = ui.signal(20.);
    let view = ui.mount(
        slider("Volume", value.clone(), 0. ..=100.)
            .step(5.)
            .w(216.)
            .px(8.),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    key(&mut ui, Key::ArrowRight);
    assert_eq!(value.get(), 25.);
    key(&mut ui, Key::PageUp);
    assert_eq!(value.get(), 75.);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(value.get(), 25.);
    key(&mut ui, Key::PageDown);
    assert_eq!(value.get(), 0.);
    // A press a third of the way along lands on a step.
    let b = ui.scene.borrow().bounds(view.node());
    let x = b.x + 8. + 200. * 0.33;
    down(&mut ui, x, b.y + b.height / 2.);
    up(&mut ui, x, b.y + b.height / 2.);
    let v = value.get();
    assert!(v % 5. == 0. && (25. ..=40.).contains(&v), "{v}");
}
