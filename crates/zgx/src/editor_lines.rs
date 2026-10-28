//! A long text's layout as its lines: an editor holding a file of 100,000
//! lines shapes the few in view, not the file. Lines are shaped when asked
//! for (a caret, a hit, a line shown) and kept by their text, so an edit
//! reshapes the line it changed and no other; where each line starts is an
//! index of offsets, a few bytes a line. Unwrapped, every line is a row
//! high; wrapped, a line's height is estimated from its length until it is
//! shaped, then corrected.
//!
//! Each hard line is laid out on its own, as a shaper lays out each of a
//! text's paragraphs, so carets, hits and selections are those of the
//! whole text's layout.
use crate::{
    scene::Rect,
    text::Text,
    text_layout::{TextAffinity, TextLayout, TextPosition},
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Range,
    rc::Rc,
};

/// Shapes one line, wrapped at a width if given (`Scene::line_shaper`).
pub(crate) type LineShaper = Rc<dyn Fn(&str, Option<f32>) -> Box<dyn TextLayout>>;

/// A line's colours: byte ranges of the line, each with the colour its
/// text shows in (the rest, the editor's).
pub type LineColors = Vec<(Range<usize>, crate::scene::Color)>;
/// Colours a line of code (`EditorHandle::set_highlighter`), given what the
/// line before it left open (`0` before the first: a block comment or a
/// string running on are other numbers, the highlighter's own): its colours
/// and what it leaves the next line. It is asked for the lines that show
/// (and those above them, once, for what they leave), and again only for a
/// line that changes, or whose line before leaves another state; so it
/// must depend on nothing but its arguments.
pub type Highlighter = Rc<dyn Fn(&str, u32) -> (LineColors, u32)>;

/// A text this long (in bytes, or lines) is laid out by its lines; a
/// shorter one whole, as before.
pub(crate) const LONG_TEXT_BYTES: usize = 64 * 1024;
pub(crate) const LONG_TEXT_LINES: usize = 2_000;
/// Whether `text` is long enough to lay out by its lines.
pub(crate) fn is_long(text: &str) -> bool {
    text.len() > LONG_TEXT_BYTES || text.match_indices('\n').nth(LONG_TEXT_LINES).is_some()
}

/// Lines kept shaped: about a few screens' worth.
const SHAPED_LINES: usize = 1_024;

/// What the shaped lines are for: a change of any shapes them again.
#[derive(Clone, PartialEq)]
struct Shaping {
    size: u32,
    wrap: Option<u32>,
    font: crate::text_layout::FontStyle,
    revision: u64,
}

