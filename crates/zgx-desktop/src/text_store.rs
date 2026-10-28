//! The document an input method on Windows (the Text Services Framework)
//! reads and edits through `ITextStoreACP2` (`tsf.rs`): the focused editor's
//! committed text, with its composition in place of the selection while one
//! is open, in UTF-16 offsets as Windows counts. Kept apart from the COM
//! interface so its logic is tested on every platform.
//!
//! Text a text service inserts without composing (the emoji panel, dictation,
//! the touch keyboard) is typed into the editor. Text an input method
//! composes is the editor's preedit until it ends the composition, which
//! commits what is left, as `InputEvent::ImeCommit` does: the editor's text
//! changes only then, as on every other platform.
use std::ops::Range;
use zgx::{text_edit::ClauseStyle, widgets::EditorHandle};

/// What the store last told the input method about, to say what changed
/// since: the editor's text revision, selection and preedit.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    revision: u64,
    selection: (usize, usize),
    preedit: Option<(String, Option<(usize, usize)>)>,
}

/// What the input method is to be told after an edit it didn't make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// The document's text from `start` changed: what was to `old_end` is to
    /// `new_end` (UTF-16 offsets).
    Text {
        start: usize,
        old_end: usize,
        new_end: usize,
    },
    /// The selection moved.
    Selection,
}

pub(crate) struct TextStore {
    editor: Option<EditorHandle>,
    /// The composition's range in the document, while one is open.
    composition: Option<Range<usize>>,
    /// The composition's text, the store's own: an editor drops its preedit
    /// when it loses focus, and the composition is still committed into it.
    composing: String,
    seen: Option<Seen>,
}

