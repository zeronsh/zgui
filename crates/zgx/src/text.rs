//! A long text held once and shared ([`Text`]): in chunks of a few
//! kilobytes, each knowing how many line breaks it holds, so an edit
//! rewrites the chunks it touches and not the text, a copy of it is a
//! pointer (`Clone`), and a line or the line of a byte is found in
//! `O(log n)`. An editor holding a long document, its layout and its
//! semantics share one.
use std::{borrow::Cow, ops::Range, sync::Arc};

/// What a chunk is cut to (a long line lies across several).
const CHUNK: usize = 8 * 1024;
/// A chunk edited past this is cut again.
const MAX: usize = 2 * CHUNK;
// Its line breaks' places fit two bytes (an edit's chunk before it is cut
// is at most `MAX` and what was typed: a paste is cut as it is joined).
const _: () = assert!(MAX < u16::MAX as usize / 2);

#[derive(Clone, Debug)]
struct Chunk {
    text: Arc<str>,
    /// Where its line breaks are (a chunk is never 64 KiB long): two bytes
    /// a line, for finding a line in it without reading it.
    breaks: Arc<[u16]>,
}
impl Chunk {
    fn new(text: &str) -> Self {
        Self {
            breaks: text.match_indices('\n').map(|(at, _)| at as u16).collect(),
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Chunks {
    chunks: Vec<Chunk>,
    /// Before each chunk, and one past the last: its bytes and line breaks.
    bytes: Vec<usize>,
    breaks: Vec<usize>,
}
impl Chunks {
    /// The sums of what comes before each chunk, from `from` on.
    fn sum(&mut self, from: usize) {
        self.bytes.truncate(from + 1);
        self.breaks.truncate(from + 1);
        if self.bytes.is_empty() {
            self.bytes.push(0);
            self.breaks.push(0);
        }
        for chunk in &self.chunks[from..] {
            self.bytes
                .push(self.bytes.last().unwrap() + chunk.text.len());
            self.breaks
                .push(self.breaks.last().unwrap() + chunk.breaks.len());
        }
    }
}

/// `text` cut into chunks of about `CHUNK` bytes, each ending at a line
/// break where one is near (so a line is most often in one chunk), else at
/// a character boundary.
fn cut(text: &str) -> Vec<Chunk> {
    let mut chunks = Vec::with_capacity(text.len() / CHUNK + 1);
    let mut rest = text;
    while rest.len() > CHUNK {
        let end = rest.floor_char_boundary(CHUNK);
        // A line break in the last quarter, if there is one: end after it.
        // (Both bounds on character boundaries: a character may straddle
        // either.)
        let quarter = rest.floor_char_boundary(CHUNK * 3 / 4);
        let end = match rest[quarter..end].rfind('\n') {
            Some(at) => quarter + at + 1,
            None => end,
        };
        chunks.push(Chunk::new(&rest[..end]));
        rest = &rest[end..];
    }
    if !rest.is_empty() || chunks.is_empty() {
        chunks.push(Chunk::new(rest));
    }
    chunks
}

/// An immutable text, shared: cloning it copies a pointer; an edit
/// ([`Text::replace_range`]) changes this one only, copying the chunks it
/// touches and the list of them, never the text.
#[derive(Clone, Debug, Default)]
pub struct Text(Arc<Chunks>);
impl Text {
    /// `text` held in chunks.
    pub fn new(text: &str) -> Self {
        let mut chunks = Chunks {
            chunks: cut(text),
            ..Default::default()
        };
        chunks.sum(0);
        Self(Arc::new(chunks))
    }
    /// Its length in bytes.
    pub fn len(&self) -> usize {
        *self.0.bytes.last().unwrap_or(&0)
    }
    /// Whether it is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// How many lines it has (one more than its line breaks).
    pub fn line_count(&self) -> usize {
        self.0.breaks.last().unwrap_or(&0) + 1
    }
    /// Whether `other` is this same text (a clone of it), not just equal.
    pub fn ptr_eq(&self, other: &Text) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Its chunks, in order: the text, a piece at a time.
    pub fn chunks(&self) -> impl Iterator<Item = &str> {
        self.0.chunks.iter().map(|chunk| &*chunk.text)
    }
    /// The chunk byte `at` is in (the last at the end).
    fn chunk_at(&self, at: usize) -> usize {
        let chunks = &self.0;
        chunks
            .bytes
            .partition_point(|start| *start <= at)
            .saturating_sub(1)
            .min(chunks.chunks.len().saturating_sub(1))
    }
    /// Whether byte `at` is on a character boundary (or the end).
    pub fn is_char_boundary(&self, at: usize) -> bool {
        if at >= self.len() {
            return at == self.len();
        }
        let i = self.chunk_at(at);
        self.0.chunks[i].text.is_char_boundary(at - self.0.bytes[i])
    }
    /// The bytes of `range`: borrowed when they lie in one chunk.
    pub fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        let (start, end) = (range.start.min(self.len()), range.end.min(self.len()));
        if start >= end {
            return Cow::Borrowed("");
        }
        let (first, last) = (self.chunk_at(start), self.chunk_at(end - 1));
        let chunks = &self.0;
        if first == last {
            let base = chunks.bytes[first];
            return Cow::Borrowed(&chunks.chunks[first].text[start - base..end - base]);
        }
        let mut out = String::with_capacity(end - start);
        for i in first..=last {
            let (base, text) = (chunks.bytes[i], &*chunks.chunks[i].text);
            let from = start.saturating_sub(base);
            let to = (end - base).min(text.len());
            out.push_str(&text[from..to]);
        }
        Cow::Owned(out)
    }
    /// The whole text in one string (a copy, if it has more than a chunk).
    pub fn to_cow(&self) -> Cow<'_, str> {
        self.slice(0..self.len())
    }
    /// The line byte `at` is on (a line's end, before its break, is its own;
    /// the byte after the break, the next line's).
    pub fn line_of(&self, at: usize) -> usize {
        let at = at.min(self.len());
        let i = self.chunk_at(at);
        let chunks = &self.0;
        let local = at - chunks.bytes[i];
        chunks.breaks[i]
            + chunks.chunks[i]
                .breaks
                .partition_point(|b| (*b as usize) < local)
    }
    /// Where line `line` starts (the end, past the last line).
    pub fn line_start(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        let chunks = &self.0;
        // The chunk holding the line's break before it: the first whose
        // breaks reach `line`.
        let i = chunks.breaks.partition_point(|before| *before < line);
        if i == 0 || i > chunks.chunks.len() {
            return self.len();
        }
        let i = i - 1;
        let nth = line - chunks.breaks[i] - 1;
        match chunks.chunks[i].breaks.get(nth) {
            Some(at) => chunks.bytes[i] + *at as usize + 1,
            None => self.len(),
        }
    }
    /// Line `line`'s bytes, its break excluded.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let start = self.line_start(line);
        let end = if line + 1 < self.line_count() {
            self.line_start(line + 1) - 1
        } else {
            self.len()
        };
        start..end.max(start)
    }
    /// Line `line`, its break excluded: borrowed when in one chunk.
    pub fn line(&self, line: usize) -> Cow<'_, str> {
        self.slice(self.line_range(line))
    }
    /// The bytes this and `other` start with in common: the chunks they
    /// share skipped by pointer (an edit's copy shares all but its own).
    pub fn common_prefix(&self, other: &Text) -> usize {
        let (a, b) = (&self.0.chunks, &other.0.chunks);
        let shared = a
            .iter()
            .zip(b)
            .take_while(|(x, y)| Arc::ptr_eq(&x.text, &y.text))
            .count();
        let at = self.0.bytes.get(shared).copied().unwrap_or(0);
        at + prefix_of(
            a[shared..].iter().map(|c| c.text.as_bytes()),
            b[shared..].iter().map(|c| c.text.as_bytes()),
        )
    }
    /// The bytes this and `other` end with in common, after `prefix`.
    pub fn common_suffix(&self, other: &Text, prefix: usize) -> usize {
        let (a, b) = (&self.0.chunks, &other.0.chunks);
        let shared = a
            .iter()
            .rev()
            .zip(b.iter().rev())
            .take_while(|(x, y)| Arc::ptr_eq(&x.text, &y.text))
            .count();
        let room = (self.len() - prefix).min(other.len() - prefix);
        let bytes: usize = a[a.len() - shared..].iter().map(|c| c.text.len()).sum();
        if bytes >= room {
            return room;
        }
        bytes
            + suffix_of(
                a[..a.len() - shared]
                    .iter()
                    .rev()
                    .map(|c| c.text.as_bytes()),
                b[..b.len() - shared]
                    .iter()
                    .rev()
                    .map(|c| c.text.as_bytes()),
                room - bytes,
            )
    }
    /// Its longest line's length in bytes.
    pub fn longest_line(&self) -> usize {
        let (mut longest, mut run) = (0, 0);
        for chunk in self.chunks() {
            let mut last = 0;
            for (at, _) in chunk.match_indices('\n') {
                longest = longest.max(run + at - last);
                run = 0;
                last = at + 1;
            }
            run += chunk.len() - last;
        }
        longest.max(run)
    }
    /// Replace `range` with `with`: the chunks it touches are made again
    /// (and cut, if they grew long), the rest shared with the text's other
    /// copies.
    pub fn replace_range(&mut self, range: Range<usize>, with: &str) {
        let len = self.len();
        let (start, end) = (
            range.start.min(len),
            range.end.min(len).max(range.start.min(len)),
        );
        let chunks = Arc::make_mut(&mut self.0);
        if chunks.chunks.is_empty() {
            chunks.chunks.push(Chunk::new(""));
            chunks.sum(0);
        }
        let first = chunks
            .bytes
            .partition_point(|s| *s <= start)
            .saturating_sub(1)
            .min(chunks.chunks.len() - 1);
        let last = chunks
            .bytes
            .partition_point(|s| *s < end)
            .saturating_sub(1)
            .max(first)
            .min(chunks.chunks.len() - 1);
        let base = chunks.bytes[first];
        let tail_base = chunks.bytes[last];
        let mut joined =
            String::with_capacity(chunks.bytes[last + 1] - base - (end - start) + with.len());
        joined.push_str(&chunks.chunks[first].text[..start - base]);
        joined.push_str(with);
        joined.push_str(&chunks.chunks[last].text[end - tail_base..]);
        let made = if joined.len() > MAX {
            cut(&joined)
        } else {
            vec![Chunk::new(&joined)]
        };
        chunks.chunks.splice(first..=last, made);
        // An empty chunk goes, unless it is the only one.
        if chunks.chunks.len() > 1 {
            chunks.chunks.retain(|chunk| !chunk.text.is_empty());
        }
        chunks.sum(first.min(chunks.chunks.len().saturating_sub(1)));
    }
}
impl Text {
    /// A handle to this text that does not keep it.
    pub fn downgrade(&self) -> WeakText {
        WeakText(Arc::downgrade(&self.0))
    }
}
/// A [`Text`] not kept alive by this handle ([`Text::downgrade`]).
#[derive(Clone, Debug, Default)]
pub struct WeakText(std::sync::Weak<Chunks>);
impl WeakText {
    /// The text, if anything still holds it.
    pub fn upgrade(&self) -> Option<Text> {
        self.0.upgrade().map(Text)
    }
}
/// Shared chunks compare equal at a pointer's glance.
impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || (self.len() == other.len() && self.common_prefix(other) == self.len())
    }
}
impl PartialEq<str> for Text {
    fn eq(&self, other: &str) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let mut at = 0;
        self.chunks().all(|chunk| {
            let same = other.as_bytes()[at..at + chunk.len()] == *chunk.as_bytes();
            at += chunk.len();
            same
        })
    }
}
impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}
impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.chunks().try_for_each(|chunk| f.write_str(chunk))
    }
}

