use std::sync::Arc;
use zgx::{
    image::ImageData,
    scene::{NodeKind, Scene, Style},
};

fn bitmap(width: u32, height: u32, shade: u8) -> Arc<ImageData> {
    Arc::new(ImageData::new(width, height, vec![shade; (width * height * 4) as usize]).unwrap())
}

#[test]
fn equal_image_dimensions_repaint_without_layout_but_intrinsic_resize_reflows() {
    let mut scene = Scene::new(100., 100.);
    let initial = bitmap(8, 12, 255);
    let node = scene.append(
        scene.root(),
        NodeKind::Image(initial.clone()),
        Style::default(),
    );
    scene.flush();
    scene.set_kind(node, NodeKind::Image(initial));
    assert!(scene.flush().is_idle());
    scene.set_kind(node, NodeKind::Image(bitmap(8, 12, 128)));
    let frame = scene.flush();
    assert_eq!(frame.layout_nodes, 0);
    assert!(!frame.damage.is_empty());
    assert_eq!(
        (scene.bounds(node).width, scene.bounds(node).height),
        (8., 12.)
    );
    scene.set_kind(node, NodeKind::Image(bitmap(24, 32, 255)));
    let frame = scene.flush();
    assert!(frame.layout_nodes > 0);
    assert_eq!(
        (scene.bounds(node).width, scene.bounds(node).height),
        (24., 32.)
    );
}
