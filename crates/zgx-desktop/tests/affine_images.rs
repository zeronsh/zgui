use std::sync::Arc;
use zgx::{affine::Affine, image::ImageData, scene::*};
use zgx_desktop::raster::Raster;

fn fixed(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        ..Default::default()
    }
}
fn pixel(bytes: &[u32], x: usize, y: usize) -> u32 {
    bytes[y * 120 + x]
}

#[test]
fn affine_pixels_damage_clipping_and_isolated_coordinates() {
    let data = Arc::new(ImageData::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap());
    for isolated in [false, true] {
        for clipped in [false, true] {
            let mut gpu = Raster::new(120, 120);
            let mut scene = Scene::new(120., 120.);
            scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
            scene.append(
                scene.root(),
                NodeKind::Rect(Color(10, 20, 30, 255)),
                fixed(120., 120.),
            );
            let parent = scene.append(
                scene.root(),
                NodeKind::Container(Layout::Overlay),
                Style {
                    clip: clipped,
                    ..fixed(40., 20.)
                },
            );
            scene.set_transform(parent, Transform { x: 40., y: 40. });
            scene.set_isolated(parent, isolated);
            let node = scene.append(parent, NodeKind::Image(data.clone()), fixed(40., 20.));
            let report = scene.flush();
            gpu.render(&scene, &report.damage);
            let matrices = [
                Affine::rotation(std::f32::consts::FRAC_PI_2),
                Affine::scale(-1., 1.),
                Affine::scale(1.2, 0.7).then(Affine::rotation(0.7)),
                Affine {
                    c: 0.4,
                    ..Affine::IDENTITY
                },
                Affine::scale(0., 1.),
                Affine::IDENTITY,
            ];
            for (index, matrix) in matrices.into_iter().enumerate() {
                scene.set_kind(node, NodeKind::Image(Arc::new(data.transformed(matrix))));
                let report = scene.flush();
                assert_eq!(report.layout_nodes, 0);
                gpu.render(&scene, &report.damage);
                let pixels = gpu.pixels.clone();
                if index == 0 {
                    let top = pixel(&pixels, 60, 35);
                    let bottom = pixel(&pixels, 60, 65);
                    if clipped {
                        assert_eq!(top, 0x0a141e);
                        assert_eq!(bottom, 0x0a141e);
                    } else {
                        assert!(top == 0xff0000, "top {top:?}, isolated={isolated}");
                        assert!(bottom == 0x00ff00, "bottom {bottom:?}");
                    }
                }
                let mut fresh = Raster::new(120, 120);
                fresh.render(&scene, &[Rect::new(0., 0., 120., 120.)]);
                assert_eq!(
                    pixels, fresh.pixels,
                    "isolated={isolated}, clipped={clipped}, transform={matrix:?}"
                );
            }
        }
    }
}
