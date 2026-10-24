//! Borders: per-side widths with per-corner radii drawn as rings by the
//! quad shader (no raster, so a resizing box keeps them crisp for free),
//! and `border_over` painting a node's border over its descendants; on the
//! wgpu path and with Metal directly.
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
const WHITE: [u8; 4] = [255; 4];
const DARK: Color = Color(20, 20, 20, 255);
const RED: Color = Color(255, 0, 0, 255);
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
/// A panel at (40, 40), `size`, filled dark, with a white border of `sides`
/// and `corners`.
fn panel(
    scene: &mut Scene,
    size: (f32, f32),
    sides: Insets,
    corners: Corners,
    over: bool,
) -> NodeId {
    let quad = QuadStyle {
        fill: DARK,
        border_color: Color(255, 255, 255, 255),
        decoration: Some(Arc::new(Decoration {
            corners: Some(corners),
            border_widths: Some(sides),
            ..Default::default()
        })),
        border_over: over,
        ..Default::default()
    };
    let root = scene.root();
    let id = scene.append(
        root,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad,
        },
        fixed(size.0, size.1),
    );
    scene.set_transform(id, Transform { x: 40., y: 40. });
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
/// Top, left and right 4 px; no bottom; rounded 16 at the top only.
const SIDES: Insets = Insets {
    top: 4.,
    right: 4.,
    bottom: 0.,
    left: 4.,
};
const TOP_ROUNDED: Corners = Corners {
    top_left: 16.,
    top_right: 16.,
    bottom_right: 0.,
    bottom_left: 0.,
};

#[test]
fn per_side_borders_follow_per_corner_radii_without_rasterizing() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = panel(&mut scene, (120., 80.), SIDES, TOP_ROUNDED, false);
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.canvas_rasterizations, 0, "direct {direct}");
        let dark = [20, 20, 20, 255];
        // Top, left and right edges white; the bottom edge the fill.
        assert_eq!(pixel(&p, 100, 41), WHITE);
        assert_eq!(pixel(&p, 41, 100), WHITE);
        assert_eq!(pixel(&p, 158, 100), WHITE);
        assert_eq!(pixel(&p, 100, 118), dark);
        assert_eq!(pixel(&p, 100, 60), dark);
        // Square bottom corners: the side borders run into them.
        assert_eq!(pixel(&p, 41, 119), WHITE);
        assert_eq!(pixel(&p, 158, 119), WHITE);
        // Rounded top corners: outside the curve, nothing; along it, the
        // ring; inside it, the fill. (43, 43) is 18 px from (56, 56).
        assert_eq!(pixel(&p, 41, 41), [0; 4]);
        assert_eq!(pixel(&p, 43, 43), [0; 4]);
        assert_eq!(pixel(&p, 46, 46), WHITE);
        assert_eq!(pixel(&p, 52, 52), dark);
        // Resized every frame, it stays analytic: still no raster.
        for width in [130., 150., 140.] {
            let mut style = scene.style(id);
            style.width = Some(width);
            scene.set_style(id, style);
            let (stats, p) = render(&mut gpu, &mut scene);
            assert_eq!(stats.canvas_rasterizations, 0);
            let right = 40 + width as u32 - 2;
            assert_eq!(pixel(&p, right, 100), WHITE, "width {width}");
            assert_eq!(pixel(&p, right - 10, 100), dark, "width {width}");
        }
    }
}

#[test]
fn borders_over_content_paint_after_the_nodes_children() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        for over in [false, true] {
            let mut scene = scene();
            let id = panel(
                &mut scene,
                (120., 80.),
                Insets::all(4.),
                Corners::all(12.),
                over,
            );
            // A child filling the panel, as a photo fills its frame.
            let child = scene.append(id, NodeKind::Rect(RED), fixed(120., 80.));
            // Over the border band: a layout border lays its content out
            // inside it, so move the child back onto the frame's edge.
            if !over {
                scene.set_transform(child, Transform { x: -4., y: -4. });
            }
            let (_, p) = render(&mut gpu, &mut scene);
            let case = format!("direct {direct}, over {over}");
            let border = pixel(&p, 100, 41);
            let middle = pixel(&p, 100, 80);
            assert_eq!(middle, [255, 0, 0, 255], "{case}");
            assert_eq!(
                border,
                if over { WHITE } else { [255, 0, 0, 255] },
                "{case}"
            );
            // Over a layer too (a rotated child), and as the child changes.
            scene.set_local_transform(
                child,
                LocalTransform {
                    rotate: 0.001,
                    ..LocalTransform::IDENTITY
                },
            );
            scene.set_kind(child, NodeKind::Rect(Color(0, 0, 255, 255)));
            let (_, p) = render(&mut gpu, &mut scene);
            let blue = [0, 0, 255, 255];
            assert_eq!(pixel(&p, 100, 80), blue, "{case}");
            assert_eq!(
                pixel(&p, 100, 41),
                if over { WHITE } else { blue },
                "{case}"
            );
            assert_eq!(
                pixel(&p, 41, 100),
                if over { WHITE } else { blue },
                "{case}"
            );
            // A plain bordered quad (no decoration) does the same.
            let mut quad = QuadStyle {
                fill: DARK,
                radius: 12.,
                border_color: Color(255, 255, 255, 255),
                border_width: 4.,
                border_over: over,
                ..Default::default()
            };
            scene.set_kind(
                id,
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: quad.clone(),
                },
            );
            let (_, p) = render(&mut gpu, &mut scene);
            assert_eq!(
                pixel(&p, 100, 41),
                if over { WHITE } else { blue },
                "{case}, plain"
            );
            assert_eq!(pixel(&p, 100, 80), blue, "{case}, plain");
            // Without it, the panel's own border is drawn as before.
            quad.border_over = false;
            scene.remove(child);
            scene.set_kind(
                id,
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad,
                },
            );
            let (_, p) = render(&mut gpu, &mut scene);
            assert_eq!(pixel(&p, 100, 41), WHITE, "{case}, alone");
        }
    }
}
