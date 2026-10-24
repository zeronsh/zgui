use zgx::{input::InputEvent, widgets::Ui};

#[test]
fn committed_editor_ingress_agrees_on_line_endings_and_preserves_tabs() {
    let source = "a\r\nb\rc\n\té🙂";
    for multiline in [false, true] {
        let expected = if multiline {
            "a\nb\nc\n\té🙂"
        } else {
            "abc\té🙂"
        };
        for route in ["text", "ime", "paste", "accessible", "external"] {
            let mut ui = Ui::new(300., 200.);
            let value = ui.signal(String::new());
            let editor = ui.text_input(ui.root(), "Editor", value.clone(), 240., multiline);
            ui.prepare_frame();
            ui.input.focus(&ui.scene, Some(editor.node));
            let cancellation = editor.editor.borrow().composition_cancel_revision();
            match route {
                "text" => {
                    ui.dispatch(InputEvent::Text(source.into()));
                }
                "ime" => {
                    ui.dispatch(InputEvent::ImePreedit {
                        text: "候補".into(),
                        cursor: None,
                    });
                    ui.dispatch(InputEvent::ImeCommit(source.into()));
                }
                "paste" => editor.paste(source),
                "external" => {
                    value.set(source.into());
                }
                "accessible" => {
                    assert!(ui.set_accessible_value(editor.node, source));
                }
                _ => unreachable!(),
            }
            ui.prepare_frame();
            if route == "ime" {
                assert_eq!(
                    editor.editor.borrow().composition_cancel_revision(),
                    cancellation,
                    "normal native commit is not external cancellation"
                );
            }
            assert_eq!(
                editor.editor.borrow().text(),
                expected,
                "route={route}, multiline={multiline}"
            );
            assert_eq!(value.get(), expected);
            assert!(editor.editor.borrow().preedit().is_none());
            assert_eq!(
                ui.semantics
                    .borrow()
                    .get(editor.node)
                    .unwrap()
                    .value
                    .as_deref(),
                Some(expected)
            );
            assert!(editor.editor.borrow_mut().undo());
            assert_eq!(editor.editor.borrow().text(), "");
            assert!(editor.editor.borrow_mut().redo());
            assert_eq!(editor.editor.borrow().text(), expected);
        }
    }
}

#[test]
fn native_text_control_rejection_remains_atomic_after_line_ending_normalization() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("stable".to_owned());
    let editor = ui.text_input(ui.root(), "Editor", value.clone(), 240., true);
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.dispatch(InputEvent::Text("allowed\r\n\u{1}rejected".into()));
    assert_eq!(value.get(), "stable");
    assert_eq!(editor.editor.borrow().text(), "stable");
    assert!(!editor.editor.borrow_mut().undo());
}
