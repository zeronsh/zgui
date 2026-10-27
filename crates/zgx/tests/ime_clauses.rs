//! Marked text as a native field shows it while an input method converts
//! (Japanese, Chinese): a thin line under the composition, a thick one under
//! the clause being converted, and its candidates placed at that clause.
use zgx::{compose::prelude::*, input::InputEvent, scene::NodeKind, widgets::Ui};

/// The heights of the marked-text lines the editor draws.
fn lines(ui: &Ui, root: zgx::scene::NodeId) -> Vec<f32> {
    let scene = ui.scene.borrow();
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        let b = scene.bounds(id);
        if matches!(
            scene.kind(id),
            NodeKind::Rect(_) | NodeKind::Quad(_) | NodeKind::Panel { .. }
        ) && b.width > 1.
            && (b.height == 1. || b.height == 2.)
        {
            found.push(b.height);
        }
        pending.extend(scene.children(id).iter().copied());
    }
    found
}

#[test]
fn the_clause_being_converted_is_underlined_and_takes_the_candidates() {
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(String::new());
    let view = ui.mount(text_input("Name", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    // Typing: the composition, its caret at the end, no clause yet.
    ui.dispatch(InputEvent::ImePreedit {
        text: "にほんご".into(),
        cursor: Some((12, 12)),
    });
    ui.prepare_frame();
    assert!(editor.composition_area().is_none(), "the caret places them");
    assert!(lines(&ui, view.node()).contains(&1.));
    assert!(!lines(&ui, view.node()).contains(&2.));
    // Converting: the first clause (two kana, six bytes) is selected.
    ui.dispatch(InputEvent::ImePreedit {
        text: "日本語".into(),
        cursor: Some((0, 6)),
    });
    ui.prepare_frame();
    let area = editor.composition_area().expect("the clause places them");
    let whole = ui.scene.borrow().bounds(view.node());
    assert!(
        area.width > 0. && area.x >= whole.x && area.x < whole.x + whole.width,
        "{area:?}"
    );
    assert!(
        lines(&ui, view.node()).contains(&2.),
        "a thick line under the clause"
    );
    // Committed: nothing marked.
    ui.dispatch(InputEvent::ImeCommit("日本語".into()));
    ui.prepare_frame();
    assert!(editor.composition_area().is_none());
    assert!(lines(&ui, view.node()).is_empty());
}

/// Given an input method's clauses (Windows' display attributes), each is
/// underlined on its own, a pixel apart from the next, thick under the one
/// being converted, which takes the candidates.
#[test]
fn each_clause_is_underlined_and_the_target_takes_the_candidates() {
    use zgx::text_edit::ClauseStyle;
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(String::new());
    let view = ui.mount(text_input("Name", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    // 日本 converted, 語 being converted.
    editor.editor.borrow_mut().set_preedit_clauses(
        "日本語",
        Some((9, 9)),
        &[(0..6, ClauseStyle::Converted), (6..9, ClauseStyle::Target)],
    );
    editor.refresh();
    ui.prepare_frame();
    let mut heights = lines(&ui, view.node());
    heights.sort_by(f32::total_cmp);
    assert_eq!(
        heights,
        [1., 2.],
        "a line under each clause, thick under the target"
    );
    let area = editor.composition_area().expect("the target places them");
    let converted = {
        editor.editor.borrow_mut().set_preedit_clauses(
            "日本語",
            Some((6, 6)),
            &[(0..6, ClauseStyle::Target), (6..9, ClauseStyle::Converted)],
        );
        editor.refresh();
        ui.prepare_frame();
        editor.composition_area().unwrap()
    };
    assert!(
        area.x > converted.x,
        "the third character's, right of the first's: {area:?} {converted:?}"
    );
    // A preedit set without clauses draws as before.
    editor
        .editor
        .borrow_mut()
        .set_preedit("日本語", Some((0, 6)));
    editor.refresh();
    ui.prepare_frame();
    assert!(editor.editor.borrow().preedit_clauses().is_empty());
    assert!(lines(&ui, view.node()).contains(&2.));
}
