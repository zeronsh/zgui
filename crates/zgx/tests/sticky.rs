//! `sticky_top`: a node holds at the top of its scroll view as the view
//! scrolls past it, as far as its parent goes, with no script running.
//! The expected numbers are worked out from CSS's `position: sticky`.
use zgx::{compose::prelude::*, reactive::Signal, scene::NodeId, style::Styles, widgets::Ui};

/// A 100 tall scroll view over three 300 tall sections, each a 20 tall
/// header (the one in `id`'s section sticks, `top` below the top) and rows.
fn sections(offset: Signal<f32>, top: f32) -> View {
    let section = |name: &'static str| {
        column()
            .id(name)
            .w(100.)
            .h(300.)
            .child(
                div()
                    .id(format!("{name}-header"))
                    .w(100.)
                    .h(20.)
                    .bg(rgb(0xff0000))
                    .sticky_top(top),
            )
            .child(
                div()
                    .id(format!("{name}-row"))
                    .w(100.)
                    .h(280.)
                    .bg(rgb(0x00ff00)),
            )
    };
    scroll(offset)
        .w(100.)
        .h(100.)
        .child(section("a"))
        .child(section("b"))
        .child(section("c"))
}
/// What a frame does before it draws.
fn frame(ui: &Ui) {
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
}
fn top_of(ui: &Ui, id: NodeId) -> f32 {
    ui.scene.borrow().bounds(id).y
}

#[test]
fn a_header_holds_at_the_top_inside_its_section() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let view = ui.mount(sections(offset.clone(), 0.));
    frame(&ui);
    let (a, b) = (
        view.find("a-header").unwrap(),
        view.find("b-header").unwrap(),
    );
    // (scroll offset, where a's header is, where b's is)
    for (at, top_a, top_b) in [
        (0., 0., 300.),
        (50., 0., 250.),
        (279., 0., 21.),
        // a's section ends 20 under the top: its header goes with it.
        (290., -10., 10.),
        (300., -20., 0.),
        (350., -70., 0.),
    ] {
        offset.set(at);
        frame(&ui);
        assert_eq!(top_of(&ui, a), top_a, "a at {at}");
        assert_eq!(top_of(&ui, b), top_b, "b at {at}");
    }
    offset.set(0.);
    frame(&ui);
    assert_eq!(top_of(&ui, a), 0., "back where it was laid out");
}

#[test]
fn it_holds_at_its_inset_and_not_before() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let view = ui.mount(sections(offset.clone(), 8.));
    frame(&ui);
    let b = view.find("b-header").unwrap();
    for (at, top) in [(0., 300.), (292., 8.), (293., 8.), (291., 9.), (400., 8.)] {
        offset.set(at);
        frame(&ui);
        assert_eq!(top_of(&ui, b), top, "at {at}");
    }
}

#[test]
fn its_section_padding_and_its_margin_end_the_hold_early() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let view = ui.mount(
        scroll(offset.clone())
            .w(100.)
            .h(100.)
            .child(
                column()
                    .w(100.)
                    .h(200.)
                    .pb(10.)
                    .child(div().id("head").w(100.).h(20.).mb(5.).sticky_top(0.)),
            )
            .child(div().w(100.).h(300.)),
    );
    frame(&ui);
    let head = view.find("head").unwrap();
    // Content box to 190, less the margin: its top stops at 165.
    offset.set(100.);
    frame(&ui);
    assert_eq!(top_of(&ui, head), 0.);
    offset.set(180.);
    frame(&ui);
    assert_eq!(top_of(&ui, head), -15.);
}

#[test]
fn a_new_layout_moves_it_with_no_scroll() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let tall = ui.signal(50_f32);
    let height = tall.clone();
    let view = ui.mount(
        scroll(offset.clone())
            .w(100.)
            .h(100.)
            .child(
                div()
                    .w(100.)
                    .reactive_style(move || Styles::new().h(height.get())),
            )
            .child(
                column()
                    .w(100.)
                    .h(400.)
                    .child(div().id("head").w(100.).h(20.).sticky_top(0.)),
            ),
    );
    frame(&ui);
    offset.set(100.);
    frame(&ui);
    let head = view.find("head").unwrap();
    assert_eq!(top_of(&ui, head), 0.);
    tall.set(200.);
    frame(&ui);
    assert_eq!(top_of(&ui, head), 100., "laid out below the top again");
}

