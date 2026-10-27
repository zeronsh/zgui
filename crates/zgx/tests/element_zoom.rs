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
