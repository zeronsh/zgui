//! The code editor example (examples/code_editor), run headless: a sample
//! of 100,000 lines opened at its start, its lines coloured as they show,
//! the caret the status bar follows, typing and undo, find and replace with
//! the matches marked, and Tab indenting.
mod common;

use common::Harness;
use zgui::{
    input::{InputEvent, Key, Modifiers},
    scene::{Color, NodeId, NodeKind},
    theme::{Themes, tok},
    widgets::EditorValue,
};
use zgui_script::ScriptHost;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/code_editor/app.luau");
fn command() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

fn code() -> Harness {
    let h = Harness::with_desktop(|runtime| ScriptHost::new(runtime, APP));
    h.ui.scene.borrow_mut().resize(1100., 760.);
    h.frames(2);
    h
}
fn run(h: &mut Harness, id: &str) {
    assert!(h.host.run_command(id), "no command `{id}`");
    h.settle();
    h.frames(2);
}
fn editor(h: &mut Harness) -> NodeId {
    h.find("editor").expect("the editor")
}
/// The editor focused, its caret at the start.
fn focus(h: &mut Harness) {
    let node = editor(h);
    h.ui.input.focus(&h.ui.scene, Some(node));
    h.frames(1);
}
/// What the editor's lines in view show: each line's text and its runs'
/// colours.
fn lines(h: &mut Harness) -> Vec<(String, Vec<(String, Color)>)> {
    let root = editor(h);
    let scene = h.ui.scene.borrow();
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        match scene.kind(id) {
            NodeKind::RichText { text } => out.push((
                text.text().to_string(),
                text.runs()
                    .iter()
                    .map(|run| (text.text()[run.range.clone()].to_string(), run.color))
                    .collect(),
            )),
            NodeKind::Text { text, color, .. } if text.contains(' ') => {
                out.push((text.to_string(), vec![(text.to_string(), *color)]))
            }
            _ => {}
        }
        pending.extend(scene.children(id).iter().copied());
    }
    out
}
fn color(h: &Harness, token: zgui::theme::ColorToken) -> Color {
    Themes::of(&h.ui.runtime).root().color(token)
}

#[test]
fn a_sample_of_100000_lines_opens_at_its_start_with_its_lines_coloured() {
    let mut h = code();
    assert_eq!(h.error(), None);
    run(&mut h, "sample");
    assert_eq!(h.text("status-message"), "A sample of 100000 lines");
    assert_eq!(h.text("status-caret"), "Ln 1, Col 1");
    assert_eq!(h.text("status-language"), "Rust");
    let shown = lines(&mut h);
    // The lines in view of the window, not the file.
    assert!(shown.len() < 80, "{}", shown.len());
    let signature = shown
        .iter()
        .find(|(text, _)| text.starts_with("pub fn compute_2("))
        .expect("the first function shows");
    assert_eq!(signature.1[0], ("pub".into(), color(&h, tok::ACCENT)));
    let comment = shown
        .iter()
        .find(|(text, _)| text.starts_with("/// A function"))
        .unwrap();
    assert_eq!(comment.1[0].1, color(&h, tok::TEXT_TERTIARY));
    // A block comment's second line, coloured by what its first left open.
    let running = shown
        .iter()
        .find(|(text, _)| text.trim() == "running on */")
        .unwrap();
    assert_eq!(running.1[0].1, color(&h, tok::TEXT_TERTIARY));
}

#[test]
fn typing_moves_the_caret_the_status_bar_shows_and_undo_puts_it_back() {
    let mut h = code();
    run(&mut h, "sample");
    focus(&mut h);
    h.key(Key::ArrowDown);
    h.key(Key::End);
    assert_eq!(h.text("status-caret"), "Ln 2, Col 72");
    h.ui.dispatch(InputEvent::Text(" // more".into()));
    h.settle();
    h.frames(1);
    assert_eq!(h.text("status-caret"), "Ln 2, Col 80");
    assert!(
        lines(&mut h)
            .iter()
            .any(|(text, _)| text.ends_with("Error> { // more"))
    );
    h.key_with(Key::Character("z".into()), command());
    assert!(
        !lines(&mut h)
            .iter()
            .any(|(text, _)| text.ends_with("// more"))
    );
    // To the end of the file, its last lines showing.
    h.key_with(Key::End, command());
    assert_eq!(h.text("status-caret"), "Ln 100000, Col 1");
    assert!(
        lines(&mut h)
            .iter()
            .any(|(text, _)| text.starts_with("    Ok(vec!["))
    );
}