impl TextStore {
    pub(crate) fn new() -> Self {
        Self {
            editor: None,
            composition: None,
            composing: String::new(),
            seen: None,
        }
    }
    /// The editor the document is now; `None` while no editor that takes an
    /// input method has focus. A composition still open is committed into
    /// the editor it began in, as IMM32's is, not carried into the next.
    pub(crate) fn focus(&mut self, editor: Option<EditorHandle>) {
        if self.editor.as_ref().map(|e| e.node) != editor.as_ref().map(|e| e.node) {
            self.end_composition();
            self.editor = editor;
            self.seen = self.editor.as_ref().map(seen);
        }
    }
    pub(crate) fn editor(&self) -> Option<&EditorHandle> {
        self.editor.as_ref()
    }
    pub(crate) fn is_composing(&self) -> bool {
        self.composition.is_some()
    }
    /// The document's pieces: the committed text before the selection (its
    /// UTF-16 length and byte end), the preedit if any (in place of the
    /// selection), and the committed text after.
    fn parts(&self, editor: &EditorHandle) -> Parts {
        let e = editor.editor.borrow();
        match e.preedit() {
            Some(preedit) => {
                let range = e.selection().range();
                Parts {
                    before: e.byte_to_utf16(range.start),
                    cut: e.byte_to_utf16(range.end) - e.byte_to_utf16(range.start),
                    preedit: Some(preedit.text.encode_utf16().collect()),
                    len: e.utf16_len(),
                }
            }
            None => Parts {
                before: 0,
                cut: 0,
                preedit: None,
                len: e.utf16_len(),
            },
        }
    }
    /// The document's length (`GetEndACP`).
    pub(crate) fn len(&self) -> usize {
        let Some(editor) = &self.editor else {
            return 0;
        };
        let parts = self.parts(editor);
        parts.len - parts.cut + parts.preedit.as_ref().map_or(0, Vec::len)
    }
    /// The document's text from `start` to `end` (`GetText`), cut to it.
    pub(crate) fn text(&self, start: usize, end: usize) -> Vec<u16> {
        let Some(editor) = &self.editor else {
            return Vec::new();
        };
        let end = end.min(self.len());
        let start = start.min(end);
        let parts = self.parts(editor);
        let e = editor.editor.borrow();
        let committed = |from: usize, to: usize| -> Vec<u16> {
            let (from, to) = (e.utf16_to_byte(from), e.utf16_to_byte(to));
            e.slice(from..to).encode_utf16().collect()
        };
        match &parts.preedit {
            None => committed(start, end),
            Some(preedit) => {
                // Committed text before the composition, the composition,
                // then committed text after it (past what it replaces).
                let (p0, p1) = (parts.before, parts.before + preedit.len());
                let shift = |at: usize| at - preedit.len() + parts.cut;
                let mut out = Vec::new();
                if start < p0 {
                    out.extend(committed(start, end.min(p0)));
                }
                if end > p0 && start < p1 {
                    out.extend_from_slice(&preedit[start.max(p0) - p0..end.min(p1) - p0]);
                }
                if end > p1 {
                    out.extend(committed(shift(start.max(p1)), shift(end)));
                }
                out
            }
        }
    }
    /// Where the document's `start..end` shows (`GetTextExt`), in the
    /// window's points: around it across every line it runs on, its caret's
    /// place for an empty range. `None` with no editor, or where its text
    /// has no lines to ask (a password's).
    pub(crate) fn area(&self, start: usize, end: usize) -> Option<zgx::scene::Rect> {
        let editor = self.editor.as_ref()?;
        let end = end.min(self.len());
        let start = start.min(end);
        let (a, b) = (shown_byte(editor, start), shown_byte(editor, end));
        editor.text_area(a..b)
    }
    /// The selection (`GetSelection`): the caret or selection in the
    /// composition while there is one, else the editor's; its start, its end,
    /// and whether the caret is at the start.
    pub(crate) fn selection(&self) -> (usize, usize, bool) {
        let Some(editor) = &self.editor else {
            return (0, 0, false);
        };
        let e = editor.editor.borrow();
        match e.preedit() {
            Some(preedit) => {
                let before = e.byte_to_utf16(e.selection().range().start);
                let (a, b) = preedit
                    .cursor
                    .unwrap_or((preedit.text.len(), preedit.text.len()));
                let unit = |at: usize| {
                    preedit.text[..at.min(preedit.text.len())]
                        .encode_utf16()
                        .count()
                };
                (before + unit(a), before + unit(b), false)
            }
            None => {
                let s = e.selection();
                let (anchor, focus) = (e.byte_to_utf16(s.anchor), e.byte_to_utf16(s.focus));
                (anchor.min(focus), anchor.max(focus), focus < anchor)
            }
        }
    }
    /// Select `start..end` (`SetSelection`): within the composition, its
    /// caret; else the editor's selection.
    pub(crate) fn set_selection(&mut self, start: usize, end: usize, caret_at_start: bool) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        let (start, end) = (start.min(end), start.max(end));
        let preedit = editor.editor.borrow().preedit().cloned();
        match preedit {
            Some(preedit) => {
                let before = self.parts(&editor).before;
                let units: Vec<u16> = preedit.text.encode_utf16().collect();
                let byte = |at: usize| {
                    String::from_utf16_lossy(&units[..at.saturating_sub(before).min(units.len())])
                        .len()
                };
                let clauses = editor.editor.borrow().preedit_clauses();
                editor.set_composition(&preedit.text, Some((byte(start), byte(end))), &clauses);
            }
            None => {
                let (from, to) = {
                    let e = editor.editor.borrow();
                    (e.utf16_to_byte(start), e.utf16_to_byte(end))
                };
                if caret_at_start {
                    editor.set_selection(to, from);
                } else {
                    editor.set_selection(from, to);
                }
            }
        }
        self.seen = Some(seen(&editor));
    }
    /// Replace the document's `start..end` with `text` (`SetText`, and
    /// `InsertTextAtSelection` at the selection): within the open
    /// composition, its text; else typed into the editor, or replacing the
    /// committed text there. The range the text is at after, in the document.
    pub(crate) fn replace(
        &mut self,
        start: usize,
        end: usize,
        text: &[u16],
    ) -> Option<Range<usize>> {
        let editor = self.editor.clone()?;
        let (start, end) = (start.min(end), start.max(end).min(self.len()));
        let new = String::from_utf16_lossy(text);
        if let Some(composition) = self.composition.clone() {
            // Within the composition: its text, the caret after the edit.
            let mut units: Vec<u16> = editor
                .editor
                .borrow()
                .preedit()
                .map(|p| p.text.encode_utf16().collect())
                .unwrap_or_default();
            let (a, b) = (
                start.clamp(composition.start, composition.end) - composition.start,
                end.clamp(composition.start, composition.end) - composition.start,
            );
            units.splice(a..b, text.iter().copied());
            let preedit = String::from_utf16_lossy(&units);
            let caret = String::from_utf16_lossy(&units[..a + text.len()]).len();
            editor.set_composition(&preedit, Some((caret, caret)), &[]);
            self.composing = preedit;
            self.composition = Some(composition.start..composition.start + units.len());
            self.seen = Some(seen(&editor));
            return Some(composition.start + a..composition.start + a + text.len());
        }
        let (from, to, selection) = {
            let e = editor.editor.borrow();
            (
                e.utf16_to_byte(start),
                e.utf16_to_byte(end),
                e.selection().range(),
            )
        };
        if from == selection.start && to == selection.end {
            editor.insert_text(&new);
        } else {
            editor.replace_text(from..to, &new);
        }
        self.seen = Some(seen(&editor));
        Some(start..start + text.len())
    }
    /// The input method opened a composition over `range` (`OnStartComposition`).
    pub(crate) fn start_composition(&mut self, range: Range<usize>) {
        if self.editor.is_some() {
            self.composition = Some(range.start..range.start);
            self.composing.clear();
        }
    }
    /// The input method marked its composition's clauses (its display
    /// attributes, read after each of its edits): document ranges and how
    /// each is marked. Ranges outside the composition are left out.
    pub(crate) fn mark_clauses(&mut self, clauses: &[(Range<usize>, ClauseStyle)]) {
        let (Some(editor), Some(composition)) = (self.editor.clone(), self.composition.clone())
        else {
            return;
        };
        let Some(preedit) = editor.editor.borrow().preedit().cloned() else {
            return;
        };
        let units: Vec<u16> = preedit.text.encode_utf16().collect();
        let byte = |at: usize| {
            String::from_utf16_lossy(
                &units[..at.saturating_sub(composition.start).min(units.len())],
            )
            .len()
        };
        let clauses: Vec<_> = clauses
            .iter()
            .map(|(range, style)| (byte(range.start)..byte(range.end), *style))
            .filter(|(range, _)| !range.is_empty())
            .collect();
        if editor.editor.borrow().preedit_clauses() == clauses {
            return;
        }
        editor.set_composition(&preedit.text, preedit.cursor, &clauses);
        self.seen = Some(seen(&editor));
    }
    /// The input method ended its composition (`OnEndComposition`): what is
    /// left of it is committed, as typed.
    pub(crate) fn end_composition(&mut self) {
        let (Some(editor), Some(_)) = (self.editor.clone(), self.composition.take()) else {
            return;
        };
        match std::mem::take(&mut self.composing) {
            text if !text.is_empty() => {
                editor.commit_composition(&text);
            }
            _ => editor.cancel_composition(),
        }
        self.seen = Some(seen(&editor));
    }
    /// What changed in the editor since the input method last heard, from
    /// edits it didn't make (typing, the app writing its value, undo): to
    /// tell it (`OnTextChange`, `OnSelectionChange`). The whole text, once,
    /// for a text change: Windows takes a wider range than changed.
    pub(crate) fn changes(&mut self, old_len: usize) -> Vec<Change> {
        let Some(editor) = &self.editor else {
            return Vec::new();
        };
        // Nothing changed (every dispatch but those that edit): compared in
        // place, nothing made.
        if self.seen.as_ref().is_some_and(|seen| seen.is(editor)) {
            return Vec::new();
        }
        let now = seen(editor);
        let Some(before) = self.seen.replace(now.clone()) else {
            return Vec::new();
        };
        let mut changes = Vec::new();
        if before.revision != now.revision || before.preedit != now.preedit {
            changes.push(Change::Text {
                start: 0,
                old_end: old_len,
                new_end: self.len(),
            });
        }
        if before.selection != now.selection || before.preedit != now.preedit {
            changes.push(Change::Selection);
        }
        changes
    }
}

