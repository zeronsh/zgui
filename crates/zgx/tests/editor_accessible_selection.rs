use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent},
    widgets::Ui,
};

#[test]
fn accessible_selection_clamps_unicode_preserves_direction_and_cancels_preedit() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("a\u{301}🙂z".to_owned());
    let editor = ui.text_input(ui.root(), "Editor", value.clone(), 180., true);
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(editor.node));
    assert!(ui.set_accessible_text_selection(editor.node, 3, 7));
    ui.dispatch(InputEvent::ImePreedit {
        text: "候補".into(),
        cursor: Some((0, 3)),
    });
    assert!(editor.editor.borrow().preedit().is_some());
    assert!(ui.set_accessible_text_selection(editor.node, usize::MAX, 2));
    ui.prepare_frame();
    assert!(editor.editor.borrow().preedit().is_none());
    let selection = editor.editor.borrow().selection();
    assert_eq!((selection.anchor, selection.focus), (8, 0));
    assert_eq!(value.get(), "a\u{301}🙂z");
    let semantics = ui.semantics.borrow();
    let node = semantics.get(editor.node).unwrap();
    assert_eq!(node.value.as_deref(), Some("a\u{301}🙂z"));
    assert_eq!(node.text_selection, Some((8, 0)));
    let revision = semantics.revision();
    drop(semantics);
    assert!(ui.set_accessible_text_selection(editor.node, 8, 0));
    assert_eq!(ui.semantics.borrow().revision(), revision);
}

#[test]
fn accessible_selection_obeys_cancellation_disability_modal_scope_and_disposal() {
    let mut ui = Ui::new(400., 300.);
    let cancel = ui.signal(true);
    let read = cancel.clone();
    let value = ui.signal("abcdef".to_owned());
    let mounted = ui.mount(
        column().children([
            text_input("Editor", value)
                .id("editor")
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Target
                        && read.get()
                        && matches!(cx.event, InputEvent::SetTextSelection { .. })
                    {
                        cx.prevent_default();
                    }
                }),
            column().id("scope").child(button().child(text("Scoped"))),
        ]),
    );
    ui.prepare_frame();
    let node = mounted.find("editor").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    let initial = editor.editor.borrow().selection();
    ui.set_accessible_text_selection(node, 1, 4);
    assert_eq!(editor.editor.borrow().selection(), initial);
    cancel.set(false);
    assert!(ui.set_accessible_text_selection(node, 1, 4));
    ui.set_disabled(node, true);
    assert!(!ui.set_accessible_text_selection(node, 0, 0));
    assert_eq!(editor.editor.borrow().selection().focus, 4);
    ui.set_disabled(node, false);
    let scope = mounted.find("scope").unwrap();
    assert!(ui.input.push_focus_scope(&ui.scene, scope));
    assert!(!ui.set_accessible_text_selection(node, 0, 0));
    assert_eq!(editor.editor.borrow().selection().focus, 4);
    mounted.unmount();
    assert!(!ui.set_accessible_text_selection(node, 0, 0));
}

#[test]
fn selection_action_reveals_wrapped_caret_and_updates_committed_metadata() {
    let mut ui = Ui::new(200., 150.);
    let value = ui.signal("abcdefghij".repeat(30));
    let mounted = ui.mount(
        text_area("Editor", value.clone())
            .size(100., 60.)
            .text_wrap(true)
            .id("editor"),
    );
    ui.prepare_frame();
    let node = mounted.find("editor").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    assert!(ui.set_accessible_text_selection(node, value.get().len(), value.get().len()));
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let bounds = scene.bounds(node);
    let caret = scene.bounds(editor.caret);
    assert!(caret.y >= bounds.y && caret.y + caret.height <= bounds.y + bounds.height);
    drop(scene);
    assert_eq!(
        ui.semantics.borrow().get(node).unwrap().text_selection,
        Some((300, 300))
    );
}