/// What an editor keeps across its long text's layouts (one an edit): the
/// lines it shaped, by their text, and its last layout.
#[derive(Default)]
pub(crate) struct LineCache {
    shaping: RefCell<Option<Shaping>>,
    shaped: RefCell<HashMap<Box<str>, Shaped>>,
    /// A wrapped line's height once shaped, by its text: kept when its
    /// layout is evicted, so the lines above what shows keep their places.
    heights: RefCell<HashMap<Box<str>, f32>>,
    clock: Cell<u64>,
    last: RefCell<Option<Rc<LinesLayout>>>,
    /// The editor's text revision `last` is of, if it is its committed text.
    last_revision: Cell<Option<u64>>,
    highlighter: RefCell<Option<Highlighter>>,
    /// What each line leaves the next, from the first; as far as known.
    states: RefCell<Vec<u32>>,
    /// What the lines below the last edit left before it: once the lines
    /// it changed leave what they did, these hold again (the edit's
    /// colours end there).
    tail: RefCell<Option<Tail>>,
    /// Highlighted lines, by their text and what the line before left.
    colored: RefCell<HashMap<(Box<str>, u32), Colored>>,
}
/// What the lines below an edit left before it (`LineCache::edited`).
struct Tail {
    /// The line the first of them is now.
    from: usize,
    /// What entered the first of them.
    entering: u32,
    states: Vec<u32>,
}
/// A shaped line, and when it was last used.
type Shaped = (Rc<dyn TextLayout>, u64);
/// A highlighted line: its colours, what it leaves the next, when used.
struct Colored {
    colors: Rc<LineColors>,
    leaves: u32,
    used: u64,
}
impl LineCache {
    /// Let go of the last layout, which holds this cache in turn: called as
    /// its editor goes (`widgets`), or the two would keep each other, their
    /// lines and the shaper alive.
    pub(crate) fn forget_last(&self) {
        self.last.borrow_mut().take();
        self.last_revision.set(None);
    }
    /// The layout of the text `text` makes, its last one if the text and
    /// shaping are the same: known by the text's `revision`
    /// (`TextEditor::revision`) if it is an editor's committed text, before
    /// `text` is asked for, else by comparing them (a pointer, for a long
    /// editor's shared text).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout(
        self: &Rc<Self>,
        text: impl FnOnce() -> Text,
        text_revision: Option<u64>,
        size: f32,
        wrap: Option<f32>,
        font: &crate::text_layout::FontStyle,
        revision: u64,
        shape: impl FnOnce() -> LineShaper,
    ) -> Rc<LinesLayout> {
        let shaping = Shaping {
            size: size.to_bits(),
            wrap: wrap.map(f32::to_bits),
            font: font.clone(),
            revision,
        };
        if self.shaping.borrow().as_ref() != Some(&shaping) {
            self.shaped.borrow_mut().clear();
            self.heights.borrow_mut().clear();
            *self.last.borrow_mut() = None;
            self.last_revision.set(None);
            *self.shaping.borrow_mut() = Some(shaping);
        }
        let previous = self.last.borrow().clone();
        if let (Some(last), Some(now)) = (&previous, text_revision)
            && self.last_revision.get() == Some(now)
        {
            return last.clone();
        }
        let text = text();
        if let Some(last) = &previous
            && last.text == text
        {
            self.last_revision.set(text_revision);
            return last.clone();
        }
        let layout = Rc::new(match &previous {
            // An edit: the old layout's lines, those it changed found again.
            Some(old) => {
                let prefix = old.text.common_prefix(&text);
                let suffix = old.text.common_suffix(&text, prefix);
                let layout = LinesLayout::edited(old, text, prefix, suffix);
                self.edited(Some((old, prefix, suffix)), &layout);
                layout
            }
            None => {
                let layout = LinesLayout::new(text, wrap, shape(), self.clone());
                self.edited(None, &layout);
                layout
            }
        });
        *self.last.borrow_mut() = Some(layout.clone());
        self.last_revision.set(text_revision);
        layout
    }
    /// The text went from `old` to `new`'s: what the lines above the edit
    /// leave holds, and what those below it left is kept for when the
    /// edited lines leave what they did.
    fn edited(&self, old: Option<(&LinesLayout, usize, usize)>, new: &LinesLayout) {
        let mut states = self.states.borrow_mut();
        let mut tail = self.tail.borrow_mut();
        *tail = None;
        let Some((old, prefix, suffix)) = old else {
            states.clear();
            return;
        };
        let first = new.line_of(prefix);
        let (old_last, new_last) = (
            old.line_of(old.text.len() - suffix),
            new.line_of(new.text.len() - suffix),
        );
        // The lines below the edit, as far as their states were known, with
        // what entered the first of them.
        if states.len() > old_last + 1 {
            *tail = Some(Tail {
                from: new_last + 1,
                entering: states[old_last],
                states: states[old_last + 1..].to_vec(),
            });
        }
        states.truncate(first);
    }
    /// The next line leaves `leaves`. Below an edit, once what a line
    /// leaves is what it left before, the lines below it leave what they
    /// did: those are known again, and asked for no more.
    fn push_state(&self, states: &mut Vec<u32>, leaves: u32) {
        states.push(leaves);
        let line = states.len() - 1;
        let mut tail = self.tail.borrow_mut();
        let Some(below) = tail.as_ref() else {
            return;
        };
        let holds = if line + 1 == below.from {
            (leaves == below.entering).then_some(0)
        } else if line >= below.from {
            match below.states.get(line - below.from) {
                Some(old) => (leaves == *old).then_some(line - below.from + 1),
                None => {
                    *tail = None;
                    return;
                }
            }
        } else {
            None
        };
        if let Some(from) = holds {
            states.extend_from_slice(&below.states[from..]);
            *tail = None;
        }
    }
    /// Colour lines with `highlighter` (none: the editor's colour).
    pub(crate) fn set_highlighter(&self, highlighter: Option<Highlighter>) {
        *self.highlighter.borrow_mut() = highlighter;
        self.states.borrow_mut().clear();
        *self.tail.borrow_mut() = None;
        self.colored.borrow_mut().clear();
    }
    /// Ask the highlighter for every line anew.
    pub(crate) fn rehighlight(&self) {
        let highlighter = self.highlighter.borrow().clone();
        self.set_highlighter(highlighter);
    }
    pub(crate) fn highlighted(&self) -> bool {
        self.highlighter.borrow().is_some()
    }
    /// Line `i`'s colours, if it has a highlighter: what the lines above
    /// it leave found first (each once, until one changes).
    pub(crate) fn colors(&self, layout: &LinesLayout, i: usize) -> Option<Rc<LineColors>> {
        let highlight = self.highlighter.borrow().clone()?;
        loop {
            let known = self.states.borrow().len();
            if known >= i {
                break;
            }
            let before = if known == 0 {
                0
            } else {
                self.states.borrow()[known - 1]
            };
            let (_, leaves) = self.highlight(&highlight, &layout.line_text(known), before);
            self.push_state(&mut self.states.borrow_mut(), leaves);
        }
        let before = if i == 0 {
            0
        } else {
            self.states.borrow()[i - 1]
        };
        let (colors, leaves) = self.highlight(&highlight, &layout.line_text(i), before);
        let mut states = self.states.borrow_mut();
        if states.len() == i {
            self.push_state(&mut states, leaves);
        }
        Some(colors)
    }
    /// `text` highlighted after a line that left `before` (from the cache
    /// if it is there; no borrow is held while the highlighter runs).
    fn highlight(&self, highlight: &Highlighter, text: &str, before: u32) -> (Rc<LineColors>, u32) {
        let tick = self.clock.get() + 1;
        self.clock.set(tick);
        let key = (Box::<str>::from(text), before);
        if let Some(hit) = self.colored.borrow_mut().get_mut(&key) {
            hit.used = tick;
            return (hit.colors.clone(), hit.leaves);
        }
        let (mut colors, leaves) = highlight(text, before);
        // Within the line, in order, on character boundaries.
        colors.retain_mut(|(range, _)| {
            range.end = range.end.min(text.len());
            range.start < range.end
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end)
        });
        colors.sort_by_key(|(range, _)| range.start);
        let colors = Rc::new(colors);
        let mut colored = self.colored.borrow_mut();
        if colored.len() >= SHAPED_LINES * 4 {
            let mut uses: Vec<u64> = colored.values().map(|c| c.used).collect();
            let half = uses.len() / 2;
            let (_, median, _) = uses.select_nth_unstable(half);
            let median = *median;
            colored.retain(|_, c| c.used > median);
        }
        colored.insert(
            key,
            Colored {
                colors: colors.clone(),
                leaves,
                used: tick,
            },
        );
        (colors, leaves)
    }
    /// Line `text`, shaped (from the cache if it is there).
    fn line(&self, text: &str, shape: &LineShaper, wrap: Option<f32>) -> Rc<dyn TextLayout> {
        let tick = self.clock.get() + 1;
        self.clock.set(tick);
        if let Some((layout, used)) = self.shaped.borrow_mut().get_mut(text) {
            *used = tick;
            return layout.clone();
        }
        let layout: Rc<dyn TextLayout> = shape(text, wrap).into();
        let mut shaped = self.shaped.borrow_mut();
        if shaped.len() >= SHAPED_LINES {
            // The least recently used half goes: amortized, a line a shape.
            let mut uses: Vec<u64> = shaped.values().map(|(_, used)| *used).collect();
            let (_, median, _) = uses.select_nth_unstable(SHAPED_LINES / 2);
            let median = *median;
            shaped.retain(|_, (_, used)| *used > median);
        }
        shaped.insert(text.into(), (layout.clone(), tick));
        layout
    }
}

