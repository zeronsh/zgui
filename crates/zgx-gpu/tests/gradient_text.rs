//! Gradient text (`Scene::set_text_gradient`, CSS's `background-clip:
//! text`): its glyphs show the gradient over its box, its selection and
//! selected-text colour paint as plain text's do, it rasterizes as finely
//! as plain text at any scale, and text without one is no layer.
use zgx::{
    canvas::GradientStop,
    compose::prelude::*,
    decoration::Gradient,
    scene::{Color, NodeId, Rect},
    style::rgb,
    widgets::Ui,
};
use zgx_gpu::GpuRenderer;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 120;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Red at its left, blue at its right.
fn red_to_blue() -> Gradient {
    let stop = |offset, color| GradientStop { offset, color };
    Gradient::Linear {
        angle: 0.,
        stops: vec![
            stop(0., Color(255, 0, 0, 255)),
            stop(1., Color(0, 0, 255, 255)),
        ]
        .into(),
    }
}

/// A black panel with one line of white `MMMMMMMM`, its gradient `fill`.
fn mount(gpu: &mut GpuRenderer, fill: Option<Gradient>) -> (Ui, NodeId) {
    let mut ui = Ui::new(WIDTH as f32, HEIGHT as f32);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    let line = text("MMMMMMMM");
    let line = match fill {
        Some(fill) => line.text_gradient(fill),
        None => line,
    };
    ui.mount(
        column()
            .size(WIDTH as f32, HEIGHT as f32)
            .p(10.)
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .text_size(40.)
            .selectable(true)
            .selection_color(Color(0, 255, 0, 255))
            .selected_text_color(Color(255, 255, 0, 255))
            .child(line),
    );
    let node = {
        let scene = ui.scene.borrow();
        let column = scene.children(scene.root())[0];
        scene.children(column)[0]
    };
    (ui, node)
}

/// The frame after `ui`'s changes, checked against a full repaint.
fn render(ui: &Ui, gpu: &mut GpuRenderer) -> Vec<u8> {
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    let scene = ui.scene.borrow();
    gpu.render(&scene, &damage).unwrap();
    let partial = gpu.readback().unwrap();
    gpu.render(&scene, &[Rect::new(0., 0., WIDTH as f32, HEIGHT as f32)])
        .unwrap();
    assert!(
        partial == gpu.readback().unwrap(),
        "incremental damage must reconstruct the full frame"
    );
    partial
}

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

/// Its glyphs red where its box starts, blue where it ends, and nowhere
/// but where plain text's glyphs are.
#[test]
fn gradient_text_fills_its_glyphs_with_the_gradient() {
    let _gpu = gpu_fixture();
    let mut gpu = GpuRenderer::new(WIDTH, HEIGHT).unwrap();
    let (plain_ui, plain_node) = mount(&mut gpu, None);
    let plain = render(&plain_ui, &mut gpu);
    assert!(
        !plain_ui.scene.borrow().is_layer(plain_node),
        "text without a gradient is no layer"
    );
    let mut gpu = GpuRenderer::new(WIDTH, HEIGHT).unwrap();
    let (ui, node) = mount(&mut gpu, Some(red_to_blue()));
    let filled = render(&ui, &mut gpu);
    assert!(ui.scene.borrow().is_layer(node));
    let bounds = ui.scene.borrow().bounds(node);
    let (mut left, mut right) = ([0u64; 3], [0u64; 3]);
    let middle = bounds.x + bounds.width / 2.;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let lit = at(&plain, WIDTH, x, y)[0];
            let p = at(&filled, WIDTH, x, y);
            // Where plain text has no glyph, nor has it.
            if lit == 0 {
                assert_eq!(p, [0, 0, 0], "at ({x}, {y}): outside its glyphs");
                continue;
            }
            // Never white: its own colour is gone.
            assert!(p[1] < 40, "at ({x}, {y}): {p:?}");
            let side = if (x as f32) < middle {
                &mut left
            } else {
                &mut right
            };
            for (sum, channel) in side.iter_mut().zip(p) {
                *sum += u64::from(channel);
            }
        }
    }
    assert!(left[0] > 2 * left[2], "red at its start: {left:?}");
    assert!(right[2] > 2 * right[0], "blue at its end: {right:?}");
}

