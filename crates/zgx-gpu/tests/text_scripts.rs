//! Real text through real shaping, in the system's own fonts: emoji
//! sequences, mixed-direction runs with numbers, combining marks and CJK
//! line breaking, as an editor shows, walks, selects and deletes them. Every
//! platform's CI runs these (docs/validation.md, "Text in every script").
use std::{cell::RefCell, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    text_layout::{FontStyle, TextAffinity, TextLayout, TextPosition},
    widgets::Ui,
};
use zgx_gpu::text::ShapedText;

thread_local! {
    /// The system's fonts, loaded once a thread: loading them is slow.
    static FONTS: Rc<RefCell<cosmic_text::FontSystem>> =
        Rc::new(RefCell::new(cosmic_text::FontSystem::new()));
}

fn fonts() -> Rc<RefCell<cosmic_text::FontSystem>> {
    FONTS.with(Rc::clone)
}

/// `text` shaped at `size`, wrapped at `width` if given.
fn shaped(text: &str, size: f32, width: Option<f32>) -> Box<dyn TextLayout> {
    Box::new(ShapedText::with_font(
        &mut fonts().borrow_mut(),
        text,
        size,
        width,
        &FontStyle::default(),
    ))
}

/// Whether the system has one of `families`.
fn has_font(families: &[&str]) -> bool {
    let fonts = fonts();
    let fonts = fonts.borrow();
    fonts.db().faces().any(|face| {
        face.families
            .iter()
            .any(|(name, _)| families.contains(&name.as_str()))
    })
}

/// The color emoji fonts each platform draws emoji with.
const EMOJI_FONTS: &[&str] = &["Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji"];

/// A focused text area holding `text`, shaped as an app's are, the caret at
/// `caret`.
struct Field {
    ui: Ui,
    editor: zgx::widgets::EditorHandle,
}

impl Field {
    fn new(text: &str, caret: usize) -> Self {
        let mut ui = Ui::new(600., 200.);
        let fonts = fonts();
        ui.scene.borrow_mut().set_font_text_shaper(
            move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
                Box::new(ShapedText::with_font(
                    &mut fonts.borrow_mut(),
                    text,
                    size,
                    width,
                    font,
                ))
            },
        );
        let view = ui.mount(
            text_area("Text", ui.signal(text.to_owned()))
                .size(560., 160.)
                .text_size(20.),
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor.editor.borrow_mut().set_selection(caret, caret);
        editor.refresh();
        ui.prepare_frame();
        Field { ui, editor }
    }
    fn press(&mut self, key: Key, shift: bool) {
        self.ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
            repeat: false,
        });
        self.ui.prepare_frame();
    }
    fn text(&self) -> String {
        self.editor.editor.borrow().text().into_owned()
    }
    fn selection(&self) -> (usize, usize) {
        let selection = self.editor.editor.borrow().selection();
        (selection.anchor, selection.focus)
    }
    fn caret_x(&self) -> f32 {
        self.ui.scene.borrow().bounds(self.editor.caret).x
    }
}

/// ZWJ sequences, flags (regional and tag), keycaps and skin tones.
const EMOJI: &[(&str, &str)] = &[
    ("family", "👨\u{200d}👩\u{200d}👧\u{200d}👦"),
    ("profession and skin tone", "👩🏽\u{200d}💻"),
    ("flag", "🇯🇵"),
    (
        "tag flag",
        "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
    ),
    ("keycap", "1\u{fe0f}\u{20e3}"),
    ("skin tone", "👍🏽"),
    ("rainbow flag", "🏳\u{fe0f}\u{200d}🌈"),
];

