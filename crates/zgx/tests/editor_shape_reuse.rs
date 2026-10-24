use std::{cell::Cell, rc::Rc};
use zgx::{
    input::{InputEvent, Key, Modifiers},
    scene::Color,
    text_layout::{FallbackTextLayout, FontFamily, FontStyle, LineHeight, TextLayout},
    widgets::{EditorHandle, Ui},
};

fn fixture() -> (Ui, EditorHandle, Rc<Cell<usize>>) {
    let mut ui = Ui::new(400., 300.);
    let calls = Rc::new(Cell::new(0));
    install_shaper(&mut ui, calls.clone(), 1.);
    let value = ui.signal("abcdefghijabcdefghij\nx\nabcdefghijabcdefghij\nlast\n".repeat(4));
    let editor = ui.text_input(ui.root(), "Editor", value, 100., true);
    editor.set_typography(
        ui.theme().color(zgx::theme::tok::TEXT_PRIMARY),
        10.,
        FontStyle::default(),
    );
    editor.set_wrap(true);
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.prepare_frame();
    calls.set(0);
    (ui, editor, calls)
}
fn install_shaper(ui: &mut Ui, calls: Rc<Cell<usize>>, scale: f32) {
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            calls.set(calls.get() + 1);
            Box::new(FallbackTextLayout::with_line_height(
                text,
                size * scale,
                width,
                font.line_height,
            ))
        },
    );
}
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
}
fn assert_stable(ui: &Ui, editor: &EditorHandle, calls: &Cell<usize>) {
    let before = calls.get();
    editor.refresh();
    ui.prepare_frame();
    assert_eq!(
        calls.get(),
        before,
        "unchanged displayed text and shaping metrics must reuse geometry"
    );
}

#[test]
fn unchanged_refresh_selection_scroll_and_visual_navigation_reuse_shaping() {
    let (mut ui, editor, calls) = fixture();
    let original = editor.value().get();
    for _ in 0..5 {
        editor.refresh();
        ui.prepare_frame();
    }
    assert_eq!(calls.get(), 0, "stable refreshes");
    for focus in [1, 2, 3] {
        editor.editor.borrow_mut().set_selection(0, focus);
        editor.refresh();
        ui.prepare_frame();
    }
    assert_eq!(calls.get(), 0, "selection-only changes");
    let before_scroll = ui.scene.borrow().bounds(editor.caret).y;
    for _ in 0..3 {
        ui.dispatch(InputEvent::Scroll {
            x: 20.,
            y: 20.,
            delta_x: 0.,
            delta_y: 14.,
        });
        ui.prepare_frame();
    }
    assert!(
        ui.scene.borrow().bounds(editor.caret).y < before_scroll,
        "test must exercise actual scrolling"
    );
    assert_eq!(calls.get(), 0, "scrolling unchanged content");
    for key_value in [Key::End, Key::ArrowDown, Key::Home, Key::ArrowUp] {
        key(&mut ui, key_value);
    }
    assert_ne!(editor.editor.borrow().selection().focus, 3);
    assert_eq!(
        calls.get(),
        0,
        "visual-line navigation and caret refresh share retained geometry"
    );
    assert_eq!(editor.value().get(), original);
}

#[test]
fn text_font_pitch_wrap_and_allocated_width_invalidate_reused_geometry() {
    let (ui, editor, calls) = fixture();
    editor.value().set("abcdefghijabcdefghij\nsecond".into());
    ui.prepare_frame();
    assert!(calls.replace(0) > 0, "external model change");
    assert_stable(&ui, &editor, &calls);
    editor.set_typography(Color(123, 45, 67, 255), 10., FontStyle::default());
    ui.prepare_frame();
    assert_eq!(calls.get(), 0, "color does not change shaping");
    editor.set_typography(
        ui.theme().color(zgx::theme::tok::TEXT_PRIMARY),
        12.,
        FontStyle::default(),
    );
    ui.prepare_frame();
    assert!(calls.replace(0) > 0, "font size change");
    assert_stable(&ui, &editor, &calls);
    for font in [
        FontStyle {
            family: FontFamily::Monospace,
            ..FontStyle::default()
        },
        FontStyle {
            weight: 700,
            ..FontStyle::default()
        },
        FontStyle {
            italic: true,
            ..FontStyle::default()
        },
        FontStyle {
            line_height: LineHeight::px(27.),
            ..FontStyle::default()
        },
    ] {
        editor.set_typography(ui.theme().color(zgx::theme::tok::TEXT_PRIMARY), 12., font);
        ui.prepare_frame();
        assert!(
            calls.replace(0) > 0,
            "font style is part of shaping identity"
        );
        assert_stable(&ui, &editor, &calls);
    }
    assert_eq!(ui.scene.borrow().bounds(editor.caret).height, 27.);
    let mut style = ui.scene.borrow().style(editor.node);
    style.width = Some(70.);
    ui.scene.borrow_mut().set_style(editor.node, style);
    ui.prepare_frame();
    assert!(calls.replace(0) > 0, "actual wrapped viewport width change");
    assert_stable(&ui, &editor, &calls);
    editor.set_wrap(false);
    ui.prepare_frame();
    assert!(calls.replace(0) > 0, "wrapping policy change");
    assert_stable(&ui, &editor, &calls);
}

