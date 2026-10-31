//! Hit targets: a node animating somewhere takes hits where it's going,
//! not where it shows, and what it holds goes with it.
use zgx::{
    compose::prelude::*,
    scene::{HitTarget, LocalTransform, NodeId, Rect, Transform},
    widgets::Ui,
};

/// A 100 by 40 card at the window's left, shown 200 to its right as if an
/// animation were taking it back, and a button inside it.
fn card(ui: &mut Ui) -> (zgx::compose::ViewHandle, NodeId, NodeId) {
    let view = ui.mount(
        overlay().w(400.).h(100.).child(
            div().id("card").w(100.).h(40.).child(
                div()
                    .id("button")
                    .w(50.)
                    .h(40.)
                    .on_click(|| {})
                    .hover(|s| s.bg(rgb(0x336699))),
            ),
        ),
    );
    ui.prepare_frame();
    let (card, button) = (view.find("card").unwrap(), view.find("button").unwrap());
    ui.scene
        .borrow_mut()
        .set_transform(card, Transform { x: 200., y: 0. });
    (view, card, button)
}
fn at(x: f32, y: f32) -> HitTarget {
    HitTarget {
        translation: Transform { x, y },
        ..HitTarget::default()
    }
}

#[test]
fn a_node_and_what_it_holds_take_hits_where_they_are_going() {
    let mut ui = Ui::new(400., 100.);
    let (_view, card, button) = card(&mut ui);
    assert_eq!(
        ui.scene.borrow().hit_test(225., 20.),
        Some(button),
        "where it shows"
    );
    ui.scene.borrow_mut().set_hit_target(card, Some(at(0., 0.)));
    let scene = ui.scene.borrow();
    assert_eq!(scene.hit_test(25., 20.), Some(button), "where it is going");
    assert_eq!(scene.hit_test(75., 20.), Some(card));
    assert_ne!(
        scene.hit_test(225., 20.),
        Some(button),
        "not where it shows"
    );
    assert_eq!(
        scene.transformed_bounds(card).x,
        200.,
        "it still shows there"
    );
    assert_eq!(scene.hit_bounds(button), Rect::new(0., 0., 50., 40.));
}

#[test]
fn letting_go_of_a_target_hits_it_where_it_shows() {
    let mut ui = Ui::new(400., 100.);
    let (_view, card, button) = card(&mut ui);
    ui.scene.borrow_mut().set_hit_target(card, Some(at(0., 0.)));
    ui.scene.borrow_mut().set_hit_target(card, None);
    let scene = ui.scene.borrow();
    assert_eq!(scene.hit_test(225., 20.), Some(button));
    assert_eq!(scene.hit_target(card), None);
    assert_eq!(scene.hit_bounds(button), scene.transformed_bounds(button));
}

#[test]
fn a_node_going_out_takes_no_hits() {
    let mut ui = Ui::new(400., 100.);
    let (_view, card, _) = card(&mut ui);
    ui.scene.borrow_mut().set_hit_target(
        card,
        Some(HitTarget {
            translation: Transform { x: 200., y: 0. },
            gone: true,
            ..HitTarget::default()
        }),
    );
    let scene = ui.scene.borrow();
    assert!(
        !matches!(scene.hit_test(225., 20.), Some(id) if id == card || scene.is_within(id, card))
    );
}

#[test]
fn a_scale_it_is_going_to_hits_as_it_will_be() {
    let mut ui = Ui::new(400., 100.);
    let (_view, card, _) = card(&mut ui);
    // Shown at 200, going to 0 and twice as wide about its left edge.
    let local = LocalTransform {
        scale: (2., 1.),
        origin: (0., 0.5),
        ..LocalTransform::default()
    };
    ui.scene.borrow_mut().set_hit_target(
        card,
        Some(HitTarget {
            local: Some(local),
            ..at(0., 0.)
        }),
    );
    let scene = ui.scene.borrow();
    let hit = scene.hit_test(150., 20.);
    assert!(
        hit.is_some_and(|id| id == card || scene.is_within(id, card)),
        "{hit:?}"
    );
}

#[test]
fn hover_follows_a_target_under_a_resting_pointer() {
    let mut ui = Ui::new(400., 100.);
    let (_view, card, button) = card(&mut ui);
    ui.dispatch(zgx::input::InputEvent::PointerMove { x: 25., y: 20. });
    ui.prepare_frame();
    assert_ne!(ui.input.hovered(), Some(button));
    ui.scene.borrow_mut().set_hit_target(card, Some(at(0., 0.)));
    ui.prepare_frame();
    assert_eq!(ui.input.hovered(), Some(button), "it came to the pointer");
    ui.scene.borrow_mut().set_hit_target(card, None);
    ui.prepare_frame();
    assert_ne!(ui.input.hovered(), Some(button), "and went");
}
