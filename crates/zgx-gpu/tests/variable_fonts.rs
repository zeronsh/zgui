//! Variable fonts: one file with a `wght` axis serves every weight, on the
//! measure and render paths, matching static instances cut from it.
//!
//! The fonts in `tests/fonts` are the design system's variable Geist 1.401
//! (OFL, see Geist-OFL.txt) subset to printable ASCII, and its 500 and 600
//! instances cut by fontTools' instancer from that subset.
use zgx::scene::*;
use zgx::text_layout::{FontFamily, FontStyle};
use zgx_gpu::GpuRenderer;
use zgx_gpu::text::FontData;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const VARIABLE: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");
const MEDIUM: &[u8] = include_bytes!("fonts/Geist-500-subset.ttf");
const SEMIBOLD: &[u8] = include_bytes!("fonts/Geist-600-subset.ttf");
const TEXT: &str = "The quick brown fox jumps over the lazy dog 0123456789";
const SIZE: (u32, u32) = (600, 60);
/// A renderer knowing only `fonts`, and a scene measuring with it.
fn renderer(fonts: &[&[u8]], direct: bool) -> (GpuRenderer, Scene) {
    let mut gpu = GpuRenderer::new(SIZE.0, SIZE.1).unwrap();
    gpu.set_direct_metal(direct);
    {
        let system = gpu.text_system();
        let mut system = system.borrow_mut();
        for bytes in fonts {
            FontData::new(bytes.to_vec()).unwrap().install(&mut system);
        }
    }
    let mut scene = Scene::new(SIZE.0 as f32, SIZE.1 as f32);
    gpu.install_text(&mut scene);
    (gpu, scene)
}
fn geist(weight: u16) -> FontStyle {
    FontStyle {
        family: FontFamily::Named("Geist".into()),
        weight,
        ..Default::default()
    }
}
/// `TEXT` at 18 px, `weight`, drawn black on clear at (10, 10).
fn drawn(gpu: &mut GpuRenderer, scene: &mut Scene, weight: u16) -> Vec<u8> {
    let text = scene.append(
        scene.root(),
        NodeKind::Text {
            text: TEXT.into(),
            color: Color(0, 0, 0, 255),
            font_size: 18.,
        },
        Style::default(),
    );
    scene.set_font(text, geist(weight));
    scene.set_transform(text, Transform { x: 10., y: 10. });
    let damage = scene.flush().damage;
    gpu.render(scene, &damage).unwrap();
    gpu.readback().unwrap()
}

#[test]
fn a_variable_font_measures_each_weight_as_its_static_instance() {
    let _gpu_fixture = gpu_fixture();
    let (_variable_gpu, variable) = renderer(&[VARIABLE], false);
    for (weight, instance) in [(500, MEDIUM), (600, SEMIBOLD)] {
        let (_static_gpu, fixed) = renderer(&[instance], false);
        let measure = |scene: &Scene| {
            scene
                .measure_text_with_font(TEXT, 18., None, &geist(weight))
                .0
        };
        let (from_axis, from_instance) = (measure(&variable), measure(&fixed));
        // The instancer rounds each glyph's advance to whole font units:
        // half a unit (of 1000 per em) per glyph at most.
        let rounding = TEXT.len() as f32 * 0.5 * 18. / 1000.;
        assert!(
            (from_axis - from_instance).abs() <= rounding,
            "weight {weight}: {from_axis} from the axis, {from_instance} from the instance"
        );
        // And the axis moves: each weight is wider than the last.
        let regular = measure_weight(&variable, 400);
        assert!(
            from_axis > regular + 5.,
            "weight {weight}: {from_axis} vs {regular} at 400"
        );
    }
}
fn measure_weight(scene: &Scene, weight: u16) -> f32 {
    scene
        .measure_text_with_font(TEXT, 18., None, &geist(weight))
        .0
}

#[test]
fn a_variable_font_draws_each_weight_as_its_static_instance() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        for (weight, instance) in [(500, MEDIUM), (600, SEMIBOLD)] {
            let (mut variable_gpu, mut variable) = renderer(&[VARIABLE], direct);
            let (mut static_gpu, mut fixed) = renderer(&[instance], direct);
            let from_axis = drawn(&mut variable_gpu, &mut variable, weight);
            let from_instance = drawn(&mut static_gpu, &mut fixed, weight);
            // Coverage pixel by pixel: the same outlines, but for the
            // instancer's rounding of points and advances to whole units.
            let alphas = |pixels: &[u8]| pixels.chunks(4).map(|p| p[3] as i32).collect::<Vec<_>>();
            let (a, b) = (alphas(&from_axis), alphas(&from_instance));
            let ink: i32 = b.iter().sum();
            let difference: i32 = a.iter().zip(&b).map(|(a, b)| (a - b).abs()).sum();
            assert!(ink > 0, "weight {weight}: nothing drawn");
            assert!(
                difference * 20 < ink,
                "direct {direct}, weight {weight}: {difference} of {ink} coverage differs"
            );
            // A lighter weight from the same axis draws visibly less ink.
            let (mut lighter_gpu, mut lighter) = renderer(&[VARIABLE], direct);
            let regular: i32 = alphas(&drawn(&mut lighter_gpu, &mut lighter, 400))
                .iter()
                .sum();
            assert!(
                regular * 21 < ink * 20,
                "weight {weight}: {regular} ink at 400, {ink}"
            );
        }
    }
}
