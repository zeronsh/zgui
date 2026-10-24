use std::sync::Arc;
use zgx::{affine::Affine, compose::prelude::*, image::ImageData, scene::*, widgets::Ui};

fn pixels() -> Arc<ImageData> {
    Arc::new(ImageData::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap())
}

#[test]
fn transformed_image_reuses_pixels_changes_hit_geometry_and_avoids_layout() {
    let data = pixels();
    let mut scene = Scene::new(120., 120.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let image = scene.append(
        scene.root(),
        NodeKind::Image(data.clone()),
        Style {
            width: Some(40.),
            height: Some(20.),
            ..Default::default()
        },
    );
    scene.set_transform(image, Transform { x: 40., y: 40. });
    scene.flush();
    let rotated = Arc::new(data.transformed(Affine::rotation(std::f32::consts::FRAC_PI_2)));
    assert_eq!(data.id(), rotated.id());
    assert_eq!(data.pixels().as_ptr(), rotated.pixels().as_ptr());
    scene.set_kind(image, NodeKind::Image(rotated.clone()));
    let report = scene.flush();
    assert_eq!(report.layout_nodes, 0);
    assert!(
        report
            .damage
            .iter()
            .any(|r| r.intersects(Rect::new(55., 31., 1., 1.)))
    );
    assert_eq!(scene.hit_test(60., 32.), Some(image));
    assert_ne!(scene.hit_test(42., 50.), Some(image));
    scene.set_kind(image, NodeKind::Image(rotated));
    assert!(scene.flush().is_idle());
    scene.set_kind(
        image,
        NodeKind::Image(Arc::new(data.transformed(Affine::scale(0., 1.)))),
    );
    scene.flush();
    assert_ne!(scene.hit_test(60., 50.), Some(image));
    scene.set_kind(
        image,
        NodeKind::Image(Arc::new(data.transformed(Affine::rotation(f32::NAN)))),
    );
    scene.flush();
    assert_eq!(scene.hit_test(42., 50.), Some(image));
}

#[test]
fn component_transform_overflows_without_changing_allocated_size_and_obeys_parent_clip() {
    for clipped in [false, true] {
        let mut ui = Ui::new(120., 120.);
        let angle = ui.signal(0.);
        let read = angle.clone();
        let source = pixels();
        let image = image_signal("rotating image", move || {
            Arc::new(source.transformed(Affine::rotation(read.get())))
        })
        .id("image")
        .w(40.)
        .h(20.);
        let parent = overlay().w(40.).h(20.).translate(40., 40.).child(image);
        let parent = if clipped {
            parent.overflow_hidden()
        } else {
            parent
        };
        let mounted = ui.mount(parent);
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
        let image_root = mounted.find("image").unwrap();
        let original = ui.scene.borrow().bounds(image_root);
        angle.set(std::f32::consts::FRAC_PI_2);
        ui.prepare_frame();
        assert_eq!(ui.scene.borrow().bounds(image_root), original);
        let scene = ui.scene.borrow();
        let hits = scene.hit_test_all(60., 32.);
        assert_eq!(
            hits.iter()
                .any(|id| matches!(scene.kind(*id), NodeKind::Image(_))),
            !clipped
        );
    }
}
