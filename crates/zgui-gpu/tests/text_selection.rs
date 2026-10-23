//! Display text selection paints under the glyphs, repaints where it
//! changes, and leaves the frame as it was once cleared.
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    scene::Rect,
    style::rgb,
    theme::tok,
    widgets::Ui,
};
use zgui_gpu::GpuRenderer;

fn render(ui: &Ui, gpu: &mut GpuRenderer) -> Vec<u8> {
    ui.prepare_frame();
    let mut scene = ui.scene.borrow_mut();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let partial = gpu.readback().unwrap();
    gpu.render(&scene, &[Rect::new(0., 0., 320., 160.)])
        .unwrap();
    assert!(
        partial == gpu.readback().unwrap(),
        "incremental selection damage must reconstruct the full frame"
    );
    partial
}

#[test]
fn selection_paints_under_text_and_repaints_only_where_it_changes() {
    let mut gpu = GpuRenderer::new(320, 160).unwrap();
    let mut ui = Ui::new(320., 160.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    ui.mount(
        column()
            .size(320., 160.)
            .p(10.)
            .gap(6.)
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .text_size(16.)
            .selectable(true)
            .child(text("Selectable display text"))
            .child(text("across two paragraphs")),
    );
    let before = render(&ui, &mut gpu);
    let press = |ui: &mut Ui, event: fn(f32, f32) -> InputEvent, x, y| {
        ui.dispatch_with_modifiers(event(x, y), Modifiers::default());
    };
    let down = |x, y| InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    };
    let up = |x, y| InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    };
    press(&mut ui, down, 12., 18.);
    press(&mut ui, |x, y| InputEvent::PointerMove { x, y }, 80., 44.);
    press(&mut ui, up, 80., 44.);
    assert!(
        ui.selected_text()
            .starts_with("Selectable display text\nacross")
    );
    let selected = render(&ui, &mut gpu);
    let color = ui.theme_scope().color_untracked(tok::SELECTION);
    let over_black = |c: u8| (c as f32 * color.3 as f32 / 255.).round() as u8;
    let highlight = selected
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            p[0].abs_diff(over_black(color.0)) <= 2
                && p[1].abs_diff(over_black(color.1)) <= 2
                && p[2].abs_diff(over_black(color.2)) <= 2
        })
        .count();
    assert!(highlight > 500, "the highlight shows: {highlight} pixels");
    // Glyphs still draw over it.
    assert!(
        selected
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 240 && p[1] > 240 && p[2] > 240)
            .count()
            > 50
    );
    ui.clear_text_selection();
    assert!(
        render(&ui, &mut gpu) == before,
        "clearing restores the text"
    );
}
