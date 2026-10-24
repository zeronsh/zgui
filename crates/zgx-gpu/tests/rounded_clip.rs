//! Rounded clipping (`overflow: hidden` with `border-radius`): a node that
//! clips with rounded corners cuts its descendants' pixels to that shape,
//! antialiased, following its laid-out size, nested clips intersecting,
//! inside rotated layers too; on the wgpu path and with Metal directly.
use std::sync::Arc;
use zgx::decoration::{Corners, Decoration};
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
const RED: Color = Color(255, 0, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);
const CLEAR: [u8; 4] = [0; 4];
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}
/// A clipping panel of `size` at (`x`, `y`) with `corners` and no fill of
/// its own.
fn card(
    scene: &mut Scene,
    parent: NodeId,
    (x, y): (f32, f32),
    size: f32,
    corners: Corners,
) -> NodeId {
    let quad = QuadStyle {
        decoration: Some(Arc::new(Decoration {
            corners: Some(corners),
            ..Default::default()
        })),
        ..Default::default()
    };
    let id = scene.append(
        parent,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad,
        },
        Style {
            clip: true,
            ..fixed(size, size)
        },
    );
    scene.set_transform(id, Transform { x, y });
    id
}
/// An opaque `color` rect of `size` at (`x`, `y`).
fn rect(
    scene: &mut Scene,
    parent: NodeId,
    (x, y): (f32, f32),
    size: (f32, f32),
    color: Color,
) -> NodeId {
    let id = scene.append(parent, NodeKind::Rect(color), fixed(size.0, size.1));
    scene.set_transform(id, Transform { x, y });
    id
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
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
fn renderers() -> impl Iterator<Item = (bool, GpuRenderer)> {
    [true, false].into_iter().map(|direct| {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        (direct, gpu)
    })
}

#[test]
fn rounded_clips_cut_descendants_corners_and_follow_their_size() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let root = scene.root();
        let clipping = card(&mut scene, root, (50., 50.), 100., Corners::all(20.));
        let fill = rect(&mut scene, clipping, (0., 0.), (120., 120.), RED);
        let (_, p) = render(&mut gpu, &mut scene);
        // Inside the curve, and away from it, red; outside it, nothing:
        // (52, 52) lies 25 px from the top-left corner's centre (70, 70).
        assert_eq!(pixel(&p, 60, 60), [255, 0, 0, 255], "direct {direct}");
        assert_eq!(pixel(&p, 100, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 52, 52), CLEAR);
        assert_eq!(pixel(&p, 147, 147), CLEAR);
        assert_eq!(pixel(&p, 147, 100), [255, 0, 0, 255]);
        // Along the curve, antialiased: at y 60.5 it crosses x 52.4.
        let row: Vec<u8> = (50..64).map(|x| pixel(&p, x, 60)[3]).collect();
        assert!(row.iter().any(|a| *a > 20 && *a < 235), "{row:?}");
        // The clip follows the card's size: grown, its corner moves out.
        let mut style = scene.style(clipping);
        style.width = Some(140.);
        style.height = Some(140.);
        scene.set_style(clipping, style);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 147, 147), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 168, 168), [255, 0, 0, 255]);
        // The red ends at 170: the corner at (190, 190) cuts nothing.
        assert_eq!(pixel(&p, 169, 169), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 175, 175), CLEAR);
        // A text-like child (any kind) is cut the same way; the red is.
        scene.set_kind(fill, NodeKind::Rect(BLUE));
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 52, 52), CLEAR);
        assert_eq!(pixel(&p, 60, 60), [0, 0, 255, 255]);
        if direct && cfg!(target_os = "macos") {
            assert!(gpu.debug_metal_frames() > 0, "never drew with Metal");
        }
    }
}

#[test]
fn per_corner_radii_and_nested_clips_intersect() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let root = scene.root();
        // Rounded only along the left: the right corners stay square.
        let outer = card(
            &mut scene,
            root,
            (20., 20.),
            160.,
            Corners {
                top_left: 40.,
                top_right: 0.,
                bottom_right: 0.,
                bottom_left: 40.,
            },
        );
        // A thumbnail at the outer card's top-left, rounded 10 on its own,
        // holding a photo larger than both.
        let inner = card(&mut scene, outer, (0., 0.), 80., Corners::all(10.));
        rect(&mut scene, inner, (-10., -10.), (120., 120.), RED);
        rect(&mut scene, outer, (100., 0.), (60., 160.), BLUE);
        let (_, p) = render(&mut gpu, &mut scene);
        // The outer card's square top-right corner keeps its blue.
        assert_eq!(pixel(&p, 179, 20), [0, 0, 255, 255], "direct {direct}");
        // The thumbnail's bottom-right corner is its own 10 px curve.
        assert_eq!(pixel(&p, 98, 98), CLEAR);
        assert_eq!(pixel(&p, 90, 90), [255, 0, 0, 255]);
        // At the shared top-left corner, the outer card's 40 px curve cuts
        // what the thumbnail's 10 px one would keep: (26, 26) is inside
        // the inner curve but outside the outer.
        assert_eq!(pixel(&p, 26, 26), CLEAR);
        assert_eq!(pixel(&p, 40, 40), [255, 0, 0, 255]);
    }
}

#[test]
fn rotated_and_blurred_layers_clip_in_their_own_space() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let root = scene.root();
        let turned = card(&mut scene, root, (50., 50.), 100., Corners::all(30.));
        rect(&mut scene, turned, (0., 0.), (100., 100.), RED);
        scene.set_local_transform(
            turned,
            LocalTransform {
                rotate: std::f32::consts::FRAC_PI_4,
                ..LocalTransform::IDENTITY
            },
        );
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 1, "direct {direct}");
        // Rotated 45° about (100, 100): its corners point up, down and to
        // the sides, each cut by its 30 px curve, so the card's tip falls
        // short of the unrounded diamond's (100, 29.3).
        assert_eq!(pixel(&p, 100, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 100, 35), CLEAR);
        assert_eq!(pixel(&p, 100, 50), [255, 0, 0, 255]);
        // Composited inside an outer rounded clip, a blurred child keeps
        // within the outer curve.
        let mut scene = self::scene();
        let root = scene.root();
        let outer = card(&mut scene, root, (50., 50.), 100., Corners::all(20.));
        let blurred = rect(&mut scene, outer, (0., 0.), (100., 100.), RED);
        scene.set_filter_blur(blurred, 2.);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 100, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 52, 52), CLEAR);
        assert_eq!(pixel(&p, 147, 147), CLEAR);
    }
}
