//! A zoomed node (`Scene::set_zoom`) draws as the same content does at the
//! window's zoom: its layer rasterized at the scale it shows at and shown
//! texel for pixel, so text, rounded corners and hairlines are as crisp, not
//! resampled from a smaller raster.
use zgx::scene::*;
use zgx_gpu::{GpuRenderer, GpuStats};

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const PIXELS: u32 = 240;

/// A panel of text, a rounded bordered box and a 1-point hairline, laid
/// out in `size` points; `zoom` on the panel when given.
fn scene(size: f32, zoom: Option<f32>) -> Scene {
    let mut scene = Scene::new(size, size);
    let root = scene.root();
    scene.set_kind(root, NodeKind::Container(Layout::Column));
    let panel = scene.append(
        root,
        NodeKind::Container(Layout::Column),
        Style {
            padding: 4.,
            gap: 3.,
            ..Default::default()
        },
    );
    scene.append(
        panel,
        NodeKind::Text {
            text: "Zoomed Ag".into(),
            color: Color(20, 20, 20, 255),
            font_size: 13.,
        },
        Style::default(),
    );
    scene.append(
        panel,
        NodeKind::Quad(QuadStyle {
            fill: Color(40, 110, 200, 255),
            radius: 6.,
            border_color: Color(10, 10, 10, 255),
            border_width: 1.,
            ..Default::default()
        }),
        Style {
            width: Some(60.),
            height: Some(24.),
            ..Default::default()
        },
    );
    scene.append(
        panel,
        NodeKind::Rect(Color(0, 0, 0, 255)),
        Style {
            width: Some(80.),
            height: Some(1.),
            ..Default::default()
        },
    );
    if let Some(zoom) = zoom {
        scene.set_zoom(panel, zoom);
    }
    scene.flush();
    scene
}
fn draw(gpu: &mut GpuRenderer, scene: &Scene, size: f32) -> Vec<u8> {
    gpu.set_background(Color(255, 255, 255, 255));
    gpu.render(scene, &[Rect::new(0., 0., size, size)]).unwrap();
    gpu.readback().unwrap()
}
/// Pixels whose channels differ by more than `tolerance`.
fn differing(a: &[u8], b: &[u8], tolerance: u8) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .any(|(a, b)| a.abs_diff(*b) > tolerance)
        })
        .count()
}

#[test]
fn a_zoomed_panel_draws_as_the_window_zoom_draws_it() {
    let _gpu = gpu_fixture();
    for zoom in [2., 1.25, 0.5] {
        // The window at `zoom`: its content laid out in fewer points, drawn
        // at the device's scale times it.
        let mut window = GpuRenderer::new(PIXELS, PIXELS).unwrap();
        window.set_scale_factor(zoom);
        let points = PIXELS as f32 / zoom;
        let at_window_zoom = draw(&mut window, &scene(points, None), points);
        // The panel zoomed in a window at 100%.
        let mut panel = GpuRenderer::new(PIXELS, PIXELS).unwrap();
        let zoomed = draw(&mut panel, &scene(PIXELS as f32, Some(zoom)), PIXELS as f32);
        // A rounded corner's coverage may differ by some levels (drawn in a
        // layer's pass, up to 25 at half size); resampled, every text edge
        // would differ.
        let off = differing(&at_window_zoom, &zoomed, 32);
        assert!(
            off <= 4,
            "at {zoom}: {off} pixels differ from the window's zoom"
        );
        // A scale transform of the same panel rasterizes at a step and
        // resamples: far from the window's zoom.
        if zoom == 1.25 {
            let mut scaled = scene(PIXELS as f32, None);
            let panel = scaled.children(scaled.root())[0];
            scaled.set_local_transform(
                panel,
                LocalTransform {
                    scale: (zoom, zoom),
                    origin: (0., 0.),
                    ..LocalTransform::IDENTITY
                },
            );
            scaled.flush();
            let mut gpu = GpuRenderer::new(PIXELS, PIXELS).unwrap();
            let resampled = draw(&mut gpu, &scaled, PIXELS as f32);
            let blurred = differing(&at_window_zoom, &resampled, 32);
            assert!(blurred > 4 * off.max(10), "resampled: {blurred}");
        }
    }
}