#[test]
fn shaper_replacement_refreshes_metrics_even_when_text_and_font_are_equal() {
    let (mut ui, editor, old_calls) = fixture();
    let before_height = ui.scene.borrow().bounds(editor.caret).height;
    let new_calls = Rc::new(Cell::new(0));
    install_shaper(&mut ui, new_calls.clone(), 2.);
    ui.prepare_frame();
    assert!(new_calls.get() > 0, "replacement shaper must be used");
    assert_eq!(old_calls.get(), 0);
    assert_eq!(
        ui.scene.borrow().bounds(editor.caret).height,
        before_height * 2.
    );
    assert_stable(&ui, &editor, &new_calls);
}

#[test]
fn preedit_cursor_changes_reuse_geometry_but_composed_text_changes_invalidate() {
    let (mut ui, editor, calls) = fixture();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    let original = editor.value().get();
    ui.dispatch(InputEvent::ImePreedit {
        text: "你好".into(),
        cursor: Some((0, 0)),
    });
    ui.prepare_frame();
    assert!(calls.replace(0) > 0);
    let start = ui.scene.borrow().bounds(editor.caret);
    ui.dispatch(InputEvent::ImePreedit {
        text: "你好".into(),
        cursor: Some((3, 3)),
    });
    ui.prepare_frame();
    let middle = ui.scene.borrow().bounds(editor.caret);
    assert!(middle.x > start.x);
    assert_eq!(
        calls.get(),
        0,
        "preedit cursor is geometry usage, not shaping identity"
    );
    ui.dispatch(InputEvent::ImePreedit {
        text: "你".into(),
        cursor: Some((3, 3)),
    });
    ui.prepare_frame();
    assert!(
        calls.replace(0) > 0,
        "preedit content changes displayed text"
    );
    assert_eq!(
        editor.value().get(),
        original,
        "preedit must not commit the model"
    );
    ui.dispatch(InputEvent::ImePreedit {
        text: String::new(),
        cursor: None,
    });
    ui.prepare_frame();
    assert!(calls.replace(0) > 0);
    assert_stable(&ui, &editor, &calls);
    assert_eq!(editor.value().get(), original);
}

#[test]
fn focus_owner_reacquires_geometry_and_retained_handle_cannot_refresh_removed_owner() {
    let (mut ui, editor, calls) = fixture();
    ui.input.focus(&ui.scene, None);
    ui.prepare_frame();
    calls.set(0);
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.prepare_frame();
    assert!(
        calls.get() > 0,
        "returning focus acquires geometry for its owner"
    );
    assert_stable(&ui, &editor, &calls);
    ui.remove(editor.node);
    ui.prepare_frame();
    assert!(ui.focused_editor().is_none());
    calls.set(0);
    editor.refresh();
    editor.value().set("detached model".into());
    ui.prepare_frame();
    assert_eq!(
        calls.get(),
        0,
        "removed owner cannot shape or recreate a subtree"
    );
    assert!(!ui.scene.borrow().contains(editor.node));
}

struct TrackedLayout {
    layout: FallbackTextLayout,
    live: Rc<Cell<usize>>,
}
impl Drop for TrackedLayout {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}
impl TextLayout for TrackedLayout {
    fn cache_weight(&self) -> Option<usize> {
        self.layout
            .cache_weight()
            .map(|weight| weight + std::mem::size_of::<Self>())
    }
    fn size(&self) -> (f32, f32) {
        self.layout.size()
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        self.layout.hit_test(x, y)
    }
    fn caret(&self, offset: usize) -> zgx::scene::Rect {
        self.layout.caret(offset)
    }
    fn selection(&self, range: std::ops::Range<usize>) -> Vec<zgx::scene::Rect> {
        self.layout.selection(range)
    }
}

#[test]
fn cached_layout_is_released_on_blur_and_removal_despite_retained_editor_handle() {
    let (mut ui, editor, _) = fixture();
    let live = Rc::new(Cell::new(0));
    let observed = live.clone();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            observed.set(observed.get() + 1);
            Box::new(TrackedLayout {
                layout: FallbackTextLayout::with_line_height(text, size, width, font.line_height),
                live: observed.clone(),
            })
        },
    );
    ui.prepare_frame();
    assert_eq!(live.get(), 1, "only focused editor geometry remains owned");
    ui.input.focus(&ui.scene, None);
    ui.prepare_frame();
    assert_eq!(live.get(), 0, "blur releases geometry");
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.prepare_frame();
    assert_eq!(live.get(), 1);
    ui.remove(editor.node);
    ui.prepare_frame();
    assert_eq!(
        live.get(),
        0,
        "removal releases geometry despite external handle"
    );
    editor.refresh();
    assert_eq!(live.get(), 0);
}