/// Heights a line at a time, and the sum of those above any line, both in
/// `O(log n)` (a Fenwick tree).
struct Heights {
    tree: Vec<f64>,
    each: Vec<f32>,
}
impl Heights {
    fn new(each: Vec<f32>) -> Self {
        let mut tree: Vec<f64> = each.iter().map(|h| *h as f64).collect();
        for i in 0..tree.len() {
            let parent = i | (i + 1);
            if parent < tree.len() {
                tree[parent] += tree[i];
            }
        }
        Self { tree, each }
    }
    /// The heights of the lines above line `i`.
    fn above(&self, i: usize) -> f64 {
        let mut sum = 0.;
        let mut at = i;
        while at > 0 {
            sum += self.tree[at - 1];
            at &= at - 1;
        }
        sum
    }
    fn set(&mut self, i: usize, height: f32) {
        let delta = height as f64 - self.each[i] as f64;
        self.each[i] = height;
        let mut at = i;
        while at < self.tree.len() {
            self.tree[at] += delta;
            at |= at + 1;
        }
    }
    /// The line at `y` (the last if below them all).
    fn line_at(&self, y: f64) -> usize {
        let n = self.tree.len();
        let (mut at, mut rest) = (0, y);
        let mut step = n.next_power_of_two();
        while step > 0 {
            let next = at + step;
            if next <= n && self.tree[next - 1] <= rest {
                at = next;
                rest -= self.tree[next - 1];
            }
            step >>= 1;
        }
        at.min(n.saturating_sub(1))
    }
    fn total(&self) -> f64 {
        self.above(self.tree.len())
    }
}

