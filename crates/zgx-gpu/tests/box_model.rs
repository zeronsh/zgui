//! The box model in pixels, on the wgpu path and with Metal directly: a
//! border inside its box with the content inside it (border-box), an
//! overlay centring its child, and an absolute box placed against a
//! relative ancestor's padding box.
use std::sync::Arc;
use zgx::decoration::Decoration;
use zgx::layout::{EdgeLengths, LayoutOptions, Length};
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
const DARK: [u8; 4] = [20, 20, 20, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
fn color([r, g, b, a]: [u8; 4]) -> Color {
    Color(r, g, b, a)
}
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
/// A dark panel with a white border of `sides`.
fn panel(layout: Layout, sides: Insets) -> NodeKind {
    NodeKind::Panel {
        layout,
        quad: QuadStyle {
            fill: color(DARK),
            border_color: color(WHITE),
            decoration: Some(Arc::new(Decoration {
                border_widths: Some(sides),
                ..Default::default()
            })),
            ..Default::default()
        },
    }
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> Vec<u8> {
    let damage = scene.flush().damage;
    gpu.render(scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    gpu.render(scene, &[Rect::new(0., 0., SIZE as f32, SIZE as f32)])
        .unwrap();
    assert_eq!(
        pixels,
        gpu.readback().unwrap(),
        "incremental frame differs from full"
    );
    pixels
}
fn renderers() -> impl Iterator<Item = (bool, GpuRenderer)> {
    [true, false].into_iter().map(|direct| {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        (direct, gpu)
    })
}
fn filling() -> Style {
    Style {
        width_percent: Some(1.),
        height_percent: Some(1.),
        ..Default::default()
    }
}

#[test]
fn content_fills_inside_the_border_not_under_it() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let sides = Insets {
            top: 4.,
            right: 8.,
            bottom: 2.,
            left: 6.,
        };
        let frame = scene.append(scene.root(), panel(Layout::Column, sides), fixed(120., 80.));
        scene.set_transform(frame, Transform { x: 40., y: 40. });
        scene.append(frame, NodeKind::Rect(color(RED)), filling());
        let p = render(&mut gpu, &mut scene);
        let case = format!("direct {direct}");
        // Each side's border shows; the fill starts and ends inside them.
        assert_eq!(pixel(&p, 100, 41), WHITE, "{case} top");
        assert_eq!(pixel(&p, 100, 44), RED, "{case} below top");
        assert_eq!(pixel(&p, 45, 80), WHITE, "{case} left");
        assert_eq!(pixel(&p, 46, 80), RED, "{case} right of left");
        assert_eq!(pixel(&p, 152, 80), WHITE, "{case} right");
        assert_eq!(pixel(&p, 151, 80), RED, "{case} left of right");
        assert_eq!(pixel(&p, 100, 118), WHITE, "{case} bottom");
        assert_eq!(pixel(&p, 100, 117), RED, "{case} above bottom");
        // Thicker, the content moves in with it.
        let sides = Insets::all(10.);
        scene.set_kind(frame, panel(Layout::Column, sides));
        let p = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 100, 48), WHITE, "{case} thick");
        assert_eq!(pixel(&p, 100, 50), RED, "{case} thick inside");
    }
}

#[test]
fn an_overlay_centres_its_child_both_ways() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let tile = scene.append(
            scene.root(),
            panel(Layout::Overlay, Insets::all(2.)),
            Style {
                align: Align::Center,
                justify: Justify::Center,
                ..fixed(100., 60.)
            },
        );
        scene.set_transform(tile, Transform { x: 50., y: 70. });
        scene.append(tile, NodeKind::Rect(color(RED)), fixed(20., 20.));
        let p = render(&mut gpu, &mut scene);
        let case = format!("direct {direct}");
        // (90, 90) to (110, 110).
        assert_eq!(pixel(&p, 91, 91), RED, "{case}");
        assert_eq!(pixel(&p, 108, 108), RED, "{case}");
        assert_eq!(pixel(&p, 88, 100), DARK, "{case}");
        assert_eq!(pixel(&p, 100, 75), DARK, "{case}");
    }
}

#[test]
fn an_absolute_box_sits_in_its_relative_ancestors_corner() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let card = scene.append(
            scene.root(),
            panel(Layout::Column, Insets::all(3.)),
            Style {
                relative: true,
                padding: 12.,
                ..fixed(120., 100.)
            },
        );
        scene.set_transform(card, Transform { x: 40., y: 40. });
        let inner = scene.append(card, NodeKind::Container(Layout::Column), Style::default());
        scene.append(
            inner,
            NodeKind::Rect(color(RED)),
            Style {
                absolute: true,
                layout_options: Some(Arc::new(LayoutOptions {
                    inset: EdgeLengths {
                        right: Some(Length::Px(0.)),
                        bottom: Some(Length::Px(0.)),
                        ..Default::default()
                    },
                    ..Default::default()
                })),
                ..fixed(10., 10.)
            },
        );
        let p = render(&mut gpu, &mut scene);
        let case = format!("direct {direct}");
        // Inside the border, ignoring the padding: (147, 127) to (157, 137).
        assert_eq!(pixel(&p, 156, 136), RED, "{case}");
        assert_eq!(pixel(&p, 148, 128), RED, "{case}");
        assert_eq!(pixel(&p, 158, 138), WHITE, "{case} border");
        assert_eq!(pixel(&p, 145, 125), DARK, "{case} outside it");
    }
}
