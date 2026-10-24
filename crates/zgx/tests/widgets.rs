use std::{cell::Cell, rc::Rc};
use zgx::{
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{Layout, NodeId},
    widgets::{Ui, fixed},
};
fn key(key: Key, modifiers: Modifiers) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    }
}
fn click(ui: &mut Ui, node: NodeId) {
    ui.scene.borrow_mut().flush();
    let b = ui.scene.borrow().bounds(node);
    ui.dispatch(InputEvent::PointerDown {
        x: b.x + 12.,
        y: b.y + 12.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: b.x + 12.,
        y: b.y + 12.,
        button: PointerButton::Primary,
    });
}
#[test]
fn button_pointer_keyboard_disabled_and_removal() {
    let mut ui = Ui::new(500., 500.);
    let n = Rc::new(Cell::new(0));
    let c = n.clone();
    let button = ui.button(ui.root(), "Save", 120., move || c.set(c.get() + 1));
    click(&mut ui, button);
    assert_eq!(n.get(), 1);
    assert_eq!(ui.input.focused(), Some(button));
    ui.dispatch(key(Key::Space, Modifiers::default()));
    assert_eq!(n.get(), 1);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    });
    assert_eq!(n.get(), 2);
    ui.set_disabled(button, true);
    assert_eq!(ui.input.focused(), None);
    click(&mut ui, button);
    assert_eq!(n.get(), 2);
    ui.set_disabled(button, false);
    click(&mut ui, button);
    assert_eq!(n.get(), 3);
    ui.remove(button);
    assert_eq!(ui.input.focused(), None);
    assert!(ui.semantics.borrow().get(button).is_none());
}
#[test]
fn checkbox_reacts_to_activation_and_external_state() {
    let mut ui = Ui::new(500., 500.);
    let checked = ui.signal(false);
    let node = ui.checkbox(ui.root(), "Enabled", checked.clone(), 200.);
    click(&mut ui, node);
    ui.runtime.flush();
    assert!(checked.get());
    assert_eq!(ui.semantics.borrow().get(node).unwrap().checked, Some(true));
    checked.set(false);
    ui.runtime.flush();
    assert_eq!(
        ui.semantics.borrow().get(node).unwrap().checked,
        Some(false)
    );
}
#[test]
fn slider_captures_drag_clamps_and_supports_keyboard() {
    let mut ui = Ui::new(500., 500.);
    let value = ui.signal(0.);
    let node = ui.slider(ui.root(), "Volume", value.clone(), 0.0..=100., 200.);
    ui.scene.borrow_mut().flush();
    ui.dispatch(InputEvent::PointerDown {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    assert_eq!(ui.input.captured(), Some(node));
    ui.dispatch(InputEvent::PointerMove { x: 490., y: 400. });
    assert_eq!(value.get(), 100.);
    ui.dispatch(InputEvent::PointerUp {
        x: 490.,
        y: 400.,
        button: PointerButton::Primary,
    });
    assert_eq!(ui.input.captured(), None);
    ui.dispatch(key(Key::Home, Modifiers::default()));
    assert_eq!(value.get(), 0.);
    ui.dispatch(key(Key::ArrowRight, Modifiers::default()));
    assert_eq!(value.get(), 1.);
    ui.runtime.flush();
    assert_eq!(
        ui.semantics.borrow().get(node).unwrap().numeric_value,
        Some(1.)
    );
}
#[test]
fn text_input_unicode_ime_clipboard_undo_and_semantics() {
    let mut ui = Ui::new(500., 500.);
    let value = ui.signal("a👩‍💻".to_owned());
    let e = ui.text_input(ui.root(), "Name", value.clone(), 300., false);
    ui.input.focus(&ui.scene, Some(e.node));
    ui.dispatch(key(Key::Backspace, Modifiers::default()));
    assert_eq!(value.get(), "a");
    ui.dispatch(InputEvent::ImePreedit {
        text: "に".into(),
        cursor: Some((3, 3)),
    });
    assert_eq!(value.get(), "a");
    ui.dispatch(InputEvent::ImeCommit("日本".into()));
    assert_eq!(value.get(), "a日本");
    let command = Modifiers {
        control: !cfg!(target_os = "macos"),
        meta: cfg!(target_os = "macos"),
        ..Default::default()
    };
    ui.dispatch(key(Key::Character("a".into()), command));
    assert_eq!(e.copy(), "a日本");
    assert_eq!(e.cut(), "a日本");
    assert_eq!(value.get(), "");
    e.paste("e\u{301}");
    assert_eq!(value.get(), "e\u{301}");
    ui.dispatch(key(Key::Character("z".into()), command));
    assert_eq!(value.get(), "");
    ui.dispatch(key(
        Key::Character("z".into()),
        Modifiers {
            shift: true,
            ..command
        },
    ));
    assert_eq!(value.get(), "e\u{301}");
    ui.runtime.flush();
    assert_eq!(
        ui.semantics.borrow().get(e.node).unwrap().value.as_deref(),
        Some("e\u{301}")
    );
    ui.dispatch(InputEvent::Text("x\ny".into()));
    assert_eq!(value.get(), "e\u{301}xy");
}
#[test]
fn removing_subtree_disposes_subscriptions_and_editors() {
    let mut ui = Ui::new(500., 500.);
    let group = ui.container(ui.root(), Layout::Column, fixed(300., 200.));
    let value = ui.signal("start".to_owned());
    ui.label_signal(group, value.clone(), fixed(100., 30.));
    let editor = ui.text_input(group, "Text", value.clone(), 200., false);
    ui.input.focus(&ui.scene, Some(editor.node));
    // The label, the editor, the editor's themed background and the colour
    // of its selection.
    assert_eq!(ui.runtime.effect_count(), 4);
    ui.remove(group);
    assert_eq!(ui.runtime.effect_count(), 0);
    assert!(ui.focused_editor().is_none());
    value.set("after removal".into());
    ui.runtime.flush();
    assert_eq!(ui.scene.borrow().len(), 1);
    assert_eq!(ui.semantics.borrow().iter().count(), 0);
}
#[test]
fn nested_modal_scopes_trap_focus_pointer_and_restore() {
    let mut ui = Ui::new(500., 500.);
    let outside = ui.button(ui.root(), "Outside", 100., || {});
    let dialog = ui.container(ui.root(), Layout::Column, fixed(200., 100.));
    let first = ui.button(dialog, "First", 100., || {});
    let second = ui.button(dialog, "Second", 100., || {});
    let nested = ui.container(dialog, Layout::Column, fixed(150., 40.));
    let inner = ui.button(nested, "Inner", 100., || {});
    ui.scene.borrow_mut().flush();
    ui.input.focus(&ui.scene, Some(outside));
    assert!(ui.input.push_focus_scope(&ui.scene, dialog));
    assert_eq!(ui.input.focused(), Some(first));
    ui.dispatch(key(Key::Tab, Modifiers::default()));
    assert_eq!(ui.input.focused(), Some(second));
    ui.input.focus(&ui.scene, Some(outside));
    assert_ne!(ui.input.focused(), Some(outside));
    ui.input.focus(&ui.scene, Some(second));
    assert!(ui.input.push_focus_scope(&ui.scene, nested));
    assert_eq!(ui.input.focused(), Some(inner));
    ui.dispatch(key(Key::Tab, Modifiers::default()));
    assert_eq!(ui.input.focused(), Some(inner));
    assert!(ui.input.pop_focus_scope(&ui.scene));
    assert_eq!(ui.input.focused(), Some(second));
    assert!(ui.input.pop_focus_scope(&ui.scene));
    assert_eq!(ui.input.focused(), Some(outside));
}

#[test]
fn disabling_focused_button_clears_focus_visual() {
    let mut ui = Ui::new(500., 500.);
    let node = ui.button(ui.root(), "Save", 100., || {});
    ui.input.focus(&ui.scene, Some(node));
    ui.set_disabled(node, true);
    ui.set_disabled(node, false);
    let scene = ui.scene.borrow();
    let bg = scene.children(node)[0];
    assert!(
        matches!(scene.paint_items().find(|item|item.id==bg).unwrap().kind,zgx::scene::NodeKind::Quad(style) if style.fill==ui.theme().color(zgx::theme::tok::INPUT_BG))
    );
}

#[test]
fn modal_blocks_outside_clicks_and_removed_scope_restores_focus() {
    let mut ui = Ui::new(500., 500.);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let outside = ui.button(ui.root(), "Outside", 100., move || {
        count.set(count.get() + 1)
    });
    let dialog = ui.container(ui.root(), Layout::Column, fixed(200., 100.));
    let inside = ui.button(dialog, "Inside", 100., || {});
    ui.input.focus(&ui.scene, Some(outside));
    ui.input.push_focus_scope(&ui.scene, dialog);
    click(&mut ui, outside);
    assert_eq!(calls.get(), 0);
    assert_eq!(ui.input.focused(), Some(inside));
    ui.remove(dialog);
    ui.dispatch(InputEvent::PointerMove { x: 400., y: 400. });
    assert_eq!(ui.input.focus_scope(), None);
    assert_eq!(ui.input.focused(), Some(outside));
}

#[test]
fn clipboard_normalizes_platform_line_endings_and_preedit_cursor_is_safe() {
    let mut ui = Ui::new(500., 500.);
    let value = ui.signal(String::new());
    let single = ui.text_input(ui.root(), "Single", value.clone(), 200., false);
    single.paste("a\r\nb\rc\nd");
    assert_eq!(value.get(), "abcd");
    ui.input.focus(&ui.scene, Some(single.node));
    ui.dispatch(InputEvent::ImePreedit {
        text: "e\u{301}👩‍💻".into(),
        cursor: Some((2, 7)),
    });
    let e = single.editor.borrow();
    let preedit = e.preedit().unwrap();
    assert_eq!(preedit.cursor, Some((0, 3)));
    drop(e);
    let multi_value = ui.signal(String::new());
    let multi = ui.text_input(ui.root(), "Multi", multi_value.clone(), 200., true);
    multi.paste("a\r\nb\rc\nd");
    assert_eq!(multi_value.get(), "a\nb\nc\nd");
}

#[test]
fn native_ime_caret_follows_selection_and_preedit_has_underline() {
    let mut ui = Ui::new(500., 500.);
    let value = ui.signal("hello\nworld".into());
    let editor = ui.text_input(ui.root(), "Document", value, 300., true);
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.scene.borrow_mut().flush();
    let end = ui.scene.borrow().bounds(editor.caret);
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.scene.borrow_mut().flush();
    let start = ui.scene.borrow().bounds(editor.caret);
    assert!(start.x < end.x);
    assert!(start.y < end.y);
    let count = ui.scene.borrow().len();
    ui.dispatch(InputEvent::ImePreedit {
        text: "compose".into(),
        cursor: Some((7, 7)),
    });
    ui.scene.borrow_mut().flush();
    assert_eq!(ui.scene.borrow().len(), count + 1);
    let composition = ui.scene.borrow().bounds(editor.caret);
    assert!(composition.x > start.x);
    ui.dispatch(InputEvent::ImeCommit("commit".into()));
    assert_eq!(ui.scene.borrow().len(), count);
}

#[test]
fn disabled_container_blocks_nested_controls_and_blurs_focused_child() {
    let mut ui = Ui::new(500., 500.);
    let group = ui.container(ui.root(), Layout::Column, fixed(300., 100.));
    let hits = Rc::new(Cell::new(0));
    let count = hits.clone();
    let button = ui.button(group, "Child", 100., move || count.set(count.get() + 1));
    click(&mut ui, button);
    assert_eq!(hits.get(), 1);
    assert_eq!(ui.input.focused(), Some(button));
    ui.set_disabled(group, true);
    assert_eq!(ui.input.focused(), None);
    click(&mut ui, button);
    assert_eq!(hits.get(), 1);
    ui.dispatch(key(Key::Tab, Modifiers::default()));
    assert_eq!(ui.input.focused(), None);
    ui.set_disabled(group, false);
    ui.dispatch(key(Key::Tab, Modifiers::default()));
    assert_eq!(ui.input.focused(), Some(button));
    click(&mut ui, button);
    assert_eq!(hits.get(), 2);
    ui.remove(group);
    assert!(ui.input.options(group).is_none());
    assert!(ui.input.options(button).is_none());
}

#[test]
fn application_listeners_extend_buttons_and_editors_without_replacing_behavior() {
    let mut ui = Ui::new(500., 500.);
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    let button = ui.button(ui.root(), "Save", 100., move || count.set(count.get() + 1));
    let observed = Rc::new(Cell::new(0));
    let obs = observed.clone();
    ui.on_event(button, false, move |e| {
        if e.event == InputEvent::Activate {
            obs.set(obs.get() + 1);
        }
    });
    click(&mut ui, button);
    assert_eq!(clicks.get(), 1);
    assert_eq!(observed.get(), 1);
    assert!(ui.input.options(button).unwrap().focusable);
    let value = ui.signal(String::new());
    let editor = ui.text_input(ui.root(), "Name", value.clone(), 200., false);
    let submits = Rc::new(Cell::new(0));
    let submit = submits.clone();
    ui.on_event(editor.node, false, move |e| {
        if matches!(
            e.event,
            InputEvent::KeyDown {
                key: Key::Enter,
                ..
            }
        ) {
            submit.set(submit.get() + 1);
        }
    });
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.dispatch(InputEvent::Text("still editable".into()));
    ui.dispatch(key(Key::Enter, Modifiers::default()));
    assert_eq!(value.get(), "still editable");
    assert_eq!(submits.get(), 1);
}
#[test]
fn invalid_slider_and_progress_values_normalize_without_nonfinite_geometry() {
    let mut ui = Ui::new(500., 500.);
    let value = ui.signal(f32::NAN);
    let slider = ui.slider(ui.root(), "Value", value.clone(), 10.0..=20., 200.);
    assert_eq!(value.get(), 10.);
    value.set(f32::INFINITY);
    assert_eq!(value.get(), 20.);
    value.set(f32::NEG_INFINITY);
    assert_eq!(value.get(), 10.);
    value.set(999.);
    assert_eq!(value.get(), 20.);
    assert_eq!(
        ui.semantics.borrow().get(slider).unwrap().numeric_value,
        Some(20.)
    );
    let extreme = ui.signal(0.);
    ui.slider(
        ui.root(),
        "Extreme",
        extreme.clone(),
        -f32::MAX..=f32::MAX,
        200.,
    );
    extreme.set(f32::MAX);
    let progress = ui.signal(f32::NAN);
    ui.progress(ui.root(), "Progress", progress.clone(), 200.);
    assert_eq!(progress.get(), 0.);
    progress.set(f32::INFINITY);
    assert_eq!(progress.get(), 1.);
    ui.scene.borrow_mut().flush();
    assert!(ui.scene.borrow().paint_items().all(|p| {
        [p.bounds.x, p.bounds.y, p.bounds.width, p.bounds.height]
            .iter()
            .all(|n| n.is_finite())
    }));
}

#[test]
fn empty_overlay_passes_through_but_disabled_interactive_overlay_blocks() {
    let mut ui = Ui::new(300., 200.);
    let root = ui.root();
    ui.scene
        .borrow_mut()
        .set_kind(root, zgx::scene::NodeKind::Container(Layout::Overlay));
    let count = Rc::new(Cell::new(0));
    let clicks = count.clone();
    let underlying = ui.button(ui.root(), "Underlying", 150., move || {
        clicks.set(clicks.get() + 1)
    });
    let _empty = ui.container(ui.root(), Layout::Overlay, fixed(300., 200.));
    ui.on_event(ui.root(), false, |_| {});
    click(&mut ui, underlying);
    assert_eq!(count.get(), 1);
    let blocking = ui.button(ui.root(), "Disabled", 150., || panic!("disabled callback"));
    ui.set_disabled(blocking, true);
    click(&mut ui, underlying);
    assert_eq!(count.get(), 1);
}

#[test]
fn empty_ime_preedit_cancels_without_hiding_selected_committed_text() {
    let mut ui = Ui::new(500., 500.);
    let value = ui.signal("original".to_owned());
    let editor = ui.text_input(ui.root(), "Name", value.clone(), 300., false);
    ui.input.focus(&ui.scene, Some(editor.node));
    editor.editor.borrow_mut().select_all();
    ui.dispatch(InputEvent::ImePreedit {
        text: "composition".into(),
        cursor: Some((3, 3)),
    });
    ui.dispatch(InputEvent::ImePreedit {
        text: "composition".into(),
        cursor: None,
    });
    assert_eq!(ui.scene.borrow().effects(editor.caret).opacity, 0.);
    ui.dispatch(InputEvent::ImePreedit {
        text: String::new(),
        cursor: None,
    });
    assert!(editor.editor.borrow().preedit().is_none());
    assert_eq!(editor.copy(), "original");
    assert_eq!(value.get(), "original");
    ui.dispatch(InputEvent::Blur);
    ui.input
        .dispatch_to(&ui.scene, editor.node, InputEvent::Focus);
    assert_eq!(editor.copy(), "original");
    assert_eq!(ui.input.focused(), Some(editor.node));
    assert_eq!(ui.scene.borrow().effects(editor.caret).opacity, 1.);
}

#[test]
fn progress_tracks_allocated_dimensions_without_model_changes() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(0.25);
    let progress = ui.progress(ui.root(), "Download", value.clone(), 200.);
    ui.prepare_frame();
    let children = ui.scene.borrow().children(progress).to_vec();
    assert_eq!(ui.scene.borrow().bounds(children[1]).width, 50.);
    ui.scene.borrow_mut().set_style(progress, fixed(400., 16.));
    ui.prepare_frame();
    assert_eq!(value.get(), 0.25);
    assert_eq!(ui.scene.borrow().bounds(children[0]).width, 400.);
    assert_eq!(ui.scene.borrow().bounds(children[1]).width, 100.);
    assert_eq!(ui.scene.borrow().bounds(children[1]).height, 16.);
    value.set(0.75);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(children[1]).width, 300.);
    ui.remove(progress);
    value.set(0.5);
    ui.prepare_frame();
    assert!(!ui.scene.borrow().contains(children[1]));
}

#[test]
fn slider_resize_keeps_pointer_mapping_and_fill_in_sync() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(50.);
    let slider = ui.slider(ui.root(), "Volume", value.clone(), 0.0..=100., 200.);
    ui.prepare_frame();
    ui.scene.borrow_mut().set_style(slider, fixed(400., 60.));
    ui.prepare_frame();
    let children = ui.scene.borrow().children(slider).to_vec();
    let fill = ui.scene.borrow().bounds(children[1]);
    let knob = ui.scene.borrow().bounds(children[2]);
    assert_eq!(fill.width, 187.);
    assert_eq!(knob.x, 195.);
    assert_eq!(knob.y, 20.);
    assert_eq!(ui.scene.borrow().bounds(children[0]).width, 400.);
    ui.dispatch(InputEvent::PointerDown {
        x: 195.,
        y: 30.,
        button: PointerButton::Primary,
    });
    assert_eq!(value.get(), 50.);
    ui.dispatch(InputEvent::PointerMove { x: 382., y: 30. });
    assert_eq!(value.get(), 100.);
    ui.dispatch(InputEvent::PointerUp {
        x: 382.,
        y: 30.,
        button: PointerButton::Primary,
    });
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(children[1]).width, 374.);
}
