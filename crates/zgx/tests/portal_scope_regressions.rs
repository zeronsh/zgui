use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent},
    widgets::Ui,
};

#[test]
fn unmounting_opening_modal_from_prior_focus_blur_is_safe() {
    let mut ui = Ui::new(400., 300.);
    let trigger = ui.mount(button().child(text("Trigger")));
    let open = ui.signal(false);
    let dialog = ui.mount(modal("Dialog", open.clone()).child(button().child(text("Close"))));
    let remove = dialog.clone();
    ui.on_event(trigger.node(), true, move |cx| {
        if cx.phase == EventPhase::Target && matches!(cx.event, InputEvent::Blur) {
            remove.unmount();
        }
    });
    ui.input.focus(&ui.scene, Some(trigger.node()));
    open.set(true);
    ui.prepare_frame();
    assert!(!dialog.is_mounted());
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn render_replacement_preserves_new_modal_focus_after_disposing_old_portal() {
    let mut ui = Ui::new(400., 300.);
    let old = ui.render(modal("Old", ui.signal(true)).child(button().id("old").child(text("Old"))));
    let new = ui.render(modal("New", ui.signal(true)).child(button().id("new").child(text("New"))));
    ui.prepare_frame();
    assert!(!old.is_mounted());
    assert!(new.is_mounted());
    assert_eq!(ui.input.focused(), new.find("new"));
    assert!(ui.input.focus_scope().is_some());
    new.unmount();
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn disabled_modal_panel_has_no_enabled_descendant_and_escape_still_dismisses() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(true);
    let dialog = ui.mount(
        modal("Disabled", open.clone())
            .disabled(true)
            .child(button().id("disabled-child").child(text("Blocked"))),
    );
    ui.prepare_frame();
    let child = dialog.find("disabled-child").unwrap();
    assert!(!ui.input.is_enabled(&ui.scene.borrow(), child));
    assert!(!ui.input.focus(&ui.scene, Some(child)));
    ui.dispatch(InputEvent::KeyDown {
        key: zgx::input::Key::Escape,
        modifiers: Default::default(),
        repeat: false,
    });
    assert!(!open.get());
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn modal_slot_keeps_callers_provider_when_receiver_shadows_it() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(true);
    let dialog = ui.mount(provide(
        String::from("caller"),
        component(move |cx| {
            let slot = cx.slot(|cx| text(cx.service::<String>().as_str()).id("slotted"));
            provide(String::from("receiver"), modal("Slot", open).child(slot))
        }),
    ));
    ui.prepare_frame();
    let label = dialog.find("slotted").unwrap();
    match ui.scene.borrow().kind(label) {
        zgx::scene::NodeKind::Text { text, .. } => assert_eq!(text.as_ref(), "caller"),
        _ => panic!("expected text"),
    }
}

fn logical_disabled_portal(anchored: bool) {
    use std::{cell::Cell, rc::Rc};
    let mut ui = Ui::new(400., 300.);
    let disabled = ui.signal(true);
    let read_disabled = disabled.clone();
    let open = ui.signal(true);
    let activations = Rc::new(Cell::new(0));
    let clicked = activations.clone();
    let portal = if anchored {
        popover(
            "Logical disabled",
            open.clone(),
            button().id("anchor").child(text("Anchor")),
        )
    } else {
        modal("Logical disabled", open.clone())
    };
    let mounted = ui.mount(
        column().disabled_when(move || read_disabled.get()).child(
            portal.child(
                button()
                    .id("inside")
                    .child(text("Action"))
                    .on_click(move || clicked.set(clicked.get() + 1)),
            ),
        ),
    );
    ui.prepare_frame();
    let inside = mounted.find("inside").unwrap();
    assert!(
        ui.input.focus_scope().is_none(),
        "initial disabled owner must not trap focus"
    );
    assert!(!ui.input.focus(&ui.scene, Some(inside)));
    ui.input
        .dispatch_to(&ui.scene, inside, InputEvent::Activate);
    assert_eq!(activations.get(), 0);
    assert!(
        !open.get(),
        "blocked opening resets the requested open state"
    );
    disabled.set(false);
    ui.prepare_frame();
    assert!(
        ui.input.focus_scope().is_none(),
        "reenabling does not unexpectedly reopen a modal"
    );
    open.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), Some(inside));
    ui.input
        .dispatch_to(&ui.scene, inside, InputEvent::Activate);
    assert_eq!(activations.get(), 1);
    disabled.set(true);
    ui.prepare_frame();
    assert!(!open.get());
    assert!(
        ui.input.focus_scope().is_none(),
        "disabling logical owner unwinds active focus scope"
    );
    assert_ne!(ui.input.focused(), Some(inside));
    ui.input
        .dispatch_to(&ui.scene, inside, InputEvent::Activate);
    assert_eq!(activations.get(), 1);
    disabled.set(false);
    open.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), Some(inside));
    mounted.unmount();
    assert!(ui.input.focus_scope().is_none());
}
#[test]
fn modal_inherits_logical_ancestor_disability() {
    logical_disabled_portal(false);
}
#[test]
fn popover_inherits_logical_ancestor_disability() {
    logical_disabled_portal(true);
}

#[test]
fn portals_name_their_owners_until_they_unmount() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let read = shown.clone();
    let (outer, inner) = (ui.signal(true), ui.signal(true));
    let view = ui.mount(switch(
        move || read.get(),
        move |shown, _| match shown {
            true => modal("Outer", outer.clone())
                .id("outer")
                .child(modal("Inner", inner.clone()).id("inner").child(text("in"))),
            false => div(),
        },
    ));
    ui.prepare_frame();
    for _ in 0..2 {
        let portals = ui.portals();
        assert_eq!(portals.len(), 2);
        let scene = ui.scene.borrow();
        // Each overlay sits outside its owner: the outer under the root,
        // the inner in the outer's overlay.
        assert_eq!(scene.parent(portals[0].1), Some(ui.root()));
        assert_eq!(scene.parent(portals[1].1), Some(portals[0].1));
        assert!(!scene.is_within(portals[0].1, portals[0].0));
        drop(scene);
        shown.set(false);
        ui.prepare_frame();
        assert!(ui.portals().is_empty(), "gone with their owners");
        shown.set(true);
        ui.prepare_frame();
    }
    view.unmount();
    assert!(ui.portals().is_empty());
}
