//! What a code editor adds to a text area: lines coloured by a highlighter
//! (asked for the lines in view, and again only for those that change),
//! ranges marked behind the text, the current line, and Tab indenting.
use std::{cell::Cell, rc::Rc};
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    reactive::Signal,
    scene::{Color, NodeId, NodeKind},
    widgets::{EditorHandle, LineColors, Ui},
};

const COMMENT: Color = Color(0, 128, 0, 255);
const KEYWORD: Color = Color(0, 0, 255, 255);
const MATCH: Color = Color(255, 255, 0, 255);
const CURRENT: Color = Color(20, 20, 20, 255);

/// `fn` as a keyword, `/* … */` as a comment that runs on across lines.
fn highlight(line: &str, open: u32) -> (LineColors, u32) {
    let mut colors = Vec::new();
    let mut at = 0;
    let mut open = open == 1;
    while at < line.len() {
        if open {
            let end = line[at..].find("*/").map_or(line.len(), |i| at + i + 2);
            colors.push((at..end, COMMENT));
            open = end == line.len() && !line[at..].contains("*/");
            at = end;
        } else if line[at..].starts_with("/*") {
            open = true;
        } else if line[at..].starts_with("fn") {
            colors.push((at..at + 2, KEYWORD));
            at += 2;
        } else {
            at += line[at..].chars().next().unwrap().len_utf8();
        }
    }
    (colors, open as u32)
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
/// The colour each run of the line `text` shows in, by its text.
fn runs(ui: &Ui, root: NodeId, line: &str) -> Vec<(String, Color)> {
    let scene = ui.scene.borrow();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        match scene.kind(id) {
            NodeKind::RichText { text } if text.text() == line => {
                return text
                    .runs()
                    .iter()
                    .map(|run| (text.text()[run.range.clone()].to_string(), run.color))
                    .collect();
            }
            NodeKind::Text { text, color, .. } if &**text == line => {
                return vec![(line.to_string(), *color)];
            }
            _ => {}
        }
        pending.extend(scene.children(id).iter().copied());
    }
    panic!("no line {line:?} shows");
}
fn rects(ui: &Ui, root: NodeId, color: Color) -> usize {
    let scene = ui.scene.borrow();
    let mut pending = vec![root];
    let mut count = 0;
    while let Some(id) = pending.pop() {
        if matches!(scene.kind(id), NodeKind::Rect(c) if *c == color) {
            count += 1;
        }
        pending.extend(scene.children(id).iter().copied());
    }
    count
}
fn code(text: String, view: impl FnOnce(View) -> View) -> (Ui, EditorHandle, Signal<String>) {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal(text);
    let editor = ui.mount(view(
        text_area("Code", value.clone())
            .w(300.)
            .h(200.)
            .text_size(10.)
            .line_height(20.),
    ));
    ui.prepare_frame();
    // The editor in what mounted (it may be inside a column).
    let mut pending = vec![editor.node()];
    let mut found = None;
    while let Some(id) = pending.pop() {
        if ui.editor(id).is_some() {
            found = Some(id);
            break;
        }
        pending.extend(ui.scene.borrow().children(id).iter().copied());
    }
    ui.input.focus(&ui.scene, found);
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    (ui, editor, value)
}

