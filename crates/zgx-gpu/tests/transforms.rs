//! Local transforms (scale, rotation, skew) composite cached layers under a
//! matrix: pixels land where the transform puts them, and transform-only
//! changes repaint no layer content.
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}
fn rotated(radians: f32) -> LocalTransform {
    LocalTransform {
        rotate: radians,
        ..LocalTransform::IDENTITY
    }
}
fn scaled(factor: f32) -> LocalTransform {
    LocalTransform {
        scale: (factor, factor),
        ..LocalTransform::IDENTITY
    }
}
const SIZE: u32 = 200;
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
/// An opaque `color` rect of `width` × `height` at (`x`, `y`).
fn rect(
    scene: &mut Scene,
    parent: NodeId,
    x: f32,
    y: f32,
    size: (f32, f32),
    color: Color,
) -> NodeId {
    let id = scene.append(parent, NodeKind::Rect(color), fixed(size.0, size.1));
    scene.set_transform(id, Transform { x, y });
    id
}
const RED: Color = Color(255, 0, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> (zgx_gpu::GpuStats, Vec<u8>) {
    let damage = scene.flush().damage;
    let stats = gpu.render(scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    gpu.render(scene, &[Rect::new(0., 0., SIZE as f32, SIZE as f32)])
        .unwrap();
    assert_eq!(
        pixels,
        gpu.readback().unwrap(),
        "incremental frame differs from full"
    );
    (stats, pixels)
}

#[test]
fn rotated_square_lands_rotated_with_smooth_edges() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        let mut scene = scene();
        let root = scene.root();
        let square = rect(&mut scene, root, 70., 70., (60., 60.), RED);
        render(&mut gpu, &mut scene);
        scene.set_local_transform(square, rotated(FRAC_PI_4));
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 1);
        assert_eq!(pixel(&p, 100, 100), [255, 0, 0, 255]);
        // The diamond reaches 42 px from its centre: above the laid-out box...
        assert_eq!(pixel(&p, 100, 62), [255, 0, 0, 255]);
        // ...but not into its corners.
        assert_eq!(pixel(&p, 73, 73), [0; 4]);
        assert_eq!(pixel(&p, 127, 127), [0; 4]);
        // Edges are antialiased rather than stair-stepped.
        let row: Vec<u8> = (50..70).map(|x| pixel(&p, x, 100)[3]).collect();
        assert!(
            row.iter().any(|a| *a > 20 && *a < 235),
            "hard edge: {row:?}"
        );
        assert_eq!(row.first(), Some(&0));
        assert_eq!(row.last(), Some(&255));
    }
}

#[test]
fn scale_and_skew_place_pixels_by_their_matrix() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    let small = rect(&mut scene, root, 30., 30., (20., 20.), RED);
    let sheared = rect(&mut scene, root, 100., 100., (40., 40.), BLUE);
    scene.set_local_transform(small, scaled(2.));
    scene.set_local_transform(
        sheared,
        LocalTransform {
            skew: (FRAC_PI_4, 0.),
            ..LocalTransform::IDENTITY
        },
    );
    let (_, p) = render(&mut gpu, &mut scene);
    // Twice the size about its centre (40, 40): 20..60.
    assert_eq!(pixel(&p, 22, 22), [255, 0, 0, 255]);
    assert_eq!(pixel(&p, 57, 40), [255, 0, 0, 255]);
    assert_eq!(pixel(&p, 18, 40), [0; 4]);
    // Skewed about (120, 120): row y = 135 spans x 115..155.
    assert_eq!(pixel(&p, 150, 135), [0, 0, 255, 255]);
    assert_eq!(pixel(&p, 105, 135), [0; 4]);
    // Row y = 105 spans x 85..125.
    assert_eq!(pixel(&p, 88, 105), [0, 0, 255, 255]);
    assert_eq!(pixel(&p, 135, 105), [0; 4]);
}

