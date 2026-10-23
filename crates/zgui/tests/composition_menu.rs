use zgui::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
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
#[test]
fn menu_keyboard_wrap_skip_typeahead_and_close_before_action() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(false);
    let calls = ui.signal(0);
    let action = calls.clone();
    let state = open.clone();
    let view = ui.mount(
        menu("Actions", open.clone(), button().id("anchor").child("Menu"))
            .p(0.)
            .children([
                menu_item("Alpha").id("alpha"),
                menu_item("Disabled").id("disabled").disabled(true),
                menu_item("Beta").id("beta").on_click(move || {
                    assert!(!state.get());
                    action.set(action.get() + 1);
                }),
                menu_item("Bravo").id("bravo"),
            ]),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("anchor"));
    open.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), view.find("alpha"));
    key(&mut ui, Key::ArrowUp);
    assert_eq!(ui.input.focused(), view.find("bravo"));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.input.focused(), view.find("alpha"));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.input.focused(), view.find("beta"));
    key(&mut ui, Key::Character("b".into()));
    assert_eq!(ui.input.focused(), view.find("bravo"));
    key(&mut ui, Key::Character("b".into()));
    assert_eq!(ui.input.focused(), view.find("beta"));
    key(&mut ui, Key::Enter);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    assert_eq!(calls.get(), 1);
    assert!(!open.get());
    assert_eq!(ui.input.focused(), view.find("anchor"));
    let anchor = view.find("anchor").unwrap();
    assert_eq!(
        ui.semantics.borrow().get(anchor).unwrap().expanded,
        Some(false)
    );
}
#[test]
fn menu_tab_closes_and_leaves_in_each_direction() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(false);
    let view = ui.mount(
        column()
            .child(button().id("before"))
            .child(
                menu("Menu", open.clone(), button().id("anchor"))
                    .child(menu_item("Item").id("item")),
            )
            .child(button().id("after")),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("anchor"));
    open.set(true);
    key(&mut ui, Key::Tab);
    assert!(!open.get());
    assert_eq!(ui.input.focused(), view.find("after"));
    ui.input.focus(&ui.scene, view.find("anchor"));
    open.set(true);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert!(!open.get());
    assert_eq!(ui.input.focused(), view.find("before"));
}
#[test]
fn keyed_component_menu_items_retain_identity_and_recover_after_focused_removal() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(true);
    let items = ui.signal(vec![1, 2, 3]);
    let read = items.clone();
    let view = ui.mount(menu("Items", open, button()).child(keyed(
        move || read.get(),
        |n, _| component(move |_| menu_item(format!("Item {n}")).id(n.to_string())),
    )));
    ui.prepare_frame();
    key(&mut ui, Key::ArrowDown);
    let second = view.find("2").unwrap();
    assert_eq!(ui.input.focused(), Some(second));
    items.set(vec![3, 2, 1]);
    assert_eq!(view.find("2"), Some(second));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.input.focused(), view.find("1"));
    items.set(vec![3, 2]);
    assert_eq!(ui.input.focused(), None);
    key(&mut ui, Key::Home);
    assert_eq!(ui.input.focused(), view.find("3"));
    assert_eq!(
        ui.semantics.borrow().get(second).unwrap().role,
        Role::MenuItem
    );
}
#[test]
fn nested_menu_activation_closes_entire_chain_before_callback() {
    let mut ui = Ui::new(500., 400.);
    let outer = ui.signal(false);
    let inner = ui.signal(false);
    let first = outer.clone();
    let second = inner.clone();
    let calls = ui.signal(0);
    let count = calls.clone();
    let view = ui.mount(menu("Outer", outer.clone(), button().id("anchor")).child(
        menu("Inner", inner.clone(), button().id("subanchor")).child(
            menu_item("Action").id("action").on_click(move || {
                assert!(!first.get() && !second.get());
                count.set(count.get() + 1);
            }),
        ),
    ));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("anchor"));
    outer.set(true);
    inner.set(true);
    ui.prepare_frame();
    ui.input.dispatch_to(
        &ui.scene,
        view.find("action").unwrap(),
        InputEvent::Activate,
    );
    assert_eq!(calls.get(), 1);
    assert!(ui.input.focus_scope().is_none());
    assert_eq!(ui.input.focused(), view.find("anchor"));
}
#[test]
fn disabled_menu_items_reject_pointer_and_invalid_leaf_cleanup() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(true);
    let calls = ui.signal(0);
    let count = calls.clone();
    let view = ui.mount(
        menu("Menu", open.clone(), button().w(50.).h(20.))
            .p(0.)
            .child(
                menu_item("Disabled")
                    .w(160.)
                    .h(30.)
                    .p(0.)
                    .disabled(true)
                    .id("disabled")
                    .on_click(move || {
                        count.set(count.get() + 1);
                    }),
            )
            .child(menu_item("Enabled").id("enabled")),
    );
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(view.find("disabled").unwrap());
    for event in [
        InputEvent::PointerDown {
            x: bounds.x + 10.,
            y: bounds.y + 10.,
            button: PointerButton::Primary,
        },
        InputEvent::PointerUp {
            x: bounds.x + 10.,
            y: bounds.y + 10.,
            button: PointerButton::Primary,
        },
    ] {
        ui.dispatch(event);
    }
    assert_eq!(calls.get(), 0);
    assert!(open.get());
    assert_eq!(ui.input.focused(), view.find("enabled"));
    view.unmount();
    let before = ui.scene.borrow().len();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || ui.mount(button().read_only(true))
        ))
        .is_err()
    );
    assert_eq!(ui.scene.borrow().len(), before);
}