#[test]
fn an_emoji_sequence_is_one_character_to_the_arrows_selection_and_deleting() {
    for (name, emoji) in EMOJI {
        let text = format!("a{emoji}b");
        let after = 1 + emoji.len();
        let mut field = Field::new(&text, 1);
        field.press(Key::ArrowRight, false);
        assert_eq!(
            field.selection(),
            (after, after),
            "{name}: Right passes it whole"
        );
        field.press(Key::ArrowLeft, false);
        assert_eq!(field.selection(), (1, 1), "{name}: Left comes back whole");
        field.press(Key::ArrowRight, true);
        assert_eq!(
            field.editor.copy(),
            *emoji,
            "{name}: Shift+Right selects it whole"
        );
        // Backspace after it and Delete before it take it whole.
        let mut field = Field::new(&text, after);
        field.press(Key::Backspace, false);
        assert_eq!(field.text(), "ab", "{name}: Backspace");
        assert_eq!(field.selection(), (1, 1));
        let mut field = Field::new(&text, 1);
        field.press(Key::Delete, false);
        assert_eq!(field.text(), "ab", "{name}: Delete");
    }
    // Two flags side by side are two characters, never a pair split across.
    let flags = "🇯🇵🇫🇷";
    let mut field = Field::new(flags, 0);
    field.press(Key::ArrowRight, false);
    assert_eq!(field.selection(), (8, 8), "one flag");
    field.press(Key::Backspace, false);
    assert_eq!(field.text(), "🇫🇷", "the first flag goes, whole");
}

#[test]
fn an_emoji_sequence_draws_as_one_glyph_and_takes_no_caret_inside() {
    let color = has_font(EMOJI_FONTS);
    // macOS and Windows always have theirs; a Linux may have none, and then
    // draws a box for each code point.
    assert!(color || !cfg!(any(target_os = "macos", windows)), "no color emoji font");
    let size = 32.;
    for (name, emoji) in EMOJI {
        let layout = shaped(emoji, size, None);
        let (start, end) = (layout.caret(0).x, layout.caret(emoji.len()).x);
        let width = (end - start).abs();
        assert!(width > 0., "{name}: drawn");
        if color {
            // One glyph about an em wide, not one for each code point.
            assert!(width < size * 1.6, "{name}: {width} wide at {size}");
        }
        // Wherever a click lands on it, the caret goes before or after it.
        let (_, height) = layout.size();
        let mut x = start.min(end) - 2.;
        while x <= start.max(end) + 2. {
            let hit = layout.hit_test(x, height / 2.);
            assert!(
                hit == 0 || hit == emoji.len(),
                "{name}: a click at {x} put it at {hit}"
            );
            x += 1.;
        }
    }
}

/// Mixed-direction lines: Hebrew and Arabic runs, numbers inside them,
/// punctuation where the runs meet.
const BIDI: &[&str] = &[
    "abc אבג 123 def",
    "שלום 123 עולם",
    "מחיר: 45.90 ₪ היום",
    "مرحبا 2024 بكم",
    "abc مرحبا، 45.6% done",
    "(אבג) [123] xyz",
];

/// Every place a caret can be in `layout`'s first line: the x of each
/// grapheme boundary, on either side where runs meet.
fn caret_places(layout: &dyn TextLayout, text: &str) -> Vec<f32> {
    let mut places = Vec::new();
    for (offset, _) in text.grapheme_indices(true).chain([(text.len(), "")]) {
        for affinity in [TextAffinity::Before, TextAffinity::After] {
            let caret = layout.caret_position(TextPosition {
                byte_offset: offset,
                affinity,
            });
            places.push(caret.x);
        }
    }
    places
}

#[test]
fn a_click_in_mixed_direction_text_puts_the_caret_at_the_nearest_place() {
    for text in BIDI {
        let layout = shaped(text, 20., None);
        let (width, height) = layout.size();
        let places = caret_places(layout.as_ref(), text);
        let mut x = 0.;
        while x <= width {
            let hit = layout.hit_position(x, height / 2.);
            let caret = layout.caret_position(hit).x;
            let nearest = places
                .iter()
                .map(|place| (place - x).abs())
                .fold(f32::INFINITY, f32::min);
            assert!(
                (caret - x).abs() <= nearest + 1.,
                "{text:?}: a click at {x:.1} put the caret at {caret:.1} ({hit:?}), \
                 {nearest:.1} from the nearest place"
            );
            x += 1.5;
        }
    }
}

