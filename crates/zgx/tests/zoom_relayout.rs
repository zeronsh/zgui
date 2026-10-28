//! A zoomed node changed after its first layout (resized, snapped, its
//! children made its contents) lays out as a scene built that way from the
//! start: its siblings move, its snap applies, its children's size is
//! taken in its own points.
use zgx::scene::{Color, Layout, NodeId, NodeKind, Rect, Scene, Style};

fn box_style(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        flex_shrink: 0.,
        ..Default::default()
    }
}
/// A box `w` wide at `zoom`, and a sibling after it in a row.
fn pair(w: f32, zoom: f32, snap: bool) -> (Scene, NodeId, NodeId) {
    let mut s = Scene::new(640., 480.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Row));
    let a = s.append(
        s.root(),
        NodeKind::Rect(Color(255, 0, 0, 255)),
        Style {
            snap,
            ..box_style(w, 10.)
        },
    );
    let b = s.append(
        s.root(),
        NodeKind::Rect(Color(0, 255, 0, 255)),
        box_style(10., 10.),
    );
    s.set_zoom(a, zoom);
    s.flush();
    (s, a, b)
}
#[test]
fn resizing_a_zoomed_box_repositions_its_sibling() {
    let (mut live, a, b) = pair(20., 2., false);
    let mut style = live.style(a);
    style.width = Some(40.);
    live.set_style(a, style);
    live.flush();
    let (fresh, fa, fb) = pair(40., 2., false);
    assert_eq!(live.transformed_bounds(a), fresh.transformed_bounds(fa));
    assert_eq!(fresh.bounds(fb).x, 80.);
    assert_eq!(live.bounds(b), fresh.bounds(fb));
    live.flush();
    assert_eq!(live.bounds(b), fresh.bounds(fb));
}
#[test]
fn enabling_snap_on_a_zoomed_box_matches_fresh_layout() {
    let (mut live, a, _) = pair(47., 0.5, false);
    let mut style = live.style(a);
    style.snap = true;
    live.set_style(a, style);
    live.flush();
    let (fresh, fa, _) = pair(47., 0.5, true);
    assert_eq!(fresh.transformed_bounds(fa).width, 24.);
    assert_eq!(live.transformed_bounds(a), fresh.transformed_bounds(fa));
}
/// Either order: zoomed, then made contents, or the other way.
#[test]
fn zoomed_contents_uses_its_child_size_in_its_own_points() {
    for zoom_first in [true, false] {
        let mut s = Scene::new(640., 480.);
        s.set_kind(s.root(), NodeKind::Container(Layout::Column));
        let a = s.append(
            s.root(),
            NodeKind::Container(Layout::Column),
            box_style(40., 10.),
        );
        let child = s.append(
            a,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            box_style(60., 20.),
        );
        let after = s.append(
            s.root(),
            NodeKind::Rect(Color(0, 255, 0, 255)),
            box_style(10., 10.),
        );
        if zoom_first {
            s.set_zoom(a, 1.25);
            s.set_contents(a, true);
        } else {
            s.set_contents(a, true);
            s.set_zoom(a, 1.25);
        }
        s.flush();
        assert_eq!(s.bounds(after).y, 25., "zoom first: {zoom_first}");
        assert_eq!(s.transformed_bounds(child), Rect::new(0., 0., 75., 25.));
        assert_eq!(s.transformed_bounds(a), Rect::new(0., 0., 75., 25.));
    }
}
/// Arranged again without being measured again (its parent's alignment
/// changed, nothing of its size): its box stays its own size, not divided
/// by its zoom a second time.
#[test]
fn changing_parent_alignment_preserves_zoomed_child_size() {
    use zgx::scene::Align;
    let build = |fresh: bool| {
        let mut s = Scene::new(320., 240.);
        let row = s.append(
            s.root(),
            NodeKind::Container(Layout::Row),
            Style {
                width: Some(200.),
                height: Some(100.),
                align: Align::Baseline,
                flex_shrink: 0.,
                ..Default::default()
            },
        );
        let zoomed = s.append(
            row,
            NodeKind::Rect(Color(1, 2, 3, 255)),
            box_style(45., 20.),
        );
        s.append(
            row,
            NodeKind::Rect(Color(4, 5, 6, 255)),
            box_style(10., 10.),
        );
        if !fresh {
            s.flush();
        }
        s.set_zoom(zoomed, 1.5);
        if !fresh {
            s.flush();
        }
        let mut style = s.style(row);
        style.align = Align::Start;
        s.set_style(row, style);
        s.flush();
        (s, zoomed)
    };
    let (live, a) = build(false);
    let (fresh, b) = build(true);
    assert_eq!(fresh.bounds(b), Rect::new(0., 0., 45., 20.));
    assert_eq!(live.bounds(a), fresh.bounds(b));
    assert_eq!(live.transformed_bounds(a), fresh.transformed_bounds(b));
}
/// A scene changed step by step (each step flushed) against one built with
/// the same steps and flushed once.
fn same_as_fresh(steps: &dyn Fn(&mut Scene, &[NodeId], bool)) {
    let build = |live: bool| {
        let mut s = Scene::new(320., 240.);
        let c = s.append(
            s.root(),
            NodeKind::Container(Layout::Row),
            box_style(41., 16.),
        );
        let a = s.append(c, NodeKind::Rect(Color(1, 1, 1, 255)), box_style(45., 20.));
        let b = s.append(c, NodeKind::Rect(Color(2, 2, 2, 255)), box_style(47., 22.));
        let ids = [c, a, b];
        if live {
            s.flush();
        }
        steps(&mut s, &ids, live);
        s.flush();
        (s, ids)
    };
    let (live, l) = build(true);
    let (fresh, f) = build(false);
    for (a, b) in l.iter().zip(&f) {
        assert_eq!(live.bounds(*a), fresh.bounds(*b));
        assert_eq!(live.transformed_bounds(*a), fresh.transformed_bounds(*b));
    }
}
/// Its snap turned on while it is zoomed and a contents node (with no
/// children, laid out as its own style says).
#[test]
fn snap_on_a_zoomed_contents_node_matches_fresh_layout() {
    same_as_fresh(&|s, ids, live| {
        let flush = |s: &mut Scene| {
            if live {
                s.flush();
            }
        };
        s.set_zoom(ids[1], 0.5);
        flush(s);
        s.set_contents(ids[1], true);
        flush(s);
        let mut style = s.style(ids[1]);
        style.snap = true;
        s.set_style(ids[1], style);
    });
}
/// The last child of a snapped contents node zoomed: the contents node
/// lays out as it now takes room.
#[test]
fn zooming_a_contents_nodes_last_child_lays_the_contents_node_out_again() {
    same_as_fresh(&|s, ids, live| {
        let flush = |s: &mut Scene| {
            if live {
                s.flush();
            }
        };
        s.set_contents(ids[0], true);
        flush(s);
        let mut style = s.style(ids[2]);
        style.height = Some(7.5);
        s.set_style(ids[2], style);
        flush(s);
        let mut style = s.style(ids[0]);
        style.snap = true;
        s.set_style(ids[0], style);
        flush(s);
        s.set_zoom(ids[2], 3.);
    });
}