#[test]
fn nested_transforms_compose_and_clip_inside_the_rotation() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    let panel = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        fixed(100., 100.),
    );
    scene.set_local_transform(
        panel,
        LocalTransform {
            scale: (2., 2.),
            origin: (0., 0.),
            ..LocalTransform::IDENTITY
        },
    );
    let card = rect(&mut scene, panel, 20., 20., (20., 20.), RED);
    scene.set_local_transform(card, rotated(FRAC_PI_4));
    let (stats, p) = render(&mut gpu, &mut scene);
    assert_eq!(stats.layer_repaints, 2);
    // The card's centre (30, 30) shows at (60, 60), its diamond 28 px out.
    assert_eq!(pixel(&p, 60, 60), [255, 0, 0, 255]);
    assert_eq!(pixel(&p, 60, 36), [255, 0, 0, 255]);
    assert_eq!(pixel(&p, 42, 42), [0; 4]);
    // Rotating the inner card repaints the panel's texture, not the card's.
    scene.set_local_transform(card, rotated(0.));
    let (stats, p) = render(&mut gpu, &mut scene);
    assert_eq!((stats.layer_repaints, stats.layer_cache_hits), (1, 0));
    assert_eq!(pixel(&p, 42, 42), [255, 0, 0, 255]);

    // A clipping card rotated a quarter turn clips its overflow to its
    // rotated box.
    let mut scene = self::scene();
    let root = scene.root();
    let clip = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            ..fixed(40., 40.)
        },
    );
    scene.set_transform(clip, Transform { x: 100., y: 100. });
    rect(&mut scene, clip, 0., 0., (80., 10.), BLUE);
    scene.set_local_transform(clip, rotated(FRAC_PI_2));
    let (_, p) = render(&mut gpu, &mut scene);
    // Rotated about (120, 120): the row runs down x 130..140, y 100..140.
    assert_eq!(pixel(&p, 135, 105), [0, 0, 255, 255]);
    assert_eq!(pixel(&p, 135, 138), [0, 0, 255, 255]);
    assert_eq!(pixel(&p, 135, 150), [0; 4]);
    assert_eq!(pixel(&p, 105, 105), [0; 4]);
}

#[test]
fn group_opacity_applies_once_to_a_rotated_layer() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    let group = scene.append(root, NodeKind::Container(Layout::Overlay), fixed(40., 40.));
    scene.set_transform(group, Transform { x: 80., y: 80. });
    rect(&mut scene, group, 0., 0., (40., 40.), RED);
    rect(&mut scene, group, 0., 0., (40., 40.), BLUE);
    scene.set_effects(
        group,
        Effects {
            opacity: 0.5,
            ..Default::default()
        },
    );
    scene.set_local_transform(group, rotated(FRAC_PI_4));
    let (_, p) = render(&mut gpu, &mut scene);
    // Blue over red, then halved once: no red shows through.
    // Half of 255 rounds either way, by GPU.
    let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 1);
    assert!(
        near(pixel(&p, 100, 100), [0, 0, 128, 128]),
        "{:?}",
        pixel(&p, 100, 100)
    );
    assert!(
        near(pixel(&p, 100, 75), [0, 0, 128, 128]),
        "{:?}",
        pixel(&p, 100, 75)
    );
}

#[test]
fn transform_only_frames_composite_cached_layers_without_repainting() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        let mut scene = scene();
        let root = scene.root();
        // Four labelled cards, far enough apart to damage separately.
        let cards: Vec<NodeId> = [(10., 10.), (110., 10.), (10., 110.), (110., 110.)]
            .into_iter()
            .map(|(x, y)| {
                let card =
                    scene.append(root, NodeKind::Container(Layout::Overlay), fixed(50., 30.));
                scene.set_transform(card, Transform { x, y });
                rect(&mut scene, card, 0., 0., (50., 30.), RED);
                scene.append(
                    card,
                    NodeKind::Text {
                        text: "card".into(),
                        color: Color(255, 255, 255, 255),
                        font_size: 12.,
                    },
                    fixed(50., 30.),
                );
                card
            })
            .collect();
        for card in &cards {
            scene.set_local_transform(*card, rotated(0.1));
        }
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 4);
        let allocated = gpu.debug_cache_stats().layer_bytes;
        for frame in 1..=5 {
            for card in &cards {
                scene.set_local_transform(*card, rotated(0.1 + frame as f32 * 0.2));
            }
            let metal = gpu.debug_metal_frames();
            let (stats, _) = render(&mut gpu, &mut scene);
            // Where the adapter takes the direct Metal path (unified memory),
            // a transform-only frame takes it too.
            if direct && metal > 0 {
                assert!(gpu.debug_metal_frames() > metal);
            }
            if !direct {
                assert_eq!(gpu.debug_metal_frames(), 0);
            }
            assert_eq!(stats.layer_repaints, 0, "direct={direct}");
            assert_eq!(stats.layer_cache_hits, 4);
            assert_eq!(stats.layer_texture_allocations, 0);
            assert_eq!(stats.shaped_nodes, 0);
            assert_eq!(stats.glyph_uploads, 0);
            // One pass of four composites: no content is drawn again.
            assert_eq!(stats.render_passes, 1);
            assert_eq!(stats.instances, 4, "direct={direct}");
        }
        assert_eq!(gpu.debug_cache_stats().layer_bytes, allocated);
        // Back through identity and out again: the texture is kept.
        scene.set_local_transform(cards[0], LocalTransform::IDENTITY);
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 0);
        assert_eq!(gpu.debug_cache_stats().layer_textures, 4);
        scene.set_local_transform(cards[0], rotated(0.3));
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 0);
        assert_eq!(stats.layer_texture_allocations, 0);
    }
}