#[test]
fn repeated_typeahead_cycles_even_when_a_label_matches_doubled_letters() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        menu("Names", ui.signal(true), button().child("Names"))
            .child(menu_item("Aaron").id("aaron"))
            .child(menu_item("Abby").id("abby"))
            .child(menu_item("Alpha").id("alpha")),
    );
    ui.prepare_frame();
    for expected in ["abby", "alpha", "aaron"] {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Character("a".into()),
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert_eq!(ui.input.focused(), view.find(expected));
    }
}

#[test]
fn items_outside_a_menu_are_plain_actions() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(true);
    let child_open = ui.signal(false);
    let calls = ui.signal(0);
    let action = calls.clone();
    let view = ui.mount(
        popover("Tools", open.clone(), button().id("anchor").child("Tools")).children([
            menu_item("Zoom").id("zoom").on_click(move || {
                action.set(action.get() + 1);
            }),
            submenu("More", child_open.clone())
                .trigger_id("more")
                .child(menu_item("Inner").id("inner")),
        ]),
    );
    ui.prepare_frame();
    let zoom = view.find("zoom").unwrap();
    ui.input.dispatch_to(&ui.scene, zoom, InputEvent::Activate);
    // Nothing to close: the popover stays open and the action runs.
    assert_eq!(calls.get(), 1);
    assert!(open.get());
    let more = view.find("more").unwrap();
    ui.input.dispatch_to(&ui.scene, more, InputEvent::Activate);
    ui.prepare_frame();
    assert!(child_open.get());
    assert!(view.find("inner").is_some());
}

/// A menu opens as its trigger is pressed, as AppKit's pop-up buttons do:
/// dragged to an item and released there, the item is chosen; released over
/// the trigger, the menu stays open to click in.
#[test]
fn menus_open_on_press_and_choose_on_release() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(false);
    let chosen = ui.signal(String::new());
    let pick = |label: &'static str| {
        let chosen = chosen.clone();
        menu_item(label).id(label).h(24.).on_click(move || {
            chosen.set(label.to_owned());
        })
    };
    let view = ui.mount(
        menu("Size", open.clone(), button().id("anchor").size(120., 28.))
            .w(160.)
            .p(0.)
            .children([pick("small"), pick("medium"), pick("large")]),
    );
    ui.prepare_frame();
    let scene = ui.scene.clone();
    let center = |id: &str| {
        let bounds = scene.borrow().bounds(view.find(id).unwrap());
        (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
    };
    let button = PointerButton::Primary;
    let (x, y) = center("anchor");
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.prepare_frame();
    assert!(open.get(), "open as pressed");
    // Released over the trigger: still open, nothing chosen.
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
    assert!(open.get());
    assert_eq!(chosen.get(), "");
    // A press outside closes it; pressed again, dragged to an item and
    // released there, the item is chosen.
    ui.dispatch(InputEvent::PointerDown {
        x: 450.,
        y: 380.,
        button,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 450.,
        y: 380.,
        button,
    });
    ui.prepare_frame();
    assert!(!open.get());
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.prepare_frame();
    let (ix, iy) = center("large");
    ui.dispatch(InputEvent::PointerMove { x: ix, y: iy });
    assert_eq!(ui.input.focused(), view.find("large"), "highlighted");
    ui.dispatch(InputEvent::PointerUp {
        x: ix,
        y: iy,
        button,
    });
    ui.prepare_frame();
    assert_eq!(chosen.get(), "large");
    assert!(!open.get());
    // Dragged away from it all and released: closed, nothing chosen.
    chosen.set(String::new());
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.prepare_frame();
    assert!(open.get());
    ui.dispatch(InputEvent::PointerMove { x: 450., y: 380. });
    ui.dispatch(InputEvent::PointerUp {
        x: 450.,
        y: 380.,
        button,
    });
    ui.prepare_frame();
    assert!(!open.get());
    assert_eq!(chosen.get(), "");
}

