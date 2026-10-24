//! Shader fills (`Background::Shader`): a box filled by a WGSL `fill`
//! function, its uniforms (time, user params) set per node per frame,
//! cut to the box's rounded corners, under its border; a frame-rate cap
//! quantizes time; a shader that fails to compile fills with the box's
//! colour. On the wgpu path and with Metal directly.
use std::sync::Arc;
use zgx::decoration::{Background, Corners, Decoration, FillShader, ShaderUniforms};
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
/// Left of the middle, `params[0]`; right of it, `params[1]` scaled by
/// time (clamped to 1); a pixel's position, size and time as the shader
/// sees them.
const SPLIT: &str = "
fn fill(input: FillInput) -> vec4<f32> {
    if input.p.x < input.size.x * 0.5 { return input.params[0]; }
    return vec4(input.params[1].rgb * min(input.time, 1.0), 1.0);
}";
/// A 100 × 100 box at (50, 50) filled by `shader`, grey under it.
fn boxed(scene: &mut Scene, shader: Arc<FillShader>, corners: f32, border: f32) -> NodeId {
    let quad = QuadStyle {
        fill: Color(50, 50, 50, 255),
        border_color: Color(255, 255, 255, 255),
        border_width: border,
        decoration: Some(Arc::new(Decoration {
            background: Some(Background::Shader(shader)),
            corners: Some(Corners::all(corners)),
            ..Default::default()
        })),
        ..Default::default()
    };
    let root = scene.root();
    let id = scene.append(
        root,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad,
        },
        Style {
            width: Some(100.),
            height: Some(100.),
            ..Default::default()
        },
    );
    scene.set_transform(id, Transform { x: 50., y: 50. });
    id
}
fn uniforms(time: f32) -> ShaderUniforms {
    ShaderUniforms {
        time,
        params: [[1., 0., 0., 1.], [0., 0., 1., 1.], [0.; 4], [0.; 4]],
    }
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> (FrameReport, Vec<u8>) {
    let frame = scene.flush();
    gpu.render(scene, &frame.damage).unwrap();
    let pixels = gpu.readback().unwrap();
    gpu.render(scene, &[Rect::new(0., 0., SIZE as f32, SIZE as f32)])
        .unwrap();
    assert_eq!(
        pixels,
        gpu.readback().unwrap(),
        "incremental frame differs from full"
    );
    (frame, pixels)
}
fn renderers() -> impl Iterator<Item = (bool, GpuRenderer)> {
    [true, false].into_iter().map(|direct| {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        (direct, gpu)
    })
}

#[test]
fn shader_fills_run_per_pixel_with_their_uniforms() {
    let _gpu_fixture = gpu_fixture();
    let shader = FillShader::new(SPLIT, None);
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = boxed(&mut scene, shader.clone(), 20., 3.);
        let other = scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 255, 0, 255)),
            Style {
                width: Some(20.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.set_shader_uniforms(id, uniforms(0.5));
        let (_, p) = render(&mut gpu, &mut scene);
        // Its two halves; the right one at half the blue for time 0.5.
        assert_eq!(pixel(&p, 70, 100), [255, 0, 0, 255], "direct {direct}");
        let right = pixel(&p, 130, 100);
        assert!(right[2].abs_diff(128) <= 1 && right[0] == 0, "{right:?}");
        // Cut to its corners, under its border.
        assert_eq!(pixel(&p, 52, 52), [0; 4]);
        assert_eq!(pixel(&p, 100, 51), [255; 4]);
        // Uniforms each frame repaint the box alone: no layout.
        scene.set_shader_uniforms(id, uniforms(1.));
        let (frame, p) = render(&mut gpu, &mut scene);
        assert_eq!(frame.layout_nodes, 0);
        assert!(
            frame.damage.iter().all(|d| d.x >= 50. && d.y >= 50.),
            "{:?}",
            frame.damage
        );
        assert_eq!(pixel(&p, 130, 100), [0, 0, 255, 255]);
        assert_eq!(pixel(&p, 10, 10), [0, 255, 0, 255]);
        let _ = other;
        if direct && cfg!(target_os = "macos") {
            assert!(gpu.debug_metal_frames() > 0, "never drew with Metal");
        }
    }
}