#[test]
fn raster_scale_rerenders_only_when_its_step_changes() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    let card = scene.append(root, NodeKind::Container(Layout::Overlay), fixed(100., 50.));
    scene.set_transform(card, Transform { x: 50., y: 75. });
    scene.append(
        card,
        NodeKind::Text {
            text: "zoom".into(),
            color: Color(255, 255, 255, 255),
            font_size: 12.,
        },
        fixed(100., 50.),
    );
    scene.set_local_transform(card, rotated(0.2));
    assert_eq!(render(&mut gpu, &mut scene).0.layer_repaints, 1);
    let bytes = gpu.debug_cache_stats().layer_bytes;
    // (scale, repaints): √2 steps, three quarters of a step of hysteresis.
    let steps = [
        (1.1, 0),
        (1.25, 0),
        (1.6, 1),
        (1.7, 0),
        (1.45, 0),
        (2.2, 1),
        (2.5, 0),
        (2.1, 0),
        (1.2, 1),
        (1.0, 1),
    ];
    for (factor, repaints) in steps {
        scene.set_local_transform(
            card,
            LocalTransform {
                rotate: 0.2,
                ..scaled(factor)
            },
        );
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, repaints, "scale {factor}");
    }
    // Rotation and skew at scale 1 never re-render.
    for angle in [0.5, 1.0, 2.0, 3.0] {
        scene.set_local_transform(
            card,
            LocalTransform {
                rotate: angle,
                skew: (angle * 0.1, 0.),
                ..LocalTransform::IDENTITY
            },
        );
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 0);
        assert_eq!(stats.layer_texture_allocations, 0);
    }
    // Magnified 3×, the layer renders finer than the window: more texels
    // (at 1× it may keep a texture a larger step allocated).
    scene.set_local_transform(card, scaled(3.));
    assert_eq!(render(&mut gpu, &mut scene).0.layer_repaints, 1);
    assert!(gpu.debug_cache_stats().layer_bytes > 4 * bytes);
}

/// Red pixels in column `x`.
fn column(pixels: &[u8], x: u32) -> u32 {
    (0..SIZE)
        .filter(|y| pixel(pixels, x, *y) == [255, 0, 0, 255])
        .count() as u32
}

#[test]
fn a_tilt_in_perspective_shows_the_nearer_side_larger() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        let mut scene = scene();
        let root = scene.root();
        let card = rect(&mut scene, root, 50., 50., (100., 100.), RED);
        render(&mut gpu, &mut scene);
        // Turned 30° about its vertical axis, seen from 300 points: its right
        // side goes away (shorter), its left comes nearer (taller).
        let tilt = LocalTransform {
            rotate_y: 30f32.to_radians(),
            perspective: 300.,
            ..LocalTransform::IDENTITY
        };
        scene.set_local_transform(card, tilt);
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 1, "direct {direct}");
        let matrix = tilt.projective(Rect::new(50., 50., 100., 100.));
        let (left, top) = matrix.point(50., 50.);
        let (right, top_right) = matrix.point(150., 50.);
        assert!(top < 50. && top_right > 50., "{top} {top_right}");
        let (near, far) = (
            column(&p, left.ceil() as u32 + 2),
            column(&p, right.floor() as u32 - 2),
        );
        assert!(
            near > 100 && far < 100 && near > far + 10,
            "direct {direct}: {near} {far}"
        );
        // Its centre stays, and what it no longer covers is clear.
        assert_eq!(pixel(&p, 100, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, right.ceil() as u32 + 2, 100), [0; 4]);
        // Hits follow the projection: inside the tilted card, not past it.
        assert_eq!(scene.hit_test(left + 3., 100.), Some(card));
        assert_ne!(scene.hit_test(right + 3., 100.), Some(card));

        // Without perspective the tilt only narrows it, both sides alike.
        scene.set_local_transform(
            card,
            LocalTransform {
                perspective: 0.,
                ..tilt
            },
        );
        let (_, p) = render(&mut gpu, &mut scene);
        let half = 50. * 30f32.to_radians().cos();
        let (l, r) = (
            (100. - half).ceil() as u32 + 1,
            (100. + half).floor() as u32 - 1,
        );
        assert_eq!(column(&p, l), 100);
        assert_eq!(column(&p, r), 100);
        assert_eq!(pixel(&p, (100. + half).ceil() as u32 + 1, 100), [0; 4]);
    }
}