/// A click whose press and release arrive together, before the menu shows
/// (as X11 test input does): the menu stays open with its first item
/// focused (unseen, as after any press), and a click on one of its disabled
/// items later neither closes it nor chooses anything.
#[test]
fn a_click_before_the_menu_shows_leaves_it_open() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(false);
    let chosen = ui.signal(String::new());
    let pick = |label: &'static str| {
        let chosen = chosen.clone();
        menu_item(label).id(label).h(24.).on_click(move || {
            chosen.set(label.to_owned());
        })
    };
    let view = ui.mount(
        menu("Size", open.clone(), button().id("anchor").size(120., 28.))
            .w(160.)
            .p(0.)
            .children([pick("small"), pick("medium").disabled(true), pick("large")]),
    );
    ui.prepare_frame();
    let scene = ui.scene.clone();
    let center = |id: &str| {
        let bounds = scene.borrow().bounds(view.find(id).unwrap());
        (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
    };
    let button = PointerButton::Primary;
    let (x, y) = center("anchor");
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
    assert!(open.get());
    assert_eq!(ui.input.focused(), view.find("small"));
    let (dx, dy) = center("medium");
    ui.dispatch(InputEvent::PointerMove { x: dx, y: dy });
    ui.dispatch(InputEvent::PointerDown {
        x: dx,
        y: dy,
        button,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: dx,
        y: dy,
        button,
    });
    ui.prepare_frame();
    assert!(open.get(), "a disabled item's click keeps it open");
    assert_eq!(chosen.get(), "");
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.input.focused(), view.find("large"), "past the disabled");
}

/// Right to left, a menu lines up with its trigger's right edge (its start)
/// and opens leftward, rather than past the window's right edge.
#[test]
fn a_menu_opens_from_its_triggers_start_right_to_left() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(true);
    let view = ui.mount(
        row().w_full().px(60.).rtl().child(
            menu("Size", open.clone(), button().id("anchor").size(120., 28.))
                .id("menu")
                .w(200.)
                .p(0.)
                .children([menu_item("small").h(24.)]),
        ),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let anchor = scene.bounds(view.find("anchor").unwrap());
    let panel = scene.bounds(view.find("menu").unwrap());
    assert!(
        anchor.x > 300.,
        "the trigger starts at the right: {anchor:?}"
    );
    assert_eq!(panel.x + panel.width, anchor.x + anchor.width, "{panel:?}");
}

/// A popover's arrow (`View::arrow`) sits between it and its trigger,
/// under the trigger's centre, and the panel opens that much further away.
#[test]
fn a_popovers_arrow_points_at_its_trigger() {
    let mut ui = Ui::new(500., 400.);
    let open = ui.signal(true);
    let view = ui.mount(
        column().p(20.).child(
            popover("Details", open, button().id("anchor").size(80., 28.))
                .id("panel")
                .w(200.)
                .arrow(8.),
        ),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let anchor = scene.bounds(view.find("anchor").unwrap());
    let panel_node = view.find("panel").unwrap();
    let panel = scene.bounds(panel_node);
    // Below the trigger, 4 points and the arrow's 8 away.
    assert_eq!(panel.y, anchor.y + anchor.height + 12.);
    let position = scene.parent(panel_node).unwrap();
    let arrow = scene.bounds(scene.children(position)[1]);
    assert_eq!(arrow.height, 8.);
    assert_eq!(arrow.y + arrow.height, panel.y, "on the panel's top edge");
    assert_eq!(
        arrow.x + arrow.width / 2.,
        anchor.x + anchor.width / 2.,
        "under the trigger's centre"
    );
}
#[test]
fn a_menu_in_a_popup_window_names_its_trigger_while_open() {
    let mut ui = Ui::new(500., 400.);
    ui.set_popup_screens(vec![zgui::scene::Rect::new(0., 0., 1000., 800.)]);
    let open = ui.signal(false);
    let view = ui.mount(
        menu("Actions", open.clone(), button().id("anchor").child("Menu"))
            .children([menu_item("Alpha").id("alpha")]),
    );
    ui.prepare_frame();
    assert!(ui.scene.borrow().popups().is_empty());
    open.set(true);
    ui.prepare_frame();
    let root = *ui.scene.borrow().popups().first().expect("a popup root");
    assert_eq!(ui.scene.borrow().popup_anchor(root), view.find("anchor"));
    open.set(false);
    ui.prepare_frame();
    assert!(ui.scene.borrow().popups().is_empty());
    assert_eq!(ui.scene.borrow().popup_anchor(root), None);
}
