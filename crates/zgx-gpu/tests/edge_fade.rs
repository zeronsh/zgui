//! `fade_edges` on all four sides: a clip fades its descendants out over
//! independent bands at each edge (a horizontally scrolling row fading its
//! right 28 px), on the wgpu path and with Metal directly.
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
fn alpha(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[((y * SIZE + x) * 4 + 3) as usize]
}
/// A 160 × 80 clip at (20, 20) fading `vertical` (top, bottom) and
/// `horizontal` (left, right) bands, filled by an opaque rect.
fn faded(vertical: [f32; 2], horizontal: [f32; 2]) -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let clip = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(160.),
            height: Some(80.),
            clip: true,
            fade_edges: vertical,
            fade_edges_x: horizontal,
            ..Default::default()
        },
    );
    scene.set_transform(clip, Transform { x: 20., y: 20. });
    scene.append(
        clip,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        Style {
            width: Some(400.),
            height: Some(80.),
            ..Default::default()
        },
    );
    scene
}
/// The alpha `distance` into a band `width` wide: the quadratic ramp.
fn ramp(distance: f32, width: f32) -> u8 {
    ((distance / width).powi(2) * 255.).round() as u8
}
fn render(direct: bool, scene: &mut Scene) -> Vec<u8> {
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    gpu.set_direct_metal(direct);
    let damage = scene.flush().damage;
    gpu.render(scene, &damage).unwrap();
    gpu.readback().unwrap()
}

#[test]
fn a_row_fades_its_right_edge_alone() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let p = render(direct, &mut faded([0., 0.], [0., 28.]));
        // Opaque to 28 px from the right edge (x 180), then a quadratic
        // ramp down to nothing; top, bottom and left stay sharp.
        assert_eq!(alpha(&p, 150, 60), 255, "direct {direct}");
        assert_eq!(alpha(&p, 21, 60), 255);
        assert_eq!(alpha(&p, 100, 21), 255);
        assert_eq!(alpha(&p, 100, 98), 255);
        // Into the band, the square of how far in: at x 166 (its centre
        // 13.5 px from the edge), (13.5 / 28)².
        let half = alpha(&p, 166, 60);
        assert!(half.abs_diff(ramp(13.5, 28.)) <= 1, "half {half}");
        assert!(alpha(&p, 179, 60) < 5);
        let ramp: Vec<u8> = (150..180).map(|x| alpha(&p, x, 60)).collect();
        assert!(ramp.windows(2).all(|w| w[0] >= w[1]), "{ramp:?}");
    }
}

#[test]
fn all_four_edges_fade_independently() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let p = render(direct, &mut faded([10., 20.], [30., 40.]));
        // Each band by its own width, at pixel centres; the middle opaque.
        let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
        assert!(
            near(alpha(&p, 100, 25), ramp(5.5, 10.)),
            "top {}",
            alpha(&p, 100, 25)
        );
        assert!(
            near(alpha(&p, 100, 90), ramp(9.5, 20.)),
            "bottom {}",
            alpha(&p, 100, 90)
        );
        assert!(
            near(alpha(&p, 35, 60), ramp(15.5, 30.)),
            "left {}",
            alpha(&p, 35, 60)
        );
        assert!(
            near(alpha(&p, 160, 60), ramp(19.5, 40.)),
            "right {}",
            alpha(&p, 160, 60)
        );
        assert_eq!(alpha(&p, 100, 60), 255, "direct {direct}");
    }
}
