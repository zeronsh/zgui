//! Unicode grapheme-aware text editing with bounded undo history and IME preedit.
//! All offsets are UTF-8 bytes, clamped to extended grapheme boundaries.
use std::{collections::VecDeque, ops::Range};
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub focus: usize,
}
impl Selection {
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }
    pub fn is_empty(self) -> bool {
        self.anchor == self.focus
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}
/// What an edit was, for joining runs of them into one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Run {
    Typing,
    DeletingBackward,
    DeletingForward,
}
#[derive(Clone)]
struct Edit {
    start: usize,
    deleted: String,
    inserted: String,
    before: Selection,
    after: Selection,
}
pub struct TextEditor {
    text: String,
    selection: Selection,
    preedit: Option<Preedit>,
    composition_cancel_revision: u64,
    undo: VecDeque<Edit>,
    redo: VecDeque<Edit>,
    undo_bytes: usize,
    redo_bytes: usize,
    history_limit: usize,
    history_bytes: usize,
    /// The run the last undo step is, while the next edit of its kind can
    /// join it: typing, or deleting one way, as AppKit's text views undo.
    run: Option<Run>,
}
impl Default for TextEditor {
    fn default() -> Self {
        Self::new("")
    }
}
impl TextEditor {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self {
            text,
            selection: Selection {
                anchor: end,
                focus: end,
            },
            preedit: None,
            composition_cancel_revision: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            undo_bytes: 0,
            redo_bytes: 0,
            history_limit: 100,
            history_bytes: 4 * 1024 * 1024,
            run: None,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.range()]
    }
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }
    /// Changes only when a local editor operation cancels nonempty preedit.
    /// This is not a model revision or native session epoch; it wraps at u64::MAX.
    /// Native empty-preedit updates and commits do not increment this revision.
    /// Hosts can use it to reset the native input context after external edits.
    pub fn composition_cancel_revision(&self) -> u64 {
        self.composition_cancel_revision
    }
    /// Bound each undo/redo stack independently by entry count and retained
    /// inserted/deleted UTF-8 payload bytes. The byte budget excludes edit and
    /// allocator metadata. Lowering either limit also releases spare stack capacity.
    pub fn set_history_limits(&mut self, entries: usize, bytes: usize) {
        let lowered = entries < self.history_limit || bytes < self.history_bytes;
        self.history_limit = entries;
        self.history_bytes = bytes;
        Self::trim(&mut self.undo, &mut self.undo_bytes, entries, bytes);
        Self::trim(&mut self.redo, &mut self.redo_bytes, entries, bytes);
        if lowered {
            self.undo.shrink_to_fit();
            self.redo.shrink_to_fit();
        }
    }
    fn trim(history: &mut VecDeque<Edit>, payload: &mut usize, entries: usize, bytes: usize) {
        while history.len() > entries || *payload > bytes {
            if let Some(edit) = history.pop_front() {
                *payload -= edit.deleted.len() + edit.inserted.len();
            } else {
                break;
            }
        }
    }
    fn boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let mut cursor = unicode_segmentation::GraphemeCursor::new(offset, self.text.len(), true);
        if cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            offset
        } else {
            cursor
                .prev_boundary(&self.text, 0)
                .ok()
                .flatten()
                .unwrap_or(0)
        }
    }
    pub fn set_selection(&mut self, anchor: usize, focus: usize) {
        self.run = None;
        self.cancel_preedit();
        self.selection = Selection {
            anchor: self.boundary(anchor),
            focus: self.boundary(focus),
        };
    }
    pub fn select_all(&mut self) {
        self.set_selection(0, self.text.len());
    }
    /// Replace the selection with `value`. Typing (one grapheme at a time)
    /// joins the undo step typing before it made, until the caret moves,
    /// another kind of edit comes, or a line break is typed; a paste or a
    /// longer insert is a step of its own.
    pub fn insert(&mut self, value: &str) -> bool {
        let typing = value != "\n" && value.graphemes(true).count() == 1;
        self.replace(value, typing.then_some(Run::Typing))
    }
    /// `insert`, as an edit of `run`'s kind (`None`: a step of its own).
    fn replace(&mut self, value: &str, run: Option<Run>) -> bool {
        let range = self.selection.range();
        if self.text[range.clone()] == *value {
            self.cancel_preedit();
            self.set_selection(range.start + value.len(), range.start + value.len());
            return false;
        }
        let joins = run.is_some() && self.run == run;
        let before = self.selection;
        let deleted = self.text[range.clone()].to_owned();
        self.cancel_preedit();
        self.redo.clear();
        self.redo_bytes = 0;
        self.text.replace_range(range.clone(), value);
        // Insertion can join an adjacent combining mark/ZWJ sequence. Keep the caret valid.
        let desired = range.start + value.len();
        let end = if desired == self.text.len() {
            desired
        } else {
            let mut cursor =
                unicode_segmentation::GraphemeCursor::new(desired, self.text.len(), true);
            if cursor.is_boundary(&self.text, 0).unwrap_or(false) {
                desired
            } else {
                cursor
                    .next_boundary(&self.text, 0)
                    .ok()
                    .flatten()
                    .unwrap_or(self.text.len())
            }
        };
        self.selection = Selection {
            anchor: end,
            focus: end,
        };
        self.undo_bytes += deleted.len() + value.len();
        let joined = joins
            && self.undo.back_mut().is_some_and(|last| {
                // Contiguous with the step before: type on at its end, delete
                // on before it or after it.
                match run {
                    Some(Run::Typing)
                        if deleted.is_empty()
                            && last.start + last.inserted.len() == range.start =>
                    {
                        last.inserted.push_str(value);
                    }
                    Some(Run::DeletingBackward)
                        if value.is_empty()
                            && last.inserted.is_empty()
                            && range.end == last.start =>
                    {
                        last.deleted.insert_str(0, &deleted);
                        last.start = range.start;
                    }
                    Some(Run::DeletingForward)
                        if value.is_empty()
                            && last.inserted.is_empty()
                            && range.start == last.start =>
                    {
                        last.deleted.push_str(&deleted);
                    }
                    _ => return false,
                }
                last.after = self.selection;
                true
            });
        if !joined {
            self.undo.push_back(Edit {
                start: range.start,
                deleted,
                inserted: value.to_owned(),
                before,
                after: self.selection,
            });
        }
        self.run = run;
        Self::trim(
            &mut self.undo,
            &mut self.undo_bytes,
            self.history_limit,
            self.history_bytes,
        );
        true
    }
    /// Replace the document, recording only its changed UTF-8 span in history.
    /// A changed value moves the caret to the end; equal values are untouched.
    pub fn set_text(&mut self, value: impl Into<String>) -> bool {
        self.set_text_ref(&value.into())
    }
    /// Borrowed model ingress avoids allocating a second full document string.
    pub(crate) fn set_text_ref(&mut self, value: &str) -> bool {
        if self.text == value {
            return false;
        }
        // Appending/truncating a stream can use the optimized slice comparison
        // instead of walking every unchanged byte through the general diff.
        let mut prefix = if value.starts_with(&self.text) {
            self.text.len()
        } else if self.text.starts_with(value) {
            value.len()
        } else {
            self.text
                .bytes()
                .zip(value.bytes())
                .take_while(|(old, new)| old == new)
                .count()
        };
        while !self.text.is_char_boundary(prefix) || !value.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let remaining = (self.text.len() - prefix).min(value.len() - prefix);
        let mut suffix = self
            .text
            .bytes()
            .rev()
            .zip(value.bytes().rev())
            .take(remaining)
            .take_while(|(old, new)| old == new)
            .count();
        while !self.text.is_char_boundary(self.text.len() - suffix)
            || !value.is_char_boundary(value.len() - suffix)
        {
            suffix -= 1;
        }
        let old_end = self.text.len() - suffix;
        let new_end = value.len() - suffix;
        self.run = None;
        let before = self.selection;
        let deleted = self.text[prefix..old_end].to_owned();
        let inserted = value[prefix..new_end].to_owned();
        self.cancel_preedit();
        self.redo.clear();
        self.redo_bytes = 0;
        self.text.replace_range(prefix..old_end, &inserted);
        self.selection = Selection {
            anchor: self.text.len(),
            focus: self.text.len(),
        };
        self.undo_bytes += deleted.len() + inserted.len();
        self.undo.push_back(Edit {
            start: prefix,
            deleted,
            inserted,
            before,
            after: self.selection,
        });
        Self::trim(
            &mut self.undo,
            &mut self.undo_bytes,
            self.history_limit,
            self.history_bytes,
        );
        true
    }
    fn previous(&self, word: bool) -> usize {
        let pos = self.selection.focus;
        if word {
            self.text[..pos]
                .unicode_word_indices()
                .map(|(i, _)| i)
                .next_back()
                .unwrap_or(0)
        } else {
            self.text[..pos]
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .next_back()
                .unwrap_or(0)
        }
    }
    fn next(&self, word: bool) -> usize {
        let pos = self.selection.focus;
        if word {
            self.text[pos..]
                .unicode_word_indices()
                .next()
                .map(|(i, w)| pos + i + w.len())
                .unwrap_or(self.text.len())
        } else {
            self.text[pos..]
                .graphemes(true)
                .next()
                .map(|g| pos + g.len())
                .unwrap_or(pos)
        }
    }
    fn move_to(&mut self, offset: usize, extend: bool) {
        self.run = None;
        self.cancel_preedit();
        self.selection.focus = self.boundary(offset);
        if !extend {
            self.selection.anchor = self.selection.focus;
        }
    }
    pub fn move_left(&mut self, word: bool, extend: bool) {
        let pos = if !extend && !self.selection.is_empty() {
            self.selection.range().start
        } else {
            self.previous(word)
        };
        self.move_to(pos, extend);
    }
    pub fn move_right(&mut self, word: bool, extend: bool) {
        let pos = if !extend && !self.selection.is_empty() {
            self.selection.range().end
        } else {
            self.next(word)
        };
        self.move_to(pos, extend);
    }
    /// Left (`left`) or right as the text shows it, which right-to-left
    /// text runs the other way from its order: of the boundaries before and
    /// after the caret, the nearest whose caret (`caret`) lies that way on
    /// the same line; else the one its order says. A selection collapses to
    /// its edge that way.
    pub fn move_visually(
        &mut self,
        word: bool,
        extend: bool,
        left: bool,
        caret: impl Fn(usize) -> crate::scene::Rect,
    ) {
        let toward = |from: f32, to: f32| {
            if left {
                to < from - 0.5
            } else {
                to > from + 0.5
            }
        };
        if !extend && !self.selection.is_empty() {
            let range = self.selection.range();
            let (start, end) = (caret(range.start).x, caret(range.end).x);
            let pos = if toward(end, start) || (start == end && left) {
                range.start
            } else {
                range.end
            };
            return self.move_to(pos, false);
        }
        let focus = self.selection.focus;
        let here = caret(focus);
        let pick = [self.previous(word), self.next(word)]
            .into_iter()
            .filter(|p| *p != focus)
            .map(|p| (p, caret(p)))
            .filter(|(_, r)| (r.y - here.y).abs() < 0.5 && toward(here.x, r.x))
            .min_by(|a, b| (a.1.x - here.x).abs().total_cmp(&(b.1.x - here.x).abs()))
            .map(|(p, _)| p);
        let pos = pick.unwrap_or_else(|| {
            if left {
                self.previous(word)
            } else {
                self.next(word)
            }
        });
        self.move_to(pos, extend);
    }
    pub fn move_home(&mut self, extend: bool) {
        let pos = self.text[..self.selection.focus]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        self.move_to(pos, extend);
    }
    pub fn move_end(&mut self, extend: bool) {
        let focus = self.selection.focus;
        let pos = self.text[focus..]
            .find('\n')
            .map_or(self.text.len(), |i| focus + i);
        self.move_to(pos, extend);
    }
    pub fn move_document_start(&mut self, extend: bool) {
        self.move_to(0, extend);
    }
    pub fn move_document_end(&mut self, extend: bool) {
        self.move_to(self.text.len(), extend);
    }
    pub fn backspace(&mut self) -> bool {
        self.delete_backward(false)
    }
    pub fn delete_backward(&mut self, word: bool) -> bool {
        if self.selection.is_empty() {
            let pos = self.previous(word);
            if pos == self.selection.focus {
                return false;
            }
            self.delete_to(pos, Run::DeletingBackward)
        } else {
            self.replace("", None)
        }
    }
    pub fn delete_forward(&mut self) -> bool {
        self.delete_next(false)
    }
    pub fn delete_next(&mut self, word: bool) -> bool {
        if self.selection.is_empty() {
            let pos = self.next(word);
            if pos == self.selection.focus {
                return false;
            }
            self.delete_to(pos, Run::DeletingForward)
        } else {
            self.replace("", None)
        }
    }
    /// Delete from the caret to `pos`, as one edit of `run`'s kind; undo
    /// restores the caret where it was.
    fn delete_to(&mut self, pos: usize, run: Run) -> bool {
        let old = self.selection;
        let joins = self.run == Some(run);
        self.selection.anchor = pos;
        let changed = self.replace("", Some(run));
        // A new step undoes to the caret it started from; a joined one
        // keeps its first.
        if changed
            && !joins
            && let Some(s) = self.undo.back_mut()
        {
            s.before = old;
        }
        changed
    }
    /// Delete from the caret to the start of its line (Cmd+Backspace on
    /// macOS), or the selection if there is one.
    pub fn delete_to_line_start(&mut self) -> bool {
        if !self.selection.is_empty() {
            return self.replace("", None);
        }
        let focus = self.selection.focus;
        let start = self.text[..focus].rfind('\n').map_or(0, |i| i + 1);
        // At a line's start, the line break before it.
        let start = if start == focus {
            self.previous(false)
        } else {
            start
        };
        if start == focus {
            return false;
        }
        self.run = None;
        self.delete_to(start, Run::DeletingBackward)
    }
    /// Delete from the caret to the end of its line (Cmd+Delete, and
    /// Control+K, on macOS), or the line break there.
    pub fn delete_to_line_end(&mut self) -> bool {
        if !self.selection.is_empty() {
            return self.replace("", None);
        }
        let focus = self.selection.focus;
        let end = self.text[focus..]
            .find('\n')
            .map_or(self.text.len(), |i| focus + i);
        let end = if end == focus { self.next(false) } else { end };
        if end == focus {
            return false;
        }
        self.run = None;
        self.delete_to(end, Run::DeletingForward)
    }
    /// Return selected text and remove it in one undoable operation. The host owns the OS clipboard.
    pub fn cut(&mut self) -> String {
        let value = self.selected_text().to_owned();
        self.replace("", None);
        value
    }
    pub fn undo(&mut self) -> bool {
        self.run = None;
        let Some(edit) = self.undo.pop_back() else {
            return false;
        };
        self.text
            .replace_range(edit.start..edit.start + edit.inserted.len(), &edit.deleted);
        self.selection = edit.before;
        self.cancel_preedit();
        let bytes = edit.deleted.len() + edit.inserted.len();
        self.undo_bytes -= bytes;
        self.redo_bytes += bytes;
        self.redo.push_back(edit);
        Self::trim(
            &mut self.redo,
            &mut self.redo_bytes,
            self.history_limit,
            self.history_bytes,
        );
        true
    }
    pub fn redo(&mut self) -> bool {
        self.run = None;
        let Some(edit) = self.redo.pop_back() else {
            return false;
        };
        self.text
            .replace_range(edit.start..edit.start + edit.deleted.len(), &edit.inserted);
        self.selection = edit.after;
        self.cancel_preedit();
        let bytes = edit.deleted.len() + edit.inserted.len();
        self.redo_bytes -= bytes;
        self.undo_bytes += bytes;
        self.undo.push_back(edit);
        Self::trim(
            &mut self.undo,
            &mut self.undo_bytes,
            self.history_limit,
            self.history_bytes,
        );
        true
    }
    /// Preedit is ephemeral: it never changes committed text or enters undo history.
    pub fn set_preedit(&mut self, text: impl Into<String>, cursor: Option<(usize, usize)>) {
        let text = text.into();
        if text.is_empty() {
            self.preedit = None;
            return;
        }

        let clamp = |n: usize| {
            let mut n = n.min(text.len());
            while !text.is_char_boundary(n) {
                n -= 1;
            }
            let mut cursor = unicode_segmentation::GraphemeCursor::new(n, text.len(), true);
            if cursor.is_boundary(&text, 0).unwrap_or(false) {
                n
            } else {
                cursor.prev_boundary(&text, 0).ok().flatten().unwrap_or(0)
            }
        };
        let cursor = cursor.map(|(a, b)| (clamp(a), clamp(b)));
        self.preedit = Some(Preedit { text, cursor });
    }
    pub fn commit_preedit(&mut self, text: &str) -> bool {
        self.preedit = None;
        self.insert(text)
    }
    pub fn cancel_preedit(&mut self) {
        if self.preedit.take().is_some() {
            self.composition_cancel_revision = self.composition_cancel_revision.wrapping_add(1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Insert `text` as an undo step of its own: placing the caret ends a
    /// typing run.
    fn step(editor: &mut TextEditor, text: &str) {
        let end = editor.selection().focus;
        editor.set_selection(end, end);
        editor.insert(text);
    }
    #[test]
    fn large_document_keeps_only_edit_deltas() {
        let mut editor = TextEditor::new("x".repeat(1_000_000));
        editor.set_history_limits(10, 8);
        step(&mut editor, "a");
        step(&mut editor, "b");
        assert_eq!(
            editor
                .undo
                .iter()
                .map(|e| e.inserted.len() + e.deleted.len())
                .sum::<usize>(),
            2
        );
        assert!(editor.undo());
        assert!(editor.undo());
        assert_eq!(editor.text().len(), 1_000_000);
    }
    #[test]
    fn graphemes_are_atomic() {
        let mut e = TextEditor::new("a👩‍💻e\u{301}");
        assert!(e.backspace());
        assert_eq!(e.text(), "a👩‍💻");
        e.move_left(false, false);
        assert_eq!(e.selection().focus, 1);
        e.delete_forward();
        assert_eq!(e.text(), "a");
        e.undo();
        assert_eq!(e.text(), "a👩‍💻");
        assert_eq!(e.selection().focus, 1);
    }
    #[test]
    fn replacement_undo_and_redo() {
        let mut e = TextEditor::new("hello world");
        e.set_selection(6, 11);
        e.insert("🌎");
        assert_eq!(e.text(), "hello 🌎");
        e.undo();
        assert_eq!(e.selected_text(), "world");
        e.redo();
        assert_eq!(e.text(), "hello 🌎");
        e.insert("!");
        assert!(!e.redo());
    }
    #[test]
    fn ime_does_not_mutate_until_commit() {
        let mut e = TextEditor::new("a");
        e.set_preedit("に", Some((1, 99)));
        assert_eq!(e.text(), "a");
        assert_eq!(e.preedit().unwrap().cursor, Some((0, 3)));
        assert!(!e.undo());
        e.commit_preedit("日本");
        assert_eq!(e.text(), "a日本");
        e.undo();
        assert_eq!(e.text(), "a");
    }
    #[test]
    fn lowered_history_limits_remain_bounded_when_transferring_between_stacks() {
        let mut editor = TextEditor::new("");
        for text in ["a", "b", "c", "d", "e", "f"] {
            step(&mut editor, text);
        }
        for _ in 0..3 {
            assert!(editor.undo());
        }
        editor.set_history_limits(2, 2);
        let bounded = |editor: &TextEditor| {
            for history in [&editor.undo, &editor.redo] {
                assert!(history.len() <= 2, "history exceeded entry bound");
                assert!(
                    history
                        .iter()
                        .map(|edit| edit.inserted.len() + edit.deleted.len())
                        .sum::<usize>()
                        <= 2,
                    "history exceeded byte bound"
                );
            }
        };
        bounded(&editor);
        assert!(editor.undo());
        assert_eq!(editor.text(), "ab");
        bounded(&editor);
        assert!(editor.redo());
        assert_eq!(editor.text(), "abc");
        bounded(&editor);
        assert!(editor.redo());
        assert_eq!(editor.text(), "abcd");
        bounded(&editor);
        assert!(!editor.redo());
        editor.set_history_limits(0, 0);
        assert!(!editor.undo());
        assert!(!editor.redo());
    }

    #[test]
    fn payload_accounting_tracks_eviction_transfers_and_new_edit_redo_clear() {
        let mut editor = TextEditor::new("old");
        let check = |editor: &TextEditor| {
            assert_eq!(
                editor.undo_bytes,
                editor
                    .undo
                    .iter()
                    .map(|e| e.deleted.len() + e.inserted.len())
                    .sum::<usize>()
            );
            assert_eq!(
                editor.redo_bytes,
                editor
                    .redo
                    .iter()
                    .map(|e| e.deleted.len() + e.inserted.len())
                    .sum::<usize>()
            );
        };
        editor.set_history_limits(1000, 10000);
        for _ in 0..200 {
            editor.insert("é");
            check(&editor);
        }
        for _ in 0..100 {
            editor.undo();
            check(&editor);
        }
        let capacities = (editor.undo.capacity(), editor.redo.capacity());
        editor.set_history_limits(3, 5);
        check(&editor);
        assert!(editor.undo.capacity() < capacities.0);
        assert!(editor.redo.capacity() < capacities.1);
        editor.undo();
        check(&editor);
        editor.redo();
        check(&editor);
        editor.insert("!");
        check(&editor);
        assert!(editor.redo.is_empty());
        editor.select_all();
        editor.insert("oversize");
        check(&editor);
        assert_eq!(editor.undo_bytes, 0);
        assert!(!editor.undo());
        editor.set_history_limits(0, 0);
        assert_eq!(editor.undo.capacity(), 0);
        assert_eq!(editor.redo.capacity(), 0);
    }

    #[test]
    fn external_composition_cancellation_has_one_revision_per_existing_preedit() {
        let mut editor = TextEditor::new("abc");
        let compose = |editor: &mut TextEditor| editor.set_preedit("中", Some((3, 3)));
        compose(&mut editor);
        editor.set_selection(0, 0);
        assert_eq!(editor.composition_cancel_revision(), 1);
        editor.set_selection(0, 0);
        editor.cancel_preedit();
        assert_eq!(editor.composition_cancel_revision(), 1);
        compose(&mut editor);
        editor.insert(""); // Equal replacement still cancels composition.
        assert_eq!(editor.composition_cancel_revision(), 2);
        compose(&mut editor);
        editor.insert("!");
        assert_eq!(editor.composition_cancel_revision(), 3);
        compose(&mut editor);
        editor.set_text("changed");
        assert_eq!(editor.composition_cancel_revision(), 4);
        compose(&mut editor);
        assert!(editor.undo());
        assert_eq!(editor.composition_cancel_revision(), 5);
        compose(&mut editor);
        assert!(editor.redo());
        assert_eq!(editor.composition_cancel_revision(), 6);
        compose(&mut editor);
        editor.move_left(false, false);
        assert_eq!(editor.composition_cancel_revision(), 7);
        compose(&mut editor);
        editor.cancel_preedit();
        assert_eq!(editor.composition_cancel_revision(), 8);
        editor.composition_cancel_revision = u64::MAX;
        compose(&mut editor);
        editor.cancel_preedit();
        assert_eq!(editor.composition_cancel_revision(), 0);
    }

    #[test]
    fn native_preedit_clear_commit_and_noop_model_update_do_not_signal_external_cancel() {
        let mut editor = TextEditor::new("abc");
        editor.set_preedit("中", None);
        editor.set_preedit("文", Some((3, 3)));
        assert!(!editor.set_text("abc"));
        assert!(editor.preedit().is_some());
        assert!(!editor.undo());
        assert!(!editor.redo());
        assert!(editor.preedit().is_some());
        assert_eq!(editor.composition_cancel_revision(), 0);
        editor.set_preedit("", None);
        assert_eq!(editor.composition_cancel_revision(), 0);
        assert!(editor.preedit().is_none());
        editor.set_preedit("中", None);
        editor.commit_preedit("中");
        assert_eq!(editor.text(), "abc中");
        assert_eq!(editor.composition_cancel_revision(), 0);
        assert!(editor.preedit().is_none());
        editor.set_preedit("ignored", None);
        editor.commit_preedit("");
        assert_eq!(editor.composition_cancel_revision(), 0);
    }

    #[test]
    fn streamed_model_appends_keep_one_hundred_small_undo_deltas() {
        let initial = "x".repeat(1_000_000);
        let mut editor = TextEditor::new(initial.clone());
        let mut model = initial.clone();
        for _ in 0..100 {
            model.push('é');
            assert!(editor.set_text_ref(&model));
        }
        assert_eq!(editor.undo.len(), 100);
        assert_eq!(editor.undo_bytes, 200);
        assert!(
            editor
                .undo
                .iter()
                .all(|edit| edit.deleted.is_empty() && edit.inserted == "é")
        );
        for _ in 0..100 {
            assert!(editor.undo());
        }
        assert_eq!(editor.text(), initial);
        assert!(!editor.undo());
        for _ in 0..100 {
            assert!(editor.redo());
        }
        assert_eq!(editor.text(), model);
        assert_eq!(editor.undo_bytes, 200);
    }

    #[test]
    fn model_delta_preserves_unicode_joins_exact_text_and_before_selection() {
        for (old, new) in [
            ("a", "a\u{301}"),
            ("👩👧", "👩‍👧"),
            ("é-tail", "ê-tail"),
            ("é", "©"),
            ("aaaa", "aaa"),
            ("aaa", "aaaa"),
            ("prefix-middle-suffix", "prefix-new-suffix"),
            ("", "中"),
            ("中", ""),
        ] {
            let mut editor = TextEditor::new(old);
            editor.set_selection(old.len(), 0);
            let selection = editor.selection();
            editor.set_preedit("候補", None);
            assert!(editor.set_text_ref(new));
            assert_eq!(editor.text(), new);
            assert_eq!(
                editor.selection(),
                Selection {
                    anchor: new.len(),
                    focus: new.len()
                }
            );
            assert_eq!(editor.composition_cancel_revision(), 1);
            assert!(editor.undo());
            assert_eq!(editor.text(), old);
            assert_eq!(editor.selection(), selection);
            assert!(editor.redo());
            assert_eq!(editor.text(), new);
        }
    }

    #[test]
    fn minimal_model_delta_obeys_limits_and_equal_values_preserve_state() {
        let mut editor = TextEditor::new("prefix-old-suffix");
        editor.set_history_limits(2, 6);
        editor.set_selection(2, 4);
        editor.set_text_ref("prefix-new-suffix");
        assert_eq!(editor.undo_bytes, 6);
        assert_eq!(editor.undo.back().unwrap().deleted, "old");
        assert_eq!(editor.undo.back().unwrap().inserted, "new");
        assert!(editor.undo());
        editor.set_preedit("中", None);
        let selection = editor.selection();
        assert!(!editor.set_text_ref("prefix-old-suffix"));
        assert_eq!(editor.selection(), selection);
        assert!(editor.preedit().is_some());
        assert_eq!(editor.redo.len(), 1);
        editor.set_text_ref("prefix-x-suffix");
        assert!(editor.redo.is_empty());
        assert_eq!(editor.undo_bytes, 4);
        editor.set_history_limits(0, 0);
        editor.set_text_ref("updated");
        assert!(editor.undo.is_empty());
        assert_eq!(editor.undo_bytes, 0);
        assert_eq!(editor.text(), "updated");
    }

    #[test]
    fn history_is_bounded() {
        let mut e = TextEditor::new("");
        e.set_history_limits(2, 100);
        step(&mut e, "a");
        step(&mut e, "b");
        step(&mut e, "c");
        assert!(e.undo());
        assert!(e.undo());
        assert!(!e.undo());
        assert_eq!(e.text(), "a");
    }
    #[test]
    fn byte_offsets_clamp_and_combining_insert_stays_valid() {
        let mut e = TextEditor::new("é");
        e.set_selection(1, 1);
        assert_eq!(e.selection().focus, 0);
        e.move_right(false, false);
        e.insert("\u{301}");
        assert_eq!(e.selection().focus, e.text().len());
        e.move_left(false, false);
        assert_eq!(e.selection().focus, 0);
    }
}

#[cfg(test)]
mod undo_run_tests {
    use super::*;
    #[test]
    fn typing_and_deleting_undo_in_runs_as_appkit_does() {
        let mut e = TextEditor::new("");
        for c in "hello world".chars() {
            e.insert(&c.to_string());
        }
        // One run of typing: one undo.
        assert!(e.undo());
        assert_eq!(e.text(), "");
        assert!(e.redo());
        assert_eq!(e.text(), "hello world");
        // Backspaces join each other, not the typing before them.
        for _ in 0..5 {
            e.delete_backward(false);
        }
        assert_eq!(e.text(), "hello ");
        assert!(e.undo());
        assert_eq!(e.text(), "hello world");
        assert_eq!(e.selection().focus, 11, "the caret goes back where it was");
        // Moving the caret ends a run; a line break is a step of its own.
        e.insert("!");
        e.move_left(false, false);
        e.insert("?");
        assert!(e.undo());
        assert_eq!(e.text(), "hello world!");
        e.move_document_end(false);
        e.insert("a");
        e.insert("\n");
        e.insert("b");
        assert!(e.undo());
        assert_eq!(e.text(), "hello world!a\n");
        assert!(e.undo());
        assert_eq!(e.text(), "hello world!a");
        // Forward deletes join too; a paste is a step of its own.
        e.set_selection(0, 0);
        e.delete_next(false);
        e.delete_next(false);
        assert_eq!(e.text(), "llo world!a");
        e.insert("pasted text");
        assert!(e.undo());
        assert_eq!(e.text(), "llo world!a");
        assert!(e.undo());
        assert_eq!(e.text(), "hello world!a");
    }
    #[test]
    fn deleting_to_a_lines_edges() {
        let mut e = TextEditor::new("one\ntwo three\nfour");
        e.set_selection(8, 8);
        assert!(e.delete_to_line_start());
        assert_eq!(e.text(), "one\nthree\nfour");
        assert!(
            e.delete_to_line_start(),
            "at a line's start: its line break"
        );
        assert_eq!(e.text(), "onethree\nfour");
        assert!(e.delete_to_line_end());
        assert_eq!(e.text(), "one\nfour");
        assert!(e.undo());
        assert_eq!(e.text(), "onethree\nfour");
    }
}
