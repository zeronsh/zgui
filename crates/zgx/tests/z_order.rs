//! `z`: raised nodes paint and take hits over their siblings, while layout,
//! focus and the accessibility tree keep document order.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    reactive::Signal,
    scene::NodeId,
    style::Styles,
    widgets::Ui,
};

/// Two overlapping boxes, `a` before `b` in the document, each `z` given.
fn stacked(z_a: i32, z_b: i32) -> View {
    overlay()
        .size(100., 100.)
        .child(div().id("a").size(100., 100.).bg(rgb(0xff0000)).z(z_a))
        .child(div().id("b").size(100., 100.).bg(rgb(0x00ff00)).z(z_b))
}
fn painted(ui: &Ui) -> Vec<NodeId> {
    ui.scene
        .borrow()
        .layer_items(None)
        .iter()
        .map(|item| item.id)
        .collect()
}

#[test]
fn a_raised_node_paints_and_takes_hits_over_a_later_sibling() {
    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(stacked(1, 0));
    ui.prepare_frame();
    let (a, b) = (view.find("a").unwrap(), view.find("b").unwrap());
    let order = painted(&ui);
    let at = |id| order.iter().position(|item| *item == id).unwrap();
    assert!(
        at(a) > at(b),
        "the first in the document paints last: {order:?}"
    );
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), Some(a));
}

#[test]
fn equal_z_keeps_document_order_and_lower_goes_under() {
    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(stacked(0, 0));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), view.find("b"));

    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(stacked(2, 2));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), view.find("b"));

    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(stacked(0, -1));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), view.find("a"));
}

#[test]
fn z_orders_siblings_only_not_past_what_the_parent_is_under() {
    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(
        overlay()
            .size(100., 100.)
            .child(
                div()
                    .size(100., 100.)
                    .child(div().id("inner").size(100., 100.).z(100)),
            )
            .child(div().id("over").size(100., 100.)),
    );
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), view.find("over"));
}

#[test]
fn layout_stays_in_document_order() {
    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(
        column()
            .child(div().id("first").size(50., 20.).z(5))
            .child(div().id("second").size(50., 20.)),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    assert_eq!(scene.bounds(view.find("first").unwrap()).y, 0.);
    assert_eq!(scene.bounds(view.find("second").unwrap()).y, 20.);
}

#[test]
fn focus_keeps_document_order() {
    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(
        overlay()
            .size(100., 100.)
            .child(button().id("a").size(100., 40.).z(1))
            .child(button().id("b").size(100., 40.)),
    );
    ui.prepare_frame();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(
        ui.input.focused(),
        view.find("a"),
        "Tab starts at the first in the document"
    );
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(ui.input.focused(), view.find("b"));
}

/// What the accessibility tree is built from stays in document order.
#[test]
fn the_accessibility_walk_keeps_document_order() {
    let mut ui = Ui::new(200., 200.);
    let view = ui.mount(stacked(1, 0));
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let order: Vec<_> = scene.paint_items().map(|item| item.id).collect();
    let at = |id| order.iter().position(|item| *item == id).unwrap();
    assert!(at(view.find("a").unwrap()) < at(view.find("b").unwrap()));
}

/// Children come and go, and `z` changes: the order follows, and nothing is
/// kept for what's gone.
#[test]
fn the_order_follows_changes_and_goes_with_its_nodes() {
    let mut ui = Ui::new(200., 200.);
    let raised: Signal<i32> = ui.runtime.signal(1);
    let shown = ui.runtime.signal(true);
    let (read, read_shown) = (raised.clone(), shown.clone());
    let view = ui.mount(
        overlay()
            .size(100., 100.)
            .child(
                div()
                    .id("a")
                    .size(100., 100.)
                    .reactive_style(move || Styles::new().z(read.get())),
            )
            .child(switch(
                move || read_shown.get(),
                |shown, _| match shown {
                    true => div().id("b").size(100., 100.),
                    false => div(),
                },
            )),
    );
    ui.prepare_frame();
    let a = view.find("a");
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), a);
    raised.set(0);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), view.find("b"));
    raised.set(3);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), a);
    // A sibling removed under it: the walks meet no dead node.
    shown.set(false);
    for _ in 0..3 {
        ui.prepare_frame();
        ui.reclaim(std::time::Duration::from_secs(1));
    }
    assert_eq!(ui.scene.borrow().hit_test(50., 50.), a);
    let _ = painted(&ui);
}

/// Raised, a node repaints all it paints: a child past its box too.
#[test]
fn raising_a_node_repaints_what_it_holds() {
    let mut ui = Ui::new(200., 200.);
    let raised: Signal<i32> = ui.runtime.signal(0);
    let read = raised.clone();
    ui.mount(
        overlay()
            .size(200., 200.)
            .child(
                div()
                    .size(40., 40.)
                    .reactive_style(move || Styles::new().z(read.get()))
                    .child(
                        div()
                            .absolute()
                            .left(120.)
                            .top(120.)
                            .size(30., 30.)
                            .bg(rgb(0xff0000)),
                    ),
            )
            .child(div().size(200., 200.).bg(rgb(0x00ff00))),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    raised.set(1);
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    assert!(
        damage.iter().any(|rect| rect.x <= 120.
            && rect.y <= 120.
            && rect.x + rect.width >= 150.
            && rect.y + rect.height >= 150.),
        "the child past its box repaints: {damage:?}"
    );
}
