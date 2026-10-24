use zgx::{
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
#[test]
fn read_only_blocks_mutations_but_keeps_selection_copy_and_external_models() {
    for multiline in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let value = ui.signal("abc".to_owned());
        let editor = ui.text_input(ui.root(), "Editor", value.clone(), 240., multiline);
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(editor.node));
        editor.editor.borrow_mut().set_selection(0, 2);
        editor.refresh();
        editor.set_read_only(true);
        assert!(editor.is_read_only());
        assert!(editor.clone().is_read_only());
        assert_eq!(ui.input.focused(), Some(editor.node));
        assert!(ui.semantics.borrow().get(editor.node).unwrap().read_only);
        for event in [
            InputEvent::Text("x".into()),
            InputEvent::ImePreedit {
                text: "中".into(),
                cursor: Some((3, 3)),
            },
            InputEvent::ImeCommit("中".into()),
            InputEvent::SetValue("replacement".into()),
        ] {
            assert!(ui.dispatch(event).default_prevented);
            assert_eq!(value.get(), "abc");
            assert!(editor.editor.borrow().preedit().is_none());
        }
        for key in [
            Key::Backspace,
            Key::Delete,
            Key::Enter,
            Key::Character("z".into()),
            Key::Character("y".into()),
        ] {
            ui.dispatch(InputEvent::KeyDown {
                key,
                modifiers: Modifiers {
                    control: !cfg!(target_os = "macos"),
                    meta: cfg!(target_os = "macos"),
                    ..Default::default()
                },
                repeat: false,
            });
            assert_eq!(value.get(), "abc");
        }
        assert_eq!(editor.copy(), "ab");
        assert_eq!(editor.cut(), "ab");
        editor.paste("paste");
        assert_eq!(value.get(), "abc");
        assert!(ui.set_accessible_text_selection(editor.node, 1, 3));
        assert_eq!(editor.copy(), "bc");
        assert!(ui.set_accessible_value(editor.node, "denied"));
        assert_eq!(value.get(), "abc");
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowLeft,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert!(editor.editor.borrow().selection().is_empty());
        value.set("external".into());
        ui.prepare_frame();
        assert_eq!(editor.editor.borrow().text(), "external");
        assert!(editor.is_read_only());
        editor.set_read_only(false);
        assert!(!ui.semantics.borrow().get(editor.node).unwrap().read_only);
        editor.paste("!");
        assert_eq!(value.get(), "external!");
    }
}
#[test]
fn entering_read_only_cancels_composition_once_and_preserves_undo_history() {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal("a".to_owned());
    let editor = ui.text_input(ui.root(), "Editor", value.clone(), 240., false);
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.dispatch(InputEvent::Text("b".into()));
    ui.dispatch(InputEvent::ImePreedit {
        text: "中".into(),
        cursor: Some((3, 3)),
    });
    let revision = editor.editor.borrow().composition_cancel_revision();
    let selection = editor.editor.borrow().selection();
    editor.set_read_only(true);
    ui.prepare_frame();
    assert_eq!(
        editor.editor.borrow().composition_cancel_revision(),
        revision + 1
    );
    assert!(editor.editor.borrow().preedit().is_none());
    assert_eq!(editor.editor.borrow().selection(), selection);
    assert_eq!(value.get(), "ab");
    ui.scene.borrow_mut().flush();
    let semantic_revision = ui.semantics.borrow().revision();
    editor.set_read_only(true);
    ui.prepare_frame();
    assert_eq!(
        editor.editor.borrow().composition_cancel_revision(),
        revision + 1
    );
    assert!(ui.scene.borrow_mut().flush().is_idle());
    assert_eq!(ui.semantics.borrow().revision(), semantic_revision);
    let undo = || InputEvent::KeyDown {
        key: Key::Character("z".into()),
        modifiers: Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Default::default()
        },
        repeat: false,
    };
    ui.dispatch(undo());
    assert_eq!(value.get(), "ab");
    editor.set_read_only(false);
    ui.dispatch(undo());
    assert_eq!(value.get(), "a");
    ui.remove(editor.node);
    editor.set_read_only(true);
    editor.set_read_only(false);
    assert!(!ui.scene.borrow().contains(editor.node));
}
