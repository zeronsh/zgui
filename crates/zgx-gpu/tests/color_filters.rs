//! Colour filters (CSS `filter: brightness() contrast() saturate()`): a
//! node's subtree drawn through an affine colour map, composed with its
//! ancestors', applied per primitive (exactly as to the group: blending
//! commutes with an affine map) and at layer composites without repainting
//! them. On the wgpu path and with Metal directly.
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2)
}
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}
/// A group at (50, 50) holding a red rect and, overlapping it, a half
/// transparent blue one.
fn group(scene: &mut Scene) -> NodeId {
    let root = scene.root();
    let group = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        fixed(100., 100.),
    );
    scene.set_transform(group, Transform { x: 50., y: 50. });
    scene.append(
        group,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(100., 100.),
    );
    let blue = scene.append(
        group,
        NodeKind::Rect(Color(0, 0, 255, 128)),
        fixed(50., 50.),
    );
    scene.set_transform(blue, Transform { x: 50., y: 50. });
    group
}
fn filtered(brightness: f32, contrast: f32, saturate: f32) -> Effects {
    Effects {
        brightness,
        contrast,
        saturate,
        ..Effects::default()
    }
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
fn brightness_contrast_and_saturate_filter_a_group() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = group(&mut scene);
        let (_, plain) = render(&mut gpu, &mut scene);
        // Red, and half blue over red (straight 127, 0, 128).
        let (red, mixed) = (pixel(&plain, 70, 70), pixel(&plain, 125, 125));
        assert_eq!(red, [255, 0, 0, 255], "direct {direct}");
        // Brightness ½ halves both, the mix as a group would.
        scene.set_effects(id, filtered(0.5, 1., 1.));
        let (_, p) = render(&mut gpu, &mut scene);
        assert!(
            near(pixel(&p, 70, 70), [128, 0, 0, 255]),
            "{:?}",
            pixel(&p, 70, 70)
        );
        let half = mixed.map(|c| (c as f32 * 0.5).round() as u8);
        assert!(
            near(pixel(&p, 125, 125), [half[0], half[1], half[2], 255]),
            "{:?} {half:?}",
            pixel(&p, 125, 125)
        );
        // Saturate 0: greyscale by Rec. 709 luma (red: 0.213).
        scene.set_effects(id, filtered(1., 1., 0.));
        let (_, p) = render(&mut gpu, &mut scene);
        assert!(
            near(pixel(&p, 70, 70), [54, 54, 54, 255]),
            "{:?}",
            pixel(&p, 70, 70)
        );
        // Contrast 0: everything mid grey.
        scene.set_effects(id, filtered(1., 0., 1.));
        let (_, p) = render(&mut gpu, &mut scene);
        assert!(
            near(pixel(&p, 70, 70), [128, 128, 128, 255]),
            "{:?}",
            pixel(&p, 70, 70)
        );
        assert!(near(pixel(&p, 125, 125), [128, 128, 128, 255]));
        // Outside the group, untouched.
        assert_eq!(pixel(&p, 10, 10), [0; 4]);
        // Back to 1: as before.
        scene.set_effects(id, Effects::default());
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(p, plain);
    }
}

#[test]
fn nested_filters_compose_and_layers_filter_at_their_composite() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = group(&mut scene);
        let red = scene.children(id)[0];
        // Brightness ½ inside brightness ½: a quarter.
        scene.set_effects(id, filtered(0.5, 1., 1.));
        scene.set_effects(red, filtered(0.5, 1., 1.));
        let (_, p) = render(&mut gpu, &mut scene);
        assert!(
            near(pixel(&p, 70, 70), [64, 0, 0, 255]),
            "direct {direct}: {:?}",
            pixel(&p, 70, 70)
        );
        // A rotated group is a layer: animating its filter composites its
        // pixels again without repainting them.
        scene.set_effects(red, Effects::default());
        scene.set_local_transform(
            id,
            LocalTransform {
                rotate: 0.0001,
                ..LocalTransform::IDENTITY
            },
        );
        render(&mut gpu, &mut scene);
        for brightness in [0.25, 0.75, 1.5] {
            scene.set_effects(id, filtered(brightness, 1., 1.));
            let (stats, p) = render(&mut gpu, &mut scene);
            assert_eq!(stats.layer_repaints, 0, "brightness {brightness}");
            let expected = (255. * brightness).min(255.).round() as u8;
            assert!(
                near(pixel(&p, 70, 70), [expected, 0, 0, 255]),
                "{brightness}: {:?}",
                pixel(&p, 70, 70)
            );
        }
    }
}
