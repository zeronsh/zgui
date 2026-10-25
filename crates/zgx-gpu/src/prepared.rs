//! Prepared text: shape once, lay out at any width many times.
//!
//! Measuring a paragraph used to build, shape and discard a cosmic-text buffer
//! on every call, and the renderer then shaped it again. Here a text is
//! prepared once, keyed by what determines its shapes (the text and its font
//! runs, not colours or other paint), and kept in two tiers:
//!
//! - Shaped lines, for the renderer and for text the compact tier cannot
//!   describe. Measuring at a new width re-runs cosmic-text's line breaking
//!   over the kept shapes. About 190 bytes per character, so these go first.
//! - Compact measurements, in the spirit of Pretext: each word's advance,
//!   blank flag and line height, and glyph advances for words too long for a
//!   line. Breaking lines at any width is arithmetic over these, replaying
//!   cosmic-text's word-or-glyph wrapping exactly (the same values added in
//!   the same order). About 8 bytes per character, so the heights of a whole
//!   long transcript stay at hand. Only left-to-right, unjustified lines
//!   qualify; the rest measure from shaped lines.
//!
//! Sizes are memoised per width, and layout and the renderer break lines from
//! the same data. A text that grows (streaming) reuses every line it shares
//! with a recently prepared text.
use cosmic_text::{
    Align, BufferLine, Ellipsize, FontSystem, Hinting, LayoutRunIter, Metrics, Wrap,
};
use rustc_hash::{FxHashMap, FxHasher};
use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    ops::Range,
    sync::Arc,
};
use zgx::{rich_text::RichText, text_layout::FontStyle};

/// Shaped lines are kept at most this many bytes (estimated), and compact
/// measurements at most `COMPACT_BUDGET`. Past either, the least recently used
/// go, down to three quarters of it at once so eviction runs rarely.
const SHAPED_BUDGET: usize = 32 * 1024 * 1024;
const COMPACT_BUDGET: usize = 16 * 1024 * 1024;
/// Shaped text a renderer does not draw is kept to this (see `trim`): a few
/// rows scrolled away or a value switched back. A dashboard whose live values
/// all change every frame held about 1.3 MB more heap at 512 KB, for no
/// measurable CPU difference.
const UNDRAWN_BUDGET: usize = 64 * 1024;
/// Estimated bytes per character of shaped lines: a shape glyph and a layout
/// glyph (96 + 88 bytes), and words and line text.
const SHAPED_BYTES_PER_CHAR: usize = 190;
/// Recently prepared entries a new text may reuse lines from.
const RECENT: usize = 6;
/// Widths whose sizes an entry remembers.
const SIZES: usize = 4;
/// As `Buffer` lays out.
const TAB_WIDTH: u16 = 8;
/// Texts at least this long (bytes) that a new text extends are dropped as a
/// stream's previous states.
const SUPERSEDED: usize = 32;
/// Lines of dropped texts kept for new texts to shape into, and the longest
/// kept: values that change every frame replace as many texts as they shape.
/// Only short lines: a reused line keeps the capacity it grew to.
const SPARE_LINES: usize = 128;
const SPARE_TEXT: usize = 32;
/// A few longer lines too, for a stream's window of text: its lines all shift
/// as it grows, so each frame replaces every one.
const SPARE_LONG_LINES: usize = 16;
const SPARE_LONG_TEXT: usize = 256;

/// A run as shaped: its range, font, size, and padding across.
type Span = (Range<usize>, FontStyle, f32, f32);
/// The spans that determine shapes: merged runs of one font and size.
#[derive(Clone, PartialEq)]
struct Key {
    text: Arc<str>,
    /// Runs as shaped: range, font, size and padding across (inline for
    /// the usual one: a key is made for every new text).
    spans: smallvec::SmallVec<[Span; 1]>,
    /// Plain text keeps a row after a final line break, for an editor's
    /// caret (as `ShapedText::with_font` lays it out); rich text does not.
    caret_row: bool,
}
impl Key {
    fn of(rich: &RichText, caret_row: bool) -> Self {
        let mut spans: smallvec::SmallVec<[Span; 1]> = smallvec::SmallVec::new();
        for run in rich.runs() {
            match spans.last_mut() {
                Some((range, font, size, padding))
                    if *font == run.font
                        && *size == run.font_size
                        && *padding == 0.
                        && run.padding[0] == 0.
                        && range.end == run.range.start =>
                {
                    range.end = run.range.end;
                }
                _ => spans.push((
                    run.range.clone(),
                    run.font.clone(),
                    run.font_size,
                    run.padding[0],
                )),
            }
        }
        Self {
            text: rich.text_arc(),
            spans,
            caret_row,
        }
    }
    fn hash(&self) -> u64 {
        let mut hasher = FxHasher::default();
        self.text.hash(&mut hasher);
        self.caret_row.hash(&mut hasher);
        for (range, font, size, padding) in &self.spans {
            Self::hash_span(&mut hasher, range, font, *size, *padding);
        }
        hasher.finish()
    }
    fn hash_span(
        hasher: &mut FxHasher,
        range: &Range<usize>,
        font: &FontStyle,
        size: f32,
        padding: f32,
    ) {
        range.hash(hasher);
        size.to_bits().hash(hasher);
        padding.to_bits().hash(hasher);
        font.weight.hash(hasher);
        font.italic.hash(hasher);
    }
    /// Plain text's key: one run over all of it, as `Key::of` makes for
    /// that text as rich text.
    fn plain(text: &Arc<str>, font_size: f32, font: &FontStyle, caret_row: bool) -> Self {
        Self {
            text: text.clone(),
            spans: smallvec::smallvec![(0..text.len(), font.clone(), font_size, 0.)],
            caret_row,
        }
    }
    /// `hash` of plain text's key (`plain`, with its caret row), without
    /// building it.
    fn plain_hash(text: &str, font_size: f32, font: &FontStyle) -> u64 {
        let mut hasher = FxHasher::default();
        text.hash(&mut hasher);
        true.hash(&mut hasher);
        Self::hash_span(&mut hasher, &(0..text.len()), font, font_size, 0.);
        hasher.finish()
    }
    /// Whether this is plain text's key (`plain_hash`).
    fn is_plain(&self, text: &str, font_size: f32, font: &FontStyle) -> bool {
        self.caret_row
            && *self.text == *text
            && matches!(&self.spans[..], [(range, f, size, padding)]
                if *range == (0..text.len()) && f == font && *size == font_size && *padding == 0.)
    }
}

/// A shaped word, reduced to what line breaking reads (in a chunk of the
/// `Slab`, packed: see `Word::pack`).
#[derive(Clone, Copy)]
struct Word {
    width: f32,
    blank: bool,
    /// The first of its glyph advances among its compact's (`Compact::glyphs`);
    /// the next word's first ends them.
    glyphs: u32,
    /// The line height its glyphs set, if they set one.
    line_height: Option<f32>,
}

/// A word packed in three of a chunk's words: its width, its first glyph
/// advance with `blank` in the top bit, and its line height (`NONE` for
/// none, a NaN no font's metrics give).
type Packed = [u32; 3];
impl Word {
    const NONE: u32 = u32::MAX;
    const BLANK: u32 = 1 << 31;
    fn pack(self) -> Packed {
        [
            self.width.to_bits(),
            self.glyphs | if self.blank { Self::BLANK } else { 0 },
            self.line_height.map_or(Self::NONE, f32::to_bits),
        ]
    }
    #[inline]
    fn of(packed: &Packed) -> Self {
        Self {
            width: f32::from_bits(packed[0]),
            blank: packed[1] & Self::BLANK != 0,
            glyphs: packed[1] & !Self::BLANK,
            line_height: (packed[2] != Self::NONE).then(|| f32::from_bits(packed[2])),
        }
    }
}

