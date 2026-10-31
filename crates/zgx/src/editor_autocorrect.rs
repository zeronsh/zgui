//! The system's substitutions as a person types (`View::autocorrect`,
//! `View::smart_punctuation`), as AppKit's text views make them: curly
//! quotes and dashes for straight ones, the person's text replacements, and
//! a misspelled word corrected as it ends. Each is an undo step of its own,
//! so undo puts back what was typed, and what was put back stays.
use crate::{
    input::{EventPhase, InputEvent},
    platform::{PlatformService, Substitutions},
    widgets::{EditorHandle, Ui},
};
use std::{cell::RefCell, ops::Range, rc::Rc};

/// What to substitute: the system's punctuation and replacements, and
/// whether to correct words.
#[derive(Clone, Copy)]
pub(crate) struct Asked {
    pub(crate) kinds: Substitutions,
    pub(crate) corrections: bool,
}

pub(crate) fn mount(
    ui: &mut Ui,
    editor: EditorHandle,
    platform: Rc<PlatformService>,
    asked: Asked,
) {
    // The last substitution: where it starts, what was there and what went
    // there. Undone, it is not made again.
    let last: RefCell<Option<(usize, String)>> = RefCell::default();
    // After the editor's own listener: the typed text is in.
    ui.on_event(editor.node, false, move |cx| {
        if cx.phase != EventPhase::Target || cx.default_prevented() {
            return;
        }
        let (InputEvent::Text(typed) | InputEvent::ImeCommit(typed)) = &cx.event else {
            return;
        };
        substitute(&editor, &platform, asked, typed, &last);
    });
}

fn substitute(
    editor: &EditorHandle,
    platform: &PlatformService,
    asked: Asked,
    typed: &str,
    last: &RefCell<Option<(usize, String)>>,
) {
    let (anchor, caret) = editor.selection();
    let text = editor.text();
    let Some(start) = caret.checked_sub(typed.len()) else {
        return;
    };
    if anchor != caret || typed.is_empty() || text.get(start..caret) != Some(typed) {
        return;
    }
    // The paragraph up to the caret: what the system reads for context.
    let from = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let context = &text[from..caret];
    let typed_at = start - from;
    // Only what the typed text is (a quote) or finishes (a dash, a
    // replacement, once the next character comes): one ending earlier was
    // left as it is, or put back, and one still being typed may go on.
    let mut edits: Vec<(Range<usize>, String)> = platform
        .0
        .substitutions(context, asked.kinds)
        .into_iter()
        .filter(|(range, _)| {
            range.end == typed_at || (range.start >= typed_at && range.end <= context.len())
        })
        .collect();
    if asked.corrections && typed.starts_with(|c: char| !is_word(c)) {
        // The word the typed text ends, without the quotes around it.
        let quote = ['\'', '’'];
        let word_end = context[..typed_at].trim_end_matches(quote).len();
        let word = context[..word_end]
            .char_indices()
            .rev()
            .take_while(|&(_, c)| is_word(c))
            .last()
            .map_or(word_end, |(i, _)| i);
        let word = word_end - context[word..word_end].trim_start_matches(quote).len();
        if word < word_end
            && !edits
                .iter()
                .any(|(range, _)| range.start < word_end && range.end > word)
            && let Some(correction) = platform.0.correction(context, word..word_end)
            && correction != context[word..word_end]
        {
            edits.push((word..word_end, correction));
        }
    }
    let undone = last.borrow().clone();
    edits.retain(|(range, _)| {
        undone.as_ref().is_none_or(|(at, was)| {
            *at != range.start + from || context.get(range.clone()) != Some(was.as_str())
        })
    });
    if edits.is_empty() {
        return;
    }
    // From the end, so the ranges before each stay where they are.
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in &edits {
        let range = range.start + from..range.end + from;
        *last.borrow_mut() = Some((range.start, text[range.clone()].to_owned()));
        editor.substitute(range, replacement);
    }
}

/// Part of a word: what a correction reads back to.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '’'
}
