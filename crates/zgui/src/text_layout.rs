//! Platform-independent shaped text geometry consumed by editors and selection widgets.
use crate::scene::Rect;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
/// Portable font selection. Named families fall back through the platform font database.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontFamily {
    /// The platform's sans-serif interface font.
    #[default]
    SansSerif,
    /// The platform's serif font.
    Serif,
    /// The platform's monospaced font.
    Monospace,
    /// A family by name (`"Inter"`).
    Named(std::sync::Arc<str>),
}
impl From<&str> for FontFamily {
    fn from(value: &str) -> Self {
        Self::Named(value.into())
    }
}
impl From<String> for FontFamily {
    fn from(value: String) -> Self {
        Self::Named(value.into())
    }
}
/// Logical line-box pitch. Normal uses `ceil(font_size * 1.4)`.
///
/// Positive finite pixel values are preserved, including pitches smaller than
/// glyph ink. Text still obeys its allocated paint bounds: tight lines may
/// overlap internally and ink at the outer text-box edges may be clipped.
/// Private normalized bits give font caches stable equality and hashing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LineHeight(u32);
impl LineHeight {
    /// The normal line pitch for the font size.
    pub const NORMAL: Self = Self(0);

    /// Invalid, zero, or negative values reset the normal line pitch.
    pub fn px(value: f32) -> Self {
        if value.is_finite() && value > 0. {
            Self(value.to_bits())
        } else {
            Self::NORMAL
        }
    }

    /// The line height in logical pixels, or `None` for normal.
    pub fn pixels(self) -> Option<f32> {
        (self != Self::NORMAL).then(|| f32::from_bits(self.0))
    }

