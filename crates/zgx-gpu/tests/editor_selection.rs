use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    text_layout::{FontStyle, TextLayout},
    widgets::Ui,
};
use zgx_gpu::text::ShapedText;

#[test]
fn triple_click_selects_the_whole_paragraph_in_mixed_direction_text() {
    let fonts = Rc::new(RefCell::new(cosmic_text::FontSystem::new()));
    for text in ["abc אבג", "אבג abc", "abc אבג xyz דהו", "abc אבג  "] {
        let mut ui = Ui::new(500., 200.);
        let fonts = fonts.clone();
        ui.scene.borrow_mut().set_font_text_shaper(
            move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
                Box::new(ShapedText::with_font(
                    &mut fonts.borrow_mut(),
                    text,
                    size,
                    width,
                    font,
                ))
            },
        );
        let view = ui.mount(
            text_area("Bidi document", ui.signal(format!("{text}\nnext row")))
                .size(440., 120.)
                .p(10.)
                .text_size(16.)
                .line_height(24.),
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor.editor.borrow_mut().set_selection(0, 0);
        editor.refresh();
        for _ in 0..3 {
            ui.dispatch_with_modifiers(
                InputEvent::PointerDown {
                    x: 40.,
                    y: 22.,
                    button: PointerButton::Primary,
                },
                Modifiers::default(),
            );
            ui.dispatch_with_modifiers(
                InputEvent::PointerUp {
                    x: 40.,
                    y: 22.,
                    button: PointerButton::Primary,
                },
                Modifiers::default(),
            );
        }
        assert_eq!(
            editor.copy(),
            format!("{text}\n"),
            "the whole bidi paragraph, trailing spaces and break included"
        );
        assert_eq!(editor.editor.borrow().selection().anchor, 0);
        assert_eq!(editor.editor.borrow().selection().focus, text.len() + 1);
    }
}

#[test]
fn the_arrows_go_the_way_right_to_left_text_shows() {
    use zgx::input::Key;
    let fonts = Rc::new(RefCell::new(cosmic_text::FontSystem::new()));
    let mut ui = Ui::new(500., 200.);
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let value = ui.signal("אבג".to_owned());
    let view = ui.mount(
        text_input("Hebrew", value.clone())
            .size(300., 36.)
            .text_size(16.),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    let press = |ui: &mut Ui, key: Key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
    };
    let focus = |editor: &zgx::widgets::EditorHandle| editor.editor.borrow().selection().focus;
    // At the start, which shows at the right: Left moves into the text.
    press(&mut ui, Key::ArrowLeft);
    assert_eq!(focus(&editor), 2, "one letter on, to its left");
    press(&mut ui, Key::ArrowLeft);
    assert_eq!(focus(&editor), 4);
    press(&mut ui, Key::ArrowRight);
    assert_eq!(focus(&editor), 2, "Right goes back toward the start");
    // Left to right text keeps its order.
    value.set("abc".to_owned());
    ui.prepare_frame();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    press(&mut ui, Key::ArrowRight);
    assert_eq!(focus(&editor), 1);
}

/// Select-all and copy over a page of 20,000 paragraphs, shaped as a window
/// shapes them: `cargo test --release -p zgx-gpu --test editor_selection
/// long_document -- --ignored --nocapture`.
#[test]
#[ignore = "a timing probe"]
fn long_document_select_all_with_shaped_text() {
    use std::time::Instant;
    let mut ui = Ui::new(800., 600.);
    let services = zgx_gpu::TextServices::new();
    services.install(&mut ui.scene.borrow_mut());
    let offset = ui.signal(0.);
    ui.mount(
        column()
            .w(800.)
            .h(600.)
            .text_size(14.)
            .line_height(20.)
            .selectable(true)
            .child(
                scroll(offset.clone())
                    .w(800.)
                    .h(600.)
                    .child(column().gap(10.).children((0..20_000).map(|i| {
                        text(format!(
                            "Paragraph {i}: the quick brown fox jumps over the lazy dog, \
                             and keeps running past the edge of the field until it wraps."
                        ))
                        .w_full()
                        .text_wrap(true)
                    }))),
            ),
    );
    let start = Instant::now();
    ui.prepare_frame();
    eprintln!("mount and first frame: {:?}", start.elapsed());
    let first = {
        let scene = ui.scene.borrow();
        let mut pending = vec![scene.root()];
        let mut found = None;
        while let Some(id) = pending.pop() {
            if matches!(scene.kind(id), zgx::scene::NodeKind::Text { .. }) {
                found = Some(id);
                break;
            }
            pending.extend(scene.children(id).iter().rev());
        }
        found.unwrap()
    };
    for _ in 0..3 {
        ui.clear_text_selection();
        let start = Instant::now();
        assert!(ui.select_all_text(first));
        eprintln!("select all: {:?}", start.elapsed());
    }
    let start = Instant::now();
    let copied = ui.selected_text();
    eprintln!("copy: {:?} ({} bytes)", start.elapsed(), copied.len());
    ui.prepare_frame();
    let start = Instant::now();
    for step in 1..=20 {
        offset.set(step as f32 * 250.);
        ui.prepare_frame();
    }
    eprintln!("scroll frame, all selected: {:?}", start.elapsed() / 20);
    ui.clear_text_selection();
    ui.prepare_frame();
    let start = Instant::now();
    for step in 1..=20 {
        offset.set(5000. + step as f32 * 250.);
        ui.prepare_frame();
    }
    eprintln!("scroll frame, nothing selected: {:?}", start.elapsed() / 20);
}
