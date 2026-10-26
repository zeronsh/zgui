//! Blend modes (`Scene::set_blend`, CSS `mix-blend-mode`): a blended node's
//! layer mixes with what lies beneath it by the mode's formula
//! (`BlendMode::mix`, the W3C spec's), over the window or inside another
//! layer, with partial coverage composited source-over. Frames with one
//! take the wgpu path, which a renderer set to draw with Metal directly
//! falls back to.
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 120;
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
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
fn rect(scene: &mut Scene, parent: NodeId, at: (f32, f32), size: f32, color: Color) -> NodeId {
    let id = scene.append(parent, NodeKind::Rect(color), fixed(size, size));
    scene.set_transform(id, Transform { x: at.0, y: at.1 });
    id
}
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> Vec<u8> {
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
        "incremental frame differs from full at {:?}",
        differs.map(|i| (i % SIZE, i / SIZE))
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
fn unit(c: Color) -> [f32; 3] {
    [c.0, c.1, c.2].map(|v| v as f32 / 255.)
}
fn close(actual: [u8; 4], expected: [f32; 4], what: &str) {
    let expected = expected.map(|v| (v * 255.).round() as i32);
    let off = actual
        .iter()
        .zip(expected)
        .map(|(a, e)| (*a as i32 - e).abs())
        .max()
        .unwrap();
    assert!(off <= 3, "{what}: {actual:?}, expected {expected:?}");
}
const BACKDROP: Color = Color(51, 153, 204, 255);
const SOURCE: Color = Color(230, 102, 26, 255);

#[test]
fn every_mode_mixes_an_opaque_layer_with_the_backdrop() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let root = scene.root();
        rect(&mut scene, root, (0., 0.), 120., BACKDROP);
        let layer = scene.append(root, NodeKind::Container(Layout::Overlay), fixed(40., 40.));
        scene.set_transform(layer, Transform { x: 40., y: 40. });
        rect(&mut scene, layer, (0., 0.), 40., SOURCE);
        for mode in BlendMode::ALL {
            scene.set_blend(layer, mode);
            assert_eq!(scene.is_layer(layer), mode != BlendMode::Normal);
            let p = render(&mut gpu, &mut scene);
            let [r, g, b] = mode.mix(unit(BACKDROP), unit(SOURCE));
            close(
                pixel(&p, 60, 60),
                [r, g, b, 1.],
                &format!("{} direct {direct}", mode.name()),
            );
            // Beside it, the backdrop as it was.
            assert_eq!(pixel(&p, 20, 20), [51, 153, 204, 255]);
        }
    }
}

#[test]
fn a_translucent_layer_composites_source_over_the_mix() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    rect(&mut scene, root, (0., 0.), 120., BACKDROP);
    let layer = scene.append(root, NodeKind::Container(Layout::Overlay), fixed(40., 40.));
    scene.set_transform(layer, Transform { x: 40., y: 40. });
    rect(&mut scene, layer, (0., 0.), 40., SOURCE);
    scene.set_blend(layer, BlendMode::Multiply);
    scene.set_effects(
        layer,
        Effects {
            opacity: 0.5,
            ..Default::default()
        },
    );
    let p = render(&mut gpu, &mut scene);
    // Over an opaque backdrop: (1 - as)·cb + as·B(cb, cs).
    let mixed = BlendMode::Multiply.mix(unit(BACKDROP), unit(SOURCE));
    let expected = [0, 1, 2].map(|i| 0.5 * unit(BACKDROP)[i] + 0.5 * mixed[i]);
    close(
        pixel(&p, 60, 60),
        [expected[0], expected[1], expected[2], 1.],
        "half multiply",
    );
}

#[test]
fn a_blended_child_mixes_with_its_layer_not_the_window() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    // A card (a tilted layer) holding a backdrop and a screened child.
    let card = scene.append(root, NodeKind::Container(Layout::Overlay), fixed(80., 80.));
    scene.set_transform(card, Transform { x: 20., y: 20. });
    rect(&mut scene, card, (0., 0.), 80., BACKDROP);
    let foil = rect(&mut scene, card, (20., 20.), 40., SOURCE);
    scene.set_blend(foil, BlendMode::Screen);
    scene.set_local_transform(
        card,
        LocalTransform {
            rotate_y: 0.2,
            perspective: 400.,
            ..LocalTransform::IDENTITY
        },
    );
    let p = render(&mut gpu, &mut scene);
    let [r, g, b] = BlendMode::Screen.mix(unit(BACKDROP), unit(SOURCE));
    close(pixel(&p, 60, 60), [r, g, b, 1.], "screened inside the card");
    // Moving the foil composites the card again, its backdrop with it.
    scene.set_transform(foil, Transform { x: 0., y: 0. });
    let p = render(&mut gpu, &mut scene);
    close(pixel(&p, 40, 40), [r, g, b, 1.], "moved");
    let [r, g, b] = unit(BACKDROP);
    close(pixel(&p, 75, 75), [r, g, b, 1.], "uncovered");
}

#[test]
fn css_names_parse() {
    for mode in BlendMode::ALL {
        assert_eq!(BlendMode::parse(mode.name()), Some(mode));
    }
    assert_eq!(BlendMode::parse("soft_light"), Some(BlendMode::SoftLight));
    assert_eq!(BlendMode::parse("plus-lighter"), None);
}

#[test]
fn a_layer_shader_runs_before_its_layer_blends() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    let mut scene = scene();
    let root = scene.root();
    rect(&mut scene, root, (0., 0.), 120., BACKDROP);
    let layer = scene.append(root, NodeKind::Container(Layout::Overlay), fixed(40., 40.));
    scene.set_transform(layer, Transform { x: 40., y: 40. });
    rect(&mut scene, layer, (0., 0.), 40., SOURCE);
    // Inverted by its shader, then multiplied with the backdrop.
    let invert = zgx::decoration::FillShader::new(
        "fn layer(input: LayerInput) -> vec4<f32> { return vec4(1.0 - input.color.rgb, input.color.a); }",
        None,
    );
    scene.set_layer_shader(layer, Some(invert));
    scene.set_blend(layer, BlendMode::Multiply);
    let p = render(&mut gpu, &mut scene);
    let inverted = unit(SOURCE).map(|v| 1. - v);
    let [r, g, b] = BlendMode::Multiply.mix(unit(BACKDROP), inverted);
    close(
        pixel(&p, 60, 60),
        [r, g, b, 1.],
        "inverted, then multiplied",
    );
}