#[test]
fn a_zoomed_panel_keeps_its_hairline_whole() {
    let _gpu = gpu_fixture();
    for zoom in [0.75, 1.1, 1.25] {
        let mut gpu = GpuRenderer::new(PIXELS, PIXELS).unwrap();
        gpu.set_scale_factor(2.);
        let points = PIXELS as f32 / 2.;
        let pixels = draw(&mut gpu, &scene(points, Some(zoom)), points);
        // The hairline's column of pixels: dark somewhere, never doubled
        // into two full-strength rows, never gone.
        // 82 of its points in (the line runs from 4 to 84; the text and the
        // box end before).
        let column = (82. * zoom * 2.) as u32;
        let darkness: Vec<u8> = (0..PIXELS)
            .map(|y| 255 - pixels[((y * PIXELS + column) * 4) as usize])
            .collect();
        let strong = darkness.iter().filter(|d| **d > 200).count();
        let any = darkness.iter().any(|d| *d > 60);
        assert!(any, "at {zoom}: the hairline vanished");
        assert!(strong <= (2. * zoom).ceil() as usize, "at {zoom}: {strong}");
    }
}

#[test]
fn a_zoomed_panel_off_a_pixel_draws_as_on_the_nearest_one() {
    let _gpu = gpu_fixture();
    // The panel after a spacer `offset` points wide.
    let draw_at = |offset: f32| {
        let mut scene = Scene::new(PIXELS as f32, PIXELS as f32);
        let root = scene.root();
        scene.set_kind(root, NodeKind::Container(Layout::Row));
        scene.append(
            root,
            NodeKind::Container(Layout::Column),
            Style {
                width: Some(offset),
                height: Some(1.),
                ..Default::default()
            },
        );
        let panel = scene.append(root, NodeKind::Container(Layout::Column), Style::default());
        scene.append(
            panel,
            NodeKind::Text {
                text: "Zoomed Ag".into(),
                color: Color(20, 20, 20, 255),
                font_size: 13.,
            },
            Style::default(),
        );
        scene.set_zoom(panel, 1.25);
        scene.flush();
        let mut gpu = GpuRenderer::new(PIXELS, PIXELS).unwrap();
        draw(&mut gpu, &scene, PIXELS as f32)
    };
    assert_eq!(differing(&draw_at(10.3), &draw_at(10.), 0), 0);
}

/// Each scene's frame after its text changes: drawn to, read back, and
/// what the frame cost.
fn after_text_change(gpu: &mut GpuRenderer, scene: &mut Scene, size: f32) -> (Vec<u8>, GpuStats) {
    draw(gpu, scene, size);
    let panel = scene.children(scene.root())[0];
    let text = scene.children(panel)[0];
    scene.set_text(text, "Zoomed Ab");
    let report = scene.flush();
    let stats = gpu.render(scene, &report.damage).unwrap();
    (gpu.readback().unwrap(), stats)
}

#[test]
fn text_changing_in_a_zoomed_panel_repaints_its_part_of_the_layer_alone() {
    let _gpu = gpu_fixture();
    let zoom = 2.;
    // The window at 200%: the change's damage alone.
    let mut window = GpuRenderer::new(PIXELS, PIXELS).unwrap();
    window.set_scale_factor(zoom);
    let points = PIXELS as f32 / zoom;
    let (_, at_window_zoom) = after_text_change(&mut window, &mut scene(points, None), points);
    // The panel at 200% in a window at 100%: its layer's part once more,
    // not the whole layer.
    let size = PIXELS as f32;
    let mut zoomed = scene(size, Some(zoom));
    let mut gpu = GpuRenderer::new(PIXELS, PIXELS).unwrap();
    let (partial, stats) = after_text_change(&mut gpu, &mut zoomed, size);
    assert_eq!((stats.layer_repaints, stats.layer_partial_repaints), (1, 1));
    // 11,810 pixels where the window zoom repaints 5,396: the layer's part
    // and the window's, each about the change (30,296 with the whole layer).
    assert!(
        stats.damaged_pixels * 2 <= at_window_zoom.damaged_pixels * 5,
        "{} pixels against the window zoom's {}",
        stats.damaged_pixels,
        at_window_zoom.damaged_pixels
    );
    // As a fresh render of the same scene draws it.
    let mut fresh = GpuRenderer::new(PIXELS, PIXELS).unwrap();
    let full = draw(&mut fresh, &zoomed, size);
    assert_eq!(differing(&partial, &full, 0), 0);
}