    /// The line pitch for text `font_size` large.
    pub fn resolve(self, font_size: f32) -> f32 {
        self.pixels().unwrap_or_else(|| {
            let size = if font_size.is_finite() {
                font_size.max(1.)
            } else {
                1.
            };
            (size * 1.4).min(f32::MAX).ceil()
        })
    }
}
/// Additional tracking in logical pixels. Native glyph clusters follow the font
/// shaper; the fallback uses grapheme cells. Nonfinite values and signed zero
/// normalize to zero; finite values saturate at ±1,000,000 logical pixels to
/// keep native geometry practical and typography cache keys consistent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LetterSpacing(u32);
impl LetterSpacing {
    /// No extra spacing.
    pub const ZERO: Self = Self(0);
    /// `value` logical pixels between characters, normalized as the type
    /// says.
    pub fn px(value: f32) -> Self {
        if value.is_finite() && value != 0. {
            Self(value.clamp(-1_000_000., 1_000_000.).to_bits())
        } else {
            Self::ZERO
        }
    }
    /// The spacing in logical pixels.
    pub fn pixels(self) -> f32 {
        f32::from_bits(self.0)
    }
}
/// Whether `text` holds right-to-left letters (Hebrew, Arabic, Syriac,
/// Thaana, N'Ko, and their presentation forms): text that a caret moves
/// through by what it shows, not its order.
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
    })
}
/// Physical paragraph alignment within an allocated text width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    /// At the start of the line: the left left to right, the right right to
    /// left.
    #[default]
    Start,
    /// At the left.
    Left,
    /// Centred.
    Center,
    /// At the right.
    Right,
    /// At the end of the line (CSS's `end`): the right left to right, the
    /// left right to left. A number column's alignment.
    End,
}
/// Display-only overflow policy. Editor documents are never truncated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    /// Cut off at the box's edge.
    #[default]
    Clip,
    /// Ended with an ellipsis where it is cut.
    Ellipsis,
}
/// How plain text that doesn't fit ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextOptions {
    /// How text past the last line or the width ends.
    pub overflow: TextOverflow,
    /// The most lines shown; `None` shows them all.
    pub line_clamp: Option<std::num::NonZeroU32>,
}
/// Canonical OpenType feature settings. Tags use four ASCII bytes such as
/// `*b"liga"`; the last occurrence of a tag wins. Sorted storage gives equivalent
/// declarations identical font-cache keys. Unsupported tags are ignored by fonts.
type FeatureSettings = std::sync::Arc<[([u8; 4], u32)]>;
/// OpenType feature settings, canonical: each tag once, sorted. Empty
/// uses the font's defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontFeatures(Option<FeatureSettings>);
impl FontFeatures {
    /// The settings `features` gives, each a four-byte tag and a value (`1`
    /// on, `0` off); a later tag replaces an earlier one.
    pub fn new(features: impl IntoIterator<Item = ([u8; 4], u32)>) -> Self {
        let mut canonical = std::collections::BTreeMap::new();
        for (tag, value) in features {
            canonical.insert(tag, value);
        }
        if canonical.is_empty() {
            Self::default()
        } else {
            Self(Some(canonical.into_iter().collect::<Vec<_>>().into()))
        }
    }
    /// The settings, sorted by tag.
    pub fn settings(&self) -> &[([u8; 4], u32)] {
        self.0.as_deref().unwrap_or(&[])
    }
    /// The bytes the settings take, for cache accounting.
    pub fn storage_bytes(&self) -> usize {
        std::mem::size_of_val(self.settings())
    }
}
/// How text is set: family, weight, style, spacing, features and
/// alignment.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontStyle {
    /// The font family.
    pub family: FontFamily,
    /// OpenType weight in the inclusive range 1..=1000 (400 regular, 700 bold).
    pub weight: u16,
    /// Whether it is italic.
    pub italic: bool,
    /// The line pitch.
    pub line_height: LineHeight,
    /// Extra spacing between characters.
    pub letter_spacing: LetterSpacing,
    /// OpenType features.
    pub features: FontFeatures,
    /// How lines align in their box.
    pub align: TextAlign,
    /// Explicit ordered families tried before platform fallback for missing glyphs.
    pub fallbacks: std::sync::Arc<[FontFamily]>,
    /// Shown masked, a bullet per character (a password field's text). Set
    /// by an editor on its own text; never inherited.
    pub masked: bool,
    /// Underlined (CSS `text-decoration: underline`): inherited.
    pub underline: Option<TextUnderline>,
}
/// An underline under text, `offset` logical pixels below the baseline
/// (CSS `text-underline-offset`; unset, a tenth of the font size).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextUnderline(Option<u32>);
impl TextUnderline {
    /// An underline `offset` logical pixels below the baseline, or at the
    /// default with `None`.
    pub fn new(offset: Option<f32>) -> Self {
        Self(offset.filter(|o| o.is_finite()).map(f32::to_bits))
    }
    /// Its offset below the baseline, if set.
    pub fn offset(self) -> Option<f32> {
        self.0.map(f32::from_bits)
    }
}
impl Default for FontStyle {
    fn default() -> Self {
        Self {
            family: FontFamily::SansSerif,
            weight: 400,
            italic: false,
            line_height: LineHeight::NORMAL,
            letter_spacing: LetterSpacing::ZERO,
            features: FontFeatures::default(),
            align: TextAlign::Start,
            fallbacks: {
                static EMPTY: std::sync::OnceLock<std::sync::Arc<[FontFamily]>> =
                    std::sync::OnceLock::new();
                EMPTY.get_or_init(|| std::sync::Arc::from([])).clone()
            },
            masked: false,
            underline: None,
        }
    }
}
impl FontStyle {
    /// Owned key payload, conservatively charging shared allocations per entry.
    pub fn storage_bytes(&self) -> usize {
        self.features
            .storage_bytes()
            .saturating_add(std::mem::size_of_val(self.fallbacks.as_ref()))
            .saturating_add(
                self.fallbacks
                    .iter()
                    .map(|family| match family {
                        FontFamily::Named(name) => name.len(),
                        _ => 0,
                    })
                    .sum::<usize>(),
            )
            .saturating_add(match &self.family {
                FontFamily::Named(name) => name.len(),
                _ => 0,
            })
    }
}
/// Font-aware shaping callback. The legacy TextShaper remains available for default fonts.
pub trait FontTextShaper {
    /// Shape `text` at `size` in `font`, wrapped at `width` if given.
    fn shape(
        &self,
        text: &str,
        size: f32,
        width: Option<f32>,
        font: &FontStyle,
    ) -> Box<dyn TextLayout>;
}
impl<F: Fn(&str, f32, Option<f32>, &FontStyle) -> Box<dyn TextLayout>> FontTextShaper for F {
    fn shape(
        &self,
        text: &str,
        size: f32,
        width: Option<f32>,
        font: &FontStyle,
    ) -> Box<dyn TextLayout> {
        self(text, size, width, font)
    }
}
/// Which side of a shared cluster or soft-wrap boundary owns a caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAffinity {
    /// Trailing edge of the preceding logical cluster; upstream at a soft wrap.
    Before,
    /// Leading edge of the following logical cluster; downstream at a soft wrap.
    #[default]
    After,
}
/// A caret position in text: a byte offset and which side of it the
/// caret sits on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextPosition {
    /// UTF-8 byte offset on a grapheme boundary.
    pub byte_offset: usize,
    /// Which side of a shared boundary the caret sits on.
    pub affinity: TextAffinity,
}
/// Shaped text's geometry: its size, carets, hit testing and selection
/// rectangles, in the layout's own logical coordinates.
pub trait TextLayout {
    /// Estimated retained bytes for cache admission, including this layout and
    /// its owned allocations. Shared font databases are excluded. This is cache
    /// accounting, not allocator usage or a bound on process RSS; caches must
    /// also charge their keys and entry overhead. Return `None` when no useful
    /// estimate is available: unknown custom layouts must not be retained by a
    /// byte-budgeted cache. An estimate must remain conservative if geometry
    /// queries use interior caches; return `None` when their growth is unknown.
    /// The default preserves existing custom shapers.
    fn cache_weight(&self) -> Option<usize> {
        None
    }
    /// Its width and height.
    fn size(&self) -> (f32, f32);
    /// Returns a UTF-8 byte offset on a grapheme boundary.
    fn hit_test(&self, x: f32, y: f32) -> usize;
    /// The caret's rectangle at UTF-8 offset `byte_offset`.
    fn caret(&self, byte_offset: usize) -> Rect;
    /// Visual spans may be disjoint for bidirectional text.
    fn selection(&self, range: Range<usize>) -> Vec<Rect>;
    /// Affinity-aware geometry; defaults preserve compatibility with custom shapers.
    fn hit_position(&self, x: f32, y: f32) -> TextPosition {
        TextPosition {
            byte_offset: self.hit_test(x, y),
            affinity: TextAffinity::After,
        }
    }
    /// The caret's rectangle at `position`, on the side its affinity says.
    fn caret_position(&self, position: TextPosition) -> Rect {
        self.caret(position.byte_offset)
    }
    /// Logical byte-ordered range of the caret's visual row, excluding hard
    /// line endings. Native bidi layouts override this: physical left/right
    /// edges need not enclose every logical cluster on a mixed-direction row.
    fn visual_line_range(&self, position: TextPosition) -> (TextPosition, TextPosition) {
        let left = self.visual_line_edge(position, false);
        let right = self.visual_line_edge(position, true);
        if left.byte_offset <= right.byte_offset {
            (left, right)
        } else {
            (right, left)
        }
    }
    /// Physical left/right edge of the caret's visual line, excluding line endings.
    fn visual_line_edge(&self, position: TextPosition, end: bool) -> TextPosition {
        let caret = self.caret_position(position);
        self.hit_position(
            if end { f32::MAX } else { -f32::MAX },
            caret.y + caret.height * 0.5,
        )
    }
}
/// Shapes text in the default font (see [`FontTextShaper`] for others).
pub trait TextShaper {
    /// Shape `text` at `font_size`, wrapped at `max_width` if given.
    fn shape(&self, text: &str, font_size: f32, max_width: Option<f32>) -> Box<dyn TextLayout>;
}
impl<F: Fn(&str, f32, Option<f32>) -> Box<dyn TextLayout>> TextShaper for F {
    fn shape(&self, text: &str, font_size: f32, max_width: Option<f32>) -> Box<dyn TextLayout> {
        self(text, font_size, max_width)
    }
}
/// Approximate grapheme cells for headless/core use. Native hosts install a real shaper.
pub struct FallbackTextLayout {
    cells: Vec<(Range<usize>, Rect)>,
    end: Rect,
    size: (f32, f32),
}
impl FallbackTextLayout {
    /// Mixed-size approximate cells used when no native rich shaper is installed.
    pub fn with_rich(rich: &crate::rich_text::RichText, width: Option<f32>) -> Self {
        let mut cells: Vec<(Range<usize>, Rect)> = Vec::new();
        let (mut x, mut y, mut max_x, mut pitch) = (0_f32, 0_f32, 0_f32, 0_f32);
        let mut row_start = 0;
        let mut run_index = 0;
        for (index, grapheme) in rich.text().grapheme_indices(true) {
            while rich.runs()[run_index].range.end <= index {
                run_index += 1;
            }
            let run = &rich.runs()[run_index];
            let line = run.font.line_height.resolve(run.font_size);
            let newline = grapheme.contains(['\r', '\n']);
            let advance = run.estimated_advance(grapheme, index);
            if newline || width.is_some_and(|w| x > 0. && x + advance > w.max(0.)) {
                if newline {
                    pitch = pitch.max(line);
                }
                for (_, rect) in &mut cells[row_start..] {
                    rect.height = pitch;
                }
                max_x = max_x.max(x);
                y += pitch;
                x = 0.;
                pitch = 0.;
                row_start = cells.len();
                if newline {
                    pitch = line;
                    continue;
                }
            }
            pitch = pitch.max(line);
            cells.push((
                index..index + grapheme.len(),
                Rect::new(x, y, advance, pitch),
            ));
            x += advance;
        }
        for (_, rect) in &mut cells[row_start..] {
            rect.height = pitch;
        }
        let mut layout = Self {
            cells,
            end: Rect::new(x, y, 1., pitch),
            size: (max_x.max(x), y + pitch),
        };
        if let Some(limit) = rich.options().line_clamp.map(|n| n.get()).or((rich
            .options()
            .overflow
            == TextOverflow::Ellipsis)
            .then_some(1))
        {
            let mut row = 0;
            let mut y = None;
            let mut keep = 0;
            for (_, rect) in &layout.cells {
                if y != Some(rect.y) {
                    row += 1;
                    y = Some(rect.y);
                }
                if row > limit {
                    break;
                }
                keep += 1;
            }
            let hidden = keep < layout.cells.len();
            layout.cells.truncate(keep);
            if hidden
                && rich.options().overflow == TextOverflow::Ellipsis
                && let Some(width) = width
            {
                let last = layout
                    .cells
                    .last()
                    .map(|(range, rect)| (range.start, rect.y));
                if let Some((index, y)) = last {
                    let advance = rich
                        .runs()
                        .iter()
                        .find(|r| r.range.contains(&index))
                        .map_or(0., |r| {
                            (r.font_size * 0.6 + r.font.letter_spacing.pixels()).max(0.)
                        });
                    while layout
                        .cells
                        .last()
                        .is_some_and(|(_, r)| r.y == y && r.x + r.width + advance > width)
                    {
                        layout.cells.pop();
                    }
                }
            }
            if let Some((_, rect)) = layout.cells.last() {
                layout.end = Rect::new(rect.x + rect.width, rect.y, 1., rect.height);
            }
            layout.size = crate::rich_text::fallback_measure(rich, width);
        }
        layout.align(
            width,
            rich.runs()
                .first()
                .map_or(TextAlign::Left, |r| r.font.align),
        );
        layout
    }
    /// `text` at `size` in the default font, wrapped at `max_width` if given.
    pub fn new(text: &str, size: f32, max_width: Option<f32>) -> Self {
        Self::with_line_height(text, size, max_width, LineHeight::NORMAL)
    }
    /// [`FallbackTextLayout::new`] with a line height.
    pub fn with_line_height(
        text: &str,
        size: f32,
        max_width: Option<f32>,
        line_height: LineHeight,
    ) -> Self {
        Self::with_font(
            text,
            size,
            max_width,
            &FontStyle {
                line_height,
                ..FontStyle::default()
            },
        )
    }
    /// Grapheme-cell fallback with the same metrics as font-aware measurement.
    /// Tracking includes the final cell advance. Extreme negative tracking
    /// collapses advances to zero; native glyph shaping may overlap instead.
    pub fn with_font(text: &str, size: f32, max_width: Option<f32>, font: &FontStyle) -> Self {
        let mut cells = Vec::new();
        let (end, size) = Self::place(text, size, max_width, font, |range, rect| {
            cells.push((range, rect))
        });
        let mut layout = Self { cells, end, size };
        layout.align(max_width, font.align);
        layout
    }
    fn align(&mut self, width: Option<f32>, alignment: TextAlign) {
        let Some(width) = width.filter(|w| w.is_finite() && *w > 0.) else {
            return;
        };
        let factor = match alignment {
            TextAlign::Start | TextAlign::Left => return,
            TextAlign::Center => 0.5,
            TextAlign::Right | TextAlign::End => 1.,
        };
        let mut start = 0;
        while start < self.cells.len() {
            let y = self.cells[start].1.y;
            let mut end = start + 1;
            while end < self.cells.len() && self.cells[end].1.y == y {
                end += 1;
            }
            let row_width = self.cells[start..end]
                .iter()
                .map(|(_, r)| r.x + r.width)
                .fold(0_f32, f32::max);
            let shift = (width - row_width).max(0.) * factor;
            for (_, rect) in &mut self.cells[start..end] {
                rect.x += shift;
            }
            if self.end.y == y {
                self.end.x += shift;
            }
            start = end;
        }
        if self.cells.last().is_none_or(|(_, r)| r.y != self.end.y) {
            self.end.x += width * factor;
        }
    }
    /// Measurement shares shaping's grapheme placement with O(1) auxiliary memory.
    pub fn measure(
        text: &str,
        size: f32,
        max_width: Option<f32>,
        line_height: LineHeight,
    ) -> (f32, f32) {
        Self::measure_with_font(
            text,
            size,
            max_width,
            &FontStyle {
                line_height,
                ..FontStyle::default()
            },
        )
    }
    /// The size of `text` at `size` in `font`, wrapped at `max_width` if
    /// given.
    pub fn measure_with_font(
        text: &str,
        size: f32,
        max_width: Option<f32>,
        font: &FontStyle,
    ) -> (f32, f32) {
        Self::place(text, size, max_width, font, |_, _| {}).1
    }
    fn place(
        text: &str,
        size: f32,
        max_width: Option<f32>,
        font: &FontStyle,
        mut cell: impl FnMut(Range<usize>, Rect),
    ) -> (Rect, (f32, f32)) {
        let size = if size.is_finite() { size.max(1.) } else { 1. };
        // Extreme negative tracking collapses cells rather than reversing their
        // order. Include the final advance, consistently with native tracking.
        let advance = (size as f64 * 0.6 + font.letter_spacing.pixels() as f64)
            .clamp(0., f32::MAX as f64) as f32;
        let height = font.line_height.resolve(size);
        let mut x = 0.;
        let mut y = 0.;
        let mut width = 0_f32;
        for (index, grapheme) in text.grapheme_indices(true) {
            if grapheme.contains(['\n', '\r']) {
                cell(index..index + grapheme.len(), Rect::new(x, y, 0., height));
                width = width.max(x);
                x = 0.;
                y = (y + height).min(f32::MAX);
                continue;
            }
            if max_width.is_some_and(|w| w > 0. && x > 0. && x + advance > w) {
                width = width.max(x);
                x = 0.;
                y = (y + height).min(f32::MAX)
            }
            cell(
                index..index + grapheme.len(),
                Rect::new(x, y, advance, height),
            );
            x = (x + advance).min(f32::MAX);
        }
        width = width.max(x);
        (
            Rect::new(x, y, 1., height),
            (width, (y + height).min(f32::MAX)),
        )
    }
}
impl TextLayout for FallbackTextLayout {
    fn cache_weight(&self) -> Option<usize> {
        Some(
            std::mem::size_of::<Self>().saturating_add(
                self.cells
                    .capacity()
                    .saturating_mul(std::mem::size_of::<(Range<usize>, Rect)>()),
            ),
        )
    }
    fn size(&self) -> (f32, f32) {
        self.size
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        self.hit_position(x, y).byte_offset
    }
    fn hit_position(&self, x: f32, y: f32) -> TextPosition {
        let y = y.max(0.);
        let mut result = TextPosition::default();
        for (range, r) in &self.cells {
            if y >= r.y && y < r.y + r.height {
                if r.width == 0. || x < r.x + r.width * 0.5 {
                    return TextPosition {
                        byte_offset: range.start,
                        affinity: TextAffinity::After,
                    };
                }
                result = TextPosition {
                    byte_offset: range.end,
                    affinity: TextAffinity::Before,
                };
            } else if y >= r.y + r.height {
                result = TextPosition {
                    byte_offset: range.end,
                    affinity: TextAffinity::After,
                };
            }
        }
        result
    }
    fn caret_position(&self, position: TextPosition) -> Rect {
        if position.affinity == TextAffinity::Before {
            for (range, rect) in &self.cells {
                if range.end == position.byte_offset && rect.width > 0. {
                    return Rect::new(rect.x + rect.width, rect.y, 1., rect.height);
                }
            }
        }
        self.caret(position.byte_offset)
    }
    fn caret(&self, byte_offset: usize) -> Rect {
        for (range, r) in &self.cells {
            if byte_offset <= range.start || range.contains(&byte_offset) {
                return Rect::new(r.x, r.y, 1., r.height);
            }
        }
        self.end
    }
    fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        if start == end {
            return Vec::new();
        }
        let mut result: Vec<Rect> = Vec::new();
        for (index, r) in &self.cells {
            if index.start < end && index.end > start && r.width > 0. {
                if let Some(last) = result.last_mut()
                    && last.y == r.y
                    && last.x + last.width >= r.x
                {
                    *last = last.union(*r);
                    continue;
                }
                result.push(*r);
            }
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_height_normalization_hashing_and_tight_pitch() {
        use std::collections::HashSet;
        let invalid = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., -0., 0.];
        let mut heights: HashSet<_> = invalid.into_iter().map(LineHeight::px).collect();
        assert_eq!(heights.len(), 1);
        assert!(heights.contains(&LineHeight::NORMAL));
        assert_eq!(LineHeight::NORMAL.resolve(10.), 14.);
        assert_eq!(LineHeight::NORMAL.resolve(11.), 16.);
        assert_eq!(LineHeight::NORMAL.resolve(f32::MAX), f32::MAX);
        assert!(LineHeight::NORMAL.resolve(f32::INFINITY).is_finite());
        heights.insert(LineHeight::px(14.));
        assert_eq!(
            heights.len(),
            2,
            "normal remains size-dependent, unlike explicit14px"
        );
        assert_eq!(LineHeight::px(0.5).resolve(40.), 0.5);
        assert_eq!(
            LineHeight::px(f32::from_bits(1)).pixels(),
            Some(f32::from_bits(1))
        );
    }
    #[test]
    fn explicit_pitch_keeps_wrapped_unicode_hit_selection_and_caret_aligned() {
        let layout = FallbackTextLayout::with_line_height(
            "a👩‍💻e\u{301}z\n",
            10.,
            Some(12.),
            LineHeight::px(7.5),
        );
        assert_eq!(layout.size(), (12., 22.5));
        assert_eq!(layout.caret(12), Rect::new(0., 7.5, 1., 7.5));
        assert_eq!(layout.hit_test(1., 8.), 12);
        assert_eq!(layout.caret(17), Rect::new(0., 15., 1., 7.5));
        assert_eq!(
            layout.selection(1..15),
            vec![Rect::new(6., 0., 6., 7.5), Rect::new(0., 7.5, 6., 7.5)]
        );
    }
    #[test]
    fn measurement_and_shape_agree_for_unbounded_and_tiny_widths() {
        for width in [
            None,
            Some(-1.),
            Some(0.),
            Some(f32::NAN),
            Some(f32::from_bits(1)),
            Some(6.),
        ] {
            for pitch in [LineHeight::NORMAL, LineHeight::px(0.5), LineHeight::px(30.)] {
                let text = "a👩‍💻e\u{301}\r\nz\n";
                let measured = FallbackTextLayout::measure(text, 10., width, pitch);
                assert_eq!(
                    measured,
                    FallbackTextLayout::with_line_height(text, 10., width, pitch).size()
                );
                assert!(measured.0 > 0. && measured.1 > 0. && measured.1.is_finite());
            }
        }
    }
    #[test]
    fn grapheme_offsets_newlines_and_wrap() {
        let layout = FallbackTextLayout::new("e\u{301}👩‍💻\nx", 10., None);
        assert_eq!(layout.hit_test(1., 2.), 0);
        assert_eq!(layout.hit_test(8., 2.), 3);
        assert_eq!(layout.caret(3).x, 6.);
        assert_eq!(layout.selection(0..3).len(), 1);
        assert_eq!(layout.size().1, 28.);
    }
    #[test]
    fn wrapped_grapheme_boundary_preserves_visual_affinity() {
        let layout = FallbackTextLayout::new("a👩‍💻e\u{301}z", 10., Some(12.));
        let end = layout.visual_line_edge(TextPosition::default(), true);
        assert_eq!(
            end,
            TextPosition {
                byte_offset: 12,
                affinity: TextAffinity::Before
            }
        );
        assert_eq!(layout.caret_position(end), Rect::new(12., 0., 1., 14.));
        let next = TextPosition {
            affinity: TextAffinity::After,
            ..end
        };
        assert_eq!(layout.caret_position(next), Rect::new(0., 14., 1., 14.));
        assert_eq!(layout.visual_line_edge(next, false), next);
        assert_eq!(layout.hit_position(100., 5.), end);
        assert_eq!(layout.hit_position(-10., 18.), next);
        assert_eq!(
            layout.selection(1..15),
            vec![Rect::new(6., 0., 6., 14.), Rect::new(0., 14., 6., 14.)]
        );
        assert!(layout.selection(2..2).is_empty());
    }
    #[test]
    fn visual_edges_exclude_crlf_and_preserve_empty_final_lines() {
        let layout = FallbackTextLayout::new("ab\r\n\n", 10., Some(30.));
        assert_eq!(layout.hit_test(100., 2.), 2);
        assert_eq!(layout.hit_test(100., 16.), 4);
        assert_eq!(layout.hit_test(100., 30.), 5);
        assert_eq!(layout.caret(5).y, 28.);
        assert_eq!(layout.hit_test(10., -10.), 2);
    }
}

