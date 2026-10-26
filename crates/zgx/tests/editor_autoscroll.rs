use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers, PointerButton},
    text_layout::{FallbackTextLayout, FontStyle, TextLayout},
    widgets::{EditorHandle, Ui},
};

fn fixture(single: bool, wrap: bool) -> (Ui, EditorHandle, Rc<Cell<usize>>) {
    let mut ui = Ui::new(400., 300.);
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
    let text = if single {
        "word tail ".repeat(100)
    } else {
        ["word tail"; 100].join("\n")
    };
    let value = ui.signal(text);
    let view = if single {
        text_input("Editor", value)
    } else {
        text_area("Editor", value)
    };
    let view = ui.mount(
        view.size(100., 80.)
            .p(10.)
            .text_size(10.)
            .line_height(20.)
            .text_wrap(wrap),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    (ui, editor, calls)
}
fn down(ui: &mut Ui, count: usize) {
    for index in 0..count {
        ui.dispatch_with_modifiers(
            InputEvent::PointerDown {
                x: 22.,
                y: 20.,
                button: PointerButton::Primary,
            },
            Modifiers::default(),
        );
        if index + 1 < count {
            up(ui);
        }
    }
}
fn up(ui: &mut Ui) {
    ui.dispatch_with_modifiers(
        InputEvent::PointerUp {
            x: 22.,
            y: 20.,
            button: PointerButton::Primary,
        },
        Modifiers::default(),
    );
}
fn outside(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerMove { x, y });
}
fn tick(ui: &mut Ui) {
    let deadline = ui
        .next_interaction_deadline()
        .expect("outside held selection schedules a tick");
    ui.advance_interactions(deadline).unwrap();
}

#[test]
fn stationary_character_word_and_paragraph_selection_advance_without_reshaping() {
    for count in 1..=3 {
        let (mut ui, editor, calls) = fixture(false, true);
        down(&mut ui, count);
        outside(&mut ui, 22., 140.);
        let initial = editor.editor.borrow().selection();
        let shaped = calls.get();
        for _ in 0..12 {
            tick(&mut ui);
        }
        let later = editor.editor.borrow().selection();
        assert_eq!(later.anchor, initial.anchor);
        assert!(later.focus > initial.focus, "click unit {count}");
        if count == 2 {
            assert_eq!(later.focus % 10, 4, "whole word edge");
        }
        if count == 3 {
            assert_eq!(
                later.focus % 10,
                0,
                "whole paragraphs, their breaks included"
            );
        }
        assert_eq!(
            calls.get(),
            shaped,
            "scrolling must reuse unchanged text layout"
        );
        assert!(!editor.editor.borrow_mut().undo());
        up(&mut ui);
        assert!(ui.next_interaction_deadline().is_none());
    }
}

#[test]
fn horizontal_single_line_and_multiline_nowrap_reverse_and_stop_at_edges() {
    for single in [true, false] {
        let (mut ui, editor, _) = fixture(single, false);
        if !single {
            editor.value().set("word tail ".repeat(100));
            editor.editor.borrow_mut().set_selection(0, 0);
            editor.refresh();
            ui.prepare_frame();
        }
        down(&mut ui, 1);
        outside(&mut ui, 300., 20.);
        let initial = editor.editor.borrow().selection().focus;
        for _ in 0..20 {
            tick(&mut ui);
        }
        assert!(editor.editor.borrow().selection().focus > initial);
        outside(&mut ui, -100., 20.);
        for _ in 0..100 {
            if ui.next_interaction_deadline().is_none() {
                break;
            }
            tick(&mut ui);
        }
        assert_eq!(editor.editor.borrow().selection().focus, 0);
        assert!(
            ui.next_interaction_deadline().is_none(),
            "left boundary must not idle-poll"
        );
        outside(&mut ui, 100_000., 20.);
        for _ in 0..1000 {
            if ui.next_interaction_deadline().is_none() {
                break;
            }
            tick(&mut ui);
        }
        assert_eq!(
            editor.editor.borrow().selection().focus,
            editor.value().get().len()
        );
        assert!(
            ui.next_interaction_deadline().is_none(),
            "right boundary must not idle-poll"
        );
    }
}