/// A text's line breaking inputs, without glyphs.
struct Compact {
    /// Its packed words (`Packed`), then their glyph advances, at
    /// `start..end` of chunk `chunk` of the cache's `Slab`.
    chunk: u32,
    start: u32,
    end: u32,
    words: u32,
    /// Every line's spans, as ranges of `words` (inline for the usual one:
    /// a compact is made for every new text).
    spans: smallvec::SmallVec<[Range<u32>; 1]>,
    /// Per line: its spans, as a range of `spans`, and the height it takes
    /// when empty.
    lines: smallvec::SmallVec<[(Range<u32>, Option<f32>); 1]>,
}
impl Compact {
    /// The measurements of shaped `lines`, or None where arithmetic over them
    /// would not match cosmic-text: right-to-left or mixed-direction lines,
    /// justified lines, or a word whose glyphs set different line heights.
    /// Its words and advances go at the end of `slab`'s lists.
    fn of(lines: &[BufferLine], font_size: f32, slab: &mut Slab) -> Option<Self> {
        // Counted first, the lists grow once: a text changing every frame
        // (a stream's) is prepared every frame.
        let (mut spans, mut words, mut glyphs) = (0, 0, 0);
        for line in lines {
            let shape = line.shape_opt()?;
            if shape.rtl || line.align() == Some(Align::Justified) {
                return None;
            }
            spans += shape.spans.len();
            for span in &shape.spans {
                words += span.words.len();
                glyphs += span
                    .words
                    .iter()
                    .map(|word| word.glyphs.len())
                    .sum::<usize>();
            }
        }
        let index = slab.room(3 * words + glyphs);
        let chunk = slab.chunks[index].as_mut().expect("live chunk");
        let start = chunk.data.len();
        // Its words, then their advances, filled in place.
        chunk.data.resize(start + 3 * words + glyphs, 0);
        let (packed, advances) = chunk.data[start..].split_at_mut(3 * words);
        let (packed, _) = packed.as_chunks_mut::<3>();
        let mut compact = Self {
            chunk: index as u32,
            start: start as u32,
            end: (start + 3 * words + glyphs) as u32,
            words: words as u32,
            spans: smallvec::SmallVec::with_capacity(spans),
            lines: smallvec::SmallVec::with_capacity(lines.len()),
        };
        let (mut word_at, mut glyph_at) = (0, 0);
        // What does not qualify leaves the chunk as it was.
        let filled = (|| {
            for line in lines {
                let shape = line.shape_opt()?;
                let first_span = compact.spans.len() as u32;
                for span in &shape.spans {
                    if span.level.is_rtl() {
                        return None;
                    }
                    let first = word_at as u32;
                    for word in &span.words {
                        let glyphs = glyph_at as u32;
                        let line_height = word
                            .glyphs
                            .first()
                            .and_then(|glyph| glyph.metrics_opt)
                            .map(|metrics| metrics.line_height);
                        for glyph in &word.glyphs {
                            if glyph.metrics_opt.map(|metrics| metrics.line_height) != line_height {
                                return None;
                            }
                            advances[glyph_at] = glyph.width(font_size).to_bits();
                            glyph_at += 1;
                        }
                        packed[word_at] = Word {
                            width: word.width(font_size),
                            blank: word.blank,
                            glyphs,
                            line_height,
                        }
                        .pack();
                        word_at += 1;
                    }
                    compact.spans.push(first..word_at as u32);
                }
                compact.lines.push((
                    first_span..compact.spans.len() as u32,
                    shape.metrics_opt.map(|metrics| metrics.line_height),
                ));
            }
            Some(())
        })();
        if filled.is_none() {
            // As if made and dropped at once: no gap, no text counted.
            slab.unmake(index, start);
            return None;
        }
        Some(compact)
    }
    fn bytes(&self) -> usize {
        (self.end - self.start) as usize * size_of::<u32>()
            + self.spans.len() * size_of::<Range<u32>>()
            + self.lines.len() * size_of::<(Range<u32>, Option<f32>)>()
    }
    /// Word `word`'s glyph advances (as bits), of `words` and `glyphs`
    /// (this compact's).
    fn advances<'a>(words: &[Packed], glyphs: &'a [u32], word: usize) -> &'a [u32] {
        let end = words
            .get(word + 1)
            .map_or(glyphs.len(), |next| Word::of(next).glyphs as usize);
        &glyphs[Word::of(&words[word]).glyphs as usize..end]
    }
    /// The size cosmic-text's word-or-glyph wrapping gives these lines at
    /// `width`: the widest line, and the sum of line heights. This follows
    /// its congruent-direction branch step by step.
    fn size(&self, slab: &Slab, width: Option<f32>, line_height: f32) -> (f32, f32) {
        let (all_words, glyphs) = slab.of(self);
        let limit = crate::text::wrap_width(width).unwrap_or(f32::INFINITY);
        let (mut widest, mut height) = (0_f32, 0_f32);
        for (spans, empty) in &self.lines {
            let spans = &self.spans[spans.start as usize..spans.end as usize];
            let mut line = Visual::default();
            let mut any = false;
            let mut push = |line: &mut Visual| {
                if line.nonempty {
                    widest = widest.max(line.w);
                    height += line.height.unwrap_or(line_height);
                    any = true;
                }
                *line = Visual::default();
            };
            for span in spans {
                let offset = span.start as usize;
                let words = &all_words[offset..span.end as usize];
                // Where the words not yet added start: (word, glyph).
                let mut fitting = (0, 0);
                let mut range_width = 0_f32;
                let mut before_last_blank = 0_f32;
                for (i, packed) in words.iter().enumerate() {
                    let word = Word::of(packed);
                    if line.w + (range_width + word.width) <= limit
                        || (word.blank && line.w + range_width <= limit)
                    {
                        if word.blank {
                            before_last_blank = range_width;
                        }
                        range_width += word.width;
                    } else if word.width > limit {
                        // Too wide for any line: finish the line, then break
                        // the word between glyphs.
                        if range_width > 0. {
                            line.add(words, fitting, (i, 0), range_width);
                            push(&mut line);
                            range_width = 0.;
                            fitting = (i, 0);
                        }
                        let advances = Self::advances(all_words, glyphs, offset + i);
                        for (glyph, advance) in advances.iter().enumerate() {
                            let advance = f32::from_bits(*advance);
                            if line.w + (range_width + advance) <= limit {
                                range_width += advance;
                            } else {
                                line.add(words, fitting, (i, glyph), range_width);
                                push(&mut line);
                                range_width = advance;
                                fitting = (i, glyph);
                            }
                        }
                    } else {
                        if range_width > 0. {
                            // A blank before the break is left out.
                            if i > 0 && Word::of(&words[i - 1]).blank {
                                line.add(words, fitting, (i - 1, 0), before_last_blank);
                            } else {
                                line.add(words, fitting, (i, 0), range_width);
                            }
                        }
                        push(&mut line);
                        if word.blank {
                            range_width = 0.;
                            fitting = (i + 1, 0);
                        } else {
                            range_width = word.width;
                            fitting = (i, 0);
                        }
                    }
                }
                line.add(words, fitting, (words.len(), 0), range_width);
            }
            push(&mut line);
            if !any {
                height += empty.unwrap_or(line_height);
            }
        }
        (widest, height)
    }
}