#[cfg(test)]
mod cache_weight_tests {
    use super::*;

    #[test]
    fn fallback_charges_spare_cell_capacity_without_counting_shared_sources() {
        let mut layout = FallbackTextLayout::new("abc", 14., None);
        let initial = layout.cache_weight().unwrap();
        layout.cells.reserve(1024);
        let expected = std::mem::size_of::<FallbackTextLayout>()
            + layout.cells.capacity() * std::mem::size_of::<(Range<usize>, Rect)>();
        assert_eq!(layout.cache_weight(), Some(expected));
        assert!(expected > initial);
    }

    #[test]
    fn existing_custom_layouts_have_unknown_weight_by_default() {
        struct Custom;
        impl TextLayout for Custom {
            fn size(&self) -> (f32, f32) {
                (0., 0.)
            }
            fn hit_test(&self, _: f32, _: f32) -> usize {
                0
            }
            fn caret(&self, _: usize) -> Rect {
                Rect::default()
            }
            fn selection(&self, _: Range<usize>) -> Vec<Rect> {
                Vec::new()
            }
        }
        assert_eq!(Custom.cache_weight(), None);
    }
}

#[cfg(test)]
mod letter_spacing_tests {
    use super::*;

    #[test]
    fn normalized_spacing_has_stable_equality_and_hash() {
        use std::collections::HashSet;
        let normalized: HashSet<_> = [0., -0., f32::NAN, f32::INFINITY, f32::NEG_INFINITY]
            .into_iter()
            .map(LetterSpacing::px)
            .collect();
        assert_eq!(normalized.len(), 1);
        assert_eq!(LetterSpacing::px(-2.).pixels(), -2.);
        assert_eq!(LetterSpacing::px(2.).pixels(), 2.);
        assert_eq!(LetterSpacing::px(f32::MAX), LetterSpacing::px(1_000_000.));
        assert_eq!(LetterSpacing::px(-f32::MAX), LetterSpacing::px(-1_000_000.));
        assert_eq!(LetterSpacing::px(999_999.).pixels(), 999_999.);
        assert_eq!(LetterSpacing::px(-999_999.).pixels(), -999_999.);
    }

