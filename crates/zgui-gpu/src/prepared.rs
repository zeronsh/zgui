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
use zgui::{rich_text::RichText, text_layout::FontStyle};

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

/// The spans that determine shapes: merged runs of one font and size.
#[derive(Clone, PartialEq)]
struct Key {
    text: Arc<str>,
    /// Runs as shaped: range, font, size and padding across.
    spans: Vec<(Range<usize>, FontStyle, f32, f32)>,
    /// Plain text keeps a row after a final line break, for an editor's
    /// caret (as `ShapedText::with_font` lays it out); rich text does not.
    caret_row: bool,
}
impl Key {
    fn of(rich: &RichText, caret_row: bool) -> Self {
        let mut spans: Vec<(Range<usize>, FontStyle, f32, f32)> = Vec::new();
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
            spans: vec![(0..text.len(), font.clone(), font_size, 0.)],
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

/// A shaped word, reduced to what line breaking reads.
#[derive(Clone, Copy)]
struct Word {
    width: f32,
    blank: bool,
    /// The first of its glyph advances in `Compact::glyphs`; the next word's
    /// first ends them.
    glyphs: u32,
    /// The line height its glyphs set, if they set one.
    line_height: Option<f32>,
}

/// A text's line breaking inputs, without glyphs.
struct Compact {
    words: Vec<Word>,
    glyphs: Vec<f32>,
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
    fn of(lines: &[BufferLine], font_size: f32) -> Option<Self> {
        // Counted first, each list is allocated once at its size: a text
        // changing every frame (a stream's) is prepared every frame.
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
        let mut compact = Self {
            words: Vec::with_capacity(words),
            glyphs: Vec::with_capacity(glyphs),
            spans: smallvec::SmallVec::with_capacity(spans),
            lines: smallvec::SmallVec::with_capacity(lines.len()),
        };
        for line in lines {
            let shape = line.shape_opt()?;
            let first_span = compact.spans.len() as u32;
            for span in &shape.spans {
                if span.level.is_rtl() {
                    return None;
                }
                let first = compact.words.len() as u32;
                for word in &span.words {
                    let glyphs = compact.glyphs.len() as u32;
                    let line_height = word
                        .glyphs
                        .first()
                        .and_then(|glyph| glyph.metrics_opt)
                        .map(|metrics| metrics.line_height);
                    for glyph in &word.glyphs {
                        if glyph.metrics_opt.map(|metrics| metrics.line_height) != line_height {
                            return None;
                        }
                        compact.glyphs.push(glyph.width(font_size));
                    }
                    compact.words.push(Word {
                        width: word.width(font_size),
                        blank: word.blank,
                        glyphs,
                        line_height,
                    });
                }
                compact.spans.push(first..compact.words.len() as u32);
            }
            compact.lines.push((
                first_span..compact.spans.len() as u32,
                shape.metrics_opt.map(|metrics| metrics.line_height),
            ));
        }
        Some(compact)
    }
    fn bytes(&self) -> usize {
        self.words.len() * size_of::<Word>()
            + self.glyphs.len() * size_of::<f32>()
            + self.spans.len() * size_of::<Range<u32>>()
            + self.lines.len() * size_of::<(Range<u32>, Option<f32>)>()
    }
    fn advances(&self, word: usize) -> &[f32] {
        let end = self
            .words
            .get(word + 1)
            .map_or(self.glyphs.len(), |next| next.glyphs as usize);
        &self.glyphs[self.words[word].glyphs as usize..end]
    }
    /// The size cosmic-text's word-or-glyph wrapping gives these lines at
    /// `width`: the widest line, and the sum of line heights. This follows
    /// its congruent-direction branch step by step.
    fn size(&self, width: Option<f32>, line_height: f32) -> (f32, f32) {
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
                let words = &self.words[offset..span.end as usize];
                // Where the words not yet added start: (word, glyph).
                let mut fitting = (0, 0);
                let mut range_width = 0_f32;
                let mut before_last_blank = 0_f32;
                for (i, word) in words.iter().enumerate() {
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
                        for (glyph, advance) in self.advances(offset + i).iter().enumerate() {
                            if line.w + (range_width + advance) <= limit {
                                range_width += advance;
                            } else {
                                line.add(words, fitting, (i, glyph), range_width);
                                push(&mut line);
                                range_width = *advance;
                                fitting = (i, glyph);
                            }
                        }
                    } else {
                        if range_width > 0. {
                            // A blank before the break is left out.
                            if i > 0 && words[i - 1].blank {
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
    fn add(&mut self, words: &[Word], start: (usize, usize), end: (usize, usize), width: f32) {
        if start == end {
            return;
        }
        self.nonempty = true;
        self.w += width;
        for word in &words[start.0..end.0 + usize::from(end.1 != 0)] {
            if let Some(height) = word.line_height {
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
    fn size(&mut self, fonts: &mut FontSystem, width: Option<f32>) -> (f32, f32) {
        if let Some((_, size)) = self.sizes.iter().find(|(w, _)| *w == width) {
            return *size;
        }
        let size = match &self.compact {
            Some(compact) => compact.size(width, self.metrics.line_height),
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
}
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
            return crate::text::ShapedText::with_runs(fonts, rich, width).size();
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
            return crate::text::ShapedText::with_runs(fonts, rich, width).size();
        }
        let hash = self.entry(fonts, Source::Rich(rich), false, Need::Detached);
        self.sized(fonts, hash, width)
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
        let size = self.update(hash, |entry| entry.size(fonts, width));
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
                self.recycle(entry.lines);
            }
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
            let compact = Compact::of(&lines, metrics.font_size);
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
                self.recycle(old.lines);
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
            self.recycle(entry.lines);
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
    /// Keep `lines` for new texts to shape into.
    fn recycle(&mut self, lines: Option<Vec<BufferLine>>) {
        for line in lines.into_iter().flatten() {
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
    }
    /// Spare lines for a text of `text`'s lines: long ones for long lines.
    fn spare_for(&mut self, text: &str) -> Vec<BufferLine> {
        let lines = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let long = text.len() > lines * SPARE_TEXT;
        let mut taken = Vec::with_capacity(lines);
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
                self.recycle(entry.lines);
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
    use zgui::{rich_text::TextRun, text_layout::LineHeight};

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
        let run = zgui::rich_text::TextRun {
            range: 0..text.len(),
            font: font.clone(),
            font_size,
            ..Default::default()
        };
        RichText::new(text.clone(), vec![run]).expect("one valid run")
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
