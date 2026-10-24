use std::time::{Duration, Instant};
use zgx::{
    compose::prelude::*,
    input::InputEvent,
    reactive::Signal,
    scene::{NodeId, NodeKind},
    semantics::Role,
    widgets::Ui,
};

fn bar(ui: &Ui) -> NodeId {
    ui.semantics
        .borrow()
        .iter()
        .find(|(_, n)| n.role == Role::ScrollBar)
        .unwrap()
        .0
}

/// The thumb's drawn size and opacity (0 to 255).
fn thumb(ui: &Ui) -> (f32, f32, u8) {
    let bar = bar(ui);
    let scene = ui.scene.borrow();
    let thumb = *scene.children(bar).last().unwrap();
    let bounds = scene.bounds(thumb);
    let NodeKind::Quad(quad) = scene.kind(thumb) else {
        panic!("a quad thumb");
    };
    (bounds.width, bounds.height, quad.fill.3)
}

fn mount(ui: &mut Ui) -> Signal<f32> {
    let offset = ui.signal(0.);
    ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .scrollbar(true)
            .child(div().h(400.)),
    );
    ui.prepare_frame();
    offset
}

fn wheel(ui: &mut Ui) {
    ui.dispatch(InputEvent::Scroll {
        x: 50.,
        y: 50.,
        delta_x: 0.,
        delta_y: 30.,
    });
    ui.prepare_frame();
}

#[test]
fn a_scrollbar_shows_at_rest_unless_scrollbars_are_overlays() {
    let mut ui = Ui::new(300., 200.);
    let offset = mount(&mut ui);
    let (width, height, alpha) = thumb(&ui);
    assert!(width > 0. && height > 0. && alpha > 0, "shown at rest");
    wheel(&mut ui);
    assert!(offset.get() > 0.);
    assert_eq!(ui.next_interaction_deadline(), None, "nothing to fade");

    ui.set_overlay_scrollbars(true);
    ui.prepare_frame();
    assert_eq!(thumb(&ui).0, 0., "an overlay hides at rest");
    assert_eq!(
        ui.scene.borrow().bounds(bar(&ui)).width,
        0.,
        "and the pointer reaches the content under it"
    );
    assert!(
        ui.semantics.borrow().get(bar(&ui)).is_some(),
        "assistive technology still has it"
    );
    // Off again, it shows as before.
    ui.set_overlay_scrollbars(false);
    ui.prepare_frame();
    assert!(thumb(&ui).0 > 0.);
}

#[test]
fn an_overlay_scrollbar_shows_as_the_user_scrolls_then_fades() {
    let mut ui = Ui::new(300., 200.);
    ui.set_overlay_scrollbars(true);
    let offset = mount(&mut ui);
    assert_eq!(thumb(&ui).0, 0.);
    // The app scrolling it does not show it, as a page's script does not.
    offset.set(20.);
    ui.prepare_frame();
    assert_eq!(thumb(&ui).0, 0.);
    assert_eq!(ui.next_interaction_deadline(), None);

    let start = Instant::now();
    wheel(&mut ui);
    let (width, _, alpha) = thumb(&ui);
    assert!(width > 0. && alpha > 0, "the user's scroll shows it");
    let deadline = ui.next_interaction_deadline().expect("it will fade");
    assert!(deadline >= start + Duration::from_millis(900));
    // It stays a second, then fades over a fifth of one.
    ui.advance_interactions(deadline - Duration::from_millis(10))
        .unwrap();
    assert_eq!(thumb(&ui).2, alpha, "it stays");
    ui.advance_interactions(deadline + Duration::from_millis(100))
        .unwrap();
    let half = thumb(&ui).2;
    assert!(0 < half && half < alpha, "fading: {half} of {alpha}");
    ui.advance_interactions(deadline + Duration::from_millis(250))
        .unwrap();
    assert_eq!(thumb(&ui).0, 0., "hidden");
    assert_eq!(
        ui.next_interaction_deadline(),
        None,
        "and nothing scheduled"
    );
}

#[test]
fn the_pointer_on_an_overlay_scrollbar_keeps_it() {
    let mut ui = Ui::new(300., 200.);
    ui.set_overlay_scrollbars(true);
    mount(&mut ui);
    wheel(&mut ui);
    let bounds = ui.scene.borrow().bounds(bar(&ui));
    let (x, y) = (bounds.x + bounds.width / 2., bounds.y + 5.);
    ui.dispatch(InputEvent::PointerMove { x, y });
    ui.prepare_frame();
    assert_eq!(ui.next_interaction_deadline(), None, "held");
    ui.advance_interactions(Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert!(thumb(&ui).0 > 0., "still shown");
    // Off it, it lingers, then goes.
    ui.dispatch(InputEvent::PointerMove { x: 50., y: 50. });
    ui.prepare_frame();
    let deadline = ui.next_interaction_deadline().expect("it will fade");
    ui.advance_interactions(deadline + Duration::from_millis(250))
        .unwrap();
    assert_eq!(thumb(&ui).0, 0.);
}
