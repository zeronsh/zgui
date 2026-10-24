use zgx::scene::{
    Align, BoxShadow, Color, Insets, Justify, Layout, NodeKind, QuadStyle, Rect, Scene, Style,
    Transform,
};

fn edges() -> Insets {
    Insets {
        top: 3.,
        right: 5.,
        bottom: 7.,
        left: 11.,
    }
}
fn panel(layout: Layout) -> NodeKind {
    NodeKind::Panel {
        layout,
        quad: QuadStyle {
            fill: Color(20, 30, 40, 255),
            ..Default::default()
        },
    }
}
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}

#[test]
fn asymmetric_padding_intrinsic_sizes_and_children() {
    for (layout, width, height, second) in [
        (Layout::Row, 50., 30., Rect::new(35., 3., 10., 20.)),
        (Layout::Column, 36., 41., Rect::new(11., 14., 10., 20.)),
        (Layout::Overlay, 36., 30., Rect::new(11., 3., 10., 20.)),
    ] {
        let mut scene = Scene::new(200., 200.);
        let container = scene.append(
            scene.root(),
            panel(layout),
            Style {
                padding: 100.,
                padding_edges: Some(edges()),
                gap: 4.,
                ..Default::default()
            },
        );
        let first = scene.append(
            container,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            fixed(20., 7.),
        );
        let last = scene.append(
            container,
            NodeKind::Rect(Color(0, 255, 0, 255)),
            fixed(10., 20.),
        );
        scene.flush();
        assert_eq!(scene.bounds(container), Rect::new(0., 0., width, height));
        assert_eq!(scene.bounds(first), Rect::new(11., 3., 20., 7.));
        assert_eq!(scene.bounds(last), second);
        assert_eq!(scene.children(container).len(), 2);
        assert!(scene.flush().is_idle());
    }
}

#[test]
fn asymmetric_padding_constrains_flex_and_reflows_on_resize() {
    for layout in [Layout::Row, Layout::Column, Layout::Overlay] {
        let mut scene = Scene::new(100., 80.);
        scene.set_kind(scene.root(), panel(layout));
        let mut style = scene.style(scene.root());
        style.padding_edges = Some(edges());
        style.align = Align::Stretch;
        // An overlay fills down by `justify` (rows and columns ignore it).
        style.justify = Justify::Stretch;
        scene.set_style(scene.root(), style);
        let child = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                flex_grow: 1.,
                ..Default::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(child), Rect::new(11., 3., 84., 70.));
        scene.resize(140., 120.);
        scene.flush();
        assert_eq!(scene.bounds(child), Rect::new(11., 3., 124., 110.));
        assert!(scene.flush().is_idle());
    }
}

#[test]
fn panel_shadow_old_and_new_positions_are_damaged() {
    let mut scene = Scene::new(200., 200.);
    let node = scene.append(
        scene.root(),
        NodeKind::Panel {
            layout: Layout::Column,
            quad: QuadStyle {
                shadow: Some(BoxShadow {
                    color: Color(0, 0, 0, 255),
                    offset: Transform { x: 8., y: 5. },
                    blur_radius: 2.,
                    spread: 1.,
                }),
                ..Default::default()
            },
        },
        fixed(20., 20.),
    );
    scene.set_transform(node, Transform { x: 30., y: 30. });
    scene.flush();
    scene.set_transform(node, Transform { x: 80., y: 80. });
    let report = scene.flush();
    for point in [Rect::new(64., 60., 1., 1.), Rect::new(114., 110., 1., 1.)] {
        assert!(
            report.damage.iter().any(|damage| damage.intersects(point)),
            "missing shadow damage at {point:?}: {:?}",
            report.damage
        );
    }
}

#[test]
fn decoration_changes_reuse_layout_but_layout_direction_reflows() {
    let mut scene = Scene::new(100., 100.);
    let parent = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Row),
        Style::default(),
    );
    scene.append(parent, NodeKind::Rect(Color(1, 2, 3, 255)), fixed(20., 10.));
    let child = scene.append(parent, NodeKind::Rect(Color(1, 2, 3, 255)), fixed(20., 10.));
    scene.flush();
    scene.set_kind(parent, panel(Layout::Row));
    let report = scene.flush();
    assert_eq!(report.layout_nodes, 0);
    assert!(!report.damage.is_empty());
    assert_eq!(scene.bounds(child), Rect::new(20., 0., 20., 10.));
    scene.set_kind(parent, panel(Layout::Column));
    assert!(scene.flush().layout_nodes > 0);
    assert_eq!(scene.bounds(child), Rect::new(0., 10., 20., 10.));
}

#[test]
fn fluent_absolute_and_relative_update_flow_reactively() {
    use zgx::compose::prelude::*;
    let mut ui = zgx::widgets::Ui::new(300., 200.);
    let floating = ui.signal(true);
    let style = floating.clone();
    let tree = ui.mount(
        column()
            .gap(5.)
            .child(div().w(20.).h(30.).id("first"))
            .child(div().w(80.).h(60.).id("float").reactive_style(move || {
                if style.get() {
                    Styles::new().absolute().ml(3.)
                } else {
                    Styles::new().relative()
                }
            }))
            .child(div().w(20.).h(30.).id("last")),
    );
    ui.prepare_frame();
    let last = tree.find("last").unwrap();
    let float = tree.find("float").unwrap();
    assert_eq!(ui.scene.borrow().bounds(last).y, 35.);
    assert_eq!(ui.scene.borrow().bounds(float).x, 3.);
    assert_eq!(ui.scene.borrow().bounds(tree.node()).height, 65.);
    floating.set(false);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(last).y, 100.);
    assert_eq!(ui.scene.borrow().bounds(tree.node()).height, 130.);
}
