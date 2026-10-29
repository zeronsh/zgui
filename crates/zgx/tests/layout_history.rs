//! A layout changed after its first flush matches one built with the final
//! styles and flushed once: a fixed-size item whose content's automatic
//! minimum drops shrinks as a fresh layout shrinks it, and a child laid
//! out by a grid and then by flex keeps nothing of the grid's sizing.
use std::sync::Arc;
use zgx::{
    layout::{Display, LayoutOptions},
    scene::{Color, Layout, NodeKind, Scene, Style},
};

/// How a grandchild changes after the first flush.
#[derive(Clone, Copy)]
enum Change {
    Clip,
    Absolute,
    Narrower,
}
/// In a 100-wide row, an 80-wide shrinkable column holding an 80-wide
/// child, then an 80-wide sibling that does not shrink: the column's
/// width, and where the sibling starts.
fn minimum(fresh: bool, change: Change) -> (f32, f32) {
    let mut s = Scene::new(100., 100.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Row));
    let p = s.append(
        s.root(),
        NodeKind::Container(Layout::Column),
        Style {
            width: Some(80.),
            height: Some(30.),
            flex_shrink: 1.,
            ..Default::default()
        },
    );
    let child = s.append(
        p,
        NodeKind::Rect(Color(1, 2, 3, 255)),
        Style {
            width: Some(80.),
            height: Some(20.),
            flex_shrink: 0.,
            ..Default::default()
        },
    );
    let sibling = s.append(
        s.root(),
        NodeKind::Rect(Color(1, 2, 3, 255)),
        Style {
            width: Some(80.),
            height: Some(20.),
            flex_shrink: 0.,
            ..Default::default()
        },
    );
    if !fresh {
        s.flush();
    }
    let mut st = s.style(child);
    match change {
        Change::Clip => st.clip = true,
        Change::Absolute => st.absolute = true,
        Change::Narrower => st.width = Some(10.),
    }
    s.set_style(child, st);
    s.flush();
    (s.bounds(p).width, s.bounds(sibling).x)
}
#[test]
fn descendant_clipping_updates_a_flex_items_automatic_minimum() {
    assert_eq!(minimum(false, Change::Clip), minimum(true, Change::Clip));
}
#[test]
fn removing_a_child_from_flow_updates_a_flex_items_automatic_minimum() {
    assert_eq!(
        minimum(false, Change::Absolute),
        minimum(true, Change::Absolute)
    );
}
#[test]
fn a_narrower_descendant_updates_a_flex_items_automatic_minimum() {
    assert_eq!(
        minimum(false, Change::Narrower),
        minimum(true, Change::Narrower)
    );
}
fn aspect(fresh: bool) -> f32 {
    let mut s = Scene::new(320., 240.);
    let p = s.append(
        s.root(),
        NodeKind::Container(Layout::Row),
        Style {
            width: Some(200.),
            height: Some(100.),
            layout_options: Some(Arc::new(LayoutOptions {
                display: Some(Display::Grid),
                columns: Some(3),
                ..Default::default()
            })),
            ..Default::default()
        },
    );
    let child = s.append(
        p,
        NodeKind::Rect(Color(1, 2, 3, 255)),
        Style {
            width: Some(61.),
            height: Some(36.),
            flex_shrink: 0.,
            ..Default::default()
        },
    );
    s.append(
        p,
        NodeKind::Rect(Color(1, 2, 3, 255)),
        Style {
            width: Some(62.),
            height: Some(37.),
            flex_shrink: 0.,
            ..Default::default()
        },
    );
    if !fresh {
        s.flush();
    }
    let mut st = s.style(child);
    st.layout_options = Some(Arc::new(LayoutOptions {
        aspect_ratio: Some(0.5),
        ..Default::default()
    }));
    s.set_style(child, st);
    if !fresh {
        s.flush();
    }
    let mut st = s.style(p);
    let mut o = st.layout_options.as_deref().copied().unwrap();
    o.display = None;
    o.columns = Some(1);
    st.layout_options = Some(Arc::new(o));
    s.set_style(p, st);
    s.flush();
    s.bounds(child).height
}
#[test]
fn grid_to_flex_does_not_keep_an_old_aspect_ratio_allocation() {
    assert_eq!(aspect(false), aspect(true));
}
