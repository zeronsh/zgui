use std::{cell::Cell, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers},
    text_layout::{FallbackTextLayout, FontStyle, TextLayout},
    widgets::Ui,
};

fn page(ui: &mut Ui, key: Key, modifiers: Modifiers) -> bool {
    let prevented = ui
        .try_dispatch(InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        })
        .unwrap()
        .default_prevented;
    ui.prepare_frame();
    prevented
}

#[test]
fn paging_uses_padded_resized_viewport_and_preserves_history_and_shift_anchor() {
    let mut ui = Ui::new(400., 300.);
    let original = ["abcdef"; 12].join("\n");
    let value = ui.signal(original.clone());
    let height = ui.signal(100.);
    let read = height.clone();
    let view = ui.mount(
        text_area("Document", value.clone())
            .w(180.)
            .p(10.)
            .text_size(10.)
            .line_height(20.)
            .reactive_style(move || Styles::new().h(read.get())),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.paste("!");
    editor.editor.borrow_mut().set_selection(2, 2);
    editor.refresh();
    assert!(page(&mut ui, Key::PageDown, Modifiers::default()));
    assert_eq!(editor.editor.borrow().selection().focus, 3 * 7 + 2);
    height.set(140.);
    ui.prepare_frame();
    assert!(page(&mut ui, Key::PageDown, Modifiers::default()));
    assert_eq!(editor.editor.borrow().selection().focus, 8 * 7 + 2);
    assert!(page(
        &mut ui,
        Key::PageUp,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        }
    ));
    let selection = editor.editor.borrow().selection();
    assert_eq!(selection.anchor, 8 * 7 + 2);
    assert_eq!(selection.focus, 3 * 7 + 2);
    let caret = ui.scene.borrow().bounds(editor.caret);
    let bounds = ui.scene.borrow().bounds(editor.node);
    assert!(caret.y >= bounds.y + 10. && caret.y + caret.height <= bounds.y + bounds.height - 10.);
    assert_eq!(value.get(), original.clone() + "!");
    page(
        &mut ui,
        Key::Character("z".into()),
        Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Modifiers::default()
        },
    );
    assert_eq!(
        value.get(),
        original,
        "paging must preserve the existing undo entry"
    );
    page(&mut ui, Key::PageUp, Modifiers::default());
    page(
        &mut ui,
        Key::Character("z".into()),
        Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        value.get(),
        original + "!",
        "paging must preserve pending redo history"
    );
}

#[test]
fn paging_keeps_preferred_column_through_short_lines_and_arrow_navigation() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("abcdefghij\nabcdefghij\nx\nabcdefghij\nabcdefghij".into());
    let view = ui.mount(
        text_area("Document", value)
            .size(120., 80.)
            .p(10.)
            .text_size(10.)
            .line_height(20.),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(8, 8);
    editor.refresh();
    page(&mut ui, Key::PageDown, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, 23);
    page(&mut ui, Key::ArrowDown, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, 32);
    page(&mut ui, Key::ArrowUp, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, 23);
    page(&mut ui, Key::PageUp, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, 8);
}

#[test]
fn paging_reaches_empty_and_trailing_line_boundaries_and_allows_read_only() {
    for text in ["", "one\n", "one\ntwo\n"] {
        let mut ui = Ui::new(300., 200.);
        let value = ui.signal(text.to_owned());
        let view = ui.mount(
            text_area("Read only", value.clone())
                .size(180., 100.)
                .p(10.)
                .line_height(20.)
                .read_only(true),
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor.editor.borrow_mut().set_selection(0, 0);
        editor.refresh();
        assert!(page(&mut ui, Key::PageDown, Modifiers::default()));
        assert_eq!(editor.editor.borrow().selection().focus, text.len());
        assert!(page(
            &mut ui,
            Key::PageUp,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            }
        ));
        assert_eq!(editor.editor.borrow().selection().focus, 0);
        assert_eq!(editor.editor.borrow().selection().anchor, text.len());
        assert_eq!(value.get(), text);
    }
}