struct Parts {
    /// Where the selection starts, and how much of the text the composition
    /// stands in place of.
    before: usize,
    cut: usize,
    preedit: Option<Vec<u16>>,
    /// The committed text's length.
    len: usize,
}

/// The byte in the text `editor` shows (the composition in place of the
/// selection) at the document's UTF-16 offset `at`.
fn shown_byte(editor: &EditorHandle, at: usize) -> usize {
    let e = editor.editor.borrow();
    let Some(preedit) = e.preedit() else {
        return e.utf16_to_byte(at);
    };
    let selection = e.selection().range();
    let before = e.byte_to_utf16(selection.start);
    let units = preedit.text.encode_utf16().count();
    if at <= before {
        e.utf16_to_byte(at)
    } else if at < before + units {
        // Within the composition: the character holding the unit (inside a
        // surrogate pair, its start).
        let mut counted = 0;
        let inside = preedit
            .text
            .char_indices()
            .find(|(_, c)| {
                counted += c.len_utf16();
                counted > at - before
            })
            .map_or(preedit.text.len(), |(i, _)| i);
        selection.start + inside
    } else {
        // After it: the committed text past what it stands in place of.
        let cut = e.byte_to_utf16(selection.end) - before;
        e.utf16_to_byte(at - units + cut) - selection.len() + preedit.text.len()
    }
}