#[test]
fn frame_rate_caps_and_broken_shaders() {
    let _gpu_fixture = gpu_fixture();
    // At 30 fps, time steps every 1/30 s: changes within a step are idle.
    let mut scene = scene();
    let capped = boxed(&mut scene, FillShader::new(SPLIT, Some(30.)), 0., 0.);
    scene.flush();
    scene.set_shader_uniforms(capped, uniforms(0.1));
    assert!(!scene.flush().is_idle());
    scene.set_shader_uniforms(capped, uniforms(0.1 + 1. / 120.));
    assert!(scene.flush().is_idle());
    scene.set_shader_uniforms(capped, uniforms(0.1 + 1. / 25.));
    assert!(!scene.flush().is_idle());
    assert_eq!(
        scene.shader_uniforms(capped).time,
        (0.14 * 30_f32).floor() / 30.
    );
    // A shader that does not compile fills with the box's colour.
    for (direct, mut gpu) in renderers() {
        let mut scene = self::scene();
        boxed(
            &mut scene,
            FillShader::new("fn fill(input: FillInput) -> vec4<f32> { nope }", None),
            0.,
            0.,
        );
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 100, 100), [50, 50, 50, 255], "direct {direct}");
    }
}

/// A lab-style ordered dither: a 4 × 4 Bayer matrix thresholding a radial
/// orb, with helpers, a constant array and a loop.
const DITHER: &str = "
const BAYER = array<f32, 16>(0., 8., 2., 10., 12., 4., 14., 6., 3., 11., 1., 9., 15., 7., 13., 5.);
fn threshold(cell: vec2<u32>) -> f32 {
    return (BAYER[(cell.y % 4u) * 4u + cell.x % 4u] + 0.5) / 16.;
}
fn orb(p: vec2<f32>, size: vec2<f32>) -> f32 {
    var level = 1. - length(p / size - vec2(0.5)) * 2.;
    for (var i = 0; i < 2; i++) { level = clamp(level, 0., 1.); }
    return level;
}
fn fill(input: FillInput) -> vec4<f32> {
    let cell = vec2<u32>(floor(input.p * input.scale));
    let on = orb(input.p, input.size) > threshold(cell);
    return select(input.params[1], input.params[0], on);
}";

#[test]
fn lab_dither_shaders_compile_and_dither() {
    let _gpu_fixture = gpu_fixture();
    let shader = FillShader::new(DITHER, Some(30.));
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = boxed(&mut scene, shader.clone(), 0., 0.);
        scene.set_shader_uniforms(id, uniforms(0.));
        let (_, p) = render(&mut gpu, &mut scene);
        // Solid at the orb's heart, empty at its corner, dithered between.
        assert_eq!(pixel(&p, 100, 100), [255, 0, 0, 255], "direct {direct}");
        assert_eq!(pixel(&p, 51, 51), [0, 0, 255, 255]);
        let ring: Vec<[u8; 4]> = (120..128).map(|x| pixel(&p, x, 100)).collect();
        assert!(
            ring.contains(&[255, 0, 0, 255]) && ring.contains(&[0, 0, 255, 255]),
            "{ring:?}"
        );
    }
}

#[test]
fn shader_fills_painted_after_a_backdrop_blur_keep_their_uniforms() {
    // The blur's records once took the regions' slots on direct Metal, so
    // whatever painted after a frosted pane read the blur's tables: a shader
    // fill's uniforms came back zero and it drew nothing.
    let _gpu_fixture = gpu_fixture();
    let shader = FillShader::new(SPLIT, None);
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let glass = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 255, 255, 40)),
            Style {
                width: Some(40.),
                height: Some(40.),
                ..Default::default()
            },
        );
        scene.set_effects(
            glass,
            Effects {
                blur_radius: 4.,
                ..Default::default()
            },
        );
        let id = boxed(&mut scene, shader.clone(), 0., 0.);
        scene.set_shader_uniforms(id, uniforms(1.));
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 70, 100), [255, 0, 0, 255], "direct {direct}");
        assert_eq!(pixel(&p, 130, 100), [0, 0, 255, 255], "direct {direct}");
        if direct && cfg!(target_os = "macos") {
            assert!(gpu.debug_metal_frames() > 0, "never drew with Metal");
        }
    }
}
