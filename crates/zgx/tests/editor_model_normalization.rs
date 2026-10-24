use zgx::{compose::prelude::*, input::InputEvent, widgets::Ui};
#[test]
fn initial_and_external_models_are_canonical_without_initial_undo_or_equal_value_edits() {
    for multiline in [false, true] {
        for composed in [false, true] {
            let mut ui = Ui::new(400., 300.);
            let expected = if multiline { "A\nB\nC\nD\t" } else { "ABCD\t" };
            let model = ui.signal("A\r\nB\rC\nD\t".to_owned());
            let node = if composed {
                let view = if multiline {
                    text_area("Editor", model.clone())
                } else {
                    text_input("Editor", model.clone())
                };
                ui.mount(view.size(240., 160.)).node()
            } else {
                ui.text_input(ui.root(), "Editor", model.clone(), 240., multiline)
                    .node
            };
            ui.prepare_frame();
            ui.input.focus(&ui.scene, Some(node));
            let editor = ui.focused_editor().unwrap();
            assert_eq!(model.get(), expected);
            assert_eq!(editor.editor.borrow().text(), expected);
            assert!(
                !editor.editor.borrow_mut().undo(),
                "initial normalization is not an edit"
            );
            editor.editor.borrow_mut().set_selection(1, 1);
            ui.dispatch(InputEvent::ImePreedit {
                text: "中".into(),
                cursor: Some((3, 3)),
            });
            let revision = editor.editor.borrow().composition_cancel_revision();
            model.set("A\r\nB\rC\nD\t".into());
            ui.prepare_frame();
            assert_eq!(model.get(), expected);
            assert_eq!(editor.editor.borrow().selection().focus, 1);
            assert!(editor.editor.borrow().preedit().is_some());
            assert_eq!(
                editor.editor.borrow().composition_cancel_revision(),
                revision
            );
            assert!(!editor.editor.borrow_mut().undo());
            model.set("X\r\nY\rZ".into());
            ui.prepare_frame();
            assert_eq!(model.get(), if multiline { "X\nY\nZ" } else { "XYZ" });
            assert!(editor.editor.borrow().preedit().is_none());
            assert_eq!(
                editor.editor.borrow().composition_cancel_revision(),
                revision + 1
            );
            assert_eq!(ui.focused_editor().unwrap().node, node);
            assert!(editor.editor.borrow_mut().undo());
            assert_eq!(editor.editor.borrow().text(), expected);
            assert_eq!(editor.editor.borrow().selection().focus, 1);
            assert!(!editor.editor.borrow_mut().undo());
        }
    }
}
#[test]
fn canonical_writeback_settles_once_without_resurrecting_unmounted_bindings() {
    let mut ui = Ui::new(400., 300.);
    let model = ui.signal("a".to_owned());
    let view = ui.mount(text_input("Editor", model.clone()));
    ui.prepare_frame();
    model.set("x\r\ny".into());
    assert_eq!(model.get(), "xy");
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    model.set("x\r\ny".into());
    ui.prepare_frame();
    let idle = ui.scene.borrow_mut().flush();
    assert_eq!(idle.layout_nodes, 0);
    assert!(idle.damage.is_empty());
    view.unmount();
    model.set("x\r\ny".into());
    assert_eq!(
        model.get(),
        "x\r\ny",
        "removed editor no longer normalizes external state"
    );
}

#[test]
fn initial_canonical_writeback_can_remove_the_editor_parent_without_retaining_bindings() {
    let mut ui = Ui::new(400., 300.);
    let baseline = ui.runtime.effect_count();
    let parent = ui.mount(column());
    let parent_node = parent.node();
    let model = ui.signal("a\r\nb".to_owned());
    let observed = model.clone();
    let _watcher = ui.runtime.effect(move || {
        if observed.get() == "ab" {
            parent.unmount();
        }
    });
    let editor = ui.text_input(parent_node, "Editor", model.clone(), 240., false);
    assert!(!ui.scene.borrow().contains(editor.node));
    assert_eq!(
        ui.runtime.effect_count(),
        baseline + 1,
        "removed subtree must not regain editor effects"
    );
    model.set("next\nvalue".into());
    ui.prepare_frame();
    assert_eq!(model.get(), "next\nvalue");
}

#[test]
fn construction_normalization_respects_an_existing_outer_batch() {
    let mut ui = Ui::new(400., 300.);
    let model = ui.signal("a\r\nb".to_owned());
    let runtime = ui.runtime.clone();
    let editor = runtime.batch(|| {
        let editor = ui.text_input(ui.root(), "Editor", model.clone(), 240., false);
        assert_eq!(editor.editor.borrow().text(), "ab");
        assert_eq!(
            model.get(),
            "a\r\nb",
            "outer batch controls effect delivery"
        );
        editor
    });
    assert_eq!(model.get(), "ab");
    assert!(!editor.editor.borrow_mut().undo());
}