/// The bytes two texts, given in pieces, start with in common: a run at a
/// time where they agree (a memory compare), byte by byte only where they
/// part.
fn prefix_of<'a>(
    mut a: impl Iterator<Item = &'a [u8]>,
    mut b: impl Iterator<Item = &'a [u8]>,
) -> usize {
    let (mut x, mut y): (&[u8], &[u8]) = (&[], &[]);
    let mut same = 0;
    loop {
        if x.is_empty() {
            match a.next() {
                Some(next) => x = next,
                None => return same,
            }
            continue;
        }
        if y.is_empty() {
            match b.next() {
                Some(next) => y = next,
                None => return same,
            }
            continue;
        }
        let n = x.len().min(y.len());
        let run = agree(&x[..n], &y[..n]);
        same += run;
        if run < n {
            return same;
        }
        (x, y) = (&x[n..], &y[n..]);
    }
}
/// The bytes two texts end with in common, up to `limit`: their pieces
/// given last first.
fn suffix_of<'a>(
    mut a: impl Iterator<Item = &'a [u8]>,
    mut b: impl Iterator<Item = &'a [u8]>,
    limit: usize,
) -> usize {
    let (mut x, mut y): (&[u8], &[u8]) = (&[], &[]);
    let mut same = 0;
    while same < limit {
        if x.is_empty() {
            match a.next() {
                Some(next) => x = next,
                None => break,
            }
            continue;
        }
        if y.is_empty() {
            match b.next() {
                Some(next) => y = next,
                None => break,
            }
            continue;
        }
        let n = x.len().min(y.len()).min(limit - same);
        let run = agree_back(&x[x.len() - n..], &y[y.len() - n..]);
        same += run;
        if run < n {
            break;
        }
        (x, y) = (&x[..x.len() - n], &y[..y.len() - n]);
    }
    same
}
/// How far two equally long runs agree from their start.
fn agree(a: &[u8], b: &[u8]) -> usize {
    let mut at = 0;
    while at + 64 <= a.len() && a[at..at + 64] == b[at..at + 64] {
        at += 64;
    }
    at + a[at..]
        .iter()
        .zip(&b[at..])
        .take_while(|(x, y)| x == y)
        .count()
}
/// How far two equally long runs agree from their end.
fn agree_back(a: &[u8], b: &[u8]) -> usize {
    let mut at = a.len();
    while at >= 64 && a[at - 64..at] == b[at - 64..at] {
        at -= 64;
    }
    a.len() - at
        + a[..at]
            .iter()
            .rev()
            .zip(b[..at].iter().rev())
            .take_while(|(x, y)| x == y)
            .count()
}
/// The bytes two texts, given in pieces, start with in common, and then
/// end with (after those).
pub(crate) fn common(a: &[&str], b: &[&str]) -> (usize, usize) {
    let bytes = |pieces: &[&str]| pieces.iter().map(|p| p.len()).sum::<usize>();
    let prefix = prefix_of(
        a.iter().map(|p| p.as_bytes()),
        b.iter().map(|p| p.as_bytes()),
    );
    let room = (bytes(a) - prefix).min(bytes(b) - prefix);
    let suffix = suffix_of(
        a.iter().rev().map(|p| p.as_bytes()),
        b.iter().rev().map(|p| p.as_bytes()),
        room,
    );
    (prefix, suffix)
}