impl Seen {
    /// What `editor` is now.
    fn is(&self, editor: &EditorHandle) -> bool {
        let e = editor.editor.borrow();
        let s = e.selection();
        self.revision == e.revision()
            && self.selection == (s.anchor, s.focus)
            && match (&self.preedit, e.preedit()) {
                (None, None) => true,
                (Some((text, cursor)), Some(p)) => *text == p.text && *cursor == p.cursor,
                _ => false,
            }
    }
}

fn seen(editor: &EditorHandle) -> Seen {
    let e = editor.editor.borrow();
    let s = e.selection();
    Seen {
        revision: e.revision(),
        selection: (s.anchor, s.focus),
        preedit: e.preedit().map(|p| (p.text.clone(), p.cursor)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Change, TextStore};
    use zgx::{compose::prelude::*, text_edit::ClauseStyle, widgets::Ui};

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    fn store(text: &str) -> (Ui, TextStore, zgx::reactive::Signal<String>) {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(text.to_owned());
        let view = ui.mount(text_input("Name", value.clone()).w(300.));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let mut store = TextStore::new();
        store.focus(ui.focused_editor());
        (ui, store, value)
    }

    #[test]
    fn the_document_is_the_editors_text_in_utf16() {
        let (_ui, store, _) = store("a😀b");
        assert_eq!(store.len(), 4);
        assert_eq!(store.text(0, 99), utf16("a😀b"));
        assert_eq!(store.text(1, 3), utf16("😀"));
        // The caret is at the end of the text, as focus puts it.
        assert_eq!(store.selection(), (4, 4, false));
    }

    #[test]
    fn a_text_service_types_at_the_selection_and_replaces_ranges() {
        // The emoji panel and dictation: InsertTextAtSelection.
        let (_ui, mut store, value) = store("hi");
        let (s, e, _) = store.selection();
        assert_eq!(store.replace(s, e, &utf16(" 😀")), Some(2..5));
        assert_eq!(value.get(), "hi 😀");
        assert_eq!(store.selection(), (5, 5, false));
        // A correction: SetText over committed text, one undo step.
        store.replace(0, 2, &utf16("Hello"));
        assert_eq!(value.get(), "Hello 😀");
        store.set_selection(0, 5, true);
        assert_eq!(store.selection(), (0, 5, true));
        assert!(
            store.changes(8).is_empty(),
            "its own edits aren't news to it"
        );
    }

    #[test]
    fn a_composition_is_the_preedit_until_it_ends() {
        let (_ui, mut store, value) = store("ab");
        store.set_selection(1, 1, false);
        store.start_composition(1..1);
        store.replace(1, 1, &utf16("に"));
        store.replace(2, 2, &utf16("ほ"));
        assert_eq!(value.get(), "ab", "nothing committed while composing");
        assert_eq!(store.text(0, 99), utf16("aにほb"));
        assert_eq!(store.selection(), (3, 3, false));
        // Converted: the whole composition replaced, its clauses marked.
        store.replace(1, 3, &utf16("日本"));
        store.mark_clauses(&[(1..2, ClauseStyle::Converted), (2..3, ClauseStyle::Target)]);
        let clauses = store.editor().unwrap().editor.borrow().preedit_clauses();
        assert_eq!(
            clauses,
            [(0..3, ClauseStyle::Converted), (3..6, ClauseStyle::Target)]
        );
        store.end_composition();
        assert_eq!(value.get(), "a日本b");
        assert!(!store.is_composing());
        assert_eq!(store.selection(), (3, 3, false));
    }

    /// `GetTextExt`: a range of the document is where the editor shows it,
    /// the composition in place of the selection.
    #[test]
    fn a_ranges_area_is_where_the_editor_shows_it() {
        let (_ui, mut store, _) = store("ab");
        store.set_selection(1, 1, false);
        store.start_composition(1..1);
        store.replace(1, 1, &utf16("😀本語"));
        let editor = store.editor().unwrap().clone();
        assert!(editor.text_area(0..1).is_some());
        // Before the composition, in it (😀 is two units, four bytes), after.
        assert_eq!(store.area(0, 1), editor.text_area(0..1));
        assert_eq!(store.area(1, 3), editor.text_area(1..5));
        assert_eq!(store.area(3, 5), editor.text_area(5..11));
        assert_eq!(store.area(5, 6), editor.text_area(11..12));
        assert_eq!(store.area(6, 6), editor.text_area(12..12));
        // Inside 😀's pair: from its start.
        assert_eq!(store.area(2, 3), editor.text_area(1..5));
        // A clause marked outside the composition is left out.
        store.mark_clauses(&[(0..1, ClauseStyle::Target), (3..5, ClauseStyle::Target)]);
        assert_eq!(
            editor.editor.borrow().preedit_clauses(),
            [(4..10, ClauseStyle::Target)]
        );
    }

    /// Focus moving mid-composition commits it into the editor it began in,
    /// as IMM32's does, though that editor dropped its preedit on blur; none
    /// of it follows into the next.
    #[test]
    fn a_composition_open_when_focus_moves_commits_where_it_began() {
        let mut ui = Ui::new(400., 300.);
        let (first, second) = (ui.signal("ab".to_owned()), ui.signal(String::new()));
        let view = ui.mount(
            column()
                .child(text_input("First", first.clone()).id("first").w(300.))
                .child(text_input("Second", second.clone()).id("second").w(300.)),
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, view.find("first"));
        let mut store = TextStore::new();
        store.focus(ui.focused_editor());
        store.start_composition(2..2);
        store.replace(2, 2, &utf16("안"));
        // The click into the other editor: the first loses its preedit.
        ui.input.focus(&ui.scene, view.find("second"));
        ui.prepare_frame();
        store.focus(ui.focused_editor());
        assert_eq!(first.get(), "ab안");
        assert_eq!(second.get(), "");
        assert!(!store.is_composing());
    }

    #[test]
    fn edits_it_didnt_make_are_told_once() {
        let (_ui, mut store, _) = store("ab");
        let len = store.len();
        store.editor().unwrap().paste("c");
        assert_eq!(
            store.changes(len),
            [
                Change::Text {
                    start: 0,
                    old_end: 2,
                    new_end: 3
                },
                Change::Selection
            ]
        );
        assert!(store.changes(3).is_empty());
    }

    #[test]
    fn a_read_only_editor_takes_nothing() {
        let (_ui, mut store, value) = store("ab");
        store.editor().unwrap().set_read_only(true);
        store.replace(2, 2, &utf16("c"));
        assert_eq!(value.get(), "ab");
    }
}
