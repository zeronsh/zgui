use zgx::{
    input::{InputEvent, Key, Modifiers},
    text_edit::{Selection, TextEditor},
    widgets::Ui,
};

#[test]
fn unicode_model_replacements_round_trip_original_selection_and_cancel_preedit() {
    for (before, after) in [
        ("prefix e\u{301} suffix", "prefix e\u{302} suffix"),
        ("a👩\u{200d}💻z", "a👩\u{200d}🔬z"),
        ("é unchanged 文", "ê unchanged 文"),
        ("é unchanged 文", "é unchanged 字"),
        ("aaaa", "aaaaa"),
        ("aaaaa", "aaaa"),
        ("abab", "abXab"),
        ("abc", "XYZ"),
        ("", "🙂"),
        ("🙂", ""),
    ] {
        let mut editor = TextEditor::new(before);
        editor.set_selection(before.len(), 0);
        let original_selection = editor.selection();
        editor.set_preedit("候補", Some((0, 3)));
        let revision = editor.composition_cancel_revision();
        assert!(editor.set_text(after));
        assert_eq!(editor.text(), after);
        assert_eq!(
            editor.selection(),
            Selection {
                anchor: after.len(),
                focus: after.len()
            }
        );
        assert!(editor.preedit().is_none());
        assert_eq!(
            editor.composition_cancel_revision(),
            revision.wrapping_add(1)
        );
        assert!(editor.undo(), "{before:?} → {after:?}");
        assert_eq!(editor.text(), before);
        assert_eq!(editor.selection(), original_selection);
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.text(), after);
        assert_eq!(editor.selection().focus, after.len());
        assert!(!editor.redo());
    }
}

#[test]
fn long_document_streaming_retains_small_updates_under_default_and_small_budgets() {
    for budget in [None, Some(32)] {
        let initial = "x".repeat(128 * 1024);
        let mut editor = TextEditor::new(initial.clone());
        if let Some(bytes) = budget {
            editor.set_history_limits(100, bytes);
        }
        assert!(!editor.undo());
        let steps = if budget.is_some() { 24 } else { 40 };
        for count in 1..=steps {
            assert!(editor.set_text(format!("{initial}{}", "a".repeat(count))));
        }
        for count in (0..steps).rev() {
            assert!(editor.undo(), "budget={budget:?}, undo to suffix {count}");
            assert_eq!(editor.text().len(), initial.len() + count);
            assert!(editor.text().starts_with(&initial));
        }
        assert!(!editor.undo());
        for count in 1..=steps {
            assert!(editor.redo());
            assert_eq!(editor.text(), format!("{initial}{}", "a".repeat(count)));
        }
        assert!(!editor.redo());
    }
}

#[test]
fn minimal_replacement_budget_counts_both_changed_sides_and_preserves_redo_branch() {
    let prefix = "retained ".repeat(1024);
    let suffix = " suffix".repeat(1024);
    let a = format!("{prefix}é{suffix}");
    let b = format!("{prefix}ê{suffix}");
    let c = format!("{prefix}文{suffix}");
    let mut editor = TextEditor::new(a.clone());
    editor.set_history_limits(10, 9); // Two changed UTF-8 scalars: 2+2, then 2+3.
    assert!(editor.set_text(&b));
    assert!(editor.set_text(&c));
    assert!(editor.undo());
    assert_eq!(editor.text(), b);
    assert!(editor.undo());
    assert_eq!(editor.text(), a);
    assert!(editor.redo());
    assert_eq!(editor.text(), b);
    assert!(!editor.set_text(&b)); // Equality must not destroy the redo branch.
    assert!(editor.redo());
    assert_eq!(editor.text(), c);
    assert!(editor.undo());
    assert!(editor.set_text(format!("{prefix}字{suffix}")));
    assert!(!editor.redo());
}

#[test]
fn external_ui_model_streaming_uses_delta_history_and_commits_undo_back_to_model() {
    let mut ui = Ui::new(300., 120.);
    let initial = "long document ".repeat(256);
    let model = ui.signal(initial.clone());
    let editor = ui.text_input(ui.root(), "Stream", model.clone(), 240., true);
    editor.editor.borrow_mut().set_history_limits(100, 24);
    for count in 1..=12 {
        model.set(format!("{initial}{}", "é".repeat(count)));
        assert_eq!(editor.editor.borrow().text(), model.get());
    }
    ui.input.focus(&ui.scene, Some(editor.node));
    for count in (0..12).rev() {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Character("z".into()),
            modifiers: Modifiers {
                control: !cfg!(target_os = "macos"),
                meta: cfg!(target_os = "macos"),
                ..Modifiers::default()
            },
            repeat: false,
        });
        assert_eq!(model.get(), format!("{initial}{}", "é".repeat(count)));
    }
    assert!(!editor.editor.borrow_mut().undo());
    for count in 1..=12 {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Character("z".into()),
            modifiers: Modifiers {
                control: !cfg!(target_os = "macos"),
                meta: cfg!(target_os = "macos"),
                shift: true,
                ..Modifiers::default()
            },
            repeat: false,
        });
        assert_eq!(model.get(), format!("{initial}{}", "é".repeat(count)));
    }
    let semantic_value = ui
        .semantics
        .borrow()
        .get(editor.node)
        .unwrap()
        .value
        .clone();
    assert_eq!(semantic_value.as_deref(), Some(model.get().as_str()));
}
