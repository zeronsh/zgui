//! Scrolling right to left: a horizontal view starts at its right (offset 0
//! shows the start), moving on reveals what is to the left, the wheel's
//! physical deltas read that way, and scrollbars sit at the start edge.
use zgui::{compose::prelude::*, input::InputEvent, scene::Rect, semantics::Role, widgets::Ui};

fn scrollbars(ui: &Ui) -> Vec<Rect> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut found = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(id) = pending.pop() {
        if semantics.get(id).is_some_and(|n| n.role == Role::ScrollBar) {
            found.push(scene.bounds(id));
        }
        pending.extend(scene.children(id).iter().copied());
    }
    found
}

#[test]
fn a_horizontal_view_starts_at_its_right() {
    let mut ui = Ui::new(600., 400.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        column().rtl().child(
            scroll_x(offset.clone())
                .id("view")
                .w(200.)
                .h(40.)
                .scrollbar(true)
                .child(div().id("first").w(100.).h(40.).shrink_0())
                .child(div().id("second").w(100.).h(40.).shrink_0())
                .child(div().id("third").w(100.).h(40.).shrink_0()),
        ),
    );
    ui.prepare_frame();
    let rect = |ui: &Ui, id: &str| ui.scene.borrow().bounds(view.find(id).unwrap());
    let viewport = rect(&ui, "view");
    // The first item shows at the right edge.
    assert_eq!(rect(&ui, "first").x + 100., viewport.x + viewport.width);
    assert_eq!(rect(&ui, "second").x, viewport.x);
    // Moving on, by the offset, brings the third in from the left.
    offset.set(100.);
    ui.prepare_frame();
    assert_eq!(rect(&ui, "third").x, viewport.x);
    // A wheel to the right goes back toward the start.
    let at = (viewport.x + 50., viewport.y + 20.);
    ui.dispatch(InputEvent::Scroll {
        x: at.0,
        y: at.1,
        delta_x: 60.,
        delta_y: 0.,
    });
    ui.prepare_frame();
    assert_eq!(offset.get(), 40.);
    // Its scrollbar's thumb is at the right while it shows the start.
    offset.set(0.);
    ui.prepare_frame();
    let bars = scrollbars(&ui);
    assert_eq!(bars.len(), 1, "{bars:?}");
    // Its thumb is at the right while the view shows its start.
    let scene = ui.scene.borrow();
    let mut deepest = Vec::new();
    let mut pending = vec![view.find("view").unwrap()];
    while let Some(id) = pending.pop() {
        let b = scene.bounds(id);
        if b.height > 0. && b.height < 10. && b.width > 0. && b.width < 200. {
            deepest.push(b);
        }
        pending.extend(scene.children(id).iter().copied());
    }
    assert!(
        deepest
            .iter()
            .any(|b| b.x + b.width > viewport.x + viewport.width - 10.),
        "a thumb at the right: {deepest:?}"
    );
}

#[test]
fn a_vertical_scrollbar_sits_at_the_start_edge() {
    let mut ui = Ui::new(600., 400.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        column().rtl().child(
            scroll(offset)
                .id("view")
                .w(200.)
                .h(100.)
                .scrollbar(true)
                .child(div().h(400.)),
        ),
    );
    ui.prepare_frame();
    let viewport = ui.scene.borrow().bounds(view.find("view").unwrap());
    let bars = scrollbars(&ui);
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].x, viewport.x, "{bars:?} in {viewport:?}");
}