#[test]
fn it_keeps_a_translation_of_its_own() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let view = ui.mount(sections(offset.clone(), 0.));
    frame(&ui);
    let a = view.find("a-header").unwrap();
    offset.set(100.);
    frame(&ui);
    let held = ui.scene.borrow().transform(a);
    assert_eq!(held.y, 100.);
    ui.scene
        .borrow_mut()
        .set_transform(a, zgx::scene::Transform { x: 0., y: 7. });
    frame(&ui);
    offset.set(120.);
    frame(&ui);
    assert_eq!(top_of(&ui, a), 7., "its 7 goes on top of the hold");
}

#[test]
fn outside_a_scroll_view_it_stays_where_it_was_laid_out() {
    let mut ui = Ui::new(100., 100.);
    let view = ui.mount(
        column()
            .child(div().w(100.).h(30.))
            .child(div().id("head").w(100.).h(20.).sticky_top(0.)),
    );
    frame(&ui);
    assert_eq!(top_of(&ui, view.find("head").unwrap()), 30.);
}

#[test]
fn it_takes_hits_and_paints_over_what_scrolls_under_it() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let view = ui.mount(sections(offset.clone(), 0.));
    frame(&ui);
    offset.set(100.);
    frame(&ui);
    let head = view.find("a-header").unwrap();
    let scene = ui.scene.borrow();
    assert_eq!(
        scene.hit_test(50., 10.),
        Some(head),
        "over its later sibling"
    );
    let items = scene.layer_items(None);
    let at = |id| items.iter().position(|item| item.id == id);
    assert!(at(head) > at(view.find("a-row").unwrap()));
}

/// A scroll still copies; what a copy takes of the header repaints, and
/// where it holds, so the frame is what a full repaint would draw.
#[test]
fn a_scroll_past_it_copies_and_repaints_its_two_places() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    ui.mount(sections(offset.clone(), 0.));
    frame(&ui);
    offset.set(100.);
    frame(&ui);
    offset.set(103.);
    ui.prepare_frame();
    let mut scene = ui.scene.borrow_mut();
    let damage = scene.flush().damage;
    let moves = scene.scroll_moves().to_vec();
    assert_eq!(moves.len(), 1, "it still copies");
    assert_eq!(moves[0].dy, -3.);
    let covered = |y: f32| {
        damage
            .iter()
            .any(|rect| rect.y <= y && y < rect.y + rect.height)
    };
    // It is 20 tall: holding at 0, a copy leaves it 3 up, its old place.
    assert!(covered(0.) && covered(19.), "where it holds: {damage:?}");
    assert!(covered(97.), "the strip the copy exposes: {damage:?}");
    assert!(!covered(50.), "the rest is copied: {damage:?}");
}

#[test]
fn clearing_it_puts_it_back() {
    let mut ui = Ui::new(100., 100.);
    let offset = ui.signal(0_f32);
    let stuck = ui.signal(true);
    let on = stuck.clone();
    let view =
        ui.mount(
            scroll(offset.clone())
                .w(100.)
                .h(100.)
                .child(column().w(100.).h(400.).child(
                    div().id("head").w(100.).h(20.).reactive_style(move || {
                        let style = Styles::new();
                        if on.get() {
                            style.sticky_top(0.)
                        } else {
                            style
                        }
                    }),
                )),
        );
    frame(&ui);
    offset.set(100.);
    frame(&ui);
    let head = view.find("head").unwrap();
    assert_eq!(top_of(&ui, head), 0.);
    stuck.set(false);
    frame(&ui);
    assert_eq!(top_of(&ui, head), -100.);
}