#[test]
fn stalled_ticks_have_bounded_motion_and_read_only_selection_preserves_history() {
    let mut focuses = Vec::new();
    for delay in [Duration::from_millis(100), Duration::from_secs(60)] {
        let (mut ui, editor, _) = fixture(false, false);
        editor.paste("!");
        let edited = editor.value().get();
        editor.set_read_only(true);
        down(&mut ui, 1);
        outside(&mut ui, 22., 100_000.);
        let before = editor.editor.borrow().selection().focus;
        assert!(
            before < 50,
            "the first far-outside move must clamp to the visible edge"
        );
        let next = ui.next_interaction_deadline().unwrap();
        ui.advance_interactions(next + delay).unwrap();
        let focus = editor.editor.borrow().selection().focus;
        assert!(
            focus >= before && focus - before <= 40,
            "50ms at1200px/s covers at most three20px rows"
        );
        focuses.push(focus);
        assert_eq!(editor.value().get(), edited);
        up(&mut ui);
        editor.set_read_only(false);
        assert!(editor.editor.borrow_mut().undo());
        assert_eq!(editor.editor.borrow().text(), ["word tail"; 100].join("\n"));
        assert!(editor.editor.borrow_mut().redo());
        assert_eq!(editor.editor.borrow().text(), edited);
    }
    assert_eq!(
        focuses[0], focuses[1],
        "a long scheduler stall must not produce an unbounded catch-up"
    );
}

#[test]
fn cancelled_or_invalid_owned_gestures_never_keep_deadlines() {
    for reason in 0..7 {
        let (mut ui, editor, _) = fixture(false, false);
        down(&mut ui, 1);
        outside(&mut ui, 22., 140.);
        let next = ui.next_interaction_deadline().unwrap();
        match reason {
            0 => up(&mut ui),
            1 => {
                ui.dispatch(InputEvent::PointerCancel);
            }
            2 => {
                ui.input.focus(&ui.scene, None);
            }
            3 => ui.set_disabled(editor.node, true),
            4 => ui.remove(editor.node),
            5 => ui.cancel_interactions(),
            _ => {
                let other = ui.mount(button().child(text("Other")));
                ui.on_event(other.node(), true, |cx| {
                    if cx.phase == EventPhase::Target {
                        cx.capture_pointer();
                    }
                });
                ui.input.dispatch_to(
                    &ui.scene,
                    other.node(),
                    InputEvent::KeyDown {
                        key: Key::ArrowDown,
                        modifiers: Modifiers::default(),
                        repeat: false,
                    },
                );
                assert_eq!(ui.input.captured(), Some(other.node()));
            }
        }
        let selection = editor.editor.borrow().selection();
        assert!(
            ui.next_interaction_deadline().is_none(),
            "cancellation reason {reason}"
        );
        ui.advance_interactions(next + Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            editor.editor.borrow().selection(),
            selection,
            "stale timer after reason {reason}"
        );
    }
}

#[test]
fn resize_and_external_replacement_revalidate_retained_drag_geometry() {
    let (mut ui, editor, _) = fixture(false, true);
    down(&mut ui, 3);
    outside(&mut ui, 22., 140.);
    tick(&mut ui);
    let mut style = ui.scene.borrow().style(editor.node);
    style.height = Some(180.);
    ui.scene.borrow_mut().set_style(editor.node, style);
    ui.prepare_frame();
    if let Some(next) = ui.next_interaction_deadline() {
        ui.advance_interactions(next).unwrap();
    }
    assert!(
        ui.next_interaction_deadline().is_none(),
        "stationary pointer is inside resized viewport"
    );
    outside(&mut ui, 22., 250.);
    assert!(ui.next_interaction_deadline().is_some());
    editor.value().set("short".into());
    ui.prepare_frame();
    if let Some(next) = ui.next_interaction_deadline() {
        ui.advance_interactions(next).unwrap();
    }
    assert!(ui.next_interaction_deadline().is_none());
    let selection = editor.editor.borrow().selection();
    assert!(selection.anchor <= 5 && selection.focus <= 5);
    assert_eq!(editor.value().get(), "short");
}

