//! Display text selection paints under the glyphs, repaints where it
//! changes, and leaves the frame as it was once cleared.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    scene::Rect,
    style::rgb,
    theme::tok,
    widgets::Ui,
};
use zgx_gpu::GpuRenderer;

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

#[test]
fn bidi_and_emoji_select_by_logical_order_and_whole_clusters() {
    let gpu = GpuRenderer::new(320, 160).unwrap();
    let mut ui = Ui::new(320., 160.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    let line = "abc אבגד efg 👩\u{200d}💻!";
    ui.mount(
        column()
            .size(320., 160.)
            .p(10.)
            .text_size(16.)
            .selectable(true)
            .child(text(line).id("line")),
    );
    ui.prepare_frame();
    let node = {
        let scene = ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if matches!(scene.kind(n), zgx::scene::NodeKind::Text { .. }) {
                found = Some(n);
            }
            stack.extend(scene.children(n).iter().copied());
        }
        found.unwrap()
    };
    let bounds = ui.scene.borrow().bounds(node);
    let pointer = |ui: &mut Ui, event| {
        ui.dispatch_with_modifiers(event, Modifiers::default());
    };
    let y = bounds.y + bounds.height / 2.;
    pointer(
        &mut ui,
        InputEvent::PointerDown {
            x: bounds.x + 1.,
            y,
            button: PointerButton::Primary,
        },
    );
    pointer(
        &mut ui,
        InputEvent::PointerMove {
            x: bounds.x + bounds.width + 20.,
            y,
        },
    );
    pointer(
        &mut ui,
        InputEvent::PointerUp {
            x: bounds.x + bounds.width + 20.,
            y,
            button: PointerButton::Primary,
        },
    );
    assert_eq!(ui.selected_text(), line, "the whole line, in logical order");
    // From the start into the right-to-left word: a logical prefix, drawn
    // as more than one piece (the run's visual order differs).
    let hebrew = line.find('א').unwrap();
    let point = |offset| zgx::selection::TextPoint { node, offset };
    assert!(ui.select_text(point(0), point(hebrew + 'א'.len_utf8() * 2)));
    assert_eq!(ui.selected_text(), "abc אב");
    let rects = ui.scene.borrow().text_highlight(node).unwrap().rects.len();
    assert!(rects >= 2, "disjoint pieces: {rects}");
    // Into the emoji: no half of its cluster is ever selected.
    let emoji = line.find('👩').unwrap();
    assert!(ui.select_text(point(emoji), point(line.len())));
    ui.prepare_frame();
    pointer(
        &mut ui,
        InputEvent::KeyDown {
            key: zgx::input::Key::ArrowLeft,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            repeat: false,
        },
    );
    assert_eq!(ui.selected_text(), "👩\u{200d}💻");
}

#[test]
fn selected_glyphs_repaint_in_the_selected_text_colour_and_highlights_round() {
    use zgx::{scene::Color, theme::ThemeOverride};
    let mut gpu = GpuRenderer::new(320, 160).unwrap();
    let mut ui = Ui::new(320., 160.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    ui.mount(theme_scope(
        ThemeOverride::new().with(tok::SELECTION_RADIUS, 8.),
        column()
            .size(320., 160.)
            .p(10.)
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .text_size(24.)
            .selectable(true)
            .selection_color(Color(0, 0, 255, 255))
            .selected_text_color(Color(255, 0, 0, 255))
            .child(text("MMMMMMMM")),
    ));
    let before = render(&ui, &mut gpu);
    let node = {
        let scene = ui.scene.borrow();
        let column = scene.children(scene.root())[0];
        scene.children(column)[0]
    };
    // The first half: its glyphs red on blue, the rest still white.
    assert!(ui.select_text(
        zgx::selection::TextPoint { node, offset: 0 },
        zgx::selection::TextPoint { node, offset: 4 },
    ));
    let selected = render(&ui, &mut gpu);
    let rect = ui.scene.borrow().text_highlight(node).unwrap().rects[0];
    let bounds = ui.scene.borrow().bounds(node);
    let pixel = |x: f32, y: f32| {
        let at = (y.round() as usize * 320 + x.round() as usize) * 4;
        [selected[at], selected[at + 1], selected[at + 2]]
    };
    let (left, top) = (bounds.x + rect.x, bounds.y + rect.y);
    let row = |from: f32, to: f32| {
        (from as usize..to as usize).map(|x| (x, pixel(x as f32, top + rect.height / 2.)))
    };
    assert!(
        row(left, left + rect.width).any(|(_, p)| p[0] > 200 && p[1] < 60 && p[2] < 60),
        "selected glyphs are red"
    );
    assert!(
        row(left + rect.width + 2., bounds.x + bounds.width - 2.)
            .any(|(_, p)| p[0] > 200 && p[1] > 200 && p[2] > 200),
        "unselected glyphs stay white"
    );
    // Rounded: a corner shows what is under it, its top edge's middle
    // (above the glyphs) the highlight.
    assert_eq!(pixel(left + 0.5, top + 0.5), [0, 0, 0], "a rounded corner");
    assert_eq!(pixel(left + rect.width / 2., top + 1.5), [0, 0, 255]);
    ui.clear_text_selection();
    assert!(render(&ui, &mut gpu) == before);
}
