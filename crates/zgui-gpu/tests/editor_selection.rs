use std::{cell::RefCell, rc::Rc};
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    text_layout::{FontStyle, TextLayout},
    widgets::Ui,
};
use zgui_gpu::text::ShapedText;

#[test]
fn triple_click_selects_full_logical_row_in_mixed_direction_text() {
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
            text,
            "complete bidi row including trailing spaces"
        );
        assert_eq!(editor.editor.borrow().selection().anchor, 0);
        assert_eq!(editor.editor.borrow().selection().focus, text.len());
    }
}

#[test]
fn the_arrows_go_the_way_right_to_left_text_shows() {
    use zgui::input::Key;
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
    let focus = |editor: &zgui::widgets::EditorHandle| editor.editor.borrow().selection().focus;
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
