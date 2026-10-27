//! A node's zoom (`Styled::zoom`, CSS's `zoom`): its subtree lays out in its
//! own points under its parent's room divided by the zoom and takes the zoom
//! times the room it needs there; it shows scaled about its top-left, its
//! hit testing following. Zooms multiply; a fixed node undoes those around
//! it. At 1 nothing differs from no zoom at all. Each layout case runs on the
//! ordinary path and on the taffy one.
use zgx::{
    compose::{ViewHandle, prelude::*},
    scene::Rect,
    widgets::Ui,
};

fn mount(view: View) -> (Ui, ViewHandle) {
    let mut ui = Ui::new(600., 400.);
    let handle = ui.mount(view);
    ui.prepare_frame();
    (ui, handle)
}
fn node(handle: &ViewHandle, id: &str) -> zgx::scene::NodeId {
    handle.find(id).unwrap_or_else(|| panic!("no `{id}`"))
}
fn bounds(ui: &Ui, handle: &ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().bounds(node(handle, id))
}
fn shown(ui: &Ui, handle: &ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().transformed_bounds(node(handle, id))
}
/// A child that sends its parent through taffy without taking room.
fn advanced() -> View {
    div().hidden()
}
fn near(a: Rect, b: Rect) -> bool {
    [a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]
        .iter()
        .all(|d| d.abs() < 0.01)
}

#[test]
fn a_zoomed_box_takes_its_zoom_times_its_room_and_its_siblings_move() {
    for taffy in [false, true] {
        let panel = column()
            .id("panel")
            .zoom(2.)
            .p(5.)
            .child(div().id("inner").w(100.).h(20.));
        let mut root = row().child(panel).child(div().id("after").w(10.).h(10.));
        if taffy {
            root = root.child(advanced());
        }
        let (ui, handle) = mount(root);
        // Its own box is in its points: 110 by 30.
        assert!(near(
            bounds(&ui, &handle, "panel"),
            Rect::new(0., 0., 110., 30.)
        ));
        // It shows twice that, and what follows comes after what shows.
        assert!(near(
            shown(&ui, &handle, "panel"),
            Rect::new(0., 0., 220., 60.)
        ));
        assert!(
            near(shown(&ui, &handle, "inner"), Rect::new(10., 10., 200., 40.)),
            "taffy {taffy}: {:?}",
            shown(&ui, &handle, "inner")
        );
        assert_eq!(bounds(&ui, &handle, "after").x, 220., "taffy {taffy}");
    }
}

#[test]
fn a_zoomed_box_lays_out_in_its_parents_room_divided_by_its_zoom() {
    for taffy in [false, true] {
        let panel = column()
            .id("panel")
            .w_full()
            .zoom(2.)
            .child(div().id("fill").w_full().h(10.));
        let mut root = column().w(300.).child(panel);
        if taffy {
            root = root.child(advanced());
        }
        let (ui, handle) = mount(root);
        // 300 across where it shows: 150 of its points.
        assert!(
            (bounds(&ui, &handle, "fill").width - 150.).abs() < 0.01,
            "taffy {taffy}: {:?}",
            bounds(&ui, &handle, "fill")
        );
        assert!((shown(&ui, &handle, "fill").width - 300.).abs() < 0.01);
    }
}

#[test]
fn zooms_multiply_and_a_fixed_node_undoes_those_around_it() {
    let view = column().id("outer").zoom(1.5).child(
        column()
            .id("middle")
            .zoom(2.)
            .child(div().id("leaf").w(10.).h(10.))
            .child(
                column()
                    .id("fixed")
                    .zoom_fixed()
                    .child(div().id("kept").w(10.).h(10.)),
            ),
    );
    let (ui, handle) = mount(view);
    assert!((shown(&ui, &handle, "leaf").width - 30.).abs() < 0.01);
    assert!((shown(&ui, &handle, "kept").width - 10.).abs() < 0.01);
    assert_eq!(ui.scene.borrow().zoom_around(node(&handle, "leaf")), 3.);
    // The window's zoom: the fixed node undoes it too.
    ui.set_zoom(1.25);
    let fixed = ui.scene.borrow().zoom(node(&handle, "fixed"));
    assert!((fixed - 1. / (1.25 * 3.)).abs() < 1e-6, "{fixed}");
}