#[test]
fn paging_cancels_preedit_but_single_line_and_modified_keys_are_unhandled() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("first\nsecond\nthird\nfourth".into());
    let view = ui.mount(
        text_area("Editor", value.clone())
            .size(180., 80.)
            .line_height(20.),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    ui.dispatch(InputEvent::ImePreedit {
        text: "中".into(),
        cursor: None,
    });
    assert!(editor.editor.borrow().preedit().is_some());
    let revision = editor.editor.borrow().composition_cancel_revision();
    page(&mut ui, Key::PageDown, Modifiers::default());
    assert!(editor.editor.borrow().preedit().is_none());
    assert_ne!(
        editor.editor.borrow().composition_cancel_revision(),
        revision
    );
    assert_eq!(value.get(), "first\nsecond\nthird\nfourth");
    let selection = editor.editor.borrow().selection();
    let bubbled = Rc::new(Cell::new(0));
    let observed = bubbled.clone();
    let _listener = ui.input.listen(ui.root(), move |event| {
        if event.phase == EventPhase::Bubble
            && !event.default_prevented()
            && matches!(
                event.event,
                InputEvent::KeyDown {
                    key: Key::PageUp | Key::PageDown,
                    ..
                }
            )
        {
            observed.set(observed.get() + 1);
        }
    });
    for modifiers in [
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        Modifiers {
            meta: true,
            ..Modifiers::default()
        },
    ] {
        assert!(!page(&mut ui, Key::PageUp, modifiers));
        assert_eq!(editor.editor.borrow().selection(), selection);
    }
    assert_eq!(bubbled.get(), 3);
    view.unmount();
    let single = ui.mount(text_input("Single", ui.signal("unchanged".into())));
    ui.input.focus(&ui.scene, Some(single.node()));
    assert!(!page(&mut ui, Key::PageDown, Modifiers::default()));
    assert!(!page(
        &mut ui,
        Key::PageUp,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        }
    ));
    assert_eq!(bubbled.get(), 5);
}

#[test]
fn wrapped_unicode_paging_reuses_shapes_and_keeps_visual_end_affinity() {
    let mut ui = Ui::new(300., 200.);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            count.set(count.get() + 1);
            Box::new(FallbackTextLayout::with_line_height(
                text,
                size,
                width,
                font.line_height,
            ))
        },
    );
    let original = "👩‍💻e\u{301}中".repeat(20);
    let offsets: Vec<_> = original
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(original.len()))
        .collect();
    let value = ui.signal(original.clone());
    let view = ui.mount(
        text_area("Wrapped", value.clone())
            .size(50., 80.)
            .p(10.)
            .text_size(10.)
            .line_height(20.)
            .text_wrap(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor
        .editor
        .borrow_mut()
        .set_selection(offsets[2], offsets[2]);
    editor.refresh();
    ui.prepare_frame();
    calls.set(0);
    page(&mut ui, Key::PageDown, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, offsets[12]);
    page(&mut ui, Key::PageUp, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, offsets[2]);
    page(&mut ui, Key::End, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, offsets[5]);
    page(&mut ui, Key::PageDown, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, offsets[15]);
    page(&mut ui, Key::Home, Modifiers::default());
    assert_eq!(editor.editor.borrow().selection().focus, offsets[10]);
    assert_eq!(
        calls.get(),
        0,
        "paging and selection reuse focused shaped layout"
    );
    assert_eq!(value.get(), original);
}

#[test]
fn fractional_pitch_exact_last_row_landing_preserves_column() {
    for pitch in [0.1_f32, 1.3, 17.3, 23.7] {
        let mut ui = Ui::new(300., 200.);
        let value = ui.signal("abcdef\nabcdef\nabcdef\nabcdef".into());
        let view = ui.mount(
            text_area("Fractional", value)
                .w(160.)
                .h(pitch * 4. + 0.01)
                .p(0.)
                .text_size(10.)
                .line_height(pitch),
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor.editor.borrow_mut().set_selection(2, 2);
        editor.refresh();
        page(&mut ui, Key::PageDown, Modifiers::default());
        assert_eq!(
            editor.editor.borrow().selection().focus,
            23,
            "pitch={pitch}: exact last-row landing must preserve column"
        );
        page(&mut ui, Key::PageUp, Modifiers::default());
        assert_eq!(
            editor.editor.borrow().selection().focus,
            2,
            "pitch={pitch}: exact first-row landing must preserve column"
        );
    }
}

#[test]
fn capture_handler_can_prevent_paging_without_changing_selection_or_scroll() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("abcdef\nabcdef\nabcdef\nabcdef\nabcdef".into());
    let view = ui.mount(
        column()
            .on_event(|event| {
                if event.phase == EventPhase::Capture
                    && matches!(
                        event.event,
                        InputEvent::KeyDown {
                            key: Key::PageDown,
                            ..
                        }
                    )
                {
                    event.prevent_default();
                }
            })
            .child(
                text_area("Protected", value)
                    .id("editor")
                    .size(160., 60.)
                    .line_height(20.),
            ),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("editor"));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(2, 2);
    editor.refresh();
    ui.prepare_frame();
    let selection = editor.editor.borrow().selection();
    let caret = ui.scene.borrow().bounds(editor.caret);
    assert!(page(&mut ui, Key::PageDown, Modifiers::default()));
    assert_eq!(editor.editor.borrow().selection(), selection);
    assert_eq!(ui.scene.borrow().bounds(editor.caret), caret);
}
