use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}
#[test]
fn disabling_open_submenu_closes_only_child_and_blocks_trigger_activation() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(true);
    let nested = ui.signal(false);
    let disabled = ui.signal(false);
    let read = disabled.clone();
    let mounted = ui.mount(
        menu("Root", open.clone(), button().child(text("Root")))
            .child(
                submenu("More", nested.clone())
                    .trigger_id("more")
                    .disabled_when(move || read.get())
                    .child(menu_item("Child").id("child")),
            )
            .child(menu_item("Other").id("other")),
    );
    ui.prepare_frame();
    let trigger = mounted.find("more").unwrap();
    ui.input.focus(&ui.scene, Some(trigger));
    key(&mut ui, Key::ArrowRight);
    assert!(nested.get());
    assert_eq!(ui.input.focused(), mounted.find("child"));
    disabled.set(true);
    ui.prepare_frame();
    assert!(!nested.get());
    assert!(open.get());
    assert!(!ui.input.focus(&ui.scene, Some(trigger)));
    ui.input
        .dispatch_to(&ui.scene, trigger, InputEvent::Activate);
    assert!(!nested.get());
    disabled.set(false);
    ui.input.focus(&ui.scene, Some(trigger));
    key(&mut ui, Key::ArrowRight);
    assert!(nested.get());
    key(&mut ui, Key::ArrowLeft);
    assert!(!nested.get());
    assert!(open.get());
    assert_eq!(ui.input.focused(), Some(trigger));
    mounted.unmount();
    assert!(ui.input.focus_scope().is_none());
}
#[test]
fn removing_open_submenu_branch_keeps_parent_menu_usable() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(true);
    let nested = ui.signal(false);
    let show = ui.signal(true);
    let read = show.clone();
    let child_open = nested.clone();
    let mounted = ui.mount(
        menu("Root", open.clone(), button().child(text("Root")))
            .child(switch(
                move || read.get(),
                move |shown, _| {
                    if shown {
                        submenu("More", child_open.clone())
                            .trigger_id("more")
                            .child(menu_item("Child").id("child"))
                    } else {
                        div()
                    }
                },
            ))
            .child(menu_item("Survivor").id("survivor")),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, mounted.find("more"));
    key(&mut ui, Key::ArrowRight);
    assert!(nested.get());
    show.set(false);
    ui.prepare_frame();
    assert!(mounted.find("child").is_none());
    assert!(open.get());
    assert!(!nested.get());
    key(&mut ui, Key::Home);
    assert_eq!(ui.input.focused(), mounted.find("survivor"));
    key(&mut ui, Key::Escape);
    assert!(!open.get());
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn opening_sibling_submenu_replaces_child_scope_and_preserves_parent() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(true);
    let first = ui.signal(false);
    let second = ui.signal(false);
    let mounted = ui.mount(
        menu("Root", open.clone(), button().child(text("Root")))
            .child(
                submenu("First", first.clone())
                    .trigger_id("first-trigger")
                    .child(menu_item("First action").id("first-action")),
            )
            .child(
                submenu("Second", second.clone())
                    .trigger_id("second-trigger")
                    .child(menu_item("Second action").id("second-action")),
            ),
    );
    ui.prepare_frame();
    first.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), mounted.find("first-action"));
    second.set(true);
    ui.prepare_frame();
    assert!(!first.get());
    assert!(second.get());
    assert!(open.get());
    assert_eq!(ui.input.focused(), mounted.find("second-action"));
    key(&mut ui, Key::ArrowLeft);
    assert!(!second.get());
    assert!(open.get());
    assert_eq!(ui.input.focused(), mounted.find("second-trigger"));
    mounted.unmount();
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn parent_menu_action_is_clickable_once_while_child_popup_is_open() {
    use std::{cell::Cell, rc::Rc};
    let mut ui = Ui::new(900., 600.);
    let open = ui.signal(true);
    let nested = ui.signal(false);
    let calls = Rc::new(Cell::new(0));
    let clicked = calls.clone();
    let mounted = ui.mount(
        menu("Root", open.clone(), button().child(text("Root")))
            .child(submenu("More", nested.clone()).child(menu_item("Child")))
            .child(
                menu_item("Parent action")
                    .id("parent-action")
                    .on_click(move || clicked.set(clicked.get() + 1)),
            ),
    );
    ui.prepare_frame();
    nested.set(true);
    ui.prepare_frame();
    let bounds = ui
        .scene
        .borrow()
        .bounds(mounted.find("parent-action").unwrap());
    for event in [
        InputEvent::PointerDown {
            x: bounds.x + bounds.width / 2.,
            y: bounds.y + bounds.height / 2.,
            button: zgx::input::PointerButton::Primary,
        },
        InputEvent::PointerUp {
            x: bounds.x + bounds.width / 2.,
            y: bounds.y + bounds.height / 2.,
            button: zgx::input::PointerButton::Primary,
        },
    ] {
        ui.dispatch(event);
    }
    assert_eq!(calls.get(), 1);
    assert!(!nested.get());
    assert!(!open.get());
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn clicking_parent_sibling_trigger_switches_child_in_one_gesture() {
    let mut ui = Ui::new(900., 600.);
    let first = ui.signal(false);
    let second = ui.signal(false);
    let mounted = ui.mount(
        menu("Root", ui.signal(true), button().child(text("Root")))
            .child(submenu("First", first.clone()).child(menu_item("First item")))
            .child(
                submenu("Second", second.clone())
                    .trigger_id("second")
                    .child(menu_item("Second item").id("second-item")),
            ),
    );
    ui.prepare_frame();
    first.set(true);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(mounted.find("second").unwrap());
    for event in [
        InputEvent::PointerDown {
            x: bounds.x + 10.,
            y: bounds.y + 10.,
            button: zgx::input::PointerButton::Primary,
        },
        InputEvent::PointerUp {
            x: bounds.x + 10.,
            y: bounds.y + 10.,
            button: zgx::input::PointerButton::Primary,
        },
    ] {
        ui.dispatch(event);
    }
    assert!(!first.get());
    assert!(second.get());
    assert_eq!(ui.input.focused(), mounted.find("second-item"));
    mounted.unmount();
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn reentrant_owner_disposal_during_pointer_scope_restore_keeps_input_metadata_removed() {
    use std::{cell::Cell, rc::Rc};
    let mut ui = Ui::new(900., 600.);
    let nested = ui.signal(false);
    let mounted = ui.mount(
        menu("Root", ui.signal(true), button().child(text("Root")))
            .child(
                submenu("More", nested.clone())
                    .trigger_id("more")
                    .child(menu_item("Child")),
            )
            .child(menu_item("Parent action").id("parent-action")),
    );
    ui.prepare_frame();
    let trigger = mounted.find("more").unwrap();
    let armed = Rc::new(Cell::new(false));
    let callback_armed = armed.clone();
    let owner = mounted.clone();
    ui.on_event(trigger, true, move |cx| {
        if callback_armed.get()
            && cx.phase == zgx::input::EventPhase::Target
            && matches!(cx.event, InputEvent::Focus)
        {
            owner.unmount();
        }
    });
    nested.set(true);
    ui.prepare_frame();
    let overlay = ui.input.focus_scope().unwrap();
    let bounds = ui
        .scene
        .borrow()
        .bounds(mounted.find("parent-action").unwrap());
    armed.set(true);
    ui.dispatch(InputEvent::PointerDown {
        x: bounds.x + 10.,
        y: bounds.y + 10.,
        button: zgx::input::PointerButton::Primary,
    });
    assert!(!mounted.is_mounted());
    assert!(ui.input.focus_scope().is_none());
    assert!(
        ui.input.options(overlay).is_none(),
        "disposed overlay must not regain input metadata"
    );
}