    #[test]
    fn spacing_changes_grapheme_advance_wrap_and_selection_consistently() {
        let font = FontStyle {
            letter_spacing: LetterSpacing::px(4.),
            ..FontStyle::default()
        };
        let text = "a\u{301}bc";
        let layout = FallbackTextLayout::with_font(text, 10., Some(20.), &font);
        assert_eq!(layout.size(), (20., 28.));
        assert_eq!(layout.caret(3), Rect::new(10., 0., 1., 14.));
        assert_eq!(layout.caret(4).y, 14.);
        assert_eq!(layout.hit_test(9., 7.), 3);
        assert_eq!(layout.selection(0..3), vec![Rect::new(0., 0., 10., 14.)]);
        assert_eq!(
            layout.size(),
            FallbackTextLayout::measure_with_font(text, 10., Some(20.), &font)
        );
        let negative = FontStyle {
            letter_spacing: LetterSpacing::px(-2.),
            ..font
        };
        assert_eq!(
            FallbackTextLayout::with_font(text, 10., None, &negative)
                .size()
                .0,
            12.
        );
    }

    #[test]
    fn extreme_spacing_keeps_fallback_geometry_finite_and_nonnegative() {
        for spacing in [-f32::MAX, f32::MAX] {
            let font = FontStyle {
                letter_spacing: LetterSpacing::px(spacing),
                ..FontStyle::default()
            };
            let layout = FallbackTextLayout::with_font("ab\ncd", 10., None, &font);
            let (w, h) = layout.size();
            assert!(w.is_finite() && h.is_finite() && w >= 0. && h >= 0.);
            let caret = layout.caret(5);
            assert!(caret.x.is_finite() && caret.x >= 0.);
            assert_eq!(
                layout.size(),
                FallbackTextLayout::measure_with_font("ab\ncd", 10., None, &font)
            );
        }
    }
}
