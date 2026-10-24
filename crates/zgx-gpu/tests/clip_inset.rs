//! Clip insets (CSS `clip-path: inset(..)`): a node and its descendants
//! paint inside its box inset by pixels or percentages of its live size,
//! negative insets reaching past it, rounded alike when it clips with
//! rounded corners; animating it composites again without repainting
//! layers. On the wgpu path and with Metal directly.
use zgx::layout::Length::{Percent, Px};
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
const RED: [u8; 4] = [255, 0, 0, 255];
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
/// A 100 × 100 panel at (50, 50) (rounded `radius`, clipping when it is),
/// holding a red rect that fills it.
fn card(scene: &mut Scene, radius: f32) -> NodeId {
    let root = scene.root();
    let id = scene.append(
        root,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad: QuadStyle {
                radius,
                ..Default::default()
            },
        },
        Style {
            clip: radius > 0.,
            ..fixed(100., 100.)
        },
    );
    scene.set_transform(id, Transform { x: 50., y: 50. });
    scene.append(id, NodeKind::Rect(Color(255, 0, 0, 255)), fixed(100., 100.));
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
fn insets_close_a_box_by_percent_as_it_animates() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = card(&mut scene, 0.);
        render(&mut gpu, &mut scene);
        // Closing to its top edge: inset(0 0 b% 0), b 0 → 100.
        for bottom in [0.25, 0.5, 1.] {
            scene.set_clip_inset(
                id,
                Some(ClipInset {
                    top: Px(0.),
                    right: Px(0.),
                    bottom: Percent(bottom),
                    left: Px(0.),
                }),
            );
            let (stats, p) = render(&mut gpu, &mut scene);
            assert_eq!(stats.layer_repaints, 0, "direct {direct}");
            let edge = 50 + (100. * (1. - bottom)) as u32;
            if bottom < 1. {
                assert_eq!(pixel(&p, 100, edge - 2), RED, "bottom {bottom}");
            }
            assert_eq!(pixel(&p, 100, edge + 1), CLEAR, "bottom {bottom}");
        }
        // Negative insets reach past the box: nothing is cut.
        scene.set_clip_inset(id, Some(ClipInset::px(-20., -20., -20., -20.)));
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 51, 51), RED);
        assert_eq!(pixel(&p, 148, 148), RED);
        // Following the live size: grown, the half inset grows with it.
        scene.set_clip_inset(
            id,
            Some(ClipInset {
                top: Percent(0.5),
                ..ClipInset::default()
            }),
        );
        let mut style = scene.style(id);
        style.height = Some(140.);
        scene.set_style(id, style);
        let child = scene.children(id)[0];
        let mut style = scene.style(child);
        style.height = Some(140.);
        scene.set_style(child, style);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 100, 115), CLEAR);
        assert_eq!(pixel(&p, 100, 125), RED);
    }
}

#[test]
fn insets_round_with_their_box_and_clip_layers_without_repainting_them() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        // A rounded clipping card inset 20 px all round: the inset box gets
        // the card's 15 px corners.
        let mut scene = scene();
        let id = card(&mut scene, 15.);
        scene.set_clip_inset(id, Some(ClipInset::px(20., 20., 20., 20.)));
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 100, 100), RED, "direct {direct}");
        assert_eq!(pixel(&p, 65, 100), CLEAR);
        assert_eq!(
            pixel(&p, 72, 72),
            CLEAR,
            "the inset box's corner is rounded"
        );
        assert_eq!(pixel(&p, 80, 80), RED);
        // A blurred node: animating its inset composites its cached
        // blur again, cut, without blurring or painting anew.
        let mut scene = self::scene();
        let id = card(&mut scene, 0.);
        scene.set_filter_blur(id, 2.);
        render(&mut gpu, &mut scene);
        for left in [10., 30., 50.] {
            scene.set_clip_inset(id, Some(ClipInset::px(0., 0., 0., left)));
            let (stats, p) = render(&mut gpu, &mut scene);
            assert_eq!((stats.layer_repaints, stats.effect_passes), (0, 0));
            let edge = 50 + left as u32;
            assert_eq!(pixel(&p, edge - 2, 100), CLEAR, "left {left}");
            assert_eq!(pixel(&p, edge + 10, 100), RED, "left {left}");
        }
        // A rotated layer clips in its own space.
        let mut scene = self::scene();
        let id = card(&mut scene, 0.);
        scene.set_local_transform(
            id,
            LocalTransform {
                rotate: std::f32::consts::FRAC_PI_2,
                ..LocalTransform::IDENTITY
            },
        );
        scene.set_clip_inset(id, Some(ClipInset::px(0., 0., 50., 0.)));
        let (_, p) = render(&mut gpu, &mut scene);
        // A quarter turn clockwise: its top half shows on the right.
        assert_eq!(pixel(&p, 120, 100), RED);
        assert_eq!(pixel(&p, 80, 100), CLEAR);
    }
}
