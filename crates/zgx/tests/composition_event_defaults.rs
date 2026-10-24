use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    semantics::Role,
    widgets::Ui,
};
fn key(key: Key) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }
}
fn wheel() -> InputEvent {
    InputEvent::Scroll {
        x: 10.,
        y: 10.,
        delta_x: 0.,
        delta_y: 40.,
    }
}
#[test]
fn prevented_slider_actions_preserve_value_but_release_drag() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(0.);
    let blocked = ui.signal(true);
    let read = blocked.clone();
    let mounted = ui.mount(
        slider("Value", value.clone(), 0. ..=100.)
            .w(200.)
            .h(40.)
            .on_event(move |cx| {
                if read.get() {
                    cx.prevent_default();
                }
            }),
    );
    ui.prepare_frame();
    let root = mounted.node();
    for event in [
        key(Key::End),
        InputEvent::SetNumericValue(75.),
        InputEvent::PointerDown {
            x: 120.,
            y: 20.,
            button: PointerButton::Primary,
        },
    ] {
        ui.input.dispatch_to(&ui.scene, root, event);
    }
    assert_eq!(value.get(), 0.);
    assert!(ui.input.captured().is_none());
    blocked.set(false);
    ui.dispatch(InputEvent::PointerDown {
        x: 120.,
        y: 20.,
        button: PointerButton::Primary,
    });
    assert!(value.get() > 0.);
    assert_eq!(ui.input.captured(), Some(root));
    blocked.set(true);
    ui.dispatch(InputEvent::PointerUp {
        x: 120.,
        y: 20.,
        button: PointerButton::Primary,
    });
    assert!(ui.input.captured().is_none());
    blocked.set(false);
    let previous = value.get();
    ui.input
        .dispatch_to(&ui.scene, root, InputEvent::PointerMove { x: 190., y: 20. });
    assert_eq!(value.get(), previous);
}
#[test]
fn prevented_scroll_and_virtual_navigation_do_not_change_offsets_or_focus() {
    let mut ui = Ui::new(500., 300.);
    let ordinary = ui.signal(0.);
    let virtual_offset = ui.signal(0.);
    let mounted = ui.mount(
        row().children([
            scroll(ordinary.clone())
                .size(200., 100.)
                .child(div().h(500.))
                .id("scroll")
                .on_event(|cx| cx.prevent_default()),
            virtual_list(
                virtual_offset.clone(),
                20.,
                1,
                || 100,
                |n| n,
                |_, n, _| text(n.to_string()),
            )
            .size(200., 100.)
            .keyboard_navigation(true)
            .id("list")
            .on_event(|cx| cx.prevent_default()),
        ]),
    );
    ui.prepare_frame();
    let ordinary_root = mounted.find("scroll").unwrap();
    let list = mounted.find("list").unwrap();
    ui.input.dispatch_to(&ui.scene, ordinary_root, wheel());
    ui.input.dispatch_to(&ui.scene, list, wheel());
    ui.input.focus(&ui.scene, Some(list));
    ui.input.dispatch_to(&ui.scene, list, key(Key::End));
    assert_eq!(ordinary.get(), 0.);
    assert_eq!(virtual_offset.get(), 0.);
    assert_eq!(ui.input.focused(), Some(list));
}
#[test]
fn menu_panel_can_cancel_navigation_and_escape_then_allow_them() {
    let mut ui = Ui::new(500., 300.);
    let open = ui.signal(true);
    let blocked = ui.signal(true);
    let read = blocked.clone();
    let mounted = ui.mount(
        menu("Actions", open.clone(), button().child(text("Open")))
            .children([menu_item("First").id("first"), menu_item("Last").id("last")])
            .on_event(move |cx| {
                if read.get() && matches!(cx.event, InputEvent::KeyDown { .. }) {
                    cx.prevent_default();
                }
            }),
    );
    ui.prepare_frame();
    let first = mounted.find("first").unwrap();
    assert_eq!(ui.input.focused(), Some(first));
    ui.dispatch(key(Key::End));
    ui.dispatch(key(Key::Escape));
    assert!(open.get());
    assert_eq!(ui.input.focused(), Some(first));
    blocked.set(false);
    ui.dispatch(key(Key::End));
    assert_eq!(ui.input.focused(), mounted.find("last"));
    ui.dispatch(key(Key::Escape));
    assert!(!open.get());
}
#[test]
fn ancestor_can_cancel_scrollbar_defaults_without_losing_release_cleanup() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let blocked = ui.signal(true);
    let read = blocked.clone();
    ui.mount(
        scroll(offset.clone())
            .size(200., 100.)
            .scrollbar(true)
            .child(div().h(500.))
            .on_event(move |cx| {
                if read.get() {
                    cx.prevent_default();
                }
            }),
    );
    ui.prepare_frame();
    let bar = ui
        .semantics
        .borrow()
        .iter()
        .find(|(_, n)| n.role == Role::ScrollBar)
        .unwrap()
        .0;
    ui.input
        .dispatch_to(&ui.scene, bar, InputEvent::SetNumericValue(100.));
    ui.input.focus(&ui.scene, Some(bar));
    ui.dispatch(key(Key::End));
    assert_eq!(offset.get(), 0.);
    blocked.set(false);
    let bounds = ui.scene.borrow().bounds(bar);
    ui.dispatch(InputEvent::PointerDown {
        x: bounds.x + 4.,
        y: bounds.y + 5.,
        button: PointerButton::Primary,
    });
    assert_eq!(ui.input.captured(), Some(bar));
    blocked.set(true);
    ui.dispatch(InputEvent::PointerUp {
        x: bounds.x + 4.,
        y: bounds.y + 5.,
        button: PointerButton::Primary,
    });
    assert!(ui.input.captured().is_none());
}

#[test]
fn legacy_control_defaults_also_respect_prevented_actions() {
    use std::{cell::Cell, rc::Rc};
    let mut ui = Ui::new(400., 300.);
    let root = ui.root();
    let calls = Rc::new(Cell::new(0));
    let write = calls.clone();
    let button = ui.button(root, "Action", 100., move || write.set(write.get() + 1));
    let checked = ui.signal(false);
    let checkbox = ui.checkbox(root, "Check", checked.clone(), 100.);
    let value = ui.signal(0.);
    let slider = ui.slider(root, "Value", value.clone(), 0. ..=100., 100.);
    ui.prepare_frame();
    let _bindings = [button, checkbox, slider]
        .map(|node| ui.input.listen_first(node, |cx| cx.prevent_default()));
    ui.input
        .dispatch_to(&ui.scene, button, InputEvent::Activate);
    ui.input
        .dispatch_to(&ui.scene, checkbox, InputEvent::Activate);
    ui.input
        .dispatch_to(&ui.scene, slider, InputEvent::SetNumericValue(75.));
    ui.input.dispatch_to(&ui.scene, slider, key(Key::End));
    assert_eq!(calls.get(), 0);
    assert!(!checked.get());
    assert_eq!(value.get(), 0.);
}
