//! An editor holding a long document (a file of thousands of lines) lays it
//! out by its lines: it shows and shapes the lines in view, numbers those,
//! and edits, selects, copies and undoes across lines it never showed.
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Signal,
    scene::{NodeId, NodeKind},
    widgets::{EditorHandle, Ui},
};

const LINES: usize = 10_000;

fn document() -> String {
    (0..LINES)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}
fn command() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Modifiers::default()
    }
}
fn press(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key: key.clone(),
        modifiers,
        repeat: false,
    });
    ui.dispatch(InputEvent::KeyUp { key, modifiers });
    ui.prepare_frame();
}
/// The texts the editor's subtree shows.
fn shown(ui: &Ui, root: NodeId) -> Vec<String> {
    let scene = ui.scene.borrow();
    let mut texts = Vec::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if let NodeKind::Text { text, .. } = scene.kind(id)
            && !text.is_empty()
        {
            texts.push(text.to_string());
        }
        pending.extend(scene.children(id).iter().copied());
    }
    texts
}
fn editor(wrap: bool) -> (Ui, EditorHandle, Signal<String>) {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal(document());
    let view = ui.mount(
        text_area("Document", value.clone())
            .w(300.)
            .h(200.)
            .text_size(10.)
            .line_height(20.)
            .line_numbers(true)
            .text_wrap(wrap),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    (ui, editor, value)
}
fn caret_at(editor: &EditorHandle, offset: usize) {
    editor.set_selection(offset, offset);
}

#[test]
fn a_long_document_shows_and_numbers_the_lines_in_view() {
    let (ui, editor, _) = editor(false);
    let at = document().find("line 5000").unwrap();
    caret_at(&editor, at);
    ui.prepare_frame();
    let texts = shown(&ui, editor.node);
    assert!(texts.iter().any(|t| t == "line 5000"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t.trim_start_matches('\u{2007}') == "5001"),
        "{texts:?}"
    );
    // Its 200 points show ten lines, numbered: not ten thousand.
    assert!(texts.len() < 40, "{}", texts.len());
    assert!(!texts.iter().any(|t| t == "line 0"));
    let caret = ui.scene.borrow().bounds(editor.caret);
    let bounds = ui.scene.borrow().bounds(editor.node);
    assert!(caret.y >= bounds.y && caret.y + caret.height <= bounds.y + bounds.height);
}

#[test]
fn keys_move_and_type_across_a_long_document_and_undo_puts_it_back() {
    let (mut ui, editor, value) = editor(false);
    let text = document();
    caret_at(&editor, text.find("line 7000").unwrap() + 4);
    press(&mut ui, Key::ArrowDown, Modifiers::default());
    assert_eq!(
        editor.selection().1,
        text.find("line 7001").unwrap() + 4,
        "down keeps the column"
    );
    ui.dispatch(InputEvent::Text("!".into()));
    ui.prepare_frame();
    assert_eq!(value.get().lines().nth(7001), Some("line! 7001"));
    assert!(shown(&ui, editor.node).iter().any(|t| t == "line! 7001"));
    press(&mut ui, Key::Character("z".into()), command());
    assert_eq!(value.get(), text);
    press(&mut ui, Key::End, command());
    assert_eq!(editor.selection().1, text.len());
    assert!(shown(&ui, editor.node).iter().any(|t| t == "line 9999"));
}

#[test]
fn selecting_a_long_document_draws_what_shows_and_copies_it_all() {
    let (mut ui, editor, _) = editor(false);
    press(&mut ui, Key::Character("a".into()), command());
    let (anchor, focus) = editor.selection();
    assert_eq!(
        (anchor.min(focus), anchor.max(focus)),
        (0, document().len())
    );
    assert_eq!(editor.editor.borrow().selected_text(), document());
    // A rectangle a line in view, not one a line of the file.
    let scene = ui.scene.borrow();
    let mut rects = 0;
    let mut pending = vec![editor.node];
    while let Some(id) = pending.pop() {
        if matches!(scene.kind(id), NodeKind::Rect(_)) {
            rects += 1;
        }
        pending.extend(scene.children(id).iter().copied());
    }
    assert!(rects < 40, "{rects}");
}

#[test]
fn a_click_in_a_long_document_puts_the_caret_on_the_line_under_it() {
    let (mut ui, editor, _) = editor(false);
    caret_at(&editor, document().find("line 4000").unwrap());
    ui.prepare_frame();
    // The caret's line, and the one under it: where a click lands.
    let caret = ui.scene.borrow().bounds(editor.caret);
    let (x, y) = (caret.x + 1., caret.y + caret.height * 1.5);
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
    ui.prepare_frame();
    assert_eq!(editor.selection().1, document().find("line 4001").unwrap());
}

#[test]
fn a_long_wrapped_document_wraps_its_lines_and_moves_through_their_rows() {
    let mut ui = Ui::new(400., 300.);
    let long_line = "word ".repeat(40);
    let text: String = (0..LINES)
        .map(|i| {
            if i == 6000 {
                long_line.clone()
            } else {
                format!("line {i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let value = ui.signal(text.clone());
    let view = ui.mount(
        text_area("Document", value)
            .w(300.)
            .h(200.)
            .text_size(10.)
            .line_height(20.)
            .text_wrap(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    let start = text.find(&long_line).unwrap();
    caret_at(&editor, start);
    ui.prepare_frame();
    let top = ui.scene.borrow().bounds(editor.caret).y;
    press(&mut ui, Key::ArrowDown, Modifiers::default());
    // The next row of the same line, not the next line.
    let (_, focus) = editor.selection();
    assert!(focus > start && focus < start + long_line.len(), "{focus}");
    assert!(ui.scene.borrow().bounds(editor.caret).y > top);
    press(&mut ui, Key::End, command());
    assert_eq!(editor.selection().1, text.len());
}
