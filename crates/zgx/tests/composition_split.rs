//! `compose::split`: two panes and a divider that resizes one of them.
use zgx::{
    compose::{SplitOptions, ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::Direction,
    widgets::Ui,
};

fn mount(options: SplitOptions, size: f32) -> (Ui, ViewHandle, zgx::reactive::Signal<f32>) {
    let mut ui = Ui::new(600., 200.);
    let size = ui.signal(size);
    let view = ui.mount(div().id("root").w(600.).h(200.).child(split(
        size.clone(),
        options,
        div().id("first").h_full(),
        div().id("second").h_full(),
    )));
    ui.prepare_frame();
    (ui, view, size)
}
fn width(ui: &Ui, view: &ViewHandle, id: &str) -> f32 {
    ui.scene.borrow().bounds(view.find(id).unwrap()).width
}
fn drag(ui: &mut Ui, from: f32, to: f32) {
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown {
        x: from,
        y: 100.,
        button,
    });
    ui.dispatch(InputEvent::PointerMove { x: to, y: 100. });
    ui.dispatch(InputEvent::PointerUp {
        x: to,
        y: 100.,
        button,
    });
    ui.prepare_frame();
}
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
}

#[test]
fn the_divider_drags_the_first_pane_within_its_limits() {
    let options = SplitOptions {
        min: 100.,
        max: 400.,
        min_rest: 150.,
        ..Default::default()
    };
    let (mut ui, view, size) = mount(options, 200.);
    assert_eq!(width(&ui, &view, "first"), 200.);
    assert_eq!(width(&ui, &view, "second"), 395.);
    drag(&mut ui, 202., 302.);
    assert_eq!(size.get(), 300.);
    assert_eq!(width(&ui, &view, "first"), 300.);
    // The second pane keeps its minimum: 600 - 150 - 5.
    drag(&mut ui, 302., 590.);
    assert_eq!(size.get(), 400.);
    drag(&mut ui, 402., 0.);
    assert_eq!(size.get(), 100.);
    // Focused by the press, the arrows move it; a double click resets it.
    key(&mut ui, Key::ArrowRight);
    assert_eq!(size.get(), 110.);
    key(&mut ui, Key::End);
    assert_eq!(size.get(), 400.);
    // Assistive technology reads its size, range and name.
    let divider = ui.input.focused().unwrap();
    let semantics = ui.semantics.borrow();
    let node = semantics.get(divider).unwrap();
    assert_eq!(node.role, zgx::semantics::Role::Splitter);
    assert_eq!(
        (node.min, node.max, node.numeric_value),
        (Some(100.), Some(400.), Some(400.))
    );
    assert_eq!(node.label, "Resize");
}

#[test]
fn sizing_the_second_pane_moves_it_the_other_way() {
    let options = SplitOptions {
        second: true,
        min: 100.,
        ..Default::default()
    };
    let (mut ui, view, size) = mount(options, 200.);
    assert_eq!(width(&ui, &view, "second"), 200.);
    assert_eq!(width(&ui, &view, "first"), 395.);
    // Dragged left, the divider makes the second pane wider.
    drag(&mut ui, 397., 297.);
    assert_eq!(size.get(), 300.);
    assert_eq!(width(&ui, &view, "second"), 300.);
    key(&mut ui, Key::ArrowLeft);
    assert_eq!(size.get(), 310., "the arrow moves the divider left");
}

#[test]
fn right_to_left_the_first_pane_is_at_the_right() {
    let (mut ui, view, size) = mount(SplitOptions::default(), 200.);
    let root = view.find("root").unwrap();
    ui.scene
        .borrow_mut()
        .set_direction(root, Some(Direction::Rtl));
    ui.prepare_frame();
    let first = ui.scene.borrow().bounds(view.find("first").unwrap());
    assert_eq!(first.x, 400.);
    // Dragging its divider left widens it.
    drag(&mut ui, 397., 347.);
    assert_eq!(size.get(), 250.);
}

/// The divider takes the keys that move it, and no others: Tab moves focus
/// on past it (it held focus, a trap in every split view).
#[test]
fn tab_moves_focus_past_the_divider() {
    let mut ui = Ui::new(600., 200.);
    let size = ui.signal(200.);
    let view = ui.mount(div().w(600.).h(200.).child(split(
        size,
        SplitOptions::default(),
        button().id("before").w(80.).h(30.).child(text("Before")),
        button().id("after").w(80.).h(30.).child(text("After")),
    )));
    ui.prepare_frame();
    let before = view.find("before").unwrap();
    ui.input.focus(&ui.scene, Some(before));
    key(&mut ui, Key::Tab);
    let divider = ui.input.focused().unwrap();
    assert_eq!(
        ui.semantics.borrow().get(divider).map(|s| s.role),
        Some(zgx::semantics::Role::Splitter)
    );
    key(&mut ui, Key::Tab);
    assert_eq!(ui.input.focused(), view.find("after"));
}
