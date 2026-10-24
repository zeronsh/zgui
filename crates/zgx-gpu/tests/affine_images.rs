use std::sync::Arc;
use zgx::{affine::Affine, image::ImageData, scene::*};
use zgx_gpu::GpuRenderer;

fn fixed(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        ..Default::default()
    }
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> &[u8] {
    &bytes[(y * 120 + x) * 4..(y * 120 + x) * 4 + 4]
}

#[test]
fn affine_pixels_damage_clipping_and_isolated_coordinates() {
    let data = Arc::new(ImageData::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap());
    for isolated in [false, true] {
        for clipped in [false, true] {
            let mut gpu = GpuRenderer::new(120, 120).unwrap();
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
            assert_eq!(gpu.render(&scene, &report.damage).unwrap().image_uploads, 1);
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
                assert_eq!(gpu.render(&scene, &report.damage).unwrap().image_uploads, 0);
                let pixels = gpu.readback().unwrap();
                if index == 0 {
                    let top = pixel(&pixels, 60, 35);
                    let bottom = pixel(&pixels, 60, 65);
                    if clipped {
                        assert_eq!(top, &[10, 20, 30, 255]);
                        assert_eq!(bottom, &[10, 20, 30, 255]);
                    } else {
                        assert!(
                            top[0] > 245 && top[1] < 5,
                            "top {top:?}, isolated={isolated}"
                        );
                        assert!(bottom[1] > 245 && bottom[0] < 5, "bottom {bottom:?}");
                    }
                }
                let mut fresh = GpuRenderer::new(120, 120).unwrap();
                fresh
                    .render(&scene, &[Rect::new(0., 0., 120., 120.)])
                    .unwrap();
                assert_eq!(
                    pixels,
                    fresh.readback().unwrap(),
                    "isolated={isolated}, clipped={clipped}, transform={matrix:?}"
                );
            }
        }
    }
}

#[test]
fn transformed_images_fade_with_their_fade_edges_ancestor() {
    // A rotated icon in a scroll view fading at its bottom edge fades where it
    // is on screen, as its untransformed neighbours do.
    let white = Arc::new(ImageData::new(1, 1, vec![255, 255, 255, 255]).unwrap());
    let mut gpu = GpuRenderer::new(120, 120).unwrap();
    let mut scene = Scene::new(120., 120.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(120., 120.),
    );
    let view = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            fade_edges: [0., 40.],
            ..fixed(120., 120.)
        },
    );
    let icon = scene.append(
        view,
        NodeKind::Image(Arc::new(
            white.transformed(Affine::rotation(std::f32::consts::FRAC_PI_2)),
        )),
        fixed(20., 20.),
    );
    scene.set_transform(icon, Transform { x: 10., y: 90. });
    let plain = scene.append(view, NodeKind::Image(white), fixed(20., 20.));
    scene.set_transform(plain, Transform { x: 60., y: 90. });
    let report = scene.flush();
    gpu.render(&scene, &report.damage).unwrap();
    let pixels = gpu.readback().unwrap();
    // 100 px down, 20 into the 40 px band: a quarter of full brightness.
    let (rotated, upright) = (pixel(&pixels, 20, 100)[0], pixel(&pixels, 70, 100)[0]);
    assert!(upright.abs_diff(64) <= 4, "upright {upright}");
    assert!(
        rotated.abs_diff(upright) <= 2,
        "rotated {rotated} upright {upright}"
    );
}