/// Selected, as plain text is: the highlight behind its glyphs, the
/// selected ones in the selected-text colour, the rest still the gradient;
/// cleared, the frame as it was.
#[test]
fn gradient_text_selects_as_plain_text_does() {
    let _gpu = gpu_fixture();
    let mut gpu = GpuRenderer::new(WIDTH, HEIGHT).unwrap();
    let (ui, node) = mount(&mut gpu, Some(red_to_blue()));
    let before = render(&ui, &mut gpu);
    assert!(ui.select_text(
        zgx::selection::TextPoint { node, offset: 0 },
        zgx::selection::TextPoint { node, offset: 4 },
    ));
    let selected = render(&ui, &mut gpu);
    let rect = ui.scene.borrow().text_highlight(node).unwrap().rects[0];
    let bounds = ui.scene.borrow().bounds(node);
    let (left, top) = (bounds.x + rect.x, bounds.y + rect.y);
    let pixels = |from: f32, to: f32| {
        let selected = &selected;
        (top as u32 + 1..(top + rect.height) as u32 - 1)
            .flat_map(move |y| (from as u32..to as u32).map(move |x| at(selected, WIDTH, x, y)))
    };
    let highlight = pixels(left, left + rect.width)
        .filter(|p| *p == [0, 255, 0])
        .count();
    let yellow = pixels(left, left + rect.width)
        .filter(|p| p[0] > 230 && p[1] > 230 && p[2] < 30)
        .count();
    assert!(highlight > 200, "the highlight behind: {highlight}");
    assert!(yellow > 100, "selected glyphs yellow: {yellow}");
    // Past the selection: its glyphs the gradient, on black.
    let rest: Vec<[u8; 3]> = pixels(left + rect.width + 2., bounds.x + bounds.width - 2.).collect();
    assert!(rest.iter().all(|p| p[1] < 40), "no highlight, no yellow");
    assert!(rest.iter().any(|p| p[2] > 120), "the gradient's blue");
    ui.clear_text_selection();
    assert!(render(&ui, &mut gpu) == before, "cleared, as before");
}

/// At twice the scale its glyphs cover what plain text's do, pixel for
/// pixel: rasterized for the scale, not one resampled.
#[test]
fn gradient_text_is_as_sharp_as_plain_text_at_any_scale() {
    let _gpu = gpu_fixture();
    let coverage = |fill: Option<Gradient>| {
        let mut gpu = GpuRenderer::new(WIDTH * 2, HEIGHT * 2).unwrap();
        gpu.set_scale_factor(2.);
        let (ui, _) = mount(&mut gpu, fill);
        let pixels = render(&ui, &mut gpu);
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[0].max(p[1]).max(p[2]) > 0)
            .collect::<Vec<bool>>()
    };
    let (plain, filled) = (coverage(None), coverage(Some(red_to_blue())));
    let differ = plain.iter().zip(&filled).filter(|(a, b)| a != b).count();
    let lit = plain.iter().filter(|lit| **lit).count();
    assert!(lit > 2000, "{lit}");
    assert_eq!(differ, 0, "the same pixels as plain text's, of {lit}");
}

/// Text that changes in a box of the same size keeps its gradient's image:
/// nothing uploaded again but its glyphs.
#[test]
fn changing_gradient_text_keeps_its_gradient_image() {
    let _gpu = gpu_fixture();
    let mut gpu = GpuRenderer::new(WIDTH, HEIGHT).unwrap();
    let mut ui = Ui::new(WIDTH as f32, HEIGHT as f32);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    ui.mount(
        column()
            .size(WIDTH as f32, HEIGHT as f32)
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .text_size(40.)
            .child(text("100").w(200.).h(60.).text_gradient(red_to_blue())),
    );
    let node = {
        let scene = ui.scene.borrow();
        let column = scene.children(scene.root())[0];
        scene.children(column)[0]
    };
    render(&ui, &mut gpu);
    let image = ui.scene.borrow().text_gradient_image(node).unwrap().id();
    for count in 101..110 {
        ui.scene.borrow_mut().set_text(node, count.to_string());
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        let stats = gpu.render(&ui.scene.borrow(), &damage).unwrap();
        assert_eq!(stats.image_uploads, 0, "at {count}");
        let now = ui.scene.borrow().text_gradient_image(node).unwrap().id();
        assert_eq!(now, image, "at {count}");
    }
}

/// A decorated text (padded, a background: a box holding its label) fills
/// its label's glyphs, its background behind them.
#[test]
fn a_boxed_text_fills_its_glyphs() {
    let _gpu = gpu_fixture();
    let mut gpu = GpuRenderer::new(WIDTH, HEIGHT).unwrap();
    let mut ui = Ui::new(WIDTH as f32, HEIGHT as f32);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    ui.mount(
        column()
            .size(WIDTH as f32, HEIGHT as f32)
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .text_size(40.)
            .child(
                text("MMMMMMMM")
                    .p(8.)
                    .bg(rgb(0x00ff00))
                    .text_gradient(red_to_blue()),
            ),
    );
    let pixels = render(&ui, &mut gpu);
    let count = |test: fn(&[u8]) -> bool| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| test(&p[..]))
            .count()
    };
    let white = count(|p| p[0] > 200 && p[1] > 200 && p[2] > 200);
    let red = count(|p| p[0] > 150 && p[1] < 60);
    let blue = count(|p| p[2] > 150 && p[1] < 60);
    let green = count(|p| p[1] > 240 && p[0] < 10 && p[2] < 10);
    assert_eq!(white, 0, "no white glyphs");
    assert!(red > 100 && blue > 100, "red {red}, blue {blue}");
    assert!(green > 1000, "its background behind: {green}");
}