#[test]
fn the_arrows_walk_numbers_inside_right_to_left_text_in_the_order_they_show() {
    for text in BIDI {
        let graphemes = text.graphemes(true).count();
        for (key, back) in [
            (Key::ArrowRight, Key::ArrowLeft),
            (Key::ArrowLeft, Key::ArrowRight),
        ] {
            // From the visual edge the key moves away from.
            let mut field = Field::new(text, 0);
            for _ in 0..graphemes + 2 {
                field.press(back.clone(), false);
            }
            let mut xs = vec![field.caret_x()];
            let mut seen = vec![field.selection().1];
            for _ in 0..graphemes + 2 {
                field.press(key.clone(), false);
                xs.push(field.caret_x());
                seen.push(field.selection().1);
            }
            // Each press moves the caret the key's way until the far edge
            // (two places share a spot where runs meet: a press may stay),
            // and then it stays there.
            let rightward = key == Key::ArrowRight;
            let moved_back = xs.windows(2).any(|w| {
                if rightward {
                    w[1] < w[0] - 0.5
                } else {
                    w[1] > w[0] + 0.5
                }
            });
            assert!(!moved_back, "{text:?} {key:?}: {xs:?} at {seen:?}");
            // Every place a caret can be is visited on the way (where runs
            // meet, two offsets share one place: one of them is enough).
            let layout = shaped(text, 20., None);
            let start = xs[0]
                - layout
                    .caret_position(TextPosition {
                        byte_offset: seen[0],
                        affinity: TextAffinity::After,
                    })
                    .x;
            for place in caret_places(layout.as_ref(), text) {
                assert!(
                    xs.iter().any(|x| (x - start - place).abs() < 0.5),
                    "{text:?} {key:?} never stops at {place}: {xs:?} at {seen:?}"
                );
            }
        }
    }
}

#[test]
fn a_selection_in_mixed_direction_text_is_one_logical_range_drawn_where_it_shows() {
    for text in BIDI {
        let layout = shaped(text, 20., None);
        let bounds: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        for (i, &start) in bounds.iter().enumerate() {
            for &end in &bounds[i + 1..] {
                let rects = layout.selection(start..end);
                assert!(!rects.is_empty(), "{text:?} {start}..{end} draws");
                // Pieces never overlap, and together are as wide as the
                // graphemes they hold.
                let mut spans: Vec<(f32, f32)> =
                    rects.iter().map(|r| (r.x, r.x + r.width)).collect();
                spans.sort_by(|a, b| a.0.total_cmp(&b.0));
                for pair in spans.windows(2) {
                    assert!(
                        pair[1].0 >= pair[0].1 - 0.5,
                        "{text:?} {start}..{end}: {spans:?}"
                    );
                }
                let drawn: f32 = spans.iter().map(|(a, b)| b - a).sum();
                let held: f32 = bounds
                    .windows(2)
                    .filter(|w| w[0] >= start && w[1] <= end)
                    .map(|w| {
                        let rects = layout.selection(w[0]..w[1]);
                        rects.iter().map(|r| r.width).sum::<f32>()
                    })
                    .sum();
                assert!(
                    (drawn - held).abs() <= 1.,
                    "{text:?} {start}..{end}: drawn {drawn}, its graphemes {held}"
                );
            }
        }
    }
}

/// Letters with combining marks: decomposed Latin, Vietnamese, Hebrew
/// points, Devanagari and Thai.
const MARKS: &[(&str, &str)] = &[
    ("decomposed Latin", "cafe\u{301} nai\u{308}ve"),
    ("Vietnamese", "Tie\u{302}\u{301}ng Vie\u{323}\u{302}t"),
    ("Hebrew points", "שָׁלוֹם"),
    ("Devanagari", "नमस्ते"),
    ("Thai", "ที่นี่"),
];