/// A long text laid out by its lines (see the module).
pub(crate) struct LinesLayout {
    /// The text, shared with its editor (its lines, and where each starts).
    text: Text,
    wrap: Option<f32>,
    /// A row's height, and an estimate of a byte's advance (for unwrapped
    /// width and wrapped heights before a line is shaped).
    row: f32,
    advance: f32,
    /// The longest line's bytes, for the unwrapped width.
    longest: usize,
    widest: Cell<f32>,
    heights: Option<RefCell<Heights>>,
    shape: LineShaper,
    cache: Rc<LineCache>,
}
impl LinesLayout {
    fn new(text: Text, wrap: Option<f32>, shape: LineShaper, cache: Rc<LineCache>) -> Self {
        let longest = text.longest_line();
        let sample = shape("0", None);
        let (advance, row) = sample.size();
        let mut layout = Self {
            text,
            wrap,
            row: row.max(1.),
            advance: advance.max(0.),
            longest,
            widest: Cell::new(0.),
            heights: None,
            shape,
            cache,
        };
        if let Some(width) = wrap {
            let known = layout.cache.heights.borrow();
            let each = (0..layout.line_count())
                .map(|i| {
                    let line = layout.line_text(i);
                    known
                        .get(&*line)
                        .copied()
                        .unwrap_or_else(|| layout.estimate(&line, width))
                })
                .collect();
            drop(known);
            layout.heights = Some(RefCell::new(Heights::new(each)));
        }
        layout
    }
    /// `old`'s layout of the text it became, `text`: the same as `text`'s
    /// own. The text starts with `prefix` bytes of `old`'s and ends with
    /// `suffix`: the lines between are measured again, the rest kept.
    fn edited(old: &LinesLayout, text: Text, prefix: usize, suffix: usize) -> Self {
        let (old_len, new_len) = (old.text.len(), text.len());
        // Lines starting before the edit; those after it, moved.
        let kept = old.text.line_of(prefix) + 1;
        let moved = old.text.line_of(old_len - suffix) + 1;
        let first_moved = text.line_of(new_len - suffix) + 1;
        let mut layout = Self {
            text,
            wrap: old.wrap,
            row: old.row,
            advance: old.advance,
            longest: old.longest,
            widest: Cell::new(old.widest.get()),
            heights: None,
            shape: old.shape.clone(),
            cache: old.cache.clone(),
        };
        // The edited lines' lengths count for the longest.
        for i in kept - 1..first_moved.min(layout.line_count()) {
            layout.longest = layout.longest.max(layout.line_range(i).len());
        }
        if let (Some(width), Some(heights)) = (old.wrap, &old.heights) {
            let old_each = &heights.borrow().each;
            let known = layout.cache.heights.borrow();
            let mut each = Vec::with_capacity(layout.line_count());
            each.extend_from_slice(&old_each[..kept - 1]);
            for i in kept - 1..first_moved {
                let line = layout.line_text(i);
                each.push(
                    known
                        .get(&*line)
                        .copied()
                        .unwrap_or_else(|| layout.estimate(&line, width)),
                );
            }
            each.extend_from_slice(&old_each[moved..]);
            drop(known);
            layout.heights = Some(RefCell::new(Heights::new(each)));
        }
        layout
    }
    /// A wrapped line's height before it is shaped: its rows at the
    /// estimated advance.
    fn estimate(&self, line: &str, width: f32) -> f32 {
        let rows = ((line.len() as f32 * self.advance) / width.max(1.))
            .ceil()
            .max(1.);
        rows * self.row
    }
    /// How many lines it has.
    pub(crate) fn line_count(&self) -> usize {
        self.text.line_count()
    }
    /// Line `i`'s bytes in the text, its line break excluded.
    pub(crate) fn line_range(&self, i: usize) -> Range<usize> {
        self.text.line_range(i)
    }
    pub(crate) fn line_text(&self, i: usize) -> std::borrow::Cow<'_, str> {
        self.text.line(i)
    }
    /// The line holding byte `offset` (a line's end, before its break, is
    /// its own; the offset after the break, the next line's).
    pub(crate) fn line_of(&self, offset: usize) -> usize {
        self.text.line_of(offset)
    }
    /// Where line `i` starts down the layout.
    pub(crate) fn top(&self, i: usize) -> f32 {
        match &self.heights {
            Some(heights) => heights.borrow().above(i) as f32,
            None => i as f32 * self.row,
        }
    }
    /// Line `i`'s height (as shaped, or estimated if it is not yet).
    pub(crate) fn height(&self, i: usize) -> f32 {
        match &self.heights {
            Some(heights) => heights.borrow().each[i],
            None => self.row,
        }
    }
    /// The line at `y` down the layout.
    pub(crate) fn line_at(&self, y: f32) -> usize {
        let y = y.max(0.);
        match &self.heights {
            Some(heights) => heights.borrow().line_at(y as f64),
            None => ((y / self.row) as usize).min(self.line_count() - 1),
        }
    }
    /// The lines that show between `top` and `bottom`, shaped: their
    /// numbers, the first and the last but one.
    pub(crate) fn lines_between(&self, top: f32, bottom: f32) -> Range<usize> {
        let first = self.line_at(top);
        let mut end = first;
        // Shaping a wrapped line corrects its height, which moves those
        // below: each is placed after the one above it is shaped.
        while end < self.line_count() && self.top(end) < bottom {
            self.line(end);
            end += 1;
        }
        first..end.max(first + 1).min(self.line_count())
    }
    /// Line `i`, shaped.
    pub(crate) fn line(&self, i: usize) -> Rc<dyn TextLayout> {
        let text = self.line_text(i);
        let layout = self.cache.line(&text, &self.shape, self.wrap);
        let (width, height) = layout.size();
        if width > self.widest.get() {
            self.widest.set(width);
        }
        if let Some(heights) = &self.heights {
            let height = height.max(self.row);
            if heights.borrow().each[i] != height {
                heights.borrow_mut().set(i, height);
                self.cache
                    .heights
                    .borrow_mut()
                    .insert((*text).into(), height);
            }
        }
        layout
    }
    /// Line `i`'s local position for text position `position`.
    fn local(&self, i: usize, position: TextPosition) -> TextPosition {
        let range = self.line_range(i);
        TextPosition {
            byte_offset: position.byte_offset.clamp(range.start, range.end) - range.start,
            affinity: position.affinity,
        }
    }
    fn shift(&self, i: usize, rect: Rect) -> Rect {
        Rect::new(rect.x, rect.y + self.top(i), rect.width, rect.height)
    }
    /// The selection of `range`, only on the lines between `top` and
    /// `bottom`: what an editor draws of a selection of the whole file.
    pub(crate) fn selection_between(
        &self,
        range: Range<usize>,
        top: f32,
        bottom: f32,
    ) -> Vec<Rect> {
        let (start, end) = (range.start.min(range.end), range.start.max(range.end));
        if start == end {
            return Vec::new();
        }
        let shown = self.lines_between(top, bottom);
        let lines = shown.start.max(self.line_of(start))..shown.end.min(self.line_of(end) + 1);
        self.selection_on(start..end, lines)
    }
    /// The selection of `range` on `lines`, each shaped.
    fn selection_on(&self, range: Range<usize>, lines: Range<usize>) -> Vec<Rect> {
        let (start, end) = (range.start, range.end);
        let mut rects = Vec::new();
        for i in lines {
            let line = self.line_range(i);
            let (a, b) = (start.max(line.start), end.min(line.end));
            if a >= b {
                continue;
            }
            let layout = self.line(i);
            rects.extend(
                layout
                    .selection(a - line.start..b - line.start)
                    .into_iter()
                    .map(|rect| self.shift(i, rect)),
            );
        }
        rects
    }
}
impl TextLayout for LinesLayout {
    fn size(&self) -> (f32, f32) {
        let height = match &self.heights {
            Some(heights) => heights.borrow().total() as f32,
            None => self.line_count() as f32 * self.row,
        };
        let width = match self.wrap {
            Some(width) => width,
            None => self.widest.get().max(self.longest as f32 * self.advance),
        };
        (width, height)
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        self.hit_position(x, y).byte_offset
    }
    fn hit_position(&self, x: f32, y: f32) -> TextPosition {
        let i = self.line_at(y);
        let layout = self.line(i);
        let local = layout.hit_position(x, y.max(0.) - self.top(i));
        let range = self.line_range(i);
        TextPosition {
            byte_offset: range.start + local.byte_offset.min(range.len()),
            // Its line's end is before its break, which a caret sits after.
            affinity: if local.byte_offset >= range.len() && i + 1 < self.line_count() {
                TextAffinity::After
            } else {
                local.affinity
            },
        }
    }
    fn caret(&self, byte_offset: usize) -> Rect {
        self.caret_position(TextPosition {
            byte_offset,
            affinity: TextAffinity::After,
        })
    }
    fn caret_position(&self, position: TextPosition) -> Rect {
        let i = self.line_of(position.byte_offset.min(self.text.len()));
        let layout = self.line(i);
        self.shift(i, layout.caret_position(self.local(i, position)))
    }
    /// The selection of `range`: its own lines shaped, not those above it
    /// (a composition's underline, a few characters of a long file).
    fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        let (start, end) = (range.start.min(range.end), range.start.max(range.end));
        if start == end {
            return Vec::new();
        }
        self.selection_on(start..end, self.line_of(start)..self.line_of(end) + 1)
    }
    fn selection_within(&self, range: Range<usize>, top: f32, bottom: f32) -> Vec<Rect> {
        self.selection_between(range, top, bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_layout::{FallbackTextLayout, FontStyle};

    fn shaper() -> LineShaper {
        Rc::new(|text, width| Box::new(FallbackTextLayout::new(text, 14., width)))
    }
    fn lines(text: &str, wrap: Option<f32>) -> Rc<LinesLayout> {
        Rc::new(LineCache::default()).layout(
            || Text::new(text),
            None,
            14.,
            wrap,
            &FontStyle::default(),
            0,
            shaper,
        )
    }

    /// Laid out by its lines, a text has the carets, hits and selections it
    /// has laid out whole, wrapped or not.
    #[test]
    fn lines_lay_out_as_the_whole_text_does() {
        let text =
            "fn main() {\n\n    let wide = \"a line long enough to wrap twice over\";\n}\nend";
        for wrap in [None, Some(120.)] {
            let whole = FallbackTextLayout::new(text, 14., wrap);
            let lines = lines(text, wrap);
            assert_eq!(whole.size().1, lines.size().1, "{wrap:?}");
            for offset in 0..=text.len() {
                if !text.is_char_boundary(offset) {
                    continue;
                }
                for affinity in [TextAffinity::Before, TextAffinity::After] {
                    let position = TextPosition {
                        byte_offset: offset,
                        affinity,
                    };
                    assert_eq!(
                        whole.caret_position(position),
                        lines.caret_position(position),
                        "caret {offset} {affinity:?} {wrap:?}"
                    );
                }
                for end in offset..=text.len() {
                    assert_eq!(
                        whole.selection(offset..end),
                        lines.selection(offset..end),
                        "selection {offset}..{end} {wrap:?}"
                    );
                }
            }
            let (_, height) = whole.size();
            let mut y = -5.;
            while y < height + 10. {
                let mut x = -5.;
                while x < 400. {
                    assert_eq!(
                        whole.hit_position(x, y).byte_offset,
                        lines.hit_position(x, y).byte_offset,
                        "hit {x} {y} {wrap:?}"
                    );
                    x += 7.;
                }
                y += 5.;
            }
        }
    }

    /// An edit finds the lines it changed and moves those after it: the
    /// same lines a layout of the new text has, wrapped or not.
    #[test]
    fn an_edit_lays_out_as_the_text_it_makes_does() {
        let mut text = String::from("one\ntwo\n\nthree four five six seven\neight");
        let edits: [(usize, usize, &str); 8] = [
            (0, 0, "zero\n"),
            (4, 9, ""),
            (7, 7, "a\nb\nc"),
            (3, 12, "x"),
            (0, 5, ""),
            (2, 2, "\n\n"),
            (1, 3, "long line that wraps around and around"),
            (0, 0, ""),
        ];
        for wrap in [None, Some(60.)] {
            let cache = Rc::new(LineCache::default());
            let mut current = text.clone();
            cache.layout(
                || Text::new(&current),
                None,
                14.,
                wrap,
                &FontStyle::default(),
                0,
                shaper,
            );
            for (start, end, insert) in edits {
                let (start, end) = (start.min(current.len()), end.min(current.len()));
                current.replace_range(start.min(end)..end, insert);
                let layout = cache.layout(
                    || Text::new(&current),
                    None,
                    14.,
                    wrap,
                    &FontStyle::default(),
                    0,
                    shaper,
                );
                let fresh = lines(&current, wrap);
                assert_eq!(
                    (0..layout.line_count())
                        .map(|i| layout.line_range(i))
                        .collect::<Vec<_>>(),
                    (0..fresh.line_count())
                        .map(|i| fresh.line_range(i))
                        .collect::<Vec<_>>(),
                    "{current:?} {wrap:?}"
                );
                assert_eq!(layout.size().1, fresh.size().1, "{current:?} {wrap:?}");
                assert!(layout.longest >= fresh.longest);
            }
            text = current;
        }
    }

    /// A file of 100,000 lines shapes the lines asked for, not the file,
    /// and keeps a few bytes a line.
    #[test]
    fn a_long_file_shapes_only_the_lines_it_shows() {
        let shaped = Rc::new(Cell::new(0));
        let count = shaped.clone();
        let shape: LineShaper = Rc::new(move |text, width| {
            count.set(count.get() + 1);
            Box::new(FallbackTextLayout::new(text, 14., width))
        });
        let text: String = (0..100_000).map(|i| format!("line {i}\n")).collect();
        let cache = Rc::new(LineCache::default());
        let layout = cache.layout(
            || Text::new(&text),
            None,
            14.,
            None,
            &FontStyle::default(),
            0,
            || shape,
        );
        let shown = layout.lines_between(50_000. * layout.row, 50_000. * layout.row + 600.);
        assert_eq!(shown.start, 50_000);
        assert!(shown.len() < 40, "{shown:?}");
        let caret = layout.caret(text.find("line 99999").unwrap());
        assert_eq!(caret.y, 99_999. * layout.row);
        // The sample row, the lines shown and the caret's.
        assert!(shaped.get() < 45, "{}", shaped.get());
        // A selection (a composition's underline) shapes its own lines, not
        // every line above it.
        let at = text.find("line 70000").unwrap();
        let before = shaped.get();
        assert_eq!(layout.selection(at..at + 4).len(), 1);
        assert!(shaped.get() - before <= 1, "{}", shaped.get() - before);
    }
}
