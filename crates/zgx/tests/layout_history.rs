//! A layout changed after its first flush matches one built with the final
//! styles and flushed once: a child laid out by a grid and then by flex
//! keeps nothing of the grid's sizing.
use std::sync::Arc;
use zgx::{
    layout::{Display, LayoutOptions},
    scene::{Color, Layout, NodeKind, Scene, Style},
};

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