#[test]
fn letters_with_combining_marks_walk_and_select_as_one_and_select_in_one_piece() {
    for (name, text) in MARKS {
        let bounds: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        // Right from the start stops at every grapheme boundary and no other
        // (a left-to-right word; Hebrew's goes the other way).
        let rtl = text.chars().any(|c| ('\u{590}'..='\u{5ff}').contains(&c));
        let key = if rtl { Key::ArrowLeft } else { Key::ArrowRight };
        let mut field = Field::new(text, 0);
        let mut seen = vec![0];
        for _ in 1..bounds.len() {
            field.press(key.clone(), false);
            seen.push(field.selection().1);
        }
        assert_eq!(
            seen, bounds,
            "{name}: the caret stops between graphemes only"
        );
        // The whole of it selects in one rounded piece a word, its marks no
        // seam: one rectangle each word.
        let layout = shaped(text, 20., None);
        for word in text.split(' ') {
            let start = word.as_ptr() as usize - text.as_ptr() as usize;
            let rects = layout.selection(start..start + word.len());
            assert_eq!(rects.len(), 1, "{name}: {word:?} in one piece: {rects:?}");
            // Each grapheme's piece is as tall as the line and not empty.
            for w in bounds
                .windows(2)
                .filter(|w| w[0] >= start && w[1] <= start + word.len())
            {
                let piece = layout.selection(w[0]..w[1]);
                assert!(
                    piece.iter().all(|r| r.width > 0.5),
                    "{name}: {:?} {piece:?}",
                    &text[w[0]..w[1]]
                );
            }
        }
    }
}

#[test]
fn backspace_takes_a_mark_or_the_letter_as_the_platform_does() {
    // macOS (as `NSTextView`): an accent goes with its letter; Hebrew's,
    // Devanagari's and Thai's marks go one at a time. Elsewhere (as
    // Chromium): one code point.
    let macos = cfg!(target_os = "macos");
    for (text, mac, other) in [
        ("cafe\u{301}", "caf", "cafe"),
        ("שָׁ", "שָ", "שָ"),
        ("नमस्ते", "नमस्त", "नमस्त"),
        ("ที่", "ที", "ที"),
    ] {
        let mut field = Field::new(text, text.len());
        field.press(Key::Backspace, false);
        let left = if macos { mac } else { other };
        assert_eq!(field.text(), left, "{text:?}");
        // Delete forward always takes the whole grapheme.
        let mut field = Field::new(text, 0);
        let first = text.graphemes(true).next().unwrap();
        field.press(Key::Delete, false);
        assert_eq!(field.text(), text[first.len()..], "{text:?}: Delete");
    }
}

/// What may not begin a line in Chinese and Japanese (closing punctuation,
/// small kana, iteration and length marks) and what may not end one
/// (opening punctuation).
const NO_LINE_START: &str =
    "。、，．！？：；」』）〕】〉》”’々ーぁぃぅぇぉっゃゅょァィゥェォッャュョ";
const NO_LINE_END: &str = "「『（〔【〈《“‘";

#[test]
fn chinese_and_japanese_never_break_before_closing_or_after_opening_punctuation() {
    for text in [
        "これは日本語の文章です。「引用」も、句読点も正しく折り返されるべきです。ちょっと待ってください！",
        "这是一个中文句子，包含标点符号。“引号”也应该正确换行！（括号）也一样。",
        "日本語とEnglishが混ざった文、です。カタカナのテキストもチェックする。",
    ] {
        let size = 20.;
        // From five ems: narrower, what may not break (a closing mark, an
        // opening quote and the characters held to them) can be wider than
        // the line, which then breaks inside it, as text that can't fit must.
        for ems in 5..=14 {
            let layout = shaped(text, size, Some(size * ems as f32 + 1.));
            let mut lines = Vec::new();
            let mut at = TextPosition {
                byte_offset: 0,
                affinity: TextAffinity::After,
            };
            loop {
                let (start, end) = layout.visual_line_range(at);
                lines.push(&text[start.byte_offset..end.byte_offset]);
                if end.byte_offset >= text.len() {
                    break;
                }
                at = TextPosition {
                    byte_offset: end.byte_offset,
                    affinity: TextAffinity::After,
                };
            }
            assert_eq!(lines.concat(), *text, "{ems} em: every line once");
            for pair in lines.windows(2) {
                let first = pair[1].chars().next().unwrap();
                let last = pair[0].chars().next_back().unwrap();
                assert!(
                    !NO_LINE_START.contains(first),
                    "{ems} em: a line starts with {first:?}: {lines:?}"
                );
                assert!(
                    !NO_LINE_END.contains(last),
                    "{ems} em: a line ends with {last:?}: {lines:?}"
                );
            }
        }
    }
}
