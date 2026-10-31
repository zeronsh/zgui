//! Unicode grapheme-aware text editing with bounded undo history and IME preedit.
//! All offsets are UTF-8 bytes, clamped to extended grapheme boundaries.
use crate::text::{LONG, Text, TextValue as Buffer, TextValue};
use std::{borrow::Cow, collections::VecDeque, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

// What an editor holds its text in: a string while short (edited in place,
// as fast as one is), chunks while long ([`Text`]): an edit is then its own
// size, and the text shared with its binding, layout and semantics, not
// copied for them.
impl Buffer {
    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        match self {
            Buffer::Flat(text) => Cow::Borrowed(&text[range]),
            Buffer::Long(text) => text.slice(range),
        }
    }
    fn is_char_boundary(&self, at: usize) -> bool {
        match self {
            Buffer::Flat(text) => text.is_char_boundary(at),
            Buffer::Long(text) => text.is_char_boundary(at),
        }
    }
    fn replace_range(&mut self, range: Range<usize>, with: &str) {
        match self {
            Buffer::Flat(text) => {
                text.replace_range(range, with);
                if text.len() > LONG {
                    *self = Buffer::Long(Text::new(text));
                }
            }
            Buffer::Long(text) => {
                text.replace_range(range, with);
                if text.len() < LONG / 2 {
                    *self = Buffer::Flat(text.to_cow().into_owned());
                }
            }
        }
    }
    /// The text around byte `at` that segmentation needs: the line it is
    /// on with its break, and where that starts (all of a short text).
    fn around(&self, at: usize) -> (usize, Cow<'_, str>) {
        match self {
            Buffer::Flat(text) => (0, Cow::Borrowed(text)),
            Buffer::Long(text) => {
                let line = text.line_range(text.line_of(at));
                let end = (line.end + 1).min(text.len());
                (line.start, text.slice(line.start..end))
            }
        }
    }
    /// Where the line `at` is on starts, and where it ends (before its
    /// break).
    fn line_around(&self, at: usize) -> Range<usize> {
        match self {
            Buffer::Flat(text) => {
                let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
                let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
                start..end
            }
            Buffer::Long(text) => text.line_range(text.line_of(at)),
        }
    }
    /// The bytes it starts with in common with `other`, and then ends with
    /// (after them): the chunks two long texts share skipped by pointer,
    /// the rest compared a run at a time.
    fn common(&self, other: &Buffer) -> (usize, usize) {
        if let (Buffer::Long(ours), Buffer::Long(theirs)) = (self, other) {
            let prefix = ours.common_prefix(theirs);
            return (prefix, ours.common_suffix(theirs, prefix));
        }
        self.with_parts(|ours| other.with_parts(|theirs| crate::text::common(ours, theirs)))
    }
    /// `f` of its pieces: a short text's one on the stack, not in a list
    /// made for it (an editor's binding compares on every change).
    fn with_parts<R>(&self, f: impl FnOnce(&[&str]) -> R) -> R {
        match self {
            Buffer::Flat(text) => f(&[text]),
            Buffer::Long(text) => f(&text.chunks().collect::<Vec<_>>()),
        }
    }
}
/// A text an editor is given to hold (`TextEditor::take`).
#[derive(Clone, Copy)]
enum Incoming<'a> {
    Str(&'a str),
    Value(&'a TextValue),
}
impl Incoming<'_> {
    fn len(self) -> usize {
        match self {
            Incoming::Str(text) => text.len(),
            Incoming::Value(text) => text.len(),
        }
    }
    fn is_char_boundary(self, at: usize) -> bool {
        match self {
            Incoming::Str(text) => text.is_char_boundary(at),
            Incoming::Value(text) => text.is_char_boundary(at),
        }
    }
    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        match *self {
            Incoming::Str(text) => Cow::Borrowed(&text[range]),
            Incoming::Value(text) => text.slice(range),
        }
    }
}
/// A text selection, from `anchor` to `focus`; equal, it is a caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// Where it began: the end that stays put as it extends.
    pub anchor: usize,
    /// Where the caret is: the end that moves.
    pub focus: usize,
}
impl Selection {
    /// From its earlier end to its later.
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }
    /// Whether it is a caret, selecting nothing.
    pub fn is_empty(self) -> bool {
        self.anchor == self.focus
    }
}
/// Text an input method is composing at the caret, not yet part of
/// the text (IME preedit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    /// The text being composed.
    pub text: String,
    /// The caret, or the range the input method selects, in `text`: start
    /// and end UTF-8 byte offsets. `None` hides the caret.
    pub cursor: Option<(usize, usize)>,
}
/// How an input method marks a clause of its composition, as Windows'
/// display attributes and macOS's marked text do: still being typed, already
/// converted, or the one it is converting now (whose candidates show).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClauseStyle {
    /// Typed and not yet converted: a thin underline.
    Input,
    /// Converted, not the one being converted: a thin underline.
    Converted,
    /// The clause being converted: a thick underline, the candidates under it.
    Target,
}
/// What kind of text an editor takes, as HTML's `inputmode` says it: a
/// hint to the platform's input (which touch keyboard Windows shows, what
/// a Wayland input method offers), never a check of what is typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputMode {
    /// Any text: the platform's usual keyboard.
    #[default]
    Text,
    /// A number with a fraction: digits and the decimal separator.
    Decimal,
    /// Digits alone (a code, a PIN that may show).
    Numeric,
    /// A telephone number: digits, `*`, `#`, `+`.
    Tel,
    /// A search: the keyboard's Enter key searches.
    Search,
    /// An email address: `@` and `.` at hand.
    Email,
    /// A URL: `/` and `.com` at hand.
    Url,
    /// No virtual keyboard: the field brings its own keys (a keypad on the
    /// page). A physical keyboard types into it as usual.
    None,
}
/// What an editor keeps for an input method that reaches into its text
/// (Windows' Text Services Framework), made when one first does: its
/// composition's clauses, and where its text's UTF-16 offsets are.
#[derive(Default)]
struct ImeState {
    clauses: Vec<(Range<usize>, ClauseStyle)>,
    utf16: Option<Utf16Starts>,
}
// An editor is Send, Sync and unwind-safe, as published releases' were:
// losing one breaks code that shares or catches across it.
const _: () = {
    const fn holds<T: Send + Sync + std::panic::RefUnwindSafe + std::panic::UnwindSafe>() {}
    holds::<TextEditor>();
};
/// The revision of a text, and its pieces' UTF-8 and UTF-16 starts then.
type Utf16Starts = (u64, Vec<(usize, usize)>);
/// `at` (a byte offset into `text`) on a character's start, back from it.
fn floor(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
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
impl Edit {
    fn bytes(&self) -> usize {
        self.deleted.len() + self.inserted.len()
    }
    /// What undoes it.
    fn inverted(self) -> Self {
        Self {
            start: self.start,
            deleted: self.inserted,
            inserted: self.deleted,
            before: self.after,
            after: self.before,
        }
    }
    /// `first` and then `second`, as one step; `text` is what `second`
    /// left.
    fn joined(first: Edit, second: Edit, text: &str) -> Self {
        let first_end = first.start + first.inserted.len();
        let second_end = second.start + second.deleted.len();
        let (from, to) = (first.start.min(second.start), first_end.max(second_end));
        // What lay between them after `first`: `text`, with `second` undone.
        let beyond = second.start + second.inserted.len();
        let mut between = String::with_capacity(to - from);
        between.push_str(&text[from..second.start]);
        between.push_str(&second.deleted);
        between.push_str(&text[beyond..beyond + (to - second_end)]);
        let swapped = |start: usize, end: usize, with: &str| {
            let mut out = String::with_capacity(between.len() + with.len());
            out.push_str(&between[..start - from]);
            out.push_str(with);
            out.push_str(&between[end - from..]);
            out
        };
        Self {
            start: from,
            deleted: swapped(first.start, first_end, &first.deleted),
            inserted: swapped(second.start, second_end, &second.inserted),
            before: first.before,
            after: second.after,
        }
    }
}
/// A plain-text editing model: the text, its selection, IME preedit
/// and a bounded undo history.
pub struct TextEditor {
    text: Buffer,
    selection: Selection,
    preedit: Option<Preedit>,
    composition_cancel_revision: u64,
    /// How many times its text changed (`revision`).
    revision: u64,
    /// `load_next`: the next text it is given leaves nothing to undo.
    loading: bool,
    /// `answer_next`: the next text it is given joins the last step.
    answering: bool,
    /// Its last step undone, rather than redone or edited.
    undone: bool,
    undo: VecDeque<Edit>,
    redo: VecDeque<Edit>,
    undo_bytes: usize,
    redo_bytes: usize,
    history_limit: usize,
    history_bytes: usize,
    /// The run the last undo step is, while the next edit of its kind can
    /// join it: typing, or deleting one way, as AppKit's text views undo.
    run: Option<Run>,
    /// For an input method that reaches into the text: one pointer until
    /// one does.
    ime: Option<Box<ImeState>>,
}
impl Default for TextEditor {
    fn default() -> Self {
        Self::new("")
    }
}
impl TextEditor {
    /// An editor holding `text`, with the caret at its end. Its history
    /// keeps 100 steps and 4 MiB of edited text (`set_history_limits`).
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_value(Buffer::from(text.into()))
    }
    /// An editor holding `text` (a long one shared, not copied), as `new`.
    pub fn with_value(text: TextValue) -> Self {
        // A short text in chunks goes back in a string, as an edit would.
        let text = match text {
            Buffer::Long(long) if long.len() < LONG / 2 => Buffer::Flat(long.to_cow().into_owned()),
            text => text,
        };
        let end = text.len();
        Self {
            text,
            selection: Selection {
                anchor: end,
                focus: end,
            },
            preedit: None,
            composition_cancel_revision: 0,
            revision: 0,
            loading: false,
            answering: false,
            undone: false,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            undo_bytes: 0,
            redo_bytes: 0,
            history_limit: 100,
            history_bytes: 4 * 1024 * 1024,
            run: None,
            ime: None,
        }
    }
    /// The committed text, without any preedit: borrowed while short, a
    /// copy of a long one (see `slice` and `long_text` for those).
    pub fn text(&self) -> Cow<'_, str> {
        self.text.to_cow()
    }
    /// Whether bytes `range` of the committed text are `text`: within it,
    /// on its characters' boundaries, and the same (what a drag took from
    /// it, the text since unchanged there).
    pub(crate) fn holds(&self, range: Range<usize>, text: &str) -> bool {
        range.start <= range.end
            && range.end <= self.text.len()
            && self.text.is_char_boundary(range.start)
            && self.text.is_char_boundary(range.end)
            && self.text.slice(range) == text
    }
    /// The committed text's length in bytes.
    pub fn len(&self) -> usize {
        self.text.len()
    }
    /// Whether the committed text is empty.
    pub fn is_empty(&self) -> bool {
        self.text.len() == 0
    }
    /// Bytes `range` of the committed text.
    pub fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        self.text.slice(range)
    }
    /// The committed text that word and line selection around byte `at`
    /// read: the line it is on with its break, and where that starts (all
    /// of a short text).
    pub fn around(&self, at: usize) -> (usize, Cow<'_, str>) {
        self.text.around(at.min(self.text.len()))
    }
    /// The bytes of the line byte `at` is on, its break excluded.
    pub fn line_around(&self, at: usize) -> Range<usize> {
        self.text.line_around(at.min(self.text.len()))
    }
    /// A long committed text, in its chunks: share it (a clone is a pointer)
    /// rather than copy it. `None` while it is short.
    pub fn long_text(&self) -> Option<&Text> {
        match &self.text {
            Buffer::Long(text) => Some(text),
            Buffer::Flat(_) => None,
        }
    }
    /// Changes whenever the committed text does (it wraps): the same number
    /// is the same text, without comparing it (a long document's).
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// The selection.
    pub fn selection(&self) -> Selection {
        self.selection
    }
    /// The selected text.
    pub fn selected_text(&self) -> Cow<'_, str> {
        self.text.slice(self.selection.range())
    }
    /// What an input method is composing, if anything.
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
        let (base, text) = self.text.around(offset);
        let local = offset - base;
        let mut cursor = unicode_segmentation::GraphemeCursor::new(local, text.len(), true);
        if cursor.is_boundary(&text, 0).unwrap_or(false) {
            offset
        } else {
            base + cursor.prev_boundary(&text, 0).ok().flatten().unwrap_or(0)
        }
    }
    /// Select from `anchor` to `focus`, each moved back to a grapheme
    /// boundary. Ends a typing run and cancels any preedit.
    pub fn set_selection(&mut self, anchor: usize, focus: usize) {
        self.run = None;
        self.cancel_preedit();
        self.selection = Selection {
            anchor: self.boundary(anchor),
            focus: self.boundary(focus),
        };
    }
    /// Select all the text.
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
    /// Insert pasted `value` over the selection: an undo step of its own,
    /// even of one character, as AppKit undoes a paste apart from typing.
    pub fn insert_pasted(&mut self, value: &str) -> bool {
        self.replace(value, None)
    }
    /// `insert`, as an edit of `run`'s kind (`None`: a step of its own).
    fn replace(&mut self, value: &str, run: Option<Run>) -> bool {
        let range = self.selection.range();
        if self.text.slice(range.clone()) == value {
            self.cancel_preedit();
            self.set_selection(range.start + value.len(), range.start + value.len());
            return false;
        }
        let joins = run.is_some() && self.run == run;
        let before = self.selection;
        let deleted = self.text.slice(range.clone()).into_owned();
        self.cancel_preedit();
        self.redo.clear();
        self.redo_bytes = 0;
        self.text.replace_range(range.clone(), value);
        self.revision = self.revision.wrapping_add(1);
        // Insertion can join an adjacent combining mark/ZWJ sequence. Keep the caret valid.
        let desired = range.start + value.len();
        let end = if desired == self.text.len() {
            desired
        } else {
            let (base, text) = self.text.around(desired);
            let mut cursor =
                unicode_segmentation::GraphemeCursor::new(desired - base, text.len(), true);
            if cursor.is_boundary(&text, 0).unwrap_or(false) {
                desired
            } else {
                cursor
                    .next_boundary(&text, 0)
                    .ok()
                    .flatten()
                    .map_or(self.text.len(), |at| base + at)
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
        self.take(Incoming::Str(value))
    }
    /// Replace the document with `value`, as `set_text` does; a long one is
    /// shared, not copied (a binding holding it and the editor then hold
    /// one text, and `value` reads it back in `O(1)`).
    pub fn set_value(&mut self, value: &TextValue) -> bool {
        if self.text.ptr_eq(value) {
            return false;
        }
        self.take(Incoming::Value(value))
    }
    /// The next text it is given (`set_text`, `set_value`, its binding) is a
    /// document rather than an edit: it leaves nothing to undo.
    pub fn load_next(&mut self) {
        self.loading = true;
    }
    /// The line byte `at` is on, from 0.
    pub fn line_of(&self, at: usize) -> usize {
        match &self.text {
            TextValue::Long(text) => text.line_of(at),
            TextValue::Flat(text) => text[..at.min(text.len())].matches('\n').count(),
        }
    }
    /// Where line `line` (from 0) starts; past the last, the text's end.
    pub fn line_start(&self, line: usize) -> usize {
        match &self.text {
            TextValue::Long(text) => text.line_start(line),
            TextValue::Flat(_) if line == 0 => 0,
            TextValue::Flat(text) => text
                .match_indices('\n')
                .nth(line - 1)
                .map_or(text.len(), |(at, _)| at + 1),
        }
    }
    /// The next text it is given answers the last step (an app trimming
    /// what was typed): it joins that step.
    pub fn answer_next(&mut self) {
        self.answering = true;
    }
    /// Its committed text as a value to keep: a copy while short, the
    /// long one shared.
    pub fn value(&self) -> TextValue {
        self.text.clone()
    }
    fn take(&mut self, value: Incoming<'_>) -> bool {
        let (mut prefix, mut suffix) = match value {
            Incoming::Str(text) => self
                .text
                .with_parts(|ours| crate::text::common(ours, &[text])),
            Incoming::Value(text) => self.text.common(text),
        };
        if self.text.len() == value.len() && prefix == value.len() {
            return false;
        }
        // The changed span alone goes into history: the text both start
        // with and end with is left as it is.
        while !self.text.is_char_boundary(prefix) || !value.is_char_boundary(prefix) {
            prefix -= 1;
        }
        suffix = suffix.min((self.text.len() - prefix).min(value.len() - prefix));
        while !self.text.is_char_boundary(self.text.len() - suffix)
            || !value.is_char_boundary(value.len() - suffix)
        {
            suffix -= 1;
        }
        let old_end = self.text.len() - suffix;
        let new_end = value.len() - suffix;
        self.run = None;
        let before = self.selection;
        let deleted = self.text.slice(prefix..old_end).into_owned();
        let inserted = value.slice(prefix..new_end).into_owned();
        self.cancel_preedit();
        // An answer to an undo keeps what redoes it.
        if !(self.answering && self.undone) {
            self.redo.clear();
            self.redo_bytes = 0;
        }
        match value {
            // A long text is taken as it is, shared with whoever gave it.
            Incoming::Value(TextValue::Long(text)) if text.len() >= LONG / 2 => {
                self.text = TextValue::Long(text.clone());
            }
            _ => self.text.replace_range(prefix..old_end, &inserted),
        }
        self.revision = self.revision.wrapping_add(1);
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
        if std::mem::take(&mut self.loading) {
            self.undo.clear();
            self.undo_bytes = 0;
        }
        if std::mem::take(&mut self.answering) {
            self.fold_answer();
        }
        true
    }
    /// The step just taken is what the app made of the one before: they
    /// are one step, or undo would go to text the app answers again and
    /// stick there. After an undo, it joins what redoes it.
    fn fold_answer(&mut self) {
        let Some(answer) = self.undo.pop_back() else {
            return;
        };
        self.undo_bytes -= answer.bytes();
        let text = self.text.to_cow().into_owned();
        if self.undone
            && let Some(redo) = self.redo.pop_back()
        {
            // Offsets of older steps hold only if the text kept its length.
            if answer.deleted.len() != answer.inserted.len() {
                self.undo.clear();
                self.undo_bytes = 0;
            }
            self.redo_bytes -= redo.bytes();
            let joined = Edit::joined(redo.inverted(), answer, &text).inverted();
            self.redo_bytes += joined.bytes();
            self.redo.push_back(joined);
        } else if let Some(edit) = self.undo.pop_back() {
            self.undo_bytes -= edit.bytes();
            let joined = Edit::joined(edit, answer, &text);
            if joined.deleted != joined.inserted {
                self.undo_bytes += joined.bytes();
                self.undo.push_back(joined);
            }
        }
    }
    fn previous(&self, word: bool) -> usize {
        let pos = self.selection.focus;
        let back = |text: &str| {
            if word {
                text.unicode_word_indices().map(|(i, _)| i).next_back()
            } else {
                text.grapheme_indices(true).map(|(i, _)| i).next_back()
            }
        };
        match &self.text {
            Buffer::Flat(text) => back(&text[..pos]).unwrap_or(0),
            // A line at a time back from the caret, as far as a word is.
            Buffer::Long(text) => {
                let mut end = pos;
                while end > 0 {
                    let start = text.line_start(text.line_of(end - 1));
                    if let Some(at) = back(&text.slice(start..end)) {
                        return start + at;
                    }
                    end = start;
                }
                0
            }
        }
    }
    /// Where Backspace takes the text back to (`backspace_takes`).
    fn backspaced(&self) -> usize {
        let pos = self.selection.focus;
        let start = self.previous(false);
        pos - backspace_takes(&self.text.slice(start..pos), cfg!(target_os = "macos"))
    }
    fn next(&self, word: bool) -> usize {
        if word && cfg!(windows) {
            return self.next_word_start();
        }
        let pos = self.selection.focus;
        let len = self.text.len();
        let on = |text: &str, at: usize| {
            if word {
                text.unicode_word_indices()
                    .next()
                    .map(|(i, w)| at + i + w.len())
            } else {
                text.graphemes(true).next().map(|g| at + g.len())
            }
        };
        match &self.text {
            Buffer::Flat(text) => on(&text[pos..], pos).unwrap_or(if word { len } else { pos }),
            // A line at a time on from the caret, as far as a word is.
            Buffer::Long(text) => {
                let mut start = pos;
                while start < len {
                    let line = text.line_range(text.line_of(start));
                    let end = (line.end + 1).min(len);
                    if let Some(at) = on(&text.slice(start..end), start) {
                        return at;
                    }
                    if !word {
                        return pos;
                    }
                    start = end.max(start + 1);
                }
                if word { len } else { pos }
            }
        }
    }
    /// Windows' word step on (Control+Right, Control+Delete), as its edit
    /// controls and Chrome there go: past the spaces after a word to the
    /// next one's start; from a line's last word to its end, and from there
    /// over the break and the next line's leading spaces.
    fn next_word_start(&self) -> usize {
        let pos = self.selection.focus;
        let line = self.text.line_around(pos);
        if pos >= line.end {
            if pos >= self.text.len() {
                return pos;
            }
            let next = self.text.line_around(pos + 1);
            let rest = self.text.slice(next.clone());
            return next.start + rest.len() - rest.trim_start().len();
        }
        let rest = self.text.slice(pos..line.end);
        rest.unicode_word_indices()
            .map(|(at, _)| pos + at)
            .find(|&at| at > pos)
            .unwrap_or(line.end)
    }
    fn move_to(&mut self, offset: usize, extend: bool) {
        self.run = None;
        self.cancel_preedit();
        self.selection.focus = self.boundary(offset);
        if !extend {
            self.selection.anchor = self.selection.focus;
        }
    }
    /// Move the caret back a grapheme, or to the start of a word with
    /// `word`. `extend` moves only the focus; without it, a selection
    /// collapses to its start.
    pub fn move_left(&mut self, word: bool, extend: bool) {
        let pos = if !extend && !self.selection.is_empty() {
            self.selection.range().start
        } else {
            self.previous(word)
        };
        self.move_to(pos, extend);
    }
    /// Move the caret forward a grapheme, or to the end of a word with
    /// `word` (on Windows, the next word's start). `extend` moves only the
    /// focus; without it, a selection collapses to its end.
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
        // Every caret place on its line (a grapheme's edges, or a word's),
        // and the nearest past it the way it goes: the order the text
        // shows, as AppKit's text views move, not its logical neighbours
        // (which, where its direction changes, can lie back where it was).
        let line = self.text.line_around(focus);
        let (base, text) = (line.start, self.text.slice(line.clone()));
        let places: Vec<usize> = if word {
            text.unicode_word_indices()
                .flat_map(|(at, w)| [base + at, base + at + w.len()])
                .chain([line.start, line.end])
                .collect()
        } else {
            text.grapheme_indices(true)
                .map(|(at, _)| base + at)
                .chain([line.end])
                .collect()
        };
        let pick = places
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
    /// Move the caret to the start of its line, just after the line break
    /// before it. `extend` moves only the focus.
    pub fn move_home(&mut self, extend: bool) {
        let pos = self.text.line_around(self.selection.focus).start;
        self.move_to(pos, extend);
    }
    /// Move the caret to the end of its line, just before the next line
    /// break. `extend` moves only the focus.
    pub fn move_end(&mut self, extend: bool) {
        let pos = self.text.line_around(self.selection.focus).end;
        self.move_to(pos, extend);
    }
    /// Move the caret to the start of its paragraph, or of the one before
    /// when it is there already (`up`); else to the end of its paragraph, or
    /// of the one after. AppKit's Option-Up and Option-Down. `extend` moves
    /// only the focus.
    pub fn move_paragraph(&mut self, up: bool, extend: bool) {
        let focus = self.selection.focus;
        let line = self.text.line_around(focus);
        let pos = if up {
            match line.start {
                start if start < focus => start,
                0 => 0,
                start => self.text.line_around(start - 1).start,
            }
        } else {
            match line.end {
                end if end > focus => end,
                end if end >= self.text.len() => end,
                end => self.text.line_around(end + 1).end,
            }
        };
        self.move_to(pos, extend);
    }
    /// Move the caret to the start of its paragraph, or of the one before
    /// when it is there already (`up`); else to the start of the next
    /// paragraph, or the text's end in its last. Control-Up and Control-Down
    /// on Windows and Linux. `extend` moves only the focus.
    pub fn move_paragraph_start(&mut self, up: bool, extend: bool) {
        if up {
            return self.move_paragraph(true, extend);
        }
        let end = self.text.line_around(self.selection.focus).end;
        self.move_to((end + 1).min(self.text.len()), extend);
    }
    /// Swap the graphemes either side of the caret, and step past them; at
    /// a paragraph's end, the two before it (Control-T on macOS, AppKit's
    /// `transpose:`). False with a selection, or fewer than two graphemes
    /// in the paragraph to swap.
    pub fn transpose(&mut self) -> bool {
        if !self.selection.is_empty() {
            return false;
        }
        let focus = self.selection.focus;
        let line = self.text.line_around(focus);
        let text = self.text.slice(line.clone());
        let edges: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(at, _)| line.start + at)
            .chain([line.end])
            .collect();
        let Some(mut at) = edges.iter().position(|&e| e == focus) else {
            return false;
        };
        if at + 1 == edges.len() {
            at = at.saturating_sub(1);
        }
        if at == 0 || at + 1 >= edges.len() {
            return false;
        }
        let (start, middle, end) = (edges[at - 1], edges[at], edges[at + 1]);
        let swapped = self.text.slice(middle..end).into_owned() + &self.text.slice(start..middle);
        self.run = None;
        self.cancel_preedit();
        self.selection = Selection {
            anchor: start,
            focus: end,
        };
        let changed = self.replace(&swapped, None);
        // Undone, the caret goes back where it was.
        if changed && let Some(s) = self.undo.back_mut() {
            s.before = Selection {
                anchor: focus,
                focus,
            };
        }
        changed
    }
    /// Move the text at `from` to `to` (a byte offset outside it), as a
    /// drag of a selection drops it: one undo step, the moved text
    /// selected after. False when `to` is within it or it is empty.
    pub fn move_text(&mut self, from: Range<usize>, to: usize) -> bool {
        let (from, to) = (self.clamp(from.start)..self.clamp(from.end), self.clamp(to));
        if from.is_empty() || (to > from.start && to < from.end) {
            return false;
        }
        let moved = self.text.slice(from.clone()).into_owned();
        // One replacement of the stretch it crosses, its parts swapped.
        let (span, value, at) = if to >= from.end {
            let between = self.text.slice(from.end..to).into_owned();
            (
                from.start..to,
                between.clone() + &moved,
                from.start + between.len(),
            )
        } else {
            let between = self.text.slice(to..from.start).into_owned();
            (to..from.end, moved.clone() + &between, to)
        };
        self.selection = Selection {
            anchor: span.start,
            focus: span.end,
        };
        if !self.replace(&value, None) {
            return false;
        }
        self.select_edited(at..at + moved.len());
        true
    }
    /// Insert `text` at `at`, as a drop of dragged text puts it: one undo
    /// step, the dropped text selected after.
    pub fn insert_dropped(&mut self, at: usize, text: &str) -> bool {
        let at = self.clamp(at);
        self.selection = Selection {
            anchor: at,
            focus: at,
        };
        if !self.replace(text, None) {
            return false;
        }
        let end = self.selection.focus;
        self.select_edited(at..end);
        true
    }
    /// After an edit, `range` selected, as the step undone and redone.
    fn select_edited(&mut self, range: Range<usize>) {
        self.selection = Selection {
            anchor: range.start,
            focus: range.end,
        };
        if let Some(last) = self.undo.back_mut() {
            last.after = self.selection;
        }
    }
    /// `at` within the text, on a grapheme boundary.
    fn clamp(&self, at: usize) -> usize {
        self.boundary(at.min(self.text.len()))
    }
    /// Move the caret to the start of the text. `extend` moves only the
    /// focus.
    pub fn move_document_start(&mut self, extend: bool) {
        self.move_to(0, extend);
    }
    /// Move the caret to the end of the text. `extend` moves only the
    /// focus.
    pub fn move_document_end(&mut self, extend: bool) {
        self.move_to(self.text.len(), extend);
    }
    /// Delete the selection, or what is before the caret: its last code
    /// point, or an emoji or a line break whole. False when there was
    /// nothing to delete.
    pub fn backspace(&mut self) -> bool {
        self.delete_backward(false)
    }
    /// Delete the selection, or before the caret: what Backspace takes
    /// (`backspaced`), or back to the start of a word with `word`. False
    /// when there was nothing to delete.
    pub fn delete_backward(&mut self, word: bool) -> bool {
        if self.selection.is_empty() {
            let pos = if word {
                self.previous(true)
            } else {
                self.backspaced()
            };
            if pos == self.selection.focus {
                return false;
            }
            self.delete_to(pos, Run::DeletingBackward)
        } else {
            self.replace("", None)
        }
    }
    /// Delete the selection, or the grapheme after the caret. False when
    /// there was nothing to delete.
    pub fn delete_forward(&mut self) -> bool {
        self.delete_next(false)
    }
    /// Delete the selection, or after the caret: a grapheme, or on to the
    /// end of a word with `word`. False when there was nothing to delete.
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
        let start = self.text.line_around(focus).start;
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
        self.kill_to_line_end().is_some()
    }
    /// As [`Self::delete_to_line_end`], returning what it deleted: Control-K's
    /// kill, for Control-Y to yank.
    pub(crate) fn kill_to_line_end(&mut self) -> Option<String> {
        if !self.selection.is_empty() {
            let killed = self.selected_text().into_owned();
            return self.replace("", None).then_some(killed);
        }
        let focus = self.selection.focus;
        let end = self.text.line_around(focus).end;
        let end = if end == focus { self.next(false) } else { end };
        if end == focus {
            return None;
        }
        let killed = self.text.slice(focus..end).into_owned();
        self.run = None;
        self.delete_to(end, Run::DeletingForward).then_some(killed)
    }
    /// Return selected text and remove it in one undoable operation. The host owns the OS clipboard.
    pub fn cut(&mut self) -> String {
        let value = self.selected_text().into_owned();
        self.replace("", None);
        value
    }
    /// Whether there is a step to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    /// Whether there is an undone step to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Undo the last step, restoring the selection from before it. False
    /// when there is none.
    pub fn undo(&mut self) -> bool {
        self.run = None;
        let Some(edit) = self.undo.pop_back() else {
            return false;
        };
        self.undone = true;
        self.text
            .replace_range(edit.start..edit.start + edit.inserted.len(), &edit.deleted);
        self.revision = self.revision.wrapping_add(1);
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
    /// Redo the last undone step, restoring the selection after it. False
    /// when there is none; any new edit clears what can be redone.
    pub fn redo(&mut self) -> bool {
        self.run = None;
        let Some(edit) = self.redo.pop_back() else {
            return false;
        };
        self.undone = false;
        self.text
            .replace_range(edit.start..edit.start + edit.deleted.len(), &edit.inserted);
        self.revision = self.revision.wrapping_add(1);
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
        if let Some(ime) = &mut self.ime {
            ime.clauses.clear();
        }
    }
    /// Set the preedit as `set_preedit` does, with the input method's
    /// clauses: byte ranges of `text` and how each is marked (Windows' display
    /// attributes), underlined each its own way, the candidates under the
    /// `Target`. Ranges past `text`, or not on character boundaries, are cut
    /// to it.
    pub fn set_preedit_clauses(
        &mut self,
        text: impl Into<String>,
        cursor: Option<(usize, usize)>,
        clauses: &[(Range<usize>, ClauseStyle)],
    ) {
        let text = text.into();
        let floor = |n: usize| {
            let mut n = n.min(text.len());
            while !text.is_char_boundary(n) {
                n -= 1;
            }
            n
        };
        let clauses: Vec<_> = clauses
            .iter()
            .map(|(range, style)| (floor(range.start)..floor(range.end), *style))
            .filter(|(range, _)| range.start < range.end)
            .collect();
        self.set_preedit(text, cursor);
        if self.preedit.is_some() && !clauses.is_empty() {
            self.ime().clauses = clauses;
        }
    }
    /// The preedit's clauses (`set_preedit_clauses`): none for a preedit set
    /// without them, or no preedit.
    pub fn preedit_clauses(&self) -> Vec<(Range<usize>, ClauseStyle)> {
        match self.ime.as_ref().filter(|_| self.preedit.is_some()) {
            Some(ime) => ime.clauses.clone(),
            None => Vec::new(),
        }
    }
    fn ime(&mut self) -> &mut ImeState {
        self.ime.get_or_insert_with(Box::default)
    }
    /// Replace the text in `range` (byte offsets, cut to grapheme
    /// boundaries) with `text` as one undo step, the caret after it: an input
    /// method converting text again, or correcting it.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) -> bool {
        let start = self.clamp(range.start);
        let end = self.clamp(range.end.max(range.start));
        self.run = None;
        self.cancel_preedit();
        let before = self.selection;
        self.selection = Selection {
            anchor: start,
            focus: end,
        };
        let changed = self.replace(text, None);
        if !changed {
            self.selection = before;
        }
        changed
    }
    /// `replace_range`, the selection staying where it was (moved with the
    /// text): a substitution made as text is typed (`View::autocorrect`).
    /// Undone, the selection goes back there too, and typing goes on.
    pub fn substitute(&mut self, range: Range<usize>, text: &str) -> bool {
        let before = self.selection;
        if !self.replace_range(range.clone(), text) {
            return false;
        }
        let moved = |at: usize| match at {
            at if at >= range.end => at + text.len() - range.len(),
            at if at > range.start => range.start + text.len(),
            at => at,
        };
        let after = Selection {
            anchor: moved(before.anchor),
            focus: moved(before.focus),
        };
        self.selection = after;
        if let Some(last) = self.undo.back_mut() {
            (last.before, last.after) = (before, after);
        }
        true
    }
    /// The committed text's length in UTF-16 code units, as Windows counts.
    pub fn utf16_len(&mut self) -> usize {
        self.utf16_starts(|starts, _| starts.last().map_or(0, |(_, u)| *u))
    }
    /// Byte offset `at` as a UTF-16 offset (a byte inside a character
    /// counts from that character's start).
    pub fn byte_to_utf16(&mut self, at: usize) -> usize {
        let at = at.min(self.text.len());
        self.utf16_starts(|starts, parts| {
            // The piece `at` is in, and on from its start.
            let piece = starts.partition_point(|(b, _)| *b <= at).saturating_sub(1);
            let (byte, unit) = starts[piece];
            let Some(text) = parts.get(piece) else {
                return unit;
            };
            unit + text[..floor(text, at - byte)].encode_utf16().count()
        })
    }
    /// UTF-16 offset `at` as a byte offset, on a character's start (an
    /// offset between a surrogate pair's halves counts from the pair).
    pub fn utf16_to_byte(&mut self, at: usize) -> usize {
        self.utf16_starts(|starts, parts| {
            let piece = starts.partition_point(|(_, u)| *u <= at).saturating_sub(1);
            let (byte, unit) = starts[piece];
            let Some(text) = parts.get(piece) else {
                return byte;
            };
            let mut units = unit;
            for (i, c) in text.char_indices() {
                if units + c.len_utf16() > at {
                    return byte + i;
                }
                units += c.len_utf16();
            }
            byte + text.len()
        })
    }
    /// `f` of each piece's UTF-8 and UTF-16 starts (the text's end last)
    /// and the pieces, kept for the revision until the text changes.
    fn utf16_starts<R>(&mut self, f: impl FnOnce(&[(usize, usize)], &[&str]) -> R) -> R {
        let revision = self.revision;
        let cached = &mut self.ime.get_or_insert_with(Box::default).utf16;
        self.text.with_parts(|parts| {
            if cached.as_ref().is_none_or(|(at, _)| *at != revision) {
                let mut starts = Vec::with_capacity(parts.len() + 1);
                let (mut byte, mut unit) = (0, 0);
                for part in parts {
                    starts.push((byte, unit));
                    byte += part.len();
                    unit += part.encode_utf16().count();
                }
                starts.push((byte, unit));
                *cached = Some((revision, starts));
            }
            f(&cached.as_ref().unwrap().1, parts)
        })
    }
    /// Clear the preedit and insert `text`, what the input method
    /// committed, as typing. False when the text didn't change.
    pub fn commit_preedit(&mut self, text: &str) -> bool {
        self.preedit = None;
        self.insert(text)
    }
    /// Drop any preedit, advancing `composition_cancel_revision` if there
    /// was one.
    pub fn cancel_preedit(&mut self) {
        if self.preedit.take().is_some() {
            self.composition_cancel_revision = self.composition_cancel_revision.wrapping_add(1);
        }
    }
}
/// The bytes Backspace takes off the end of `grapheme`. Backspace undoes a
/// keystroke, so a mistyped mark in a syllable goes and the syllable stays;
/// what counts as one differs by platform, measured against its native field
/// and browsers. A line break always goes whole.
///
/// `macos`, as `NSTextView` does, and Safari: one code point in the scripts
/// from Hebrew to Limbu, U+0590 to U+193F (every combining mark in Unicode
/// tried) but for Hangul's jamo, a trailing joiner with it; otherwise the
/// whole grapheme, an accent with its letter.
///
/// Otherwise as Chromium does: one code point, unless the grapheme holds an
/// emoji, a keycap or a variation selector. On Windows too, whose `RichEdit`
/// agrees but for flags and keycaps: there it leaves half of one behind, the
/// defect this rule exists to avoid.
fn backspace_takes(grapheme: &str, macos: bool) -> usize {
    let mut last = grapheme.chars().rev();
    let code_point = last.next().map_or(0, char::len_utf8);
    if grapheme == "\r\n" {
        return grapheme.len();
    }
    if macos {
        let one_by_one = |c: char| matches!(c, '\u{590}'..='\u{10FF}' | '\u{1200}'..='\u{193F}');
        return match (grapheme.chars().next_back(), last.next()) {
            (Some('\u{200D}'), Some(before)) if one_by_one(before) => {
                code_point + before.len_utf8()
            }
            (Some(end), _) if one_by_one(end) => code_point,
            _ => grapheme.len(),
        };
    }
    let whole = grapheme.chars().any(|c| {
        matches!(
            c,
            '\u{20E3}'
                | '\u{180B}'..='\u{180D}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{1F000}'..='\u{1FAFF}'
                | '\u{E0100}'..='\u{E01EF}'
        )
    });
    if whole { grapheme.len() } else { code_point }
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
    /// A long text, held in chunks, edits, undoes and finds what a new
    /// value changed as a string does.
    #[test]
    fn a_long_text_in_chunks_edits_and_undoes_as_a_string_does() {
        let original: String = (0..20_000).map(|i| format!("line {i}\n")).collect();
        assert!(original.len() > LONG);
        for (start, end, with) in [
            (0, 0, "head "),
            (70_000, 70_004, "a\nb"),
            (original.len() - 3, original.len(), ""),
            (10, original.len() - 10, "middle"),
        ] {
            let mut editor = TextEditor::new(original.clone());
            assert!(editor.long_text().is_some());
            let mut next = original.clone();
            next.replace_range(start..end, with);
            assert!(editor.set_text(next.clone()));
            assert_eq!(editor.text(), next);
            assert!(editor.undo());
            assert_eq!(editor.text(), original);
            assert!(editor.redo());
            assert_eq!(editor.text(), next);
            assert!(!editor.set_text(next.clone()));
        }
        // Typing, words and lines across chunk edges.
        let mut editor = TextEditor::new(original.clone());
        editor.set_selection(8190, 8190);
        editor.insert("xy");
        editor.move_home(false);
        let line = editor.selection().focus;
        assert!(line == 0 || editor.text().as_bytes()[line - 1] == b'\n');
        assert_eq!(editor.line_around(line).start, line);
        editor.move_end(false);
        assert_eq!(editor.text().as_bytes()[editor.selection().focus], b'\n');
        editor.move_right(true, false);
        editor.move_left(true, false);
        assert!(editor.selection().focus <= editor.len());
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
        // Backspace's own measure is `backspace_takes`; the caret and
        // forward delete take graphemes.
        e.delete_to(
            e.selection().focus - "e\u{301}".len(),
            Run::DeletingBackward,
        );
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
    fn transpose_swaps_around_the_caret_or_before_a_paragraphs_end() {
        let mut e = TextEditor::new("abc\né");
        e.set_selection(1, 1);
        assert!(e.transpose());
        assert_eq!((e.text().as_ref(), e.selection().focus), ("bac\né", 2));
        e.set_selection(3, 3);
        assert!(e.transpose(), "the two before the paragraph's end");
        assert_eq!((e.text().as_ref(), e.selection().focus), ("bca\né", 3));
        e.undo();
        assert_eq!((e.text().as_ref(), e.selection().focus), ("bac\né", 3));
        for at in [0, 4, 6] {
            e.set_selection(at, at);
            assert!(!e.transpose(), "nothing before, or one grapheme, at {at}");
        }
        e.set_selection(0, 2);
        assert!(!e.transpose(), "not a selection");
        assert_eq!(e.text(), "bac\né");
    }
    #[test]
    fn utf16_offsets_count_as_windows_does_and_follow_edits() {
        let mut e = TextEditor::new("aé😀日");
        // a 1, é 1, 😀 2 (a surrogate pair), 日 1.
        assert_eq!(e.utf16_len(), 5);
        assert_eq!(
            [0, 1, 3, 7, 10].map(|b| e.byte_to_utf16(b)),
            [0, 1, 2, 4, 5]
        );
        assert_eq!(
            [0, 1, 2, 4, 5].map(|u| e.utf16_to_byte(u)),
            [0, 1, 3, 7, 10]
        );
        // Between the pair's halves: the pair's start. Past the end: the end.
        assert_eq!(e.utf16_to_byte(3), 3);
        assert_eq!(e.utf16_to_byte(99), 10);
        assert_eq!(e.byte_to_utf16(5), 2, "inside 😀's bytes: its start");
        e.set_selection(0, 0);
        e.insert("😀");
        assert_eq!(e.utf16_len(), 7, "counted again after an edit");
        // A long text, in chunks: offsets in a later chunk.
        let long = "x".repeat(super::LONG * 2) + "😀é";
        let mut e = TextEditor::new(long.clone());
        assert!(e.long_text().is_some());
        let n = super::LONG * 2;
        assert_eq!(e.utf16_len(), n + 3);
        assert_eq!(e.byte_to_utf16(n + 4), n + 2);
        assert_eq!(e.utf16_to_byte(n + 2), n + 4);
        assert_eq!(e.utf16_to_byte(n + 1), n, "inside the pair: its start");
    }
    #[test]
    fn replace_range_is_one_undo_step_with_the_caret_after() {
        let mut e = TextEditor::new("hello world");
        e.set_selection(1, 1);
        assert!(e.replace_range(6..11, "there"));
        assert_eq!(
            (e.text().as_ref(), e.selection().focus),
            ("hello there", 11)
        );
        assert!(e.undo());
        assert_eq!(e.text(), "hello world");
        assert!(!e.replace_range(0..5, "hello"), "nothing changed");
        assert_eq!(
            e.selection().range(),
            6..11,
            "a replacement that changes nothing leaves the selection"
        );
    }
    #[test]
    fn preedit_clauses_are_cut_to_the_text_and_go_with_it() {
        use super::ClauseStyle;
        let mut e = TextEditor::new("");
        e.set_preedit_clauses(
            "日本",
            None,
            &[
                (0..4, ClauseStyle::Target),
                (4..99, ClauseStyle::Input),
                (6..6, ClauseStyle::Input),
            ],
        );
        assert_eq!(
            e.preedit_clauses(),
            [(0..3, ClauseStyle::Target), (3..6, ClauseStyle::Input)]
        );
        e.set_preedit("日本", None);
        assert!(e.preedit_clauses().is_empty(), "a preedit without them");
        e.set_preedit_clauses("日本", None, &[(0..3, ClauseStyle::Converted)]);
        e.commit_preedit("日本");
        assert!(e.preedit_clauses().is_empty(), "committed");
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
    fn an_answer_joins_the_step_it_answers() {
        let mut editor = TextEditor::new("ab");
        editor.set_selection(2, 2);
        editor.insert("c ");
        editor.answer_next();
        editor.set_text("ABC");
        assert!(editor.undo());
        assert_eq!(editor.text(), "ab", "one step back, past the answer");
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.text(), "ABC");
        assert!(!editor.redo());
        assert_eq!(editor.undo_bytes + editor.redo_bytes, 2 + 3);
    }

    #[test]
    fn an_answer_that_takes_an_edit_back_leaves_no_step() {
        let mut editor = TextEditor::new("ab");
        editor.set_selection(2, 2);
        editor.insert(" ");
        editor.answer_next();
        editor.set_text("ab");
        assert_eq!(editor.text(), "ab");
        assert!(!editor.undo(), "nothing happened, in the end");
        assert_eq!(editor.undo_bytes, 0);
    }

    #[test]
    fn an_answer_to_an_undo_joins_what_redoes_it() {
        let mut editor = TextEditor::new("one");
        editor.set_selection(3, 3);
        editor.insert(" two");
        assert!(editor.undo());
        // The app makes something else of what the undo left.
        editor.answer_next();
        editor.set_text("ONE");
        assert_eq!(editor.text(), "ONE");
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.text(), "one two", "redo goes where it went");
        assert!(editor.undo());
        assert_eq!(editor.text(), "ONE");
    }

    #[test]
    fn an_answer_that_moves_text_keeps_older_steps_whole() {
        let mut editor = TextEditor::new("日本");
        editor.set_selection(6, 6);
        editor.insert("語");
        editor.set_selection(9, 9);
        editor.insert(" x");
        editor.answer_next();
        editor.set_text("日本語 X");
        assert!(editor.undo());
        assert_eq!(editor.text(), "日本語");
        assert!(editor.undo());
        assert_eq!(editor.text(), "日本");
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
    /// What Backspace takes off one grapheme, as `NSTextView` and Chromium
    /// were measured to (Oct 30): code points on macOS, on the rest.
    #[test]
    fn backspace_takes_what_the_platform_does() {
        let cases: &[(&str, &str, usize, usize)] = &[
            ("accent", "e\u{301}", 2, 1),
            ("two accents", "a\u{323}\u{302}", 3, 1),
            ("Thai tone mark", "\u{E01}\u{E35}\u{E48}", 1, 1),
            ("Thai sara am", "\u{E49}\u{E33}", 1, 1),
            ("Lao", "\u{E81}\u{EC9}", 1, 1),
            ("Devanagari vowel", "\u{915}\u{93F}", 1, 1),
            ("Devanagari conjunct", "\u{915}\u{94D}\u{937}", 1, 1),
            ("Devanagari joiner", "\u{915}\u{94D}\u{200D}", 2, 1),
            ("Tamil", "\u{B95}\u{BCA}", 1, 1),
            ("Bengali", "\u{995}\u{9BF}", 1, 1),
            ("Arabic", "\u{628}\u{651}\u{64E}", 1, 1),
            ("Hebrew", "\u{5E9}\u{5C1}\u{5B8}", 1, 1),
            ("Hangul jamo", "\u{1112}\u{1161}\u{11AB}", 3, 1),
            ("dakuten", "\u{304B}\u{3099}", 2, 1),
            ("skin tone", "\u{1F44D}\u{1F3FD}", 2, 2),
            (
                "ZWJ family",
                "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
                5,
                5,
            ),
            ("flag", "\u{1F1EE}\u{1F1F9}", 2, 2),
            ("keycap", "1\u{FE0F}\u{20E3}", 3, 3),
            ("variation sequence", "\u{2764}\u{FE0F}", 2, 2),
            ("line break", "\r\n", 2, 2),
            ("letter", "a", 1, 1),
        ];
        let bytes = |text: &str, code_points: usize| {
            text.chars()
                .rev()
                .take(code_points)
                .map(char::len_utf8)
                .sum::<usize>()
        };
        for (name, text, macos, other) in cases {
            assert_eq!(
                backspace_takes(text, true),
                bytes(text, *macos),
                "{name} on macOS"
            );
            assert_eq!(
                backspace_takes(text, false),
                bytes(text, *other),
                "{name} elsewhere"
            );
        }
    }

    /// A long document's buffer takes the same as a short one's.
    #[test]
    fn backspace_is_the_same_in_a_long_document() {
        for text in [
            "\u{E01}\u{E35}\u{E48}",
            "e\u{301}",
            "\u{1F1EE}\u{1F1F9}",
            "ab",
        ] {
            let mut short = TextEditor::new(text);
            short.move_document_end(false);
            short.backspace();
            let lines = "line\n".repeat(20_000);
            let mut long = TextEditor::new(format!("{lines}{text}"));
            long.move_document_end(false);
            long.backspace();
            let kept = long.text().to_string();
            assert_eq!(kept.strip_prefix(&lines), Some(&*short.text().to_string()));
        }
    }

    /// Caret movement stays on graphemes.
    #[test]
    fn the_caret_still_moves_by_grapheme() {
        let mut e = TextEditor::new("e\u{301}x");
        e.move_document_end(false);
        e.move_left(false, false);
        e.move_left(false, false);
        assert_eq!(e.selection().focus, 0);
    }

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
    fn paragraph_moves_go_to_its_edges_then_the_next_ones() {
        let mut e = TextEditor::new("one\ntwo three\nfour");
        e.set_selection(8, 8);
        e.move_paragraph(true, false);
        assert_eq!(e.selection().focus, 4, "its start");
        e.move_paragraph(true, false);
        assert_eq!(e.selection().focus, 0, "at a start: the one before's");
        e.move_paragraph(true, false);
        assert_eq!(e.selection().focus, 0, "the text's start stays");
        e.move_paragraph(false, false);
        assert_eq!(e.selection().focus, 3, "its end, before the break");
        e.move_paragraph(false, false);
        assert_eq!(e.selection().focus, 13, "at an end: the next one's");
        e.move_paragraph(false, true);
        assert_eq!(
            e.selection(),
            Selection {
                anchor: 13,
                focus: 18
            },
            "extending"
        );
        e.move_paragraph(false, false);
        assert_eq!(e.selection().focus, 18, "the text's end stays");
    }

    #[test]
    fn dragged_text_moves_and_drops_in_one_undo_step() {
        let mut e = TextEditor::new("one two three");
        // "two " after "three".
        assert!(e.move_text(4..8, 13));
        assert_eq!(e.text(), "one threetwo ");
        assert_eq!(e.selected_text(), "two ", "the moved text stays selected");
        assert!(e.undo());
        assert_eq!(e.text(), "one two three", "one step puts it back");
        assert!(e.redo());
        assert_eq!(e.text(), "one threetwo ");
        assert_eq!(e.selected_text(), "two ");
        // Back before "one".
        let mut e = TextEditor::new("one two three");
        assert!(e.move_text(8..13, 0));
        assert_eq!(e.text(), "threeone two ");
        assert_eq!(e.selected_text(), "three");
        // Into itself: nothing.
        assert!(!e.move_text(0..5, 2));
        assert_eq!(e.text(), "threeone two ");
        // Dropped from elsewhere: inserted, selected, one step.
        let mut e = TextEditor::new("ab");
        assert!(e.insert_dropped(1, "XY"));
        assert_eq!((&*e.text(), &*e.selected_text()), ("aXYb", "XY"));
        assert!(e.undo());
        assert_eq!(e.text(), "ab");
    }

    #[test]
    fn flags_move_and_delete_whole() {
        // Two regional indicators make one flag: a caret never parts them.
        let mut e = TextEditor::new("🇫🇷🇩🇪");
        e.set_selection(0, 0);
        e.move_right(false, false);
        assert_eq!(e.selection().focus, "🇫🇷".len(), "past the first flag whole");
        e.move_right(false, false);
        assert_eq!(e.selection().focus, e.text().len());
        assert!(e.backspace());
        assert_eq!(e.text(), "🇫🇷", "the second flag deleted whole");
        e.set_selection(4, 4);
        assert_eq!(e.selection().focus, 0, "inside a flag: back to its start");
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