/// A text past this many bytes is held in chunks ([`TextValue::Long`]); one
/// edited below half of it, in a string again.
pub const LONG: usize = 64 * 1024;

/// A text to keep and bind an editor to (`text_area` takes a
/// `Signal<TextValue>` as well as a `Signal<String>`): a short one as a
/// string, a long one as a [`Text`]. An editor bound to a long document
/// then holds it once with its binding, and an edit's new value costs the
/// chunks it touched, not a copy of the document.
#[derive(Clone, Debug)]
pub enum TextValue {
    /// A short text.
    Flat(String),
    /// A long text, shared.
    Long(Text),
}
impl TextValue {
    /// Its length in bytes.
    pub fn len(&self) -> usize {
        match self {
            TextValue::Flat(text) => text.len(),
            TextValue::Long(text) => text.len(),
        }
    }
    /// Whether it is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// All of it in one string: borrowed while short, a copy of a long one.
    pub fn to_cow(&self) -> Cow<'_, str> {
        match self {
            TextValue::Flat(text) => Cow::Borrowed(text),
            TextValue::Long(text) => text.to_cow(),
        }
    }
    /// Its pieces, in order.
    pub fn chunks(&self) -> impl Iterator<Item = &str> {
        let (flat, long) = match self {
            TextValue::Flat(text) => (Some(text.as_str()), None),
            TextValue::Long(text) => (None, Some(text.chunks())),
        };
        flat.into_iter().chain(long.into_iter().flatten())
    }
    /// The long text it holds, shared.
    pub fn as_text(&self) -> Option<&Text> {
        match self {
            TextValue::Flat(_) => None,
            TextValue::Long(text) => Some(text),
        }
    }
    /// Whether `other` is this same long text (a clone of it), not just
    /// equal.
    pub fn ptr_eq(&self, other: &TextValue) -> bool {
        matches!((self, other), (TextValue::Long(a), TextValue::Long(b)) if a.ptr_eq(b))
    }
}
impl Default for TextValue {
    fn default() -> Self {
        TextValue::Flat(String::new())
    }
}
/// In chunks if long.
impl From<String> for TextValue {
    fn from(text: String) -> Self {
        if text.len() > LONG {
            TextValue::Long(Text::new(&text))
        } else {
            TextValue::Flat(text)
        }
    }
}
impl From<&str> for TextValue {
    fn from(text: &str) -> Self {
        if text.len() > LONG {
            TextValue::Long(Text::new(text))
        } else {
            TextValue::Flat(text.to_owned())
        }
    }
}
impl From<Text> for TextValue {
    fn from(text: Text) -> Self {
        TextValue::Long(text)
    }
}
/// Equal texts are equal however they are held.
impl PartialEq for TextValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TextValue::Flat(a), TextValue::Flat(b)) => a == b,
            (TextValue::Long(a), TextValue::Long(b)) => a == b,
            (TextValue::Flat(s), TextValue::Long(t)) | (TextValue::Long(t), TextValue::Flat(s)) => {
                *t == **s
            }
        }
    }
}
impl PartialEq<str> for TextValue {
    fn eq(&self, other: &str) -> bool {
        match self {
            TextValue::Flat(text) => text == other,
            TextValue::Long(text) => *text == *other,
        }
    }
}
impl std::fmt::Display for TextValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.chunks().try_for_each(|chunk| f.write_str(chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2-, 3- and 4-byte character at every byte around where chunks are
    /// cut (the last quarter's start, a chunk's end, the next chunk's),
    /// with and without line breaks: the text cut, sliced and edited there
    /// is the text, and every chunk ends on a character boundary.
    #[test]
    fn a_character_straddling_any_cut_is_kept_whole() {
        for wide in ["é", "候", "😀"] {
            for around in [CHUNK * 3 / 4, CHUNK, 2 * CHUNK, 2 * CHUNK + CHUNK * 3 / 4] {
                for at in around - 5..around + 5 {
                    for newline in [false, true] {
                        let mut plain = "a".repeat(at);
                        plain.push_str(wide);
                        plain.push_str(&"b".repeat(3 * CHUNK - at));
                        if newline {
                            plain.insert(CHUNK * 3 / 4 + 40, '\n');
                        }
                        let mut text = Text::new(&plain);
                        assert_eq!(text.to_cow(), plain, "{wide} at {at}");
                        assert!(text.chunks().all(|chunk| !chunk.is_empty()));
                        let start = plain.find(wide).unwrap();
                        let end = start + wide.len();
                        assert_eq!(text.slice(start..end), wide);
                        // Edited where it is: removed, and put back twice.
                        text.replace_range(start..end, "");
                        plain.replace_range(start..end, "");
                        assert_eq!(text.to_cow(), plain, "{wide} removed at {at}");
                        let twice = wide.repeat(2);
                        text.replace_range(start..start, &twice);
                        plain.replace_range(start..start, &twice);
                        assert_eq!(text.to_cow(), plain, "{wide} put back at {at}");
                    }
                }
            }
        }
    }

    fn lines(n: usize) -> String {
        (0..n)
            .map(|i| format!("line {i} {}\n", "x".repeat(i % 50)))
            .collect()
    }

    /// Lines, slices and edits read as the same string's do, across chunks.
    #[test]
    fn a_text_in_chunks_reads_and_edits_as_its_string_does() {
        let mut plain = lines(3_000);
        let mut text = Text::new(&plain);
        assert!(text.0.chunks.len() > 10);
        let check = |text: &Text, plain: &str| {
            assert_eq!(text.len(), plain.len());
            assert!(*text == *plain);
            let starts: Vec<usize> = std::iter::once(0)
                .chain(plain.match_indices('\n').map(|(at, _)| at + 1))
                .collect();
            assert_eq!(text.line_count(), starts.len());
            for (line, start) in starts.iter().enumerate().step_by(97) {
                assert_eq!(text.line_start(line), *start, "line {line}");
                assert_eq!(text.line_of(*start), line);
                let end = plain[*start..]
                    .find('\n')
                    .map_or(plain.len(), |i| start + i);
                assert_eq!(text.line(line), &plain[*start..end]);
            }
            for (a, b) in [(0, 10), (8190, 8200), (100, 20_000), (0, plain.len())] {
                let (a, b) = (a.min(plain.len()), b.min(plain.len()));
                assert_eq!(text.slice(a..b), &plain[a..b]);
            }
        };
        check(&text, &plain);
        let before = text.clone();
        for (start, end, with) in [
            (0, 0, "head\n"),
            (5000, 5010, ""),
            (8100, 8300, "a line\nacross\nchunks"),
            (20_000, 20_000, &"long ".repeat(10_000)),
            (1000, 60_000, "x"),
        ] {
            let (start, end) = (start.min(plain.len()), end.min(plain.len()));
            plain.replace_range(start..end, with);
            text.replace_range(start..end, with);
            check(&text, &plain);
        }
        // The copy taken before the edits is as it was.
        assert!(before == *lines(3_000));
        assert!(!before.ptr_eq(&text));
    }

    /// Two copies of a text, one edited, find what they share as two
    /// strings would: the chunks the edit did not touch by pointer.
    #[test]
    fn copies_find_what_they_start_and_end_with() {
        let plain = lines(5_000);
        let original = Text::new(&plain);
        for (start, end, with) in [
            (0, 0, "x"),
            (40_000, 40_010, "abc\n"),
            (plain.len(), plain.len(), "!"),
        ] {
            let mut edited = original.clone();
            edited.replace_range(start..end, with);
            let mut other = plain.clone();
            other.replace_range(start..end, with);
            let prefix = plain
                .bytes()
                .zip(other.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            let room = (plain.len() - prefix).min(other.len() - prefix);
            let suffix = plain
                .bytes()
                .rev()
                .zip(other.bytes().rev())
                .take(room)
                .take_while(|(a, b)| a == b)
                .count();
            assert_eq!(original.common_prefix(&edited), prefix);
            assert_eq!(original.common_suffix(&edited, prefix), suffix);
        }
        assert_eq!(
            original.longest_line(),
            plain.lines().map(str::len).max().unwrap()
        );
    }

    /// An edit copies the chunks it touches and a list of pointers, not the
    /// text: a copy of the text shares the rest.
    #[test]
    fn an_edit_shares_the_chunks_it_does_not_touch() {
        let original = Text::new(&lines(20_000));
        let mut edited = original.clone();
        edited.replace_range(100..100, "x");
        let shared = original
            .0
            .chunks
            .iter()
            .zip(&edited.0.chunks)
            .filter(|(a, b)| Arc::ptr_eq(&a.text, &b.text))
            .count();
        assert!(shared >= original.0.chunks.len() - 1, "{shared}");
    }

    /// Texts in pieces cut in different places compare as the strings they
    /// make; a value is equal to its text however it is held.
    #[test]
    fn pieces_compare_as_their_strings_and_values_as_their_text() {
        let plain = lines(4_000);
        let long = Text::new(&plain);
        let mut edited = plain.clone();
        edited.replace_range(30_000..30_010, "changed!");
        let halves = [&edited[..12_345], &edited[12_345..]];
        let (prefix, suffix) = common(&long.chunks().collect::<Vec<_>>(), &halves);
        let expect_prefix = plain
            .bytes()
            .zip(edited.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        let expect_suffix = plain
            .bytes()
            .rev()
            .zip(edited.bytes().rev())
            .take(plain.len().min(edited.len()) - expect_prefix)
            .take_while(|(a, b)| a == b)
            .count();
        assert_eq!((prefix, suffix), (expect_prefix, expect_suffix));
        assert_eq!(common(&[&plain], &[&plain]), (plain.len(), 0));
        assert_eq!(common(&["abc"], &["abd"]), (2, 0));
        assert_eq!(common(&["xab", "c"], &["y", "abc"]), (0, 3));
        // Values: a long one in chunks, a short one a string, equal by text.
        let value = TextValue::from(plain.as_str());
        assert!(value.as_text().is_some() && value == TextValue::Flat(plain.clone()));
        let other = TextValue::from(edited.as_str());
        assert!(value == *plain.as_str() && value != other);
        assert!(TextValue::from("short").as_text().is_none());
        assert!(value.ptr_eq(&value.clone()) && !value.ptr_eq(&TextValue::from(plain.as_str())));
        assert_eq!(value.to_string(), plain);
    }
}
