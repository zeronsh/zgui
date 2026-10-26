//! Layer shaders (`Scene::set_layer_shader`): a node's subtree, cached as a
//! layer, composites through a WGSL `filter` that sees each of its pixels
//! at the node's own position, can read the layer elsewhere, and takes
//! per-node uniforms that composite again without repainting. Under a local
//! transform it still works in the node's space. On the wgpu path, which
//! frames with Metal directly fall back to.
use zgx::decoration::{FillShader, ShaderUniforms};
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
fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2)
}
/// Its left half tinted by `params[0]`; its right half shows the layer's
/// left half (`zgx_sample`), unfiltered.
const TINT_AND_COPY: &str = "
fn layer(input: LayerInput) -> vec4<f32> {
    if input.p.x < input.size.x * 0.5 {
        return vec4(input.color.rgb * input.params[0].rgb, input.color.a);
    }
    return zgx_sample(input.p - vec2(input.size.x * 0.5, 0.0));
}";
const RED: Color = Color(255, 0, 0, 255);
const GREEN: Color = Color(0, 255, 0, 255);
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}
/// A 100 × 100 container at (50, 50): its left half red, its right green.
fn card(scene: &mut Scene) -> NodeId {
    let root = scene.root();
    let id = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        fixed(100., 100.),
    );
    scene.set_transform(id, Transform { x: 50., y: 50. });
    scene.append(id, NodeKind::Rect(RED), fixed(50., 100.));
    let right = scene.append(id, NodeKind::Rect(GREEN), fixed(50., 100.));
    scene.set_transform(right, Transform { x: 50., y: 0. });
    id
}
fn tint(r: f32) -> ShaderUniforms {
    ShaderUniforms {
        time: 0.,
        params: [[r, 1., 1., 1.], [0.; 4], [0.; 4], [0.; 4]],
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
    let full = gpu.readback().unwrap();
    let differs =
        (0..SIZE * SIZE).find(|i| pixels[*i as usize * 4..][..4] != full[*i as usize * 4..][..4]);
    assert!(
        differs.is_none(),
        "incremental frame differs from full at {:?}: {:?} vs {:?}",
        differs.map(|i| (i % SIZE, i / SIZE)),
        differs.map(|i| pixel(&pixels, i % SIZE, i / SIZE)),
        differs.map(|i| pixel(&full, i % SIZE, i / SIZE)),
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
fn a_layer_composites_through_its_shader_in_its_own_space() {
    let _gpu_fixture = gpu_fixture();
    let shader = FillShader::new(TINT_AND_COPY, None);
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = card(&mut scene);
        scene.set_layer_shader(id, Some(shader.clone()));
        scene.set_layer_uniforms(id, tint(0.5));
        assert!(scene.is_layer(id));
        let (_, p) = render(&mut gpu, &mut scene);
        // Left: red at half; right: the layer's left half, red as painted.
        assert!(
            near(pixel(&p, 70, 100), [128, 0, 0, 255]),
            "direct {direct}: {:?}",
            pixel(&p, 70, 100)
        );
        assert!(
            near(pixel(&p, 130, 100), [255, 0, 0, 255]),
            "{:?}",
            pixel(&p, 130, 100)
        );
        assert_eq!(pixel(&p, 20, 20), [0; 4]);

        // New uniforms composite again: no layout, the layer kept.
        scene.set_layer_uniforms(id, tint(1.));
        let (frame, p) = render(&mut gpu, &mut scene);
        assert_eq!(frame.layout_nodes, 0);
        assert!(
            near(pixel(&p, 70, 100), [255, 0, 0, 255]),
            "{:?}",
            pixel(&p, 70, 100)
        );

        // Under a local transform, `p` is still the node's own: halved about
        // its centre, its left quarter of the window is tinted.
        scene.set_layer_uniforms(id, tint(0.));
        scene.set_local_transform(
            id,
            LocalTransform {
                scale: (0.5, 0.5),
                ..LocalTransform::IDENTITY
            },
        );
        let (_, p) = render(&mut gpu, &mut scene);
        assert!(
            near(pixel(&p, 85, 100), [0, 0, 0, 255]),
            "{:?}",
            pixel(&p, 85, 100)
        );
        assert!(
            near(pixel(&p, 115, 100), [255, 0, 0, 255]),
            "{:?}",
            pixel(&p, 115, 100)
        );
        assert_eq!(pixel(&p, 60, 100), [0; 4]);

        // Without its shader it composites as painted.
        scene.set_layer_shader(id, None);
        scene.set_local_transform(id, LocalTransform::IDENTITY);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 70, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 130, 100), [0, 255, 0, 255]);
    }
}

#[test]
fn a_layer_shader_that_does_not_compile_composites_the_layer_as_painted() {
    let _gpu_fixture = gpu_fixture();
    assert!(
        zgx_gpu::check_layer_shader(
            "fn layer(input: LayerInput) -> vec4<f32> { return input.nope; }"
        )
        .is_err()
    );
    assert_eq!(zgx_gpu::check_layer_shader(TINT_AND_COPY), Ok(()));
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let id = card(&mut scene);
    scene.set_layer_shader(id, Some(FillShader::new("fn layer(", None)));
    let (_, p) = render(&mut gpu, &mut scene);
    assert_eq!(pixel(&p, 70, 100), [255, 0, 0, 255]);
    assert_eq!(pixel(&p, 130, 100), [0, 255, 0, 255]);
}