#[test]
fn prevented_moves_do_not_arm_and_equal_model_writes_keep_the_gesture() {
    let (mut ui, editor, _) = fixture(false, false);
    let cancel = Rc::new(Cell::new(true));
    let read = cancel.clone();
    let listener = ui.input.listen_first(editor.node, move |cx| {
        if read.get() && matches!(cx.event, InputEvent::PointerMove { .. }) {
            cx.prevent_default();
        }
    });
    down(&mut ui, 1);
    outside(&mut ui, 22., 140.);
    assert!(ui.next_interaction_deadline().is_none());
    cancel.set(false);
    outside(&mut ui, 22., 140.);
    let next = ui.next_interaction_deadline().unwrap();
    editor.value().set(editor.value().get());
    ui.prepare_frame();
    assert_eq!(ui.next_interaction_deadline(), Some(next));
    tick(&mut ui);
    assert!(ui.next_interaction_deadline().is_some());
    editor.value().set(["different text"; 100].join("\n"));
    assert!(
        ui.next_interaction_deadline().is_none(),
        "changed model cancels original unit anchors"
    );
    outside(&mut ui, 22., 140.);
    assert!(
        ui.next_interaction_deadline().is_none(),
        "old held gesture cannot restart after replacement"
    );
    drop(listener);
}

#[test]
fn held_boundary_rearms_after_resize_without_another_pointer_move() {
    let (mut ui, editor, _) = fixture(false, false);
    editor.value().set(["word tail"; 6].join("\n"));
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    down(&mut ui, 1);
    outside(&mut ui, 22., 140.);
    for _ in 0..100 {
        if ui.next_interaction_deadline().is_none() {
            break;
        }
        tick(&mut ui);
    }
    assert!(ui.next_interaction_deadline().is_none());
    let mut style = ui.scene.borrow().style(editor.node);
    style.height = Some(40.);
    ui.scene.borrow_mut().set_style(editor.node, style);
    ui.prepare_frame();
    assert!(
        ui.next_interaction_deadline().is_some(),
        "smaller viewport makes held boundary scrollable again"
    );
    tick(&mut ui);
    assert!(ui.next_interaction_deadline().is_some());
}

#[test]
fn idle_advance_does_not_prepare_unrelated_dirty_layout() {
    let (mut ui, editor, calls) = fixture(false, false);
    assert!(ui.next_interaction_deadline().is_none());
    let mut style = ui.scene.borrow().style(editor.node);
    style.width = Some(70.);
    ui.scene.borrow_mut().set_style(editor.node, style);
    let shaped = calls.get();
    let bounds = ui.scene.borrow().bounds(editor.node);
    assert!(!ui.advance_interactions(std::time::Instant::now()).unwrap());
    assert_eq!(calls.get(), shaped);
    assert_eq!(ui.scene.borrow().bounds(editor.node), bounds);
}

#[test]
fn reversing_stationary_unit_drag_uses_opposite_original_edge() {
    for count in [2, 3] {
        let (mut ui, editor, _) = fixture(false, true);
        editor.editor.borrow_mut().set_selection(202, 202);
        editor.refresh();
        ui.prepare_frame();
        down(&mut ui, count);
        let original = editor.editor.borrow().selection();
        assert!(original.anchor > 0);
        outside(&mut ui, 22., 140.);
        for _ in 0..8 {
            tick(&mut ui);
        }
        assert!(editor.editor.borrow().selection().focus > original.focus);
        outside(&mut ui, 22., -100.);
        for _ in 0..30 {
            if editor.editor.borrow().selection().focus < original.anchor {
                break;
            }
            tick(&mut ui);
        }
        let reversed = editor.editor.borrow().selection();
        assert_eq!(reversed.anchor, original.focus);
        assert!(reversed.focus < original.anchor);
        assert_eq!(
            reversed.focus % 10,
            0,
            "reverse selection reaches the complete original unit edge"
        );
    }
}

