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
use zgx::{
    text_edit::{ClauseStyle, InputMode},
    widgets::EditorHandle,
};

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
    /// The editor dropped the composition (the app set its text or its
    /// selection): the input method is to end its own, committing nothing.
    CompositionCancelled,
}

pub(crate) struct TextStore {
    editor: Option<EditorHandle>,
    /// The composition's range in the document, while one is open.
    composition: Option<Range<usize>>,
    /// The composition's text, the store's own: an editor drops its preedit
    /// when it loses focus, and the composition is still committed into it.
    composing: String,
    /// The committed text a composition opened over (reconversion: the
    /// person selected it and pressed Convert, or Hanja), while it is open.
    reconverting: Option<String>,
    /// The editor's `composition_cancel_revision` as the composition began:
    /// once it moves, the editor dropped it.
    cancels: u64,
    seen: Option<Seen>,
}

impl TextStore {
    pub(crate) fn new() -> Self {
        Self {
            editor: None,
            composition: None,
            composing: String::new(),
            reconverting: None,
            cancels: 0,
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
    /// The focused editor's input scope (`input_scope`), the default with
    /// none.
    pub(crate) fn input_scope(&self) -> i32 {
        input_scope(
            self.editor
                .as_ref()
                .map_or(InputMode::Text, EditorHandle::input_mode),
        )
    }
    pub(crate) fn is_composing(&self) -> bool {
        self.composition.is_some()
    }
    /// The document's pieces: the committed text before the selection (its
    /// UTF-16 length and byte end), the preedit if any (in place of the
    /// selection), and the committed text after.
    fn parts(&self, editor: &EditorHandle) -> Parts {
        let mut e = editor.editor.borrow_mut();
        match e
            .preedit()
            .map(|preedit| preedit.text.encode_utf16().collect())
        {
            Some(preedit) => {
                let range = e.selection().range();
                Parts {
                    before: e.byte_to_utf16(range.start),
                    cut: e.byte_to_utf16(range.end) - e.byte_to_utf16(range.start),
                    preedit: Some(preedit),
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
        let mut e = editor.editor.borrow_mut();
        // The committed text's units `from..to` exactly: a range starting or
        // ending between a surrogate pair's halves has the one half it holds,
        // so pages read one after another (`GetText` cut to its buffer) join
        // up as the text.
        let mut committed = |from: usize, to: usize| -> Vec<u16> {
            if from >= to {
                return Vec::new();
            }
            let a = e.utf16_to_byte(from);
            let mut b = e.utf16_to_byte(to);
            if e.byte_to_utf16(b) < to {
                // `to` is inside the pair at `b`: take its character.
                b += e.slice(b..e.len()).chars().next().map_or(0, char::len_utf8);
            }
            let skip = from - e.byte_to_utf16(a);
            e.slice(a..b)
                .encode_utf16()
                .skip(skip)
                .take(to - from)
                .collect()
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
        let mut e = editor.editor.borrow_mut();
        let cursor = e.preedit().map(|preedit| {
            let (a, b) = preedit
                .cursor
                .unwrap_or((preedit.text.len(), preedit.text.len()));
            let unit = |at: usize| {
                preedit.text[..at.min(preedit.text.len())]
                    .encode_utf16()
                    .count()
            };
            (unit(a), unit(b))
        });
        match cursor {
            Some((a, b)) => {
                let start = e.selection().range().start;
                let before = e.byte_to_utf16(start);
                (before + a, before + b, false)
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
                    let mut e = editor.editor.borrow_mut();
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
        // A composition the editor dropped is no longer the document's.
        self.drop_cancelled();
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
            let mut e = editor.editor.borrow_mut();
            (
                e.utf16_to_byte(start),
                e.utf16_to_byte(end),
                e.selection().range(),
            )
        };
        let before = self.len();
        if from == selection.start && to == selection.end {
            editor.insert_text(&new);
        } else {
            editor.replace_text(from..to, &new);
        }
        self.seen = Some(seen(&editor));
        // What went in, as the editor took it: a field drops line breaks, a
        // text area makes "\r\n" one.
        let inserted = (self.len() + (end - start)).saturating_sub(before);
        Some(start..start + inserted)
    }
    /// The input method opened a composition over `range` (`OnStartComposition`).
    pub(crate) fn start_composition(&mut self, range: Range<usize>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        self.reconverting = None;
        let end = range.end.min(self.len());
        if range.start >= end || editor.editor.borrow().preedit().is_some() {
            self.cancels = editor.editor.borrow().composition_cancel_revision();
            self.composition = Some(range.start..range.start);
            self.composing.clear();
            return;
        }
        // Over committed text (reconversion): that text, selected, is the
        // composition in its place, the document as it was.
        let (from, to) = {
            let mut e = editor.editor.borrow_mut();
            (e.utf16_to_byte(range.start), e.utf16_to_byte(end))
        };
        editor.set_selection(from, to);
        let text = editor.editor.borrow().slice(from..to).into_owned();
        editor.set_composition(&text, Some((text.len(), text.len())), &[]);
        // From here on (selecting it dropped no composition of its own).
        self.cancels = editor.editor.borrow().composition_cancel_revision();
        if editor.editor.borrow().preedit().is_none() {
            // A read-only editor takes none.
            self.composition = Some(range.start..range.start);
            self.composing.clear();
            return;
        }
        self.composition = Some(range.start..end);
        self.composing = text.clone();
        self.reconverting = Some(text);
        self.seen = Some(seen(&editor));
    }
    /// Whether the editor dropped the open composition (the app set its text
    /// or its selection, or made it read-only): then it is the store's no
    /// longer, and nothing of it is committed. (A focus moving away commits
    /// it first, `focus`.)
    fn drop_cancelled(&mut self) -> bool {
        let Some(editor) = &self.editor else {
            return false;
        };
        if self.composition.is_none()
            || editor.editor.borrow().composition_cancel_revision() == self.cancels
        {
            return false;
        }
        self.composition = None;
        self.composing.clear();
        self.reconverting = None;
        true
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
        match (
            std::mem::take(&mut self.composing),
            self.reconverting.take(),
        ) {
            // Reconverted to what it was: nothing to commit (no undo step).
            (text, Some(original)) if text == original => editor.cancel_composition(),
            // Reconverted, even to nothing: in place of what it was.
            (text, Some(_)) => {
                editor.commit_composition(&text);
            }
            (text, None) if !text.is_empty() => {
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
        let cancelled = self.drop_cancelled();
        let Some(editor) = &self.editor else {
            return Vec::new();
        };
        // Nothing changed (every dispatch but those that edit): compared in
        // place, nothing made.
        if !cancelled && self.seen.as_ref().is_some_and(|seen| seen.is(editor)) {
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
        // After the text it no longer holds is told.
        if cancelled {
            changes.push(Change::CompositionCancelled);
        }
        changes
    }
}

/// The Text Services Framework's input scope (`InputScope`, `IS_*`) for an
/// editor's `InputMode`: which touch keyboard Windows shows, what its input
/// methods offer. `None`'s keyboard isn't shown at all (the host's part);
/// its scope is the default.
pub(crate) fn input_scope(mode: InputMode) -> i32 {
    match mode {
        InputMode::Text | InputMode::None => 0, // IS_DEFAULT
        InputMode::Url => 1,                    // IS_URL
        InputMode::Email => 5,                  // IS_EMAIL_SMTPEMAILADDRESS
        InputMode::Numeric => 28,               // IS_DIGITS
        InputMode::Decimal => 29,               // IS_NUMBER
        InputMode::Tel => 32,                   // IS_TELEPHONE_FULLTELEPHONENUMBER
        InputMode::Search => 50,                // IS_SEARCH
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
    let mut e = editor.editor.borrow_mut();
    let Some((units, len)) = e
        .preedit()
        .map(|preedit| (preedit.text.encode_utf16().count(), preedit.text.len()))
    else {
        return e.utf16_to_byte(at);
    };
    let selection = e.selection().range();
    let before = e.byte_to_utf16(selection.start);
    if at <= before {
        e.utf16_to_byte(at)
    } else if at < before + units {
        // Within the composition: the character holding the unit (inside a
        // surrogate pair, its start).
        let mut counted = 0;
        let Some(preedit) = e.preedit() else {
            return selection.start;
        };
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
        e.utf16_to_byte(at - units + cut) - selection.len() + len
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
    use zgx::{
        compose::prelude::*,
        text_edit::{ClauseStyle, InputMode},
        widgets::Ui,
    };

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

    /// Ranges between a surrogate pair's halves hold the half they cover:
    /// read one unit at a time, the pages join up as the text.
    #[test]
    fn ranges_inside_a_surrogate_pair_hold_its_halves() {
        let (_ui, mut store, _) = store("a😀b");
        assert_eq!(store.text(0, 2), [0x61, 0xD83D]);
        assert_eq!(store.text(2, 3), [0xDE00]);
        assert_eq!(store.text(2, 4), [0xDE00, 0x62]);
        let paged: Vec<u16> = (0..4).flat_map(|at| store.text(at, at + 1)).collect();
        assert_eq!(paged, utf16("a😀b"));
        // And in the committed text after a composition.
        store.set_selection(0, 0, false);
        store.start_composition(0..0);
        store.replace(0, 0, &utf16("x"));
        let paged: Vec<u16> = (0..5).flat_map(|at| store.text(at, at + 1)).collect();
        assert_eq!(paged, utf16("xa😀b"));
        assert_eq!(store.text(3, 4), [0xDE00]);
    }

    /// The range a text service's text went in at is what the editor took:
    /// a field drops its line breaks, a text area makes "\r\n" one.
    #[test]
    fn an_insertion_is_as_long_as_what_the_editor_took() {
        let (_ui, mut store, value) = store("");
        assert_eq!(store.replace(0, 0, &utf16("a\r\nb")), Some(0..2));
        assert_eq!((value.get().as_str(), store.len()), ("ab", 2));
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::new());
        let view = ui.mount(text_area("Notes", value.clone()).w(300.).h(80.));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let mut store = TextStore::new();
        store.focus(ui.focused_editor());
        assert_eq!(store.replace(0, 0, &utf16("a\r\nb")), Some(0..3));
        assert_eq!((value.get().as_str(), store.len()), ("a\nb", 3));
        // Over committed text: the range of the replacement as taken.
        assert_eq!(store.replace(0, 1, &utf16("x\r\ny")), Some(0..3));
        assert_eq!(value.get(), "x\ny\nb");
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

    /// A field "abcd" composing 候補 at 2, the input method's way.
    fn composing() -> (Ui, TextStore, zgx::reactive::Signal<String>) {
        let (ui, mut store, value) = store("abcd");
        store.set_selection(2, 2, false);
        store.start_composition(2..2);
        store.replace(2, 2, &utf16("候補"));
        assert_eq!(value.get(), "abcd");
        assert!(store.editor().unwrap().editor.borrow().preedit().is_some());
        (ui, store, value)
    }

    /// The app setting the text drops the composition (research A batch 012):
    /// the input method is told to end its own, and ending it commits none.
    #[test]
    fn the_apps_edit_cancels_the_composition() {
        let (_ui, mut store, value) = composing();
        let before = store.len();
        value.set("REPLACED".to_owned());
        assert!(store.editor().unwrap().editor.borrow().preedit().is_none());
        let changes = store.changes(before);
        assert_eq!(changes.last(), Some(&Change::CompositionCancelled));
        assert!(!store.is_composing());
        store.end_composition();
        assert_eq!(value.get(), "REPLACED");
    }

    /// After that edit, the input method writing at the selection it reads
    /// types there (it panicked, on the composition's old range).
    #[test]
    fn writing_after_a_cancelled_composition_types_at_the_selection() {
        let (_ui, mut store, value) = composing();
        let before = store.len();
        value.set("REPLACED".to_owned());
        store.changes(before);
        let (a, b, _) = store.selection();
        assert!(a <= b && b <= store.len());
        store.replace(a, b, &utf16("!"));
        assert_eq!(value.get().matches('!').count(), 1);
        // And without the change heard first.
        let (_ui, mut store, value) = composing();
        value.set("REPLACED".to_owned());
        let (a, b, _) = store.selection();
        store.replace(a, b, &utf16("!"));
        assert_eq!(value.get().matches('!').count(), 1);
    }

    /// The app moving the selection drops the composition too.
    #[test]
    fn the_apps_selection_cancels_the_composition() {
        let (_ui, mut store, value) = composing();
        let before = store.len();
        store.editor().unwrap().set_selection(0, 0);
        assert!(store.editor().unwrap().editor.borrow().preedit().is_none());
        assert!(
            store
                .changes(before)
                .contains(&Change::CompositionCancelled)
        );
        store.end_composition();
        assert_eq!(value.get(), "abcd");
    }

    /// An edit that leaves the text as it was keeps the composition live.
    #[test]
    fn an_edit_to_the_same_text_keeps_the_composition() {
        let (_ui, mut store, value) = composing();
        let before = store.len();
        value.set("abcd".to_owned());
        assert!(
            !store
                .changes(before)
                .contains(&Change::CompositionCancelled)
        );
        assert!(store.editor().unwrap().editor.borrow().preedit().is_some());
        store.end_composition();
        assert_eq!(value.get(), "ab候補cd");
    }

    /// Reconversion: the input method opens its composition over committed
    /// text (selected, then Convert or Hanja). That text is the composition
    /// in its place; what it ends as replaces it, and ending it unchanged
    /// edits nothing.
    #[test]
    fn a_composition_over_committed_text_reconverts_it() {
        let (_ui, mut store, value) = store("a日本b");
        let revision = store.editor().unwrap().editor.borrow().revision();
        store.start_composition(1..3);
        assert!(store.is_composing());
        assert_eq!(store.text(0, 99), utf16("a日本b"), "the document as it was");
        assert_eq!(store.len(), 4);
        // Unchanged: nothing committed, no undo step.
        store.end_composition();
        assert_eq!(value.get(), "a日本b");
        assert_eq!(store.editor().unwrap().editor.borrow().revision(), revision);
        // Converted to another: in place of the text it opened over.
        store.start_composition(1..3);
        assert_eq!(store.replace(1, 3, &utf16("二本")), Some(1..3));
        assert_eq!(value.get(), "a日本b", "nothing committed while composing");
        store.end_composition();
        assert_eq!(value.get(), "a二本b");
        // To nothing: the text it opened over goes.
        store.start_composition(1..3);
        store.replace(1, 3, &[]);
        store.end_composition();
        assert_eq!(value.get(), "ab");
        // An empty range composes at the caret, as before.
        store.set_selection(1, 1, false);
        store.start_composition(1..1);
        store.replace(1, 1, &utf16("x"));
        store.end_composition();
        assert_eq!(value.get(), "axb");
    }

    /// An editor's input mode (HTML's `inputmode`) is the input scope the
    /// framework asks for: the touch keyboard Windows shows.
    #[test]
    fn an_editors_input_mode_is_its_input_scope() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::new());
        let view = ui.mount(
            text_input("Email", value)
                .input_mode(InputMode::Email)
                .w(300.),
        );
        ui.prepare_frame();
        let mut store = TextStore::new();
        assert_eq!(store.input_scope(), 0, "IS_DEFAULT with no editor");
        ui.input.focus(&ui.scene, Some(view.node()));
        store.focus(ui.focused_editor());
        assert_eq!(store.input_scope(), 5, "IS_EMAIL_SMTPEMAILADDRESS");
        let scopes = [
            InputMode::Text,
            InputMode::Decimal,
            InputMode::Numeric,
            InputMode::Tel,
            InputMode::Search,
            InputMode::Email,
            InputMode::Url,
            InputMode::None,
        ]
        .map(super::input_scope);
        assert_eq!(scopes, [0, 29, 28, 32, 50, 5, 1, 0]);
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