#[test]
fn a_point_on_a_zoomed_box_hits_what_shows_there() {
    let view = row()
        .child(div().w(50.).h(50.))
        .child(column().zoom(2.).child(div().id("target").w(20.).h(20.)));
    let (ui, handle) = mount(view);
    let target = node(&handle, "target");
    let scene = ui.scene.borrow();
    // It shows from x 50 to 90: a point at 85 is on it, one past 90 not.
    assert_eq!(scene.hit_test(85., 35.), Some(target));
    assert_ne!(scene.hit_test(95., 35.), Some(target));
}

#[test]
fn a_zoom_of_one_is_no_zoom_at_all() {
    let tree = |zoom: Option<f32>| {
        let mut panel = column()
            .id("panel")
            .p(3.)
            .gap(4.)
            .child(text("Hello, zoom").id("text"))
            .child(
                div()
                    .id("box")
                    .w_full()
                    .h(12.)
                    .bg(rgb(0x336699))
                    .rounded(4.),
            );
        if let Some(zoom) = zoom {
            panel = panel.zoom(zoom);
        }
        row().w(240.).child(panel)
    };
    let (plain, plain_handle) = mount(tree(None));
    let (one, one_handle) = mount(tree(Some(1.)));
    for id in ["panel", "text", "box"] {
        assert_eq!(
            bounds(&plain, &plain_handle, id),
            bounds(&one, &one_handle, id),
            "{id}"
        );
    }
    let scene = one.scene.borrow();
    let panel = node(&one_handle, "panel");
    assert!(!scene.is_layer(panel), "no layer");
    assert_eq!(scene.zoom(panel), 1.);
    // The same paint, item for item.
    let paint = |ui: &Ui| {
        ui.scene
            .borrow()
            .paint_items()
            .map(|item| (item.bounds, item.isolated, item.matrix().is_some()))
            .collect::<Vec<_>>()
    };
    drop(scene);
    assert_eq!(paint(&plain), paint(&one));
}

#[test]
fn a_zoom_change_lays_its_subtree_out_again() {
    let (ui, handle) =
        mount(row().child(column().id("panel").child(div().id("leaf").w(40.).h(10.))));
    assert!((shown(&ui, &handle, "leaf").width - 40.).abs() < 0.01);
    let panel = node(&handle, "panel");
    ui.scene.borrow_mut().set_zoom(panel, 1.5);
    ui.prepare_frame();
    assert!((shown(&ui, &handle, "leaf").width - 60.).abs() < 0.01);
    assert!(ui.scene.borrow().is_layer(panel));
    ui.scene.borrow_mut().set_zoom(panel, 1.);
    ui.prepare_frame();
    assert!((shown(&ui, &handle, "leaf").width - 40.).abs() < 0.01);
}

#[test]
fn a_zoomed_node_keeps_its_style_its_snap_and_its_direction() {
    let view = row()
        .w(300.)
        .rtl()
        .child(div().id("first").w(30.).h(10.))
        .child(
            column()
                .id("panel")
                .zoom(2.)
                .snap(true)
                .child(div().id("leaf").w(20.3).h(10.)),
        );
    let (ui, handle) = mount(view);
    let panel = node(&handle, "panel");
    {
        let scene = ui.scene.borrow();
        // The style given, not the one it lays out by.
        assert!(scene.style(panel).snap);
        // Right to left: after the first child, from the right.
        let shown = scene.transformed_bounds(panel);
        assert!((shown.x + shown.width - 270.).abs() < 0.01, "{shown:?}");
        // Snapped: its edges on whole points where it shows.
        let own = scene.bounds(panel);
        assert_eq!(own.x, own.x.round());
    }
    // Unzoomed, it lays out as it did before it was zoomed.
    ui.scene.borrow_mut().set_zoom(panel, 1.);
    ui.prepare_frame();
    let (plain, plain_handle) = mount(
        row()
            .w(300.)
            .rtl()
            .child(div().id("first").w(30.).h(10.))
            .child(
                column()
                    .id("panel")
                    .snap(true)
                    .child(div().id("leaf").w(20.3).h(10.)),
            ),
    );
    for id in ["first", "panel", "leaf"] {
        assert_eq!(
            bounds(&ui, &handle, id),
            bounds(&plain, &plain_handle, id),
            "{id}"
        );
    }
    assert_eq!(
        ui.scene.borrow().style(panel),
        plain.scene.borrow().style(node(&plain_handle, "panel"))
    );
}