/// Every compact's words and glyph advances, in chunks of a fixed size
/// rather than two allocations a text: texts dropped among kept ones left
/// allocator pages it could not give back (in a dashboard, 2 MiB of holes
/// in the 3.5 and 7 KB size classes), and one list grown by doubling held
/// its old and new copies at once. A text goes in the newest chunk with
/// room (one too big for a chunk gets its own), its packed words then its
/// advances; a chunk whose texts are all dropped is freed, its pages with
/// it, and one mostly gaps has its texts moved to the newest and is freed.
#[derive(Default)]
struct Slab {
    chunks: Vec<Option<Chunk>>,
    /// Where texts go while it has room.
    newest: Option<usize>,
}
struct Chunk {
    data: Vec<u32>,
    /// Texts in it, and the length dropped ones held.
    texts: usize,
    dropped: usize,
}
/// A chunk's length: 64 KiB, an allocation of whole pages of its own,
/// given back whole.
const CHUNK: usize = 16 * 1024;
impl Slab {
    /// `compact`'s words and glyph advances.
    fn of(&self, compact: &Compact) -> (&[Packed], &[u32]) {
        let chunk = self.chunks[compact.chunk as usize]
            .as_ref()
            .expect("live chunk");
        let data = &chunk.data[compact.start as usize..compact.end as usize];
        let (words, glyphs) = data.split_at(3 * compact.words as usize);
        (words.as_chunks().0, glyphs)
    }
    /// The chunk a text of `length` goes in, counted in it.
    fn room(&mut self, length: usize) -> usize {
        let fits = |chunk: &Chunk| chunk.data.capacity() - chunk.data.len() >= length;
        let index = match self.newest {
            Some(index) if self.chunks[index].as_ref().is_some_and(fits) => index,
            _ => {
                let large = length > CHUNK;
                let chunk = Chunk {
                    data: Vec::with_capacity(length.max(CHUNK)),
                    texts: 0,
                    dropped: 0,
                };
                let index = match self.chunks.iter().position(Option::is_none) {
                    Some(free) => {
                        self.chunks[free] = Some(chunk);
                        free
                    }
                    None => {
                        self.chunks.push(Some(chunk));
                        self.chunks.len() - 1
                    }
                };
                // A text's own chunk takes no others.
                if !large {
                    self.newest = Some(index);
                }
                index
            }
        };
        self.chunks[index].as_mut().expect("live chunk").texts += 1;
        index
    }
    /// Uncount a text from chunk `index`, which ends at `start` again; a
    /// chunk left with no texts is freed.
    fn unmake(&mut self, index: usize, start: usize) {
        let chunk = self.chunks[index].as_mut().expect("live chunk");
        chunk.data.truncate(start);
        chunk.texts -= 1;
        if chunk.texts == 0 {
            self.chunks[index] = None;
            if self.newest == Some(index) {
                self.newest = None;
            }
        }
    }
    /// Count `compact`'s part of its chunk as a gap (none at the chunk's
    /// end: a stream's previous state, dropped just after it was made); a
    /// chunk left with no texts is freed, and one mostly gaps has its
    /// texts, found among `entries`, moved to the newest chunk.
    fn drop(&mut self, compact: &Compact, entries: &mut FxHashMap<u64, Entry>) {
        let index = compact.chunk as usize;
        let chunk = self.chunks[index].as_mut().expect("live chunk");
        if compact.end as usize == chunk.data.len() {
            self.unmake(index, compact.start as usize);
            return;
        }
        chunk.dropped += (compact.end - compact.start) as usize;
        chunk.texts -= 1;
        if chunk.texts == 0 {
            self.chunks[index] = None;
            if self.newest == Some(index) {
                self.newest = None;
            }
            return;
        }
        if self.newest == Some(index) || 2 * chunk.dropped <= chunk.data.len() {
            return;
        }
        // Mostly gaps: its texts to the newest chunk, and it goes.
        let old = self.chunks[index].take().expect("live chunk");
        for compact in entries
            .values_mut()
            .filter_map(|entry| entry.compact.as_mut())
            .filter(|compact| compact.chunk as usize == index)
        {
            let data = &old.data[compact.start as usize..compact.end as usize];
            let to = self.room(data.len());
            let chunk = self.chunks[to].as_mut().expect("live chunk");
            compact.chunk = to as u32;
            compact.start = chunk.data.len() as u32;
            compact.end = compact.start + data.len() as u32;
            chunk.data.extend_from_slice(data);
        }
    }
}

/// A line being filled while breaking.
#[derive(Default)]
struct Visual {
    w: f32,
    height: Option<f32>,
    nonempty: bool,
}
impl Visual {
    /// Add the words from `start` to `end` ((word, glyph) positions), as
    /// cosmic-text's `add_to_visual_line` does.
    fn add(&mut self, words: &[Packed], start: (usize, usize), end: (usize, usize), width: f32) {
        if start == end {
            return;
        }
        self.nonempty = true;
        self.w += width;
        for word in &words[start.0..end.0 + usize::from(end.1 != 0)] {
            if let Some(height) = Word::of(word).line_height {
                self.height = Some(self.height.map_or(height, |line| line.max(height)));
            }
        }
    }
}

/// A size and the width it was measured at.
type SizeAt = (Option<f32>, (f32, f32));
struct Entry {
    key: Key,
    /// Shaped lines; None once evicted.
    lines: Option<Vec<BufferLine>>,
    /// None if the text does not qualify.
    compact: Option<Compact>,
    metrics: Metrics,
    /// The width `lines` are laid out at, if any.
    laid: Option<Option<f32>>,
    /// Inline: an entry is made for every new text, and keeps `SIZES`.
    sizes: smallvec::SmallVec<[SizeAt; SIZES]>,
    used: u64,
}
impl Entry {
    fn shaped_bytes(&self) -> usize {
        self.lines
            .as_ref()
            .map_or(0, |_| self.key.text.len() * SHAPED_BYTES_PER_CHAR + 512)
    }
    fn compact_bytes(&self) -> usize {
        256 + self.key.spans.len() * 32 + self.compact.as_ref().map_or(0, Compact::bytes)
    }
    fn lay_out(&mut self, fonts: &mut FontSystem, width: Option<f32>) {
        if self.laid == Some(width) {
            return;
        }
        let lines = self.lines.as_mut().expect("shaped lines");
        for line in lines.iter_mut() {
            line.reset_layout();
            line.layout(
                fonts,
                self.metrics.font_size,
                crate::text::wrap_width(width),
                Wrap::WordOrGlyph,
                Ellipsize::None,
                None,
                TAB_WIDTH,
                Hinting::default(),
            );
        }
        self.laid = Some(width);
    }
    /// One line of text's min-content size (`TextCache::min_content_plain`):
    /// each run of words between opportunities to break a line, its
    /// trailing blanks left out, as a line of its own, as `Compact::size`
    /// measures `min_content_lines`' copy unwrapped. None where its words
    /// are not the copy's.
    fn min_content(&self, text: &str) -> Option<(f32, f32)> {
        let [line] = self.lines.as_deref()? else {
            return None;
        };
        let shape = line.shape_opt()?;
        if shape.rtl || line.align() == Some(Align::Justified) {
            return None;
        }
        let font_size = self.metrics.font_size;
        // Words in order, with their byte ranges, widths, line heights and
        // spans.
        let mut words = Vec::new();
        for (index, span) in shape.spans.iter().enumerate() {
            if span.level.is_rtl() {
                return None;
            }
            for word in &span.words {
                let (Some(first), Some(last)) = (word.glyphs.first(), word.glyphs.last()) else {
                    return None;
                };
                let height = first.metrics_opt.map(|metrics| metrics.line_height);
                if word
                    .glyphs
                    .iter()
                    .any(|glyph| glyph.metrics_opt.map(|metrics| metrics.line_height) != height)
                {
                    return None;
                }
                words.push((first.start..last.end, word.width(font_size), height, index));
            }
        }
        let empty = shape.metrics_opt.map(|metrics| metrics.line_height);
        let line_height = self.metrics.line_height;
        let (mut widest, mut height) = (0_f32, 0_f32);
        let mut next = 0;
        let mut start = 0;
        for end in zgx::text_layout::break_opportunities(text) {
            let kept = start + text[start..end].trim_end().len();
            let mut visual = Visual::default();
            let mut range_width = 0_f32;
            // The copy's line adds each span's words as cosmic-text does.
            let mut span = None;
            while let Some((range, width, word_height, index)) = words.get(next) {
                if range.start >= end {
                    break;
                }
                // A word across a break (cosmic-text keeps a likely
                // ligature whole): not the copy's.
                if range.end > end || (range.start < kept && range.end > kept) {
                    return None;
                }
                if range.start < kept {
                    if span.is_some_and(|span| span != *index) {
                        visual.w += range_width;
                        range_width = 0.;
                    }
                    span = Some(*index);
                    visual.nonempty = true;
                    range_width += width;
                    if let Some(word_height) = word_height {
                        visual.height =
                            Some(visual.height.map_or(*word_height, |h| h.max(*word_height)));
                    }
                }
                next += 1;
            }
            visual.w += range_width;
            if visual.nonempty {
                widest = widest.max(visual.w);
                height += visual.height.unwrap_or(line_height);
            } else if shape.spans.len() > 1 {
                // An empty line of the copy takes a run's height that its
                // spans don't say.
                return None;
            } else {
                height += empty.unwrap_or(line_height);
            }
            start = end;
        }
        (next == words.len()).then_some((widest, height))
    }
    fn size(&mut self, fonts: &mut FontSystem, slab: &Slab, width: Option<f32>) -> (f32, f32) {
        if let Some((_, size)) = self.sizes.iter().find(|(w, _)| *w == width) {
            return *size;
        }
        let size = match &self.compact {
            Some(compact) => compact.size(slab, width, self.metrics.line_height),
            None => {
                self.lay_out(fonts, width);
                let lines = self.lines.as_deref().expect("laid out");
                LayoutRunIter::from_lines(lines, None, self.metrics.line_height, 0., 0)
                    .fold((0_f32, 0_f32), |(w, h), run| {
                        (w.max(run.line_w), h.max(run.line_top + run.line_height))
                    })
            }
        };
        if self.sizes.len() == SIZES {
            self.sizes.remove(0);
        }
        self.sizes.push((width, size));
        size
    }
}

