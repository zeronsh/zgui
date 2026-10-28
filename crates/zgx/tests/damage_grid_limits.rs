//! Damage past the separate rectangles a flush keeps goes on a grid of
//! cells over the viewport; a viewport too large for one joins it instead.
use zgx::scene::{Color, Layout, NodeKind, Rect, Scene, Style, Transform};

#[test]
fn extreme_viewport_does_not_overflow_damage_grid_capacity() {
    let mut scene = Scene::new(f32::MAX, f32::MAX);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let mut rects = Vec::new();
    for i in 0..65 {
        let id = scene.append(
            scene.root(),
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(1.),
                height: Some(1.),
                absolute: true,
                ..Default::default()
            },
        );
        scene.set_transform(
            id,
            Transform {
                x: (i * 3) as f32,
                y: 0.,
            },
        );
        rects.push(id);
    }
    let damage = scene.flush().damage;
    // Each square is still damaged.
    for id in rects {
        let square = scene.bounds(id);
        assert!(
            damage
                .iter()
                .any(|d| d.intersection(square) == Some(square)),
            "{square:?} left out of {damage:?}"
        );
    }
    // And again, the frame after.
    let moved = scene.append(
        scene.root(),
        NodeKind::Rect(Color(1, 2, 3, 255)),
        Style::default(),
    );
    scene.set_transform(moved, Transform { x: 5., y: 5. });
    let _: Vec<Rect> = scene.flush().damage;
}