#[test]
fn a_zoomed_node_keeps_its_zoom_when_its_transform_goes() {
    use zgx::scene::LocalTransform;
    let (ui, handle) = mount(row().child(div().id("panel").w(50.).h(20.)));
    let panel = node(&handle, "panel");
    let scaled = LocalTransform {
        scale: (1.5, 1.5),
        origin: (0., 0.),
        ..LocalTransform::IDENTITY
    };
    // Transformed first, then zoomed: its transform, then its zoom.
    ui.scene.borrow_mut().set_local_transform(panel, scaled);
    ui.scene.borrow_mut().set_zoom(panel, 2.);
    ui.prepare_frame();
    assert!(near(
        shown(&ui, &handle, "panel"),
        Rect::new(0., 0., 150., 60.)
    ));
    // Its transform gone, its zoom stays: still a transformed node.
    ui.scene
        .borrow_mut()
        .set_local_transform(panel, LocalTransform::IDENTITY);
    ui.prepare_frame();
    assert!(near(
        shown(&ui, &handle, "panel"),
        Rect::new(0., 0., 100., 40.)
    ));
    assert_eq!(ui.scene.borrow().zoom_around(panel), 2.);
}

/// The pointer moved to `at`, as the host says before a wheel or a pinch
/// there (they go to what the pointer is over).
fn point(ui: &mut Ui, at: (f32, f32)) {
    ui.dispatch(zgx::input::InputEvent::PointerMove { x: at.0, y: at.1 });
}
/// A Control-wheel the host dispatches (`zgx::zoom::wheel`): to the nodes
/// first; whether one took it.
fn control_wheel(ui: &mut Ui, at: (f32, f32), notches: f32) -> bool {
    point(ui, at);
    zgx::zoom::wheel(40., || {
        ui.dispatch(zgx::input::InputEvent::Scroll {
            x: at.0,
            y: at.1,
            delta_x: 0.,
            delta_y: -40. * notches,
        })
        .default_prevented
    })
}
fn pinch(ui: &mut Ui, at: (f32, f32), delta: f32) -> bool {
    point(ui, at);
    ui.dispatch(zgx::input::InputEvent::Pinch {
        x: at.0,
        y: at.1,
        delta,
    })
    .default_prevented
}

#[test]
fn a_zoomable_takes_the_control_wheel_over_it_a_level_a_notch() {
    let (mut ui, zoom, handle) = panel_and_rest();
    // Over it: Chrome's levels, a notch at a time, and the window's left be.
    assert!(control_wheel(&mut ui, (20., 20.), 1.));
    ui.prepare_frame();
    assert_eq!(zoom.get(), 1.1);
    assert!(control_wheel(&mut ui, (20., 20.), 1.));
    ui.prepare_frame();
    assert_eq!(zoom.get(), 1.25);
    assert!(near(
        shown(&ui, &handle, "panel"),
        Rect::new(0., 0., 125., 62.5)
    ));
    assert!(control_wheel(&mut ui, (20., 20.), -2.));
    ui.prepare_frame();
    assert_eq!(zoom.get(), 1.);
    // A touchpad's half notches add up.
    assert!(control_wheel(&mut ui, (20., 20.), 0.5));
    assert_eq!(zoom.get(), 1.);
    assert!(control_wheel(&mut ui, (20., 20.), 0.5));
    assert_eq!(zoom.get(), 1.1);
    // Elsewhere (a gesture of its own: one stays with what took it, as a
    // scroll view's does) it isn't taken: the window zooms there.
    let (mut ui, zoom, handle) = panel_and_rest();
    let rest = shown(&ui, &handle, "rest");
    assert!(!control_wheel(&mut ui, (rest.x + 10., rest.y + 10.), 1.));
    assert_eq!(zoom.get(), 1.);
    // A plain wheel over it scrolls as ever.
    let (mut ui, zoom, _) = panel_and_rest();
    point(&mut ui, (20., 20.));
    let plain = ui.dispatch(zgx::input::InputEvent::Scroll {
        x: 20.,
        y: 20.,
        delta_x: 0.,
        delta_y: -40.,
    });
    assert!(!plain.default_prevented);
    assert_eq!(zoom.get(), 1.);
}

