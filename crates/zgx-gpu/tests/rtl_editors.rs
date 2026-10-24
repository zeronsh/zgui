//! Editors in a right-to-left window, as the renderer draws them: their
//! text starts at the right, inside their padding, and the caret is where
//! the glyphs end.
use zgx::{compose::prelude::*, scene::Direction, scene::Rect, style::rgb, widgets::Ui};
use zgx_gpu::GpuRenderer;

const W: u32 = 400;
const H: u32 = 200;

fn render(ui: &Ui, gpu: &mut GpuRenderer) -> Vec<u8> {
    ui.prepare_frame();
    let mut scene = ui.scene.borrow_mut();
    scene.flush();
    gpu.render(&scene, &[Rect::new(0., 0., W as f32, H as f32)])
        .unwrap();
    gpu.readback().unwrap()
}

/// The columns in `rows` of `area` where something bright is drawn: the
/// text's ink, left and right.
fn ink(pixels: &[u8], area: Rect) -> Option<(f32, f32)> {
    let mut found: Option<(u32, u32)> = None;
    for y in area.y as u32..(area.y + area.height) as u32 {
        for x in area.x as u32..(area.x + area.width) as u32 {
            let at = ((y * W + x) * 4) as usize;
            if pixels[at] > 128 {
                found = Some(found.map_or((x, x), |(l, r)| (l.min(x), r.max(x))));
            }
        }
    }
    found.map(|(l, r)| (l as f32, r as f32 + 1.))
}

/// An editor showing `value` in a right-to-left column, and where its text
/// is drawn, unfocused (no caret), its caret's box at the end of the text,
/// and where a click just inside the ink's left edge put the caret.
fn editor(value: &str, multiline: bool) -> (Rect, (f32, f32), Rect, usize) {
    let mut gpu = GpuRenderer::new(W, H).unwrap();
    let mut ui = Ui::new(W as f32, H as f32);
    gpu.install_text(&mut ui.scene.borrow_mut());
    ui.refresh_text_geometry();
    let field = if multiline {
        text_area("Notes", ui.signal(value.to_owned())).text_wrap(true)
    } else {
        text_input("Name", ui.signal(value.to_owned()))
    };
    let view = ui.mount(
        column()
            .direction(Direction::Rtl)
            .size(W as f32, H as f32)
            .p(20.)
            .bg(rgb(0x000000))
            .text_color(rgb(0xffffff))
            .text_size(16.)
            .child(field.size(300., 80.).p(12.).bg(rgb(0x000000))),
    );
    let pixels = render(&ui, &mut gpu);
    let root = ui.scene.borrow().children(view.node())[0];
    let bounds = ui.scene.borrow().bounds(root);
    // The content box, less a line's height of slack below for descenders.
    let content = Rect::new(bounds.x, bounds.y + 12., bounds.width, 40.);
    let ink = ink(&pixels, content).expect("the text is drawn");
    ui.input.focus(&ui.scene, Some(root));
    let editor = ui.focused_editor().unwrap();
    editor
        .editor
        .borrow_mut()
        .set_selection(value.len(), value.len());
    editor.refresh();
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let caret = ui.scene.borrow().bounds(editor.caret);
    use zgx::input::{InputEvent, PointerButton};
    // On the text's line (a field's one line is centred in it).
    let (x, y) = (ink.0 + 1., caret.y + caret.height / 2.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    let clicked = editor.editor.borrow().selection().focus;
    (bounds, ink, caret, clicked)
}

#[test]
fn right_to_left_editors_draw_inside_their_padding_with_the_caret_at_the_text() {
    for multiline in [false, true] {
        // Left-to-right text, in a field running right to left: it starts at
        // the right, so it ends there, where the caret goes.
        let (field, (left, right), caret, clicked) = editor("Hello world", multiline);
        let (start, end) = (field.x + 12., field.x + field.width - 12.);
        assert!(
            left >= start - 1. && right <= end + 1.,
            "{multiline}: ink {left}..{right} in {start}..{end}"
        );
        assert!(
            right > end - 8.,
            "{multiline}: aligned right, ink ends at {right} ({end})"
        );
        assert!(
            (caret.x - right).abs() <= 3.,
            "{multiline}: caret {} at ink end {right}",
            caret.x
        );
        // A click at the drawn text's left edge is at its start.
        assert_eq!(clicked, 0, "{multiline}: a click at its left");
        // Right-to-left text ends at its left.
        let (_, (left, right), caret, clicked) = editor("שלום עולם", multiline);
        assert!(
            right <= end + 1. && right > end - 8.,
            "{multiline}: hebrew ink {left}..{right}"
        );
        assert!(
            (caret.x - left).abs() <= 3.,
            "{multiline}: caret {} at ink start {left}",
            caret.x
        );
        assert_eq!(
            clicked,
            "שלום עולם".len(),
            "{multiline}: its left is its end"
        );
    }
}