/// The file's text is held once: the script's state, the editor's binding
/// and the editor share it, through typing, and a script reading the state
/// reads the edited text.
#[test]
fn the_file_is_shared_by_the_state_and_the_editor_as_it_is_typed_into() {
    let mut h = code();
    run(&mut h, "sample");
    focus(&mut h);
    let node = editor(&mut h);
    let shared = |h: &Harness| {
        let editor = h.ui.editor(node).unwrap();
        let ours = editor.editor.borrow().value();
        matches!(editor.value(), EditorValue::Text(v) if v.with_untracked(|v| v.ptr_eq(&ours)))
    };
    assert!(shared(&h), "the opened file goes to the editor as it is");
    h.ui.dispatch(InputEvent::Text("// typed\n".into()));
    h.settle();
    h.frames(1);
    assert!(shared(&h), "an edit's value is the editor's text");
    assert_eq!(h.error(), None);
}

#[test]
fn find_marks_every_match_steps_through_them_and_replaces() {
    let mut h = code();
    run(&mut h, "sample");
    run(&mut h, "find");
    h.type_text("find", "Overflow");
    h.frames(1);
    // Once in every ten lines.
    assert_eq!(h.text("find-count"), "10000 matches");
    h.key(Key::Enter);
    h.frames(1);
    assert_eq!(h.text("find-count"), "1 of 10000");
    // The match selected in the editor, and marked (in view, the current
    // one stronger).
    let warning = color(&h, tok::WARNING);
    let marked = {
        let root = editor(&mut h);
        let scene = h.ui.scene.borrow();
        let mut pending = vec![root];
        let mut found = 0;
        while let Some(id) = pending.pop() {
            if matches!(scene.kind(id), NodeKind::Rect(c) if *c == warning) {
                found += 1;
            }
            pending.extend(scene.children(id).iter().copied());
        }
        found
    };
    assert_eq!(marked, 1);
    h.key(Key::Enter);
    h.frames(1);
    assert_eq!(h.text("find-count"), "2 of 10000");
    h.type_text("replace", "Underflow");
    h.click("replace-one");
    h.frames(1);
    assert_eq!(h.text("find-count"), "2 of 9999");
    h.click("replace-all");
    h.frames(1);
    assert_eq!(h.error(), None);
    assert_eq!(h.text("find-count"), "No results");
    assert_eq!(h.text("status-message"), "Replaced 9999 matches");
    assert!(
        lines(&mut h)
            .iter()
            .all(|(text, _)| !text.contains("Overflow"))
    );
}

#[test]
fn tab_indents_the_selected_lines() {
    let mut h = code();
    run(&mut h, "sample");
    focus(&mut h);
    // The first two lines selected (to the third's start, which a
    // selection ending there leaves be), and indented.
    h.key_with(
        Key::ArrowDown,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    h.key_with(
        Key::ArrowDown,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    h.key(Key::Tab);
    let shown = lines(&mut h);
    assert!(
        shown
            .iter()
            .any(|(t, _)| t == "    /// A function, its docs and its body.")
    );
    assert!(
        shown
            .iter()
            .any(|(t, _)| t.starts_with("    pub fn compute_2("))
    );
    assert!(shown.iter().any(|(t, _)| t.starts_with("    let name")));
    assert!(!shown.iter().any(|(t, _)| t.starts_with("        let name")));
    h.key_with(
        Key::Tab,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert!(
        lines(&mut h)
            .iter()
            .any(|(t, _)| t == "/// A function, its docs and its body.")
    );
}