/// A zoomable panel beside another box.
fn panel_and_rest() -> (Ui, zgx::reactive::Signal<f32>, ViewHandle) {
    let mut ui = Ui::new(600., 400.);
    let zoom = ui.signal(1_f32);
    let handle = ui.mount(
        row()
            .child(
                div()
                    .id("panel")
                    .zoomable(zoom.clone())
                    .child(div().w(100.).h(50.)),
            )
            .child(div().id("rest").w(100.).h(50.)),
    );
    ui.prepare_frame();
    (ui, zoom, handle)
}

#[test]
fn a_pinch_zooms_a_zoomable_and_stays_taken_at_its_limits() {
    let mut ui = Ui::new(600., 400.);
    let zoom = ui.signal(1_f32);
    ui.mount(div().zoomable(zoom.clone()).child(div().w(100.).h(50.)));
    ui.prepare_frame();
    // Near its corner, over it at every zoom.
    assert!(pinch(&mut ui, (5., 5.), 0.2));
    assert!((zoom.get() - 1.2).abs() < 1e-5);
    // Pinched past 500%: held there, and still taken.
    for _ in 0..30 {
        assert!(pinch(&mut ui, (5., 5.), 0.5));
        ui.prepare_frame();
    }
    assert_eq!(zoom.get(), 5.);
    for _ in 0..60 {
        assert!(pinch(&mut ui, (5., 5.), -0.5));
        ui.prepare_frame();
    }
    assert_eq!(zoom.get(), 0.25);
}

#[test]
fn the_innermost_zoomable_under_the_pointer_takes_the_gesture() {
    let nested = || {
        let mut ui = Ui::new(600., 400.);
        let (outer, inner) = (ui.signal(1_f32), ui.signal(1_f32));
        let handle = ui.mount(
            div().id("outer").zoomable(outer.clone()).p(20.).child(
                div()
                    .id("inner")
                    .zoomable(inner.clone())
                    .child(div().w(80.).h(40.)),
            ),
        );
        ui.prepare_frame();
        (ui, outer, inner, handle)
    };
    let (mut ui, outer, inner, handle) = nested();
    let at = shown(&ui, &handle, "inner");
    assert!(control_wheel(&mut ui, (at.x + 5., at.y + 5.), 1.));
    assert_eq!((outer.get(), inner.get()), (1., 1.1));
    // At its limit, the inner keeps it: the outer never zooms in its place.
    inner.set(5.);
    ui.prepare_frame();
    let at = shown(&ui, &handle, "inner");
    assert!(control_wheel(&mut ui, (at.x + 5., at.y + 5.), 1.));
    assert_eq!((outer.get(), inner.get()), (1., 5.));
    // Over the outer alone (a gesture of its own), the outer.
    let (mut ui, outer, inner, _) = nested();
    assert!(control_wheel(&mut ui, (5., 5.), 1.));
    assert_eq!((outer.get(), inner.get()), (1.1, 1.));
}

#[test]
fn what_is_under_the_pointer_stays_there_as_a_zoomable_zooms_in_a_scroll_view() {
    let mut ui = Ui::new(300., 200.);
    let offset = ui.signal(0_f32);
    let zoom = ui.signal(1_f32);
    let handle = ui.mount(
        scroll(offset.clone()).w_full().h(200.).child(
            column()
                .id("doc")
                .zoomable(zoom.clone())
                .children((0..20).map(|i| div().id(format!("row{i}")).w(200.).h(40.))),
        ),
    );
    ui.prepare_frame();
    // The pointer over row 3, 10 points into it.
    let row = shown(&ui, &handle, "row3");
    let pointer = (50., row.y + 10.);
    assert!(control_wheel(&mut ui, pointer, 4.));
    ui.prepare_frame();
    assert_eq!(zoom.get(), 1.75);
    // Row 3 is 70 tall now; the point 10 into it is 17.5 into it, under the
    // pointer still.
    let row = shown(&ui, &handle, "row3");
    assert!(
        (row.y + 17.5 - pointer.1).abs() < 1.,
        "row 3 at {}, the pointer at {}",
        row.y,
        pointer.1
    );
    assert!(offset.get() > 0.);
}