/// What a text is prepared from: rich text, or plain text as one run
/// (laid out as rich text is, without building one to look it up or shape).
#[derive(Clone, Copy)]
enum Source<'a> {
    Rich(&'a RichText),
    Plain {
        text: &'a Arc<str>,
        font_size: f32,
        font: &'a FontStyle,
    },
}
impl Source<'_> {
    fn text(&self) -> &str {
        match self {
            Source::Rich(rich) => rich.text(),
            Source::Plain { text, .. } => text,
        }
    }
}

/// What a caller needs of an entry.
#[derive(Clone, Copy, PartialEq)]
enum Need {
    Layout,
    Measure,
    Detached,
}

/// Lines of a prepared text laid out at one width.
pub struct Laid<'a> {
    lines: &'a [BufferLine],
    line_height: f32,
    key: u64,
}
impl<'a> Laid<'a> {
    /// Identifies the prepared text, for `TextCache::trim`.
    pub fn key(&self) -> u64 {
        self.key
    }
    /// Runs from the top, stopping below `height` as a sized buffer would.
    pub fn runs(&self, height: Option<f32>) -> LayoutRunIter<'a> {
        LayoutRunIter::from_lines(self.lines, height, self.line_height, 0., 0)
    }
    /// Where each line starts in the source text.
    pub fn line_starts(&self) -> Vec<usize> {
        let mut offset = 0;
        self.lines
            .iter()
            .map(|line| {
                let start = offset;
                offset += line.text().len() + line.ending().as_str().len();
                start
            })
            .collect()
    }
}