#[test]
fn dormant_boundary_gesture_cannot_reclaim_transferred_capture_or_another_editor_deadline() {
    for new_editor in [false, true] {
        let (mut ui, old, _) = fixture(false, false);
        old.value().set(["word tail"; 6].join("\n"));
        old.editor.borrow_mut().set_selection(0, 0);
        old.refresh();
        ui.prepare_frame();
        down(&mut ui, 1);
        outside(&mut ui, 22., 140.);
        for _ in 0..100 {
            if ui.next_interaction_deadline().is_none() {
                break;
            }
            tick(&mut ui);
        }
        assert!(ui.next_interaction_deadline().is_none());
        assert_eq!(ui.input.captured(), Some(old.node));
        let old_selection = old.editor.borrow().selection();
        // An application hook may suppress the editor's Blur handler. Keep the
        // old gesture dormant so ownership validation, rather than Blur cleanup,
        // must protect the other control's capture and pending work.
        let blur_hook = ui.input.listen_first(old.node, |cx| {
            if matches!(cx.event, InputEvent::Blur) {
                cx.stop_immediate_propagation();
            }
        });
        let view = if new_editor {
            text_area("Second", ui.signal(["word tail"; 100].join("\n")))
                .size(100., 80.)
                .p(10.)
                .text_size(10.)
                .line_height(20.)
        } else {
            button().child(text("Other")).size(100., 80.)
        };
        let other = ui.mount(view.absolute().translate(150., 0.));
        ui.prepare_frame();
        let second = if new_editor {
            ui.input.focus(&ui.scene, Some(other.node()));
            let second = ui.focused_editor().unwrap();
            second.editor.borrow_mut().set_selection(0, 0);
            second.refresh();
            ui.prepare_frame();
            Some(second)
        } else {
            ui.on_event(other.node(), true, |cx| {
                if cx.phase == EventPhase::Target {
                    cx.capture_pointer();
                }
            });
            None
        };
        // Transfer capture independently before beginning the new editor's
        // gesture; direct dispatch does not perform normal pointer hit routing.
        let transfer = ui.input.listen_first(other.node(), |cx| {
            if matches!(cx.event, InputEvent::KeyUp { .. }) {
                cx.capture_pointer();
            }
        });
        ui.input.dispatch_to(
            &ui.scene,
            other.node(),
            InputEvent::KeyUp {
                key: Key::ArrowDown,
                modifiers: Modifiers::default(),
            },
        );
        drop(transfer);
        ui.input.dispatch_to(
            &ui.scene,
            other.node(),
            InputEvent::PointerDown {
                x: 172.,
                y: 20.,
                button: PointerButton::Primary,
            },
        );
        outside(&mut ui, 172., 140.);
        assert_eq!(ui.input.captured(), Some(other.node()));
        let deadline = ui.next_interaction_deadline();
        assert_eq!(deadline.is_some(), new_editor);
        let mut style = ui.scene.borrow().style(old.node);
        style.height = Some(40.);
        ui.scene.borrow_mut().set_style(old.node, style);
        ui.prepare_frame();
        old.refresh();
        assert_eq!(ui.input.captured(), Some(other.node()));
        assert_eq!(ui.next_interaction_deadline(), deadline);
        if let Some(second) = second {
            let before = second.editor.borrow().selection().focus;
            for _ in 0..4 {
                tick(&mut ui);
            }
            assert!(second.editor.borrow().selection().focus > before);
        }
        assert_eq!(old.editor.borrow().selection(), old_selection);
        drop(blur_hook);
    }
}