#[test]
fn a_highlighter_colours_the_lines_in_view_and_again_only_one_that_changes() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let text: String = (0..5_000)
        .map(|i| match i {
            1 => "/* a comment".to_string(),
            2 => "running on fn".to_string(),
            3 => "ending */ fn after".to_string(),
            i => format!("fn line_{i}()"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (mut ui, editor, value) = code(text.clone(), |v| {
        v.highlighter(move |line, open| {
            counted.set(counted.get() + 1);
            highlight(line, open)
        })
    });
    // The caret starts at the end, so the lines there show: each line is
    // asked for once (those above for what they leave), and no more.
    assert!(calls.get() <= 5_000, "{}", calls.get());
    // At the top, the lines in view at most (what is kept of the lines
    // swept is bounded).
    let before = calls.get();
    editor.set_selection(0, 0);
    ui.prepare_frame();
    assert!(calls.get() - before <= 12, "{}", calls.get() - before);
    let first = runs(&ui, editor.node, "fn line_0()");
    assert_eq!(first[0], ("fn".into(), KEYWORD));
    assert_eq!(first[1].0, " line_0()");
    assert_ne!(first[1].1, KEYWORD);
    // What the comment's first line leaves open colours the next ones.
    assert_eq!(runs(&ui, editor.node, "running on fn")[0].1, COMMENT);
    let ending = runs(&ui, editor.node, "ending */ fn after");
    assert_eq!(ending[0], ("ending */".into(), COMMENT));
    assert_eq!(ending[2], ("fn".into(), KEYWORD));
    // Typing in line 6 asks again for it alone.
    let before = calls.get();
    let at = text.find("fn line_6()").unwrap() + 2;
    editor.set_selection(at, at);
    ui.dispatch(InputEvent::Text("x".into()));
    ui.prepare_frame();
    assert!(value.get().contains("fnx line_6()"));
    assert!(calls.get() - before <= 2, "{}", calls.get() - before);
    // Back at the end: the lines whose text did not change, and leave what
    // they did, are not asked for again.
    let before = calls.get();
    press(&mut ui, Key::End, command());
    runs(&ui, editor.node, "fn line_4999()");
    assert!(calls.get() - before <= 2, "{}", calls.get() - before);
}

#[test]
fn decorations_and_the_current_line_show_behind_the_text_in_view() {
    let text: String = (0..3_000).map(|i| format!("item {i}\n")).collect();
    let needle = "item 1";
    let matches: Vec<_> = text
        .match_indices(needle)
        .map(|(at, _)| (at..at + needle.len(), MATCH))
        .collect();
    assert!(matches.len() > 1000);
    let (ui, editor, _) = code(text.clone(), move |v| {
        v.decorations(move || matches.clone())
            .current_line(|| Some(CURRENT))
    });
    // At the top one line matches (`item 1`), and one is the caret's.
    editor.set_selection(0, 0);
    ui.prepare_frame();
    assert_eq!(rects(&ui, editor.node, MATCH), 1);
    assert_eq!(rects(&ui, editor.node, CURRENT), 1);
    // Around line 1500 every line does: those in view are drawn.
    let at = text.find("item 1500").unwrap();
    editor.set_selection(at, at);
    ui.prepare_frame();
    let shown = rects(&ui, editor.node, MATCH);
    assert!((5..=12).contains(&shown), "{shown}");
    assert_eq!(rects(&ui, editor.node, CURRENT), 1);
}

#[test]
fn tab_indents_and_outdents_lines_and_undo_takes_it_back() {
    let text = "a\nb\nc\nd".to_string();
    let (mut ui, editor, value) = code(text.clone(), |v| v.tab_indent("    "));
    editor.set_selection(0, 0);
    press(&mut ui, Key::Tab, Modifiers::default());
    assert_eq!(value.get(), "    a\nb\nc\nd");
    // Lines b to c selected: each indented, still selected.
    editor.set_selection(7, 10);
    press(&mut ui, Key::Tab, Modifiers::default());
    assert_eq!(value.get(), "    a\n    b\n    c\nd");
    let (anchor, focus) = editor.selection();
    assert_eq!((anchor.min(focus), anchor.max(focus)), (6, 17));
    press(
        &mut ui,
        Key::Tab,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(value.get(), "    a\nb\nc\nd");
    press(&mut ui, Key::Character("z".into()), command());
    assert_eq!(value.get(), "    a\n    b\n    c\nd");
    // The editor keeps focus: Tab did not move it on.
    assert_eq!(ui.focused_editor().map(|e| e.node), Some(editor.node));
    // Without an indent, Tab moves focus on, as in any field.
    let (mut ui, editor, value) = code(text.clone(), |v| {
        column().child(v).child(button().child(self::text("Next")))
    });
    press(&mut ui, Key::Tab, Modifiers::default());
    assert_eq!(value.get(), text);
    assert_ne!(ui.focused_editor().map(|e| e.node), Some(editor.node));
}