/// Prepared rich texts, shared by layout measurement and the renderer.
#[derive(Default)]
pub struct TextCache {
    entries: FxHashMap<u64, Entry>,
    recent: VecDeque<u64>,
    clock: u64,
    shaped_bytes: usize,
    compact_bytes: usize,
    /// Shaped bytes drawn at the last `trim`.
    drawn_bytes: usize,
    /// Texts shaped, and requests served without shaping, for tests and stats.
    pub shaped: u64,
    /// Requests served without shaping.
    pub hits: u64,
    /// Lines of dropped texts, their allocations reused by new ones: freeing
    /// and allocating a shaped line's many small parts costs more than
    /// shaping a short value.
    spare: Vec<BufferLine>,
    spare_long: Vec<BufferLine>,
    /// The emptied line lists of dropped texts, filled again by new ones.
    spare_lists: Vec<Vec<BufferLine>>,
    /// Every compact's words and advances.
    slab: Slab,
    /// Sizes of the texts it does not prepare (clamped or ellipsized: see
    /// `handles`), by what shapes them and their options, at the widths
    /// they were measured at: shaped whole for each measurement, a list of
    /// them moving by a row (a mail archived) shaped every row again.
    clamped: FxHashMap<u64, Clamped>,
}
/// A clamped or ellipsized text's sizes (`TextCache::clamped`).
struct Clamped {
    key: Key,
    options: zgx::text_layout::TextOptions,
    sizes: smallvec::SmallVec<[SizeAt; SIZES]>,
}
/// Clamped texts' sizes kept at most: past it they start again.
const CLAMPED: usize = 4096;
impl TextCache {
    /// Whether `rich` goes through the cache: clamped or ellipsized text is
    /// laid out by its own path.
    pub fn handles(rich: &RichText) -> bool {
        rich.options() == Default::default()
    }
    /// `rich`'s size wrapped at `width`, as `ShapedText::with_runs` measures.
    pub fn measure(
        &mut self,
        fonts: &mut FontSystem,
        rich: &RichText,
        width: Option<f32>,
    ) -> (f32, f32) {
        if !Self::handles(rich) {
            return self.clamped_size(fonts, rich, width);
        }
        let hash = self.entry(fonts, Source::Rich(rich), false, Need::Measure);
        self.sized(fonts, hash, width)
    }
    /// As `measure`, for text that is not drawn (the estimated heights of
    /// rows a virtualized list has not mounted): a text prepared here keeps
    /// only its compact measurements, not glyphs, unless it needs them to be
    /// measured at all.
    pub fn measure_detached(
        &mut self,
        fonts: &mut FontSystem,
        rich: &RichText,
        width: Option<f32>,
    ) -> (f32, f32) {
        if !Self::handles(rich) {
            return self.clamped_size(fonts, rich, width);
        }
        let hash = self.entry(fonts, Source::Rich(rich), false, Need::Detached);
        self.sized(fonts, hash, width)
    }
    /// A clamped or ellipsized text's size at `width`, as
    /// `ShapedText::with_runs` measures it, shaped only the first time.
    fn clamped_size(
        &mut self,
        fonts: &mut FontSystem,
        rich: &RichText,
        width: Option<f32>,
    ) -> (f32, f32) {
        let key = Key::of(rich, false);
        let options = rich.options();
        let mut hasher = FxHasher::default();
        key.hash().hash(&mut hasher);
        options.hash(&mut hasher);
        let hash = hasher.finish();
        if let Some(known) = self.clamped.get(&hash)
            && known.key == key
            && known.options == options
            && let Some((_, size)) = known.sizes.iter().find(|(w, _)| *w == width)
        {
            self.hits += 1;
            return *size;
        }
        let size = crate::text::ShapedText::with_runs(fonts, rich, width).size();
        if self.clamped.len() >= CLAMPED {
            self.clamped.clear();
        }
        let known = self.clamped.entry(hash).or_insert_with(|| Clamped {
            key: key.clone(),
            options,
            sizes: smallvec::SmallVec::new(),
        });
        // A hash shared with another text: this one's now.
        if known.key != key || known.options != options {
            *known = Clamped {
                key,
                options,
                sizes: smallvec::SmallVec::new(),
            };
        }
        if known.sizes.len() == SIZES {
            known.sizes.remove(0);
        }
        known.sizes.push((width, size));
        size
    }
    /// `rich` laid out at `width`. Only for texts `handles` accepts.
    pub fn layout(
        &mut self,
        fonts: &mut FontSystem,
        rich: &RichText,
        width: Option<f32>,
    ) -> Laid<'_> {
        let hash = self.entry(fonts, Source::Rich(rich), false, Need::Layout);
        self.laid_out(fonts, hash, width)
    }
    /// Whether plain text goes through the cache: nonempty, at a valid size.
    pub fn handles_plain(text: &str, font_size: f32) -> bool {
        !text.is_empty() && font_size.is_finite() && font_size > 0.
    }
    /// Plain text's size wrapped at `width`, as `ShapedText::with_font`
    /// measures it.
    pub fn measure_plain(
        &mut self,
        fonts: &mut FontSystem,
        text: &Arc<str>,
        font_size: f32,
        font: &FontStyle,
        width: Option<f32>,
    ) -> (f32, f32) {
        if !Self::handles_plain(text, font_size) {
            return crate::text::ShapedText::with_font(fonts, text.clone(), font_size, width, font)
                .size();
        }
        let hash = self.plain_entry(fonts, text, font_size, font, Need::Measure);
        self.sized(fonts, hash, width)
    }
    /// `rich`'s min-content size, CSS's: as `rich.min_content_lines()`
    /// measures unwrapped, worked out from the words `rich` was shaped
    /// into where they are its lines' words (`min_content_plain`).
    pub fn min_content(&mut self, fonts: &mut FontSystem, rich: &RichText) -> (f32, f32) {
        if Self::handles(rich) && !rich.text().contains(['\n', '\r']) {
            let hash = self.entry(fonts, Source::Rich(rich), false, Need::Measure);
            let size = self.entries[&hash].min_content(rich.text());
            self.evict(hash);
            if let Some(size) = size {
                return size;
            }
        }
        self.measure(fonts, &rich.min_content_lines(), None)
    }
    /// Plain text's min-content size, CSS's: as `min_content_lines(text)`
    /// measures unwrapped, a line at every opportunity to break one and never
    /// one inside a word. Worked out from the words `text` was shaped into,
    /// not by shaping that copy, where they are its lines' words (one line of
    /// left-to-right text, each word inside one of the copy's lines).
    pub fn min_content_plain(
        &mut self,
        fonts: &mut FontSystem,
        text: &Arc<str>,
        font_size: f32,
        font: &FontStyle,
    ) -> (f32, f32) {
        if Self::handles_plain(text, font_size) && !text.contains(['\n', '\r']) {
            let hash = self.plain_entry(fonts, text, font_size, font, Need::Measure);
            let size = self.entries[&hash].min_content(text);
            self.evict(hash);
            if let Some(size) = size {
                return size;
            }
        }
        let lines: Arc<str> = zgx::text_layout::min_content_lines(text).into();
        self.measure_plain(fonts, &lines, font_size, font, None)
    }
    /// Plain text laid out at `width`. Only for text `handles_plain` accepts.
    pub fn layout_plain(
        &mut self,
        fonts: &mut FontSystem,
        text: &Arc<str>,
        font_size: f32,
        font: &FontStyle,
        width: Option<f32>,
    ) -> Laid<'_> {
        let hash = self.plain_entry(fonts, text, font_size, font, Need::Layout);
        self.laid_out(fonts, hash, width)
    }
    /// The entry for plain text. Found, as most lookups are (a layout
    /// measures a text at a few widths, each frame draws it again), it costs
    /// no rich text or key built to look it up.
    fn plain_entry(
        &mut self,
        fonts: &mut FontSystem,
        text: &Arc<str>,
        font_size: f32,
        font: &FontStyle,
        need: Need,
    ) -> u64 {
        let hash = Key::plain_hash(text, font_size, font);
        if let Some(entry) = self.entries.get_mut(&hash)
            && entry.key.is_plain(text, font_size, font)
            && (entry.lines.is_some() || (need != Need::Layout && entry.compact.is_some()))
        {
            self.clock += 1;
            entry.used = self.clock;
            self.hits += 1;
            return hash;
        }
        let source = Source::Plain {
            text,
            font_size,
            font,
        };
        self.entry(fonts, source, true, need)
    }
    fn sized(&mut self, fonts: &mut FontSystem, hash: u64, width: Option<f32>) -> (f32, f32) {
        // Measuring changes no compact (`update` is not needed).
        let entry = self.entries.get_mut(&hash).expect("prepared");
        let size = entry.size(fonts, &self.slab, width);
        self.evict(hash);
        size
    }
    fn laid_out(&mut self, fonts: &mut FontSystem, hash: u64, width: Option<f32>) -> Laid<'_> {
        self.update(hash, |entry| entry.lay_out(fonts, width));
        self.evict(hash);
        let entry = &self.entries[&hash];
        Laid {
            lines: entry.lines.as_deref().expect("laid out"),
            line_height: entry.metrics.line_height,
            key: hash,
        }
    }
    /// Whether texts are held past what `trim` keeps.
    pub fn needs_trim(&self) -> bool {
        self.shaped_bytes > self.drawn_bytes + UNDRAWN_BUDGET
    }
    /// Keep the texts `drawn` (glyphs a renderer holds), and of the rest the
    /// most recently used within a small budget: text that changed or left
    /// the screen goes, keeping memory to what is shown. Measurements of
    /// text that is not drawn (`measure_detached`) are kept apart.
    pub fn trim(&mut self, drawn: impl IntoIterator<Item = u64>) {
        let drawn: rustc_hash::FxHashSet<u64> = drawn.into_iter().collect();
        self.drawn_bytes = drawn
            .iter()
            .filter_map(|hash| self.entries.get(hash))
            .map(Entry::shaped_bytes)
            .sum();
        let mut undrawn: Vec<(u64, u64)> = self
            .entries
            .iter()
            .filter(|(hash, entry)| entry.lines.is_some() && !drawn.contains(hash))
            .map(|(hash, entry)| (entry.used, *hash))
            .collect();
        // Newest first; keep those within the budget.
        undrawn.sort_unstable_by(|a, b| b.cmp(a));
        let mut kept = 0;
        for (_, hash) in undrawn {
            let entry = &self.entries[&hash];
            kept += entry.shaped_bytes();
            if kept > UNDRAWN_BUDGET {
                let entry = self.remove(hash);
                self.retire(entry);
            }
        }
        // The table keeps the room its most entries took (an interface's
        // first layout measures every text), 360 bytes an entry: give back
        // what is more than twice what is held.
        if self.entries.capacity() > 2 * self.entries.len() + 64 {
            self.entries.shrink_to_fit();
        }
    }
    /// Estimated bytes held: shaped lines, and compact measurements.
    pub fn bytes(&self) -> (usize, usize) {
        (self.shaped_bytes, self.compact_bytes)
    }
    /// Run `change` on an entry, keeping the compact byte count.
    fn update<R>(&mut self, hash: u64, change: impl FnOnce(&mut Entry) -> R) -> R {
        let entry = self.entries.get_mut(&hash).expect("prepared");
        let before = entry.compact_bytes();
        let result = change(entry);
        self.compact_bytes = self.compact_bytes - before + entry.compact_bytes();
        result
    }
    /// The entry for `rich`, shaped unless it is cached with what is needed:
    /// shaped lines for layout, or either tier for measuring.
    fn entry(
        &mut self,
        fonts: &mut FontSystem,
        source: Source<'_>,
        caret_row: bool,
        need: Need,
    ) -> u64 {
        self.clock += 1;
        let key = match source {
            Source::Rich(rich) => Key::of(rich, caret_row),
            Source::Plain {
                text,
                font_size,
                font,
            } => Key::plain(text, font_size, font, caret_row),
        };
        let hash = key.hash();
        let cached = self.entries.get_mut(&hash).filter(|entry| entry.key == key);
        if let Some(entry) = &cached
            && (entry.lines.is_some() || (need != Need::Layout && entry.compact.is_some()))
        {
            let entry = cached.expect("cached");
            entry.used = self.clock;
            self.hits += 1;
            return hash;
        }
        let known = cached.is_some();
        self.shaped += 1;
        let (lines, metrics) = self.shape(fonts, source, caret_row);
        if known {
            // Evicted lines, shaped again for drawing; measurements stay.
            let entry = self.entries.get_mut(&hash).expect("cached");
            entry.lines = Some(lines);
            entry.laid = None;
            entry.used = self.clock;
            self.shaped_bytes += entry.shaped_bytes();
        } else {
            // Shaping is all measuring needs: no line breaking by cosmic-text
            // unless the text does not qualify for arithmetic.
            let mut lines = lines;
            for line in &mut lines {
                line.shape(fonts, TAB_WIDTH);
            }
            let compact = Compact::of(&lines, metrics.font_size, &mut self.slab);
            // Text that is not drawn keeps no glyphs it can be measured without.
            let detached = need == Need::Detached && compact.is_some();
            let entry = Entry {
                key,
                compact,
                lines: (!detached).then_some(lines),
                metrics,
                laid: None,
                sizes: smallvec::SmallVec::new(),
                used: self.clock,
            };
            self.shaped_bytes += entry.shaped_bytes();
            self.compact_bytes += entry.compact_bytes();
            if let Some(old) = self.entries.insert(hash, entry) {
                // A hash collision: the newer text wins.
                self.shaped_bytes -= old.shaped_bytes();
                self.compact_bytes -= old.compact_bytes();
                self.retire(old);
            }
            if detached {
                return hash;
            }
        }
        self.recent.retain(|recent| *recent != hash);
        self.recent.push_back(hash);
        if self.recent.len() > RECENT {
            self.recent.pop_front();
        }
        hash
    }
    /// Unshaped lines for `rich`, with the shapes of the lines it shares with
    /// a recently prepared text: a streaming text shares all but its last
    /// lines with what it was a moment ago. That text is superseded, so its
    /// lines move rather than copy, and it goes.
    fn shape(
        &mut self,
        fonts: &mut FontSystem,
        source: Source<'_>,
        caret_row: bool,
    ) -> (Vec<BufferLine>, Metrics) {
        // `ShapedText::with_runs` adds an unshaped row for an editor's caret
        // after a final line break, which its size leaves out: so is it here.
        // `ShapedText::with_font` shapes it, so plain text keeps it.
        let text = source.text();
        let recycled = self.spare_for(text);
        // The font and size of its last run, for plain text its one.
        let (mut buffer, last) = match source {
            // Plain text without per-span attributes, which cosmic-text looks
            // up for every word: shaped as `ShapedText::with_font` shapes it.
            Source::Plain {
                text,
                font_size,
                font,
            } if caret_row => (
                crate::text::plain_buffer_unshaped(fonts, text, font_size, font, recycled),
                Some((font, font_size)),
            ),
            Source::Rich(rich) => match rich.runs() {
                [run] if caret_row => (
                    crate::text::plain_buffer_unshaped(
                        fonts,
                        rich.text(),
                        run.font_size,
                        &run.font,
                        recycled,
                    ),
                    Some((&run.font, run.font_size)),
                ),
                runs => (
                    crate::text::rich_buffer_unshaped(fonts, rich, false, recycled),
                    runs.last().map(|run| (&run.font, run.font_size)),
                ),
            },
            Source::Plain { .. } => unreachable!("plain text keeps its caret row"),
        };
        if caret_row
            && text.ends_with(['\r', '\n'])
            && crate::text::ends_open(&buffer)
            && let Some((font, font_size)) = last
        {
            let size = font_size.max(1.);
            let mut line = BufferLine::new(
                "",
                cosmic_text::LineEnding::None,
                cosmic_text::AttrsList::new(&crate::text::attrs(font, size)),
                cosmic_text::Shaping::Advanced,
            );
            line.set_align(crate::text::alignment(font));
            buffer.lines.push(line);
        }
        let metrics = buffer.metrics();
        let mut lines = std::mem::take(&mut buffer.lines);
        // A one-line text could only take a line equal to all of it, which
        // the lookup before shaping would have found.
        let donor = (lines.len() > 1)
            .then(|| {
                self.recent
                    .iter()
                    .filter_map(|recent| Some((*recent, self.entries.get(recent)?)))
                    .filter(|(_, entry)| entry.metrics == metrics)
                    .filter_map(|(hash, entry)| {
                        let shared = entry
                            .lines
                            .as_ref()?
                            .iter()
                            .zip(&lines)
                            .take_while(|(old, new)| {
                                old.text() == new.text()
                                    && old.ending() == new.ending()
                                    && old.attrs_list() == new.attrs_list()
                                    && old.align() == new.align()
                            })
                            .count();
                        (shared > 0).then_some((shared, hash))
                    })
                    .max_by_key(|(shared, _)| *shared)
            })
            .flatten();
        if let Some((shared, hash)) = donor {
            let entry = self.remove(hash);
            self.retire_compact(entry.compact);
            let old = entry.lines.expect("donor lines");
            for (line, old) in lines.iter_mut().zip(old).take(shared) {
                if old.shape_opt().is_some() {
                    *line = old;
                }
            }
        }
        // A text this one extends is a stream's previous state: it goes too.
        // Short texts (labels, where "1" and "12" can both show) stay.
        let extended: Vec<u64> = if text.len() <= SUPERSEDED {
            Vec::new()
        } else {
            self.recent
                .iter()
                .copied()
                .filter(|recent| {
                    self.entries.get(recent).is_some_and(|entry| {
                        entry.metrics == metrics
                            && entry.key.text.len() >= SUPERSEDED
                            && entry.key.text.len() < text.len()
                            && text.starts_with(&*entry.key.text)
                    })
                })
                .collect()
        };
        for hash in extended {
            let entry = self.remove(hash);
            self.retire(entry);
        }
        (lines, metrics)
    }
    fn remove(&mut self, hash: u64) -> Entry {
        let entry = self.entries.remove(&hash).expect("cached");
        self.shaped_bytes -= entry.shaped_bytes();
        self.compact_bytes -= entry.compact_bytes();
        self.recent.retain(|recent| *recent != hash);
        entry
    }
    /// Keep a dropped text's lines for new texts, and give back its part
    /// of the compact lists.
    fn retire(&mut self, entry: Entry) {
        self.recycle(entry.lines);
        self.retire_compact(entry.compact);
    }
    fn retire_compact(&mut self, compact: Option<Compact>) {
        if let Some(compact) = compact {
            self.slab.drop(&compact, &mut self.entries);
        }
    }
    /// Keep `lines` for new texts to shape into.
    fn recycle(&mut self, lines: Option<Vec<BufferLine>>) {
        let Some(mut lines) = lines else {
            return;
        };
        for line in lines.drain(..) {
            let len = line.text().len();
            if len <= SPARE_TEXT && self.spare.len() < SPARE_LINES {
                self.spare.push(line);
            } else if len > SPARE_TEXT
                && len <= SPARE_LONG_TEXT
                && self.spare_long.len() < SPARE_LONG_LINES
            {
                self.spare_long.push(line);
            }
        }
        // A one-line text's list: most texts are one line.
        if lines.capacity() == 1 && self.spare_lists.len() < SPARE_LINES {
            self.spare_lists.push(lines);
        }
    }
    /// Spare lines for a text of `text`'s lines: long ones for long lines.
    fn spare_for(&mut self, text: &str) -> Vec<BufferLine> {
        let lines = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let long = text.len() > lines * SPARE_TEXT;
        let mut taken = match lines {
            1 => self.spare_lists.pop().unwrap_or_default(),
            _ => Vec::new(),
        };
        taken.reserve(lines);
        for pool in if long {
            [&mut self.spare_long, &mut self.spare]
        } else {
            [&mut self.spare, &mut self.spare_long]
        } {
            let keep = pool.len().saturating_sub(lines - taken.len());
            taken.extend(pool.drain(keep..));
        }
        taken
    }
    /// Past a budget, drop the least recently used (never `keep`): shaped
    /// lines first, keeping measurements where the text has them.
    fn evict(&mut self, keep: u64) {
        if self.shaped_bytes > SHAPED_BUDGET {
            for hash in self.by_age(keep, |entry| entry.lines.is_some()) {
                if self.shaped_bytes <= SHAPED_BUDGET / 4 * 3 {
                    break;
                }
                let entry = self.entries.get_mut(&hash).expect("listed");
                self.shaped_bytes -= entry.shaped_bytes();
                let lines = entry.lines.take();
                entry.laid = None;
                if entry.compact.is_none() {
                    // Nothing left to measure from.
                    self.compact_bytes -= entry.compact_bytes();
                    self.entries.remove(&hash);
                }
                self.recycle(lines);
            }
            self.recent.retain(|hash| self.entries.contains_key(hash));
        }
        if self.compact_bytes > COMPACT_BUDGET {
            for hash in self.by_age(keep, |_| true) {
                if self.compact_bytes <= COMPACT_BUDGET / 4 * 3 {
                    break;
                }
                let entry = self.entries.remove(&hash).expect("listed");
                self.shaped_bytes -= entry.shaped_bytes();
                self.compact_bytes -= entry.compact_bytes();
                self.retire(entry);
            }
            self.recent.retain(|hash| self.entries.contains_key(hash));
        }
    }
    /// Entries matching `filter`, least recently used first, except `keep`.
    fn by_age(&self, keep: u64, filter: impl Fn(&Entry) -> bool) -> Vec<u64> {
        let mut by_age: Vec<(u64, u64)> = self
            .entries
            .iter()
            .filter(|(hash, entry)| **hash != keep && filter(entry))
            .map(|(hash, entry)| (entry.used, *hash))
            .collect();
        by_age.sort_unstable();
        by_age.into_iter().map(|(_, hash)| hash).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::ShapedText;
    use zgx::{rich_text::TextRun, text_layout::LineHeight};

    fn rich(text: &str, runs: &[(usize, FontStyle, f32)]) -> RichText {
        let mut start = 0;
        let runs: Vec<TextRun> = runs
            .iter()
            .map(|(end, font, font_size)| {
                let range = start..*end;
                start = *end;
                TextRun {
                    range,
                    font: font.clone(),
                    font_size: *font_size,
                    ..Default::default()
                }
            })
            .collect();
        RichText::new(text, runs).unwrap()
    }

    /// Plain text as one run, laid out as rich text is.
    fn plain(text: &Arc<str>, font_size: f32, font: &FontStyle) -> RichText {
        let run = zgx::rich_text::TextRun {
            range: 0..text.len(),
            font: font.clone(),
            font_size,
            ..Default::default()
        };
        RichText::new(text.clone(), vec![run]).expect("one valid run")
    }
    /// Plain text's min-content size worked out from its own words is the
    /// size of its broken copy (`min_content_lines`) measured unwrapped, to
    /// the bit: words and blanks, punctuation, hyphens, non-breaking and
    /// leading spaces, emoji and combining marks, at three sizes; and it is
    /// worked out so for ordinary words, not measured from the copy.
    #[test]
    fn min_content_from_its_words_is_its_broken_copys_size() {
        let texts = [
            "Your invoice is ready",
            "  leading spaces and  double  spaces ",
            "hyphen-ated words, commas; and (brackets) [too]",
            "non\u{a0}breaking\u{a0}space and a !bang ?",
            "a -> b != c |> d :: e",
            "url like https://example.com/path/to/thing?x=1",
            "émoji 👩‍💻 and 🇫🇷 flags with e\u{301} marks",
            "supercalifragilisticexpialidocious",
            "x",
            "tab\tseparated\twords",
            "x->y!=z|>w and foo/bar/baz-qux",
            "1,000.50 (USD) — em-dash–en-dash",
        ];
        for mut fonts in [FontSystem::new()] {
            for font in [
                FontStyle::default(),
                FontStyle {
                    weight: 700,
                    ..Default::default()
                },
            ] {
                for font_size in [11., 16., 22.] {
                    let mut cache = TextCache::default();
                    for text in texts {
                        let text: Arc<str> = text.into();
                        let fast = cache.min_content_plain(&mut fonts, &text, font_size, &font);
                        let copy: Arc<str> = zgx::text_layout::min_content_lines(&text).into();
                        let measured = TextCache::default()
                            .measure_plain(&mut fonts, &copy, font_size, &font, None);
                        assert_eq!(
                            (fast.0.to_bits(), fast.1.to_bits()),
                            (measured.0.to_bits(), measured.1.to_bits()),
                            "{text:?} at {font_size}: {fast:?} vs {measured:?}"
                        );
                    }
                    let text: Arc<str> = "Your invoice is ready".into();
                    let hash =
                        cache.plain_entry(&mut fonts, &text, font_size, &font, Need::Measure);
                    assert!(cache.entries[&hash].min_content(&text).is_some());
                    // Rich text: one run, and runs of other sizes and weights
                    // meeting inside a word and at a break.
                    let bold = FontStyle {
                        weight: 700,
                        ..font.clone()
                    };
                    for text in texts {
                        let third = |i: usize| {
                            (text.len() * i / 3..=text.len())
                                .find(|&at| text.is_char_boundary(at))
                                .unwrap()
                        };
                        // Or at the space after it, as styled words meet.
                        let space = |i: usize| {
                            text[third(i)..]
                                .find(' ')
                                .map_or(text.len(), |at| third(i) + at + 1)
                        };
                        let runs = [
                            vec![(text.len(), font.clone(), font_size)],
                            vec![
                                (third(1), font.clone(), font_size),
                                (third(2), bold.clone(), font_size * 1.5),
                                (text.len(), font.clone(), font_size * 0.75),
                            ],
                            vec![
                                (space(1), font.clone(), font_size),
                                (space(2), bold.clone(), font_size * 1.5),
                                (text.len(), font.clone(), font_size * 0.75),
                            ],
                        ];
                        for runs in runs {
                            let mut end = 0;
                            let runs: Vec<_> = runs
                                .into_iter()
                                .filter(|r| {
                                    let keep = r.0 > end;
                                    end = end.max(r.0);
                                    keep
                                })
                                .collect();
                            let rich = rich(text, &runs);
                            let fast = cache.min_content(&mut fonts, &rich);
                            let measured = TextCache::default().measure(
                                &mut fonts,
                                &rich.min_content_lines(),
                                None,
                            );
                            assert_eq!(
                                (fast.0.to_bits(), fast.1.to_bits()),
                                (measured.0.to_bits(), measured.1.to_bits()),
                                "rich {text:?} at {font_size}: {fast:?} vs {measured:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn plain_text_is_found_by_the_key_its_rich_text_makes() {
        let font = FontStyle {
            weight: 600,
            italic: true,
            ..Default::default()
        };
        for text in ["hello", "two words", "a\nb", "héllo wörld"] {
            let text: Arc<str> = text.into();
            let key = Key::of(&plain(&text, 13., &font), true);
            assert!(Key::plain(&text, 13., &font, true) == key, "{text:?}");
            assert_eq!(Key::plain_hash(&text, 13., &font), key.hash(), "{text:?}");
            assert!(key.is_plain(&text, 13., &font));
            assert!(!key.is_plain(&text, 14., &font));
            assert!(!key.is_plain(&text, 13., &FontStyle::default()));
            assert!(!Key::of(&plain(&text, 13., &font), false).is_plain(&text, 13., &font));
        }
    }
    /// Plain text prepared from its text (`Source::Plain`) lays out as the
    /// one-run rich text it was prepared from before, line breaks, a final
    /// one's caret row and tabs included, at every width.
    #[test]
    fn plain_text_lays_out_as_its_one_run_rich_text() {
        let mut fonts = FontSystem::new();
        let font = FontStyle::default();
        let texts = [
            "a\nb",
            "line one\nline two\n",
            "\n\nthree\r\nlines",
            "tab\there\n  indented",
            "one line only",
        ];
        let laid = |cache: &mut TextCache, fonts: &mut FontSystem, hash, width| {
            let laid = cache.laid_out(fonts, hash, width);
            laid.runs(None)
                .map(|run| {
                    let glyphs: Vec<_> = run
                        .glyphs
                        .iter()
                        .map(|g| (g.glyph_id, g.start, g.end, g.x.to_bits(), g.w.to_bits()))
                        .collect();
                    (
                        run.line_i,
                        run.line_y.to_bits(),
                        run.line_w.to_bits(),
                        glyphs,
                    )
                })
                .collect::<Vec<_>>()
        };
        for text in texts {
            let text: Arc<str> = text.into();
            for width in [None, Some(40.), Some(500.)] {
                let mut plain_cache = TextCache::default();
                let source = Source::Plain {
                    text: &text,
                    font_size: 15.,
                    font: &font,
                };
                let hash = plain_cache.entry(&mut fonts, source, true, Need::Layout);
                let from_plain = laid(&mut plain_cache, &mut fonts, hash, width);
                let mut rich_cache = TextCache::default();
                let rich = plain(&text, 15., &font);
                let hash = rich_cache.entry(&mut fonts, Source::Rich(&rich), true, Need::Layout);
                let from_rich = laid(&mut rich_cache, &mut fonts, hash, width);
                assert!(from_plain == from_rich, "{text:?} at {width:?}");
                let lines: std::collections::BTreeSet<_> =
                    from_plain.iter().map(|run| run.0).collect();
                assert_eq!(
                    lines.len(),
                    text.split('\n').count(),
                    "{text:?}: a line per line break and the caret row"
                );
            }
        }
    }
    #[test]
    fn sizes_match_shaped_text_at_every_width() {
        let mut fonts = FontSystem::new();
        let body = FontStyle::default();
        let bold = FontStyle {
            weight: 700,
            ..FontStyle::default()
        };
        let tall = FontStyle {
            line_height: LineHeight::px(31.),
            ..FontStyle::default()
        };
        let long = "The renderer keeps every glyph retained while streaming tokens arrive; \
                    supercalifragilisticexpialidocious words wrap between glyphs.\n\n  indented  \
                    blanks   and\ttabs\r\nend\n";
        let texts = [
            rich("", &[]),
            rich(long, &[(long.len(), body.clone(), 14.)]),
            rich(
                long,
                &[
                    (4, bold.clone(), 22.),
                    (40, tall, 13.),
                    (long.len(), body.clone(), 14.),
                ],
            ),
            rich("abc אבג xyz דהו", &[(21, body.clone(), 16.)]),
            rich("אבג abc", &[(10, body, 16.)]),
        ];
        let mut cache = TextCache::default();
        for text in &texts {
            for width in [
                None,
                Some(900.),
                Some(333.3),
                Some(120.),
                Some(41.7),
                Some(9.),
                Some(0.),
            ] {
                assert_eq!(
                    cache.measure(&mut fonts, text, width),
                    ShapedText::with_runs(&mut fonts, text, width).size(),
                    "{:?} at {width:?}",
                    text.text()
                );
            }
        }
        // Left-to-right text measures by arithmetic; bidi from shaped lines.
        assert!(
            cache
                .entries
                .values()
                .filter(|e| e.compact.is_some())
                .count()
                >= 3
        );
        assert_eq!(cache.shaped, texts.len() as u64);
    }

    #[test]
    fn measurements_outlive_evicted_glyphs() {
        let mut fonts = FontSystem::new();
        let texts: Vec<RichText> = (0..40)
            .map(|i| {
                let text = format!("{i} ").repeat(4000);
                rich(&text, &[(text.len(), FontStyle::default(), 14.)])
            })
            .collect();
        let mut cache = TextCache::default();
        let sizes: Vec<_> = texts
            .iter()
            .map(|text| cache.measure(&mut fonts, text, Some(500.)))
            .collect();
        assert!(cache.bytes().0 <= SHAPED_BUDGET);
        assert!(cache.entries.values().any(|entry| entry.lines.is_none()));
        let shaped = cache.shaped;
        for (text, size) in texts.iter().zip(&sizes) {
            assert_eq!(cache.measure(&mut fonts, text, Some(500.)), *size);
            let narrower = cache.measure(&mut fonts, text, Some(300.));
            assert_eq!(
                narrower,
                ShapedText::with_runs(&mut fonts, text, Some(300.)).size()
            );
        }
        assert_eq!(cache.shaped, shaped, "measuring again never reshapes");
        // Drawing an evicted text shapes it again.
        cache.layout(&mut fonts, &texts[0], Some(500.));
        assert_eq!(cache.shaped, shaped + 1);
    }

    #[test]
    fn detached_measurement_keeps_no_glyphs_until_drawn() {
        let mut fonts = FontSystem::new();
        let text = "Rows a list has not mounted are measured without glyphs. ".repeat(20);
        let rich = rich(&text, &[(text.len(), FontStyle::default(), 14.)]);
        let mut cache = TextCache::default();
        let size = cache.measure_detached(&mut fonts, &rich, Some(300.));
        assert_eq!(
            size,
            ShapedText::with_runs(&mut fonts, &rich, Some(300.)).size()
        );
        assert_eq!(cache.bytes().0, 0, "no shaped lines kept");
        assert!(cache.bytes().1 > 0);
        // Other widths measure from the compact tier, without shaping.
        cache.measure_detached(&mut fonts, &rich, Some(200.));
        cache.measure(&mut fonts, &rich, Some(250.));
        assert_eq!(cache.shaped, 1);
        // Drawing shapes it once, and keeps it.
        cache.layout(&mut fonts, &rich, Some(300.));
        cache.layout(&mut fonts, &rich, Some(300.));
        assert_eq!(cache.shaped, 2);
        assert!(cache.bytes().0 > 0);
    }

    /// Clamped text the cache does not prepare is shaped once per width:
    /// measured again (a list's rows re-measured as it moves), its size is
    /// the one `ShapedText::with_runs` gave.
    #[test]
    fn clamped_text_is_measured_once() {
        let mut fonts = FontSystem::new();
        let mut cache = TextCache::default();
        let text = "A subject long enough to be cut short with an ellipsis at the end";
        let clamped = rich(text, &[(text.len(), FontStyle::default(), 13.)]).with_options(
            zgx::text_layout::TextOptions {
                overflow: zgx::text_layout::TextOverflow::Ellipsis,
                line_clamp: std::num::NonZeroU32::new(1),
            },
        );
        assert!(!TextCache::handles(&clamped));
        let expected = ShapedText::with_runs(&mut fonts, &clamped, Some(120.)).size();
        assert_eq!(cache.measure(&mut fonts, &clamped, Some(120.)), expected);
        let hits = cache.hits;
        assert_eq!(cache.measure(&mut fonts, &clamped, Some(120.)), expected);
        assert_eq!(cache.hits, hits + 1, "measured again without shaping");
        // Another width, or other options, is measured as itself.
        let wide = ShapedText::with_runs(&mut fonts, &clamped, Some(400.)).size();
        assert_eq!(cache.measure(&mut fonts, &clamped, Some(400.)), wide);
        let two = clamped.clone().with_options(zgx::text_layout::TextOptions {
            overflow: zgx::text_layout::TextOverflow::Ellipsis,
            line_clamp: std::num::NonZeroU32::new(2),
        });
        let expected = ShapedText::with_runs(&mut fonts, &two, Some(120.)).size();
        assert_eq!(cache.measure(&mut fonts, &two, Some(120.)), expected);
    }

    #[test]
    fn streaming_keeps_only_the_latest_state() {
        let mut fonts = FontSystem::new();
        let full = "Tokens stream into one paragraph, which grows every frame.\n".repeat(8);
        let mut cache = TextCache::default();
        let mut end = 5;
        while end <= full.len() {
            let text = &full[..end];
            let rich = rich(text, &[(text.len(), FontStyle::default(), 14.)]);
            let size = cache.measure(&mut fonts, &rich, Some(320.));
            assert_eq!(
                size,
                ShapedText::with_runs(&mut fonts, &rich, Some(320.)).size()
            );
            cache.layout(&mut fonts, &rich, Some(320.));
            end += 7;
        }
        // Only states shorter than `SUPERSEDED` remain besides the latest.
        let long: Vec<_> = cache
            .entries
            .values()
            .filter(|entry| entry.key.text.len() >= SUPERSEDED)
            .collect();
        assert_eq!(long.len(), 1, "earlier states were superseded");
        assert_eq!(long[0].key.text.len(), end - 7);
    }

    /// Compact measurements go in chunks, and a chunk whose texts are all
    /// dropped is freed: after a burst of long texts goes, what is held is
    /// what the kept texts need, and every text measures as before.
    #[test]
    fn dropped_texts_give_back_their_chunks() {
        let mut fonts = FontSystem::new();
        let font = FontStyle::default();
        let mut cache = TextCache::default();
        let held = |cache: &TextCache| -> usize {
            cache
                .slab
                .chunks
                .iter()
                .flatten()
                .map(|chunk| chunk.data.capacity() * size_of::<u32>())
                .sum()
        };
        let value = |i: usize| -> Arc<str> { format!("{i} ms").into() };
        let mut sizes = Vec::new();
        for i in 0..8 {
            sizes.push(cache.measure_plain(&mut fonts, &value(i), 14., &font, None));
        }
        for i in 0..40 {
            let long: Arc<str> = format!("{i} {}", "a few words of a long line ".repeat(40)).into();
            cache.layout_plain(&mut fonts, &long, 14., &font, Some(320.));
        }
        let grown = held(&cache);
        assert!(cache.slab.chunks.iter().flatten().count() > 2);
        // None drawn: past the budget for undrawn text, they are dropped.
        cache.trim([]);
        let live = cache.slab.chunks.iter().flatten().count();
        assert!(live <= 1, "{live} chunks left");
        assert!(
            held(&cache) <= grown / 4,
            "{} of {grown} bytes held",
            held(&cache)
        );
        for (i, size) in sizes.iter().enumerate() {
            assert_eq!(
                cache.measure_plain(&mut fonts, &value(i), 14., &font, None),
                *size
            );
        }
    }

    /// An interface's first layout measures every text; once most are gone,
    /// the table holding them gives back its room.
    #[test]
    fn the_table_shrinks_after_a_burst() {
        let mut fonts = FontSystem::new();
        let font = FontStyle::default();
        let mut cache = TextCache::default();
        for i in 0..2000 {
            let text: Arc<str> = format!("row {i}").into();
            cache.layout_plain(&mut fonts, &text, 14., &font, None);
        }
        let burst = cache.entries.capacity();
        assert!(burst >= 2000);
        // None drawn: past the budget for undrawn text, they are dropped.
        cache.trim([]);
        assert!(cache.entries.len() < 1000, "{}", cache.entries.len());
        assert!(
            cache.entries.capacity() <= 2 * cache.entries.len() + 64,
            "room for {} after the burst's {burst}, holding {}",
            cache.entries.capacity(),
            cache.entries.len()
        );
    }

    #[test]
    fn plain_text_matches_shaped_text_with_font() {
        let mut fonts = FontSystem::new();
        let tall = FontStyle {
            line_height: LineHeight::px(27.),
            ..FontStyle::default()
        };
        let bold = FontStyle {
            weight: 700,
            ..FontStyle::default()
        };
        let texts = [
            "pid 1017",
            "12.5%",
            "a\n",
            "\n",
            "abc\ndef\n",
            "trailing\r\n",
            "tabs\tand  spaces   end ",
            "emoji 🙂 and 漢字 fall back to other fonts",
            "abc אבג xyz",
            "A longer description that wraps across several lines at narrow widths.",
        ];
        let mut cache = TextCache::default();
        for font in [FontStyle::default(), tall, bold] {
            for text in texts {
                let text: Arc<str> = text.into();
                for width in [None, Some(400.), Some(90.), Some(13.)] {
                    for size in [11., 14.] {
                        assert_eq!(
                            cache.measure_plain(&mut fonts, &text, size, &font, width),
                            ShapedText::with_font(&mut fonts, text.clone(), size, width, &font)
                                .size(),
                            "{text:?} at {width:?}, {size} px"
                        );
                    }
                }
            }
        }
        // Plain and rich text of the same content are prepared apart: only
        // plain text keeps the row after a final line break.
        let text: Arc<str> = "row\n".into();
        let rich = rich(&text, &[(text.len(), FontStyle::default(), 14.)]);
        let plain = cache.measure_plain(&mut fonts, &text, 14., &FontStyle::default(), None);
        assert_eq!(plain.1, 2. * cache.measure(&mut fonts, &rich, None).1);
    }
}
