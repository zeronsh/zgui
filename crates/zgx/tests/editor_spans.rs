//! Styled highlight spans (`View::span_highlighter`): a weight or italics
//! shapes its line (its width, carets and hits follow what shows); strikes
//! and washes paint; overlapping spans merge.
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    rich_text::{RichText, TextRun},
    scene::{Color, NodeId, NodeKind},
    text_layout::{FallbackTextLayout, LetterSpacing},
    widgets::{EditorHandle, LineSpans, SpanStyle, Ui},
};

const CODE: Color = Color(200, 80, 80, 255);
const WASH: Color = Color(40, 40, 40, 255);

/// Between the first two of `marker` in `line`, markers included.
fn between(line: &str, marker: &str) -> Option<std::ops::Range<usize>> {
    let start = line.find(marker)?;
    let end = line[start + marker.len()..].find(marker)? + start + 2 * marker.len();
    Some(start..end)
}
/// A little Markdown: `**bold**`, `_italic_`, `~~struck~~` and `` `code` ``
/// in a wash.
fn markdown(line: &str, _: u32) -> (LineSpans, u32) {
    let mut spans = Vec::new();
    if let Some(range) = between(line, "**") {
        spans.push((range, SpanStyle::default().weight(600)));
    }
    if let Some(range) = between(line, "_") {
        spans.push((range, SpanStyle::default().italic()));
    }
    if let Some(range) = between(line, "~~") {
        spans.push((range, SpanStyle::default().strike()));
    }
    if let Some(range) = between(line, "`") {
        spans.push((range, SpanStyle::from(CODE).background(WASH)));
    }
    (spans, 0)
}
/// An editor of `text` styled by `markdown` (or not), its text 10 points
/// from its left, where a bold character is half again as wide (the
/// fallback shaper's cells are as wide in any weight).
fn editor(text: &str, styled: bool) -> (Ui, EditorHandle) {
    let mut ui = Ui::new(400., 300.);
    ui.scene.borrow_mut().set_rich_text_shaper(|rich, width| {
        let runs: Vec<TextRun> = rich
            .runs()
            .iter()
            .cloned()
            .map(|mut run| {
                if run.font.weight >= 600 {
                    run.font.letter_spacing = LetterSpacing::px(3.);
                }
                run
            })
            .collect();
        let rich = RichText::new(rich.text(), runs).unwrap();
        Box::new(FallbackTextLayout::with_rich(&rich, width))
    });
    let area = text_area("Draft", ui.signal(text.to_owned()))
        .size(300., 160.)
        .p(10.)
        .text_size(10.)
        .line_height(20.);
    let view = ui.mount(if styled {
        area.span_highlighter(markdown)
    } else {
        area
    });
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.set_selection(0, 0);
    ui.prepare_frame();
    (ui, editor)
}
fn caret_x(ui: &Ui, editor: &EditorHandle, at: usize) -> f32 {
    editor.set_selection(at, at);
    ui.prepare_frame();
    ui.scene.borrow().bounds(editor.caret).x
}
fn click(ui: &mut Ui, x: f32, y: f32) {
    for event in [
        InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        },
        InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        },
    ] {
        ui.dispatch_with_modifiers(event, Modifiers::default());
    }
    ui.prepare_frame();
}
/// The runs line `line` shows in.
fn runs(ui: &Ui, root: NodeId, line: &str) -> Vec<(String, TextRun)> {
    let scene = ui.scene.borrow();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if let NodeKind::RichText { text } = scene.kind(id)
            && text.text() == line
        {
            return text
                .runs()
                .iter()
                .map(|run| (text.text()[run.range.clone()].to_string(), run.clone()))
                .collect();
        }
        pending.extend(scene.children(id).iter().copied());
    }
    panic!("no line {line:?} shows");
}

#[test]
fn a_weight_shapes_its_line_so_carets_and_hits_follow_it() {
    let text = "ab **cd** ef\nplain";
    let (plain_ui, plain) = editor(text, false);
    let (mut ui, styled) = editor(text, true);
    let origin = caret_x(&ui, &styled, 0);
    assert_eq!(caret_x(&plain_ui, &plain, 0), origin);
    // Before `e`: `ab ` (3 cells of 6), `**cd**` bold (6 of 9), a space.
    assert_eq!(caret_x(&plain_ui, &plain, 10) - origin, 60.);
    assert_eq!(caret_x(&ui, &styled, 10) - origin, 78.);
    // At its end, past the bold run.
    assert_eq!(caret_x(&ui, &styled, 12) - origin, 90.);
    // A line without a weight is shaped as before.
    assert_eq!(caret_x(&ui, &styled, 18) - origin, 30.);
    // A click lands where the wider letters show: 50 points in is past
    // the middle of `d` (45 to 54), not on the second `*` after it.
    click(&mut ui, origin + 50., 20.);
    assert_eq!(styled.selection(), (7, 7));
    let (mut plain_ui, plain) = (plain_ui, plain);
    click(&mut plain_ui, origin + 50., 20.);
    assert_eq!(plain.selection(), (8, 8));
    // What shows is shaped the same: the bold run is weight 600.
    let shown = runs(&ui, styled.node, "ab **cd** ef");
    assert_eq!(shown[1].0, "**cd**");
    assert_eq!(shown[1].1.font.weight, 600);
    assert_eq!(shown[0].1.font.weight, 400);
}

#[test]
fn italics_strikes_and_washes_show_and_overlapping_spans_merge() {
    let (line, nested) = ("_it_ ~~old~~ `code`", "**a `b` c**");
    let (ui, editor) = editor(&format!("{line}\n{nested}"), true);
    let mut shown = runs(&ui, editor.node, line);
    shown.extend(runs(&ui, editor.node, nested));
    let run = |text: &str| {
        shown
            .iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| {
                panic!(
                    "no run {text:?} in {:?}",
                    shown.iter().map(|(t, _)| t).collect::<Vec<_>>()
                )
            })
            .1
            .clone()
    };
    assert!(run("_it_").font.italic);
    assert!(run("~~old~~").strikethrough.is_some());
    assert!(run(" ").strikethrough.is_none());
    let code = run("`code`");
    assert_eq!((code.color, code.background), (CODE, Some(WASH)));
    // Code inside bold: its colour and wash, the bold's weight.
    assert_eq!(run("**a ").font.weight, 600);
    let inner = run("`b`");
    assert_eq!(inner.font.weight, 600);
    assert_eq!((inner.color, inner.background), (CODE, Some(WASH)));
    assert_eq!(run(" c**").background, None);
}
