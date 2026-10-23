//! Reusable shaped text geometry for editor hit testing, carets and bidi selections.
pub use cosmic_text::Affinity;
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};
use std::{ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
use zgui::scene::Rect;
/// The width text wraps at: the box's, and a thousandth of a point more.
/// A box sized to its text gets that width back through layout arithmetic
/// (a bubble's padding added, then taken off), where a float's last bit
/// can come out short and break a line that fits: "hi" in two lines. The
/// measuring and drawing paths both wrap here, so they agree.
pub fn wrap_width(width: Option<f32>) -> Option<f32> {
    width.map(|w| if w.is_finite() { w + 1e-3 } else { w })
}

/// Validated TrueType/OpenType font or font collection, shared without copying
/// its bytes between windows. Registration is additive; choose its family by name.
#[derive(Clone, Debug)]
pub struct FontData {
    bytes: Arc<[u8]>,
    families: Arc<[String]>,
}
impl FontData {
    /// A font from `bytes`: 1 byte to 64 MiB of TrueType or OpenType data.
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Result<Self, &'static str> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 {
            return Err("font must contain 1..=64 MiB of data");
        }
        let mut db = cosmic_text::fontdb::Database::new();
        let ids = db.load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(bytes.clone())));
        if ids.is_empty() {
            return Err("invalid or unsupported TrueType/OpenType font");
        }
        let mut families: Vec<_> = db
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();
        families.sort();
        families.dedup();
        Ok(Self {
            bytes,
            families: families.into(),
        })
    }
    /// The family names it holds, sorted.
    pub fn families(&self) -> &[String] {
        &self.families
    }
    /// Its size in bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
    /// Install before shaping text; callers replacing a live database must also
    /// invalidate retained text shapes and their layout caches.
    pub fn install(&self, fonts: &mut FontSystem) {
        fonts
            .db_mut()
            .load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(
                self.bytes.clone(),
            )));
    }
}
pub(crate) fn attrs(font: &zgui::text_layout::FontStyle, size: f32) -> Attrs<'_> {
    use zgui::text_layout::FontFamily;
    let family = match &font.family {
        FontFamily::SansSerif => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    };
    let mut features = cosmic_text::FontFeatures::new();
    for (tag, value) in font.features.settings() {
        features.set(cosmic_text::FeatureTag::new(tag), *value);
    }
    let attrs = Attrs::new()
        .font_features(features)
        .family(family)
        // Variable fonts with an optical size axis (Inter's `opsz`) take the
        // font size, as browsers do (`font-optical-sizing: auto`): large
        // titles get their display cut, tighter and narrower.
        .optical(size.round().clamp(1., 1000.) as u16)
        .masked(font.masked)
        .weight(cosmic_text::Weight(font.weight.clamp(1, 1000)))
        .style(if font.italic {
            cosmic_text::Style::Italic
        } else {
            cosmic_text::Style::Normal
        });
    let spacing = font.letter_spacing.pixels();
    if spacing == 0. {
        attrs
    } else {
        // cosmic-text 0.14 accepts tracking in EM, while our public styles use
        // logical pixels. Use the same effective size as Buffer's metrics.
        attrs.letter_spacing(spacing / size.max(1.))
    }
}
pub(crate) fn alignment(font: &zgui::text_layout::FontStyle) -> Option<cosmic_text::Align> {
    Some(match font.align {
        zgui::text_layout::TextAlign::Start => return None,
        zgui::text_layout::TextAlign::Left => cosmic_text::Align::Left,
        zgui::text_layout::TextAlign::Center => cosmic_text::Align::Center,
        zgui::text_layout::TextAlign::Right => cosmic_text::Align::Right,
    })
}
fn family(family: &zgui::text_layout::FontFamily) -> Family<'_> {
    use zgui::text_layout::FontFamily;
    match family {
        FontFamily::SansSerif => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    }
}
/// Select explicit fallback families per grapheme without splitting combining
/// sequences. Adjacent matching attributes are coalesced before shaping, so
/// ligatures and contextual scripts retain continuous shaping.
fn fallback_spans<'a>(
    fonts: &mut FontSystem,
    text: &'a str,
    font: &'a zgui::text_layout::FontStyle,
    base: Attrs<'a>,
) -> Vec<(&'a str, Attrs<'a>)> {
    if font.fallbacks.is_empty() {
        return vec![(text, base)];
    }
    let families: Vec<_> = std::iter::once(&font.family)
        .chain(font.fallbacks.iter())
        .collect();
    let faces: Vec<_> = families
        .iter()
        .map(|f| {
            let id = fonts.db().query(&cosmic_text::fontdb::Query {
                families: &[family(f)],
                weight: cosmic_text::Weight(font.weight),
                style: if font.italic {
                    cosmic_text::Style::Italic
                } else {
                    cosmic_text::Style::Normal
                },
                ..Default::default()
            });
            id.and_then(|id| fonts.get_font(id, cosmic_text::Weight(font.weight)))
        })
        .collect();
    let mut ranges: Vec<(std::ops::Range<usize>, usize)> = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        let selected = faces
            .iter()
            .position(|face| {
                face.as_ref().is_some_and(|face| {
                    let map = face.as_swash().charmap();
                    grapheme.chars().all(|c| {
                        c.is_control()
                            || matches!(c, '\u{200c}' | '\u{200d}' | '\u{fe00}'..='\u{fe0f}')
                            || map.map(c) != 0
                    })
                })
            })
            .unwrap_or(0);
        if let Some((range, previous)) = ranges.last_mut()
            && *previous == selected
        {
            range.end = start + grapheme.len();
        } else {
            ranges.push((start..start + grapheme.len(), selected));
        }
    }
    ranges
        .into_iter()
        .map(|(range, index)| (&text[range], base.clone().family(family(families[index]))))
        .collect()
}
/// Whether the buffer's last line ends in a line break, leaving no row for a
/// caret after it.
pub(crate) fn ends_open(buffer: &Buffer) -> bool {
    buffer
        .lines
        .last()
        .is_some_and(|line| line.ending() != cosmic_text::LineEnding::None)
}
/// Shared plain-text population: explicit fallback ordering is identical for
/// renderer, measurement, caret layout and the software rasterizer.
pub fn set_buffer_text(
    fonts: &mut FontSystem,
    buffer: &mut Buffer,
    text: &str,
    font: &zgui::text_layout::FontStyle,
    size: f32,
) {
    let base = attrs(font, size);
    if font.fallbacks.is_empty() {
        buffer.set_text(text, &base, Shaping::Advanced, alignment(font));
    } else {
        let spans = fallback_spans(fonts, text, font, base.clone());
        buffer.set_rich_text(spans, &base, Shaping::Advanced, alignment(font));
    }
}
/// A caret position in shaped text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextCursor {
    /// The UTF-8 byte offset.
    pub byte_index: usize,
    /// Which side of the offset the caret sits on.
    pub affinity: Affinity,
}
/// Owns a shaped text buffer. Reuse it until text/font/width changes.
/// Coordinates are logical pixels relative to the text node origin.
/// Ligature interiors divide the advance among graphemes, matching cosmic-text's
/// editor convention. This does not read font-specific GDEF ligature caret tables.
pub struct ShapedText {
    buffer: Buffer,
    text: Arc<str>,
    line_offsets: Vec<usize>,
    line_height: f32,
}
impl ShapedText {
    /// `text` shaped in the default font at `font_size`, wrapped at `width`
    /// if given.
    pub fn new(
        fonts: &mut FontSystem,
        text: impl Into<Arc<str>>,
        font_size: f32,
        width: Option<f32>,
    ) -> Self {
        Self::with_font(fonts, text, font_size, width, &Default::default())
    }
    /// `text` shaped in `font` at `font_size`, wrapped at `width` if given.
    pub fn with_font(
        fonts: &mut FontSystem,
        text: impl Into<Arc<str>>,
        font_size: f32,
        width: Option<f32>,
        font: &zgui::text_layout::FontStyle,
    ) -> Self {
        let text = text.into();
        let size = font_size.max(1.);
        let line_height = font.line_height.resolve(size);
        let mut buffer = Buffer::new_empty(Metrics::new(size, line_height));
        buffer.set_size(wrap_width(width), None);
        set_buffer_text(fonts, &mut buffer, &text, font, size);
        // Editors need a visual row for the caret after a final line ending;
        // add it unless cosmic-text already ended with that empty line.
        if text.ends_with(['\r', '\n']) && ends_open(&buffer) {
            buffer.lines.push(cosmic_text::BufferLine::new(
                "",
                cosmic_text::LineEnding::None,
                cosmic_text::AttrsList::new(&attrs(font, size)),
                Shaping::Advanced,
            ));
            buffer.lines.last_mut().unwrap().set_align(alignment(font));
        }
        buffer.shape_until_scroll(fonts, false);
        let mut offset = 0;
        let line_offsets = buffer
            .lines
            .iter()
            .map(|line| {
                let start = offset;
                offset += line.text().len() + line.ending().as_str().len();
                start
            })
            .collect();
        Self {
            buffer,
            text,
            line_offsets,
            line_height,
        }
    }
    /// Shape all styled spans together, preserving paragraph bidi and wrapping.
    pub fn with_runs(
        fonts: &mut FontSystem,
        rich: &zgui::rich_text::RichText,
        width: Option<f32>,
    ) -> Self {
        let mut buffer = rich_buffer(fonts, rich, width, None);
        let line_height = rich
            .runs()
            .first()
            .map_or(1., |run| run.font.line_height.resolve(run.font_size));
        if rich.options() == Default::default()
            && rich.text().ends_with(['\r', '\n'])
            && ends_open(&buffer)
        {
            let font = rich.runs().last().unwrap();
            buffer.lines.push(cosmic_text::BufferLine::new(
                "",
                cosmic_text::LineEnding::None,
                cosmic_text::AttrsList::new(&attrs(&font.font, font.font_size)),
                Shaping::Advanced,
            ));
            buffer
                .lines
                .last_mut()
                .unwrap()
                .set_align(alignment(&font.font));
            buffer.shape_until_scroll(fonts, false);
        }
        let mut offset = 0;
        let line_offsets = buffer
            .lines
            .iter()
            .map(|line| {
                let start = offset;
                offset += line.text().len() + line.ending().as_str().len();
                start
            })
            .collect();
        Self {
            buffer,
            text: rich.text_arc(),
            line_offsets,
            line_height,
        }
    }
    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Its width and height.
    pub fn size(&self) -> (f32, f32) {
        self.buffer
            .layout_runs()
            .fold((0_f32, 0_f32), |(w, h), run| {
                (w.max(run.line_w), h.max(run.line_top + run.line_height))
            })
    }
    /// The caret position at `x`, `y`, if the text has one there.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<TextCursor> {
        let cursor = self.buffer.hit(x, y)?;
        Some(TextCursor {
            byte_index: self.line_offsets.get(cursor.line)? + cursor.index,
            affinity: cursor.affinity,
        })
    }
    /// The caret's rectangle at `cursor`, if it is a char boundary in the
    /// text.
    pub fn caret(&self, cursor: TextCursor) -> Option<Rect> {
        if cursor.byte_index > self.text.len() || !self.text.is_char_boundary(cursor.byte_index) {
            return None;
        }
        let mut fallback = None;
        for run in self.buffer.layout_runs() {
            let offset = self.line_offsets[run.line_i];
            if run.glyphs.is_empty() && cursor.byte_index == offset {
                return Some(Rect::new(0., run.line_top, 1., run.line_height));
            }
            for glyph in run.glyphs {
                let start = offset + glyph.start;
                let end = offset + glyph.end;
                if cursor.byte_index < start || cursor.byte_index > end {
                    continue;
                }
                let cluster = &self.text[start..end];
                let count = cluster.graphemes(true).count().max(1);
                let before = cluster
                    .grapheme_indices(true)
                    .filter(|(i, _)| start + i < cursor.byte_index)
                    .count();
                let fraction = before as f32 / count as f32;
                let x = if glyph.level.is_rtl() {
                    glyph.x + glyph.w * (1. - fraction)
                } else {
                    glyph.x + glyph.w * fraction
                };
                let rect = Rect::new(x, run.line_top, 1., run.line_height);
                if (cursor.byte_index == start && cursor.affinity == Affinity::After)
                    || (cursor.byte_index == end && cursor.affinity == Affinity::Before)
                    || (cursor.byte_index > start && cursor.byte_index < end)
                {
                    return Some(rect);
                }
                fallback = Some(rect);
            }
        }
        fallback
    }
    /// Returns visual rectangles, including disjoint spans for mixed-direction text.
    pub fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        let start = range.start.min(range.end);
        let end = range.start.max(range.end).min(self.text.len());
        if start >= end {
            return Vec::new();
        }
        let mut rectangles: Vec<Rect> = Vec::new();
        for run in self.buffer.layout_runs() {
            let offset = self.line_offsets[run.line_i];
            let mut spans = Vec::new();
            for glyph in run.glyphs {
                if glyph.metadata == usize::MAX {
                    continue;
                }
                let cluster_start = offset + glyph.start;
                let cluster_end = offset + glyph.end;
                if cluster_start >= end || cluster_end <= start {
                    continue;
                }
                let cluster = &self.text[cluster_start..cluster_end];
                let count = cluster.graphemes(true).count().max(1);
                for (index, (local, grapheme)) in cluster.grapheme_indices(true).enumerate() {
                    if cluster_start + local >= end
                        || cluster_start + local + grapheme.len() <= start
                    {
                        continue;
                    }
                    let width = glyph.w / count as f32;
                    let x = if glyph.level.is_rtl() {
                        glyph.x + glyph.w - (index + 1) as f32 * width
                    } else {
                        glyph.x + index as f32 * width
                    };
                    // Strong negative tracking can reverse a glyph's advance.
                    // Selection paint rectangles still need ordered edges.
                    spans.push(Rect::new(
                        x.min(x + width),
                        run.line_top,
                        width.abs(),
                        run.line_height,
                    ));
                }
            }
            spans.sort_by(|a, b| a.x.total_cmp(&b.x));
            for span in spans {
                if let Some(last) = rectangles.last_mut()
                    && last.y == span.y
                    && last.x + last.width >= span.x - 0.01
                {
                    *last = last.union(span);
                    continue;
                }
                rectangles.push(span);
            }
        }
        rectangles
    }
    /// Its line pitch.
    pub fn line_height(&self) -> f32 {
        self.line_height
    }
}

impl zgui::text_layout::TextLayout for ShapedText {
    fn cache_weight(&self) -> Option<usize> {
        // Charge capacities of all publicly inspectable owned shape/layout
        // vectors, including spare capacity. cosmic-text hides String capacity
        // and attribute allocation details, so private strings receive twice
        // their length plus a per-line allowance. This deliberately conservative
        // accounting estimate is not an allocator/RSS upper bound. FontSystem
        // and its shared font/shape scratch caches are not owned by this layout.
        fn vector_bytes<T>(values: &Vec<T>) -> usize {
            values.capacity().saturating_mul(std::mem::size_of::<T>())
        }
        let mut bytes = std::mem::size_of::<Self>()
            .saturating_add(self.text.len())
            .saturating_add(2 * std::mem::size_of::<usize>())
            .saturating_add(vector_bytes(&self.line_offsets))
            .saturating_add(vector_bytes(&self.buffer.lines));
        for line in &self.buffer.lines {
            let family_bytes = match line.attrs_list().defaults().family {
                Family::Name(name) => name.len(),
                _ => 0,
            };
            bytes = bytes.saturating_add(
                line.text()
                    .len()
                    .saturating_add(family_bytes)
                    .saturating_mul(2)
                    .saturating_add(128),
            );
            if let Some(shape) = line.shape_opt() {
                bytes = bytes.saturating_add(vector_bytes(&shape.spans));
                for span in &shape.spans {
                    bytes = bytes.saturating_add(vector_bytes(&span.words));
                    for word in &span.words {
                        bytes = bytes.saturating_add(vector_bytes(&word.glyphs));
                    }
                }
            }
            if let Some(layout) = line.layout_opt() {
                bytes = bytes.saturating_add(vector_bytes(layout));
                for row in layout {
                    bytes = bytes.saturating_add(vector_bytes(&row.glyphs));
                }
            }
        }
        Some(bytes)
    }
    fn size(&self) -> (f32, f32) {
        ShapedText::size(self)
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        ShapedText::hit_test(self, x, y).map_or(0, |cursor| cursor.byte_index)
    }
    fn hit_position(&self, x: f32, y: f32) -> zgui::text_layout::TextPosition {
        use zgui::text_layout::{TextAffinity, TextPosition};
        ShapedText::hit_test(self, x, y).map_or_else(TextPosition::default, |cursor| TextPosition {
            byte_offset: cursor.byte_index,
            affinity: match cursor.affinity {
                Affinity::Before => TextAffinity::Before,
                Affinity::After => TextAffinity::After,
            },
        })
    }
    fn caret(&self, byte_offset: usize) -> Rect {
        self.caret_position(zgui::text_layout::TextPosition {
            byte_offset,
            affinity: zgui::text_layout::TextAffinity::After,
        })
    }
    fn caret_position(&self, position: zgui::text_layout::TextPosition) -> Rect {
        use zgui::text_layout::TextAffinity;
        let offset = if position.byte_offset >= self.text.len() {
            self.text.len()
        } else {
            self.text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|&i| i <= position.byte_offset)
                .last()
                .unwrap_or(0)
        };
        let affinity = match position.affinity {
            TextAffinity::Before => Affinity::Before,
            TextAffinity::After => Affinity::After,
        };
        ShapedText::caret(
            self,
            TextCursor {
                byte_index: offset,
                affinity,
            },
        )
        .or_else(|| {
            ShapedText::caret(
                self,
                TextCursor {
                    byte_index: offset,
                    affinity: match affinity {
                        Affinity::Before => Affinity::After,
                        Affinity::After => Affinity::Before,
                    },
                },
            )
        })
        .unwrap_or(Rect::new(0., 0., 1., self.line_height))
    }
    fn visual_line_range(
        &self,
        position: zgui::text_layout::TextPosition,
    ) -> (
        zgui::text_layout::TextPosition,
        zgui::text_layout::TextPosition,
    ) {
        use zgui::text_layout::{TextAffinity, TextPosition};
        let caret = self.caret_position(position);
        let Some(run) = self
            .buffer
            .layout_runs()
            .find(|run| run.line_top == caret.y)
        else {
            return (position, position);
        };
        let offset = self.line_offsets[run.line_i];
        let line_len = self.buffer.lines[run.line_i].text().len();
        let start = run
            .glyphs
            .iter()
            .map(|glyph| glyph.start)
            .min()
            .unwrap_or(0)
            .min(line_len);
        let end = run
            .glyphs
            .iter()
            .map(|glyph| glyph.end)
            .max()
            .unwrap_or(0)
            .min(line_len);
        (
            TextPosition {
                byte_offset: offset + start,
                affinity: TextAffinity::After,
            },
            TextPosition {
                byte_offset: offset + end,
                affinity: TextAffinity::Before,
            },
        )
    }
    fn selection(&self, range: Range<usize>) -> Vec<Rect> {
        ShapedText::selection(self, range)
    }
}

/// Lazy software shaping/rasterization for the reference backend's styled fonts.
/// Glyph images are not retained in Swash's unbounded internal image map.
pub struct CpuTextRaster {
    fonts: FontSystem,
    swash: cosmic_text::SwashCache,
}
impl Default for CpuTextRaster {
    fn default() -> Self {
        Self {
            fonts: crate::font_cache::system_fonts(),
            swash: cosmic_text::SwashCache::new(),
        }
    }
}
/// Rich text rasterized: each glyph's position, colour and image, then
/// the decorations (backgrounds, underlines, strikethroughs).
pub type RichRaster = (
    Vec<(i32, i32, zgui::scene::Color, cosmic_text::SwashImage)>,
    Vec<RichDecoration>,
);
impl CpuTextRaster {
    /// Make `font` available to the text it rasterizes.
    pub fn register_font(&mut self, font: &FontData) {
        font.install(&mut self.fonts);
    }
    /// Rasterize `rich` laid out in `width` by `height`.
    pub fn rasterize_rich(
        &mut self,
        rich: &zgui::rich_text::RichText,
        width: f32,
        height: f32,
    ) -> RichRaster {
        let buffer = rich_buffer(&mut self.fonts, rich, Some(width), Some(height));
        let starts = line_starts(&buffer);
        let mut images = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let p = glyph.physical((0., 0.), 1.);
                if let Some(image) = self.swash.get_image_uncached(&mut self.fonts, p.cache_key) {
                    images.push((
                        p.x,
                        p.y + run.line_y as i32,
                        rich.runs()[run_at(rich, starts[run.line_i] + glyph.start)].color,
                        image,
                    ));
                }
            }
        }
        (images, rich_decorations(&buffer, rich))
    }
    /// Rasterize `text` at `size` in `font`, laid out in `width` by `height`:
    /// each glyph's position and image.
    pub fn rasterize(
        &mut self,
        text: &str,
        size: f32,
        width: f32,
        height: f32,
        font: &zgui::text_layout::FontStyle,
    ) -> Vec<(i32, i32, cosmic_text::SwashImage)> {
        let size = size.max(1.);
        let mut buffer = Buffer::new_empty(Metrics::new(size, font.line_height.resolve(size)));
        buffer.set_size(wrap_width(Some(width)), Some(height));
        set_buffer_text(&mut self.fonts, &mut buffer, text, font, size);
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut images = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((0., 0.), 1.);
                if let Some(image) = self
                    .swash
                    .get_image_uncached(&mut self.fonts, physical.cache_key)
                {
                    images.push((physical.x, physical.y + run.line_y as i32, image));
                }
            }
        }
        images
    }
}

#[cfg(test)]
mod visual_line_tests {
    use super::*;
    use zgui::text_layout::{TextAffinity, TextLayout, TextPosition};

    #[test]
    fn logical_visual_row_range_includes_mixed_bidi_clusters_beyond_physical_edges() {
        // cosmic-text 0.14 put the physical edges of these rows inside the
        // logical range; 0.19 no longer does, but the range must hold either way.
        let mut fonts = FontSystem::new();
        for content in ["abc אבג", "אבג abc", "abc אבג xyz דהו", "אבג abc דהו xyz"]
        {
            for width in [None, Some(74.)] {
                let layout = ShapedText::new(&mut fonts, content, 18., width);
                for run in layout.buffer.layout_runs() {
                    let hit = layout.hit_position(run.line_w * 0.5, run.line_top + 1.);
                    let (start, end) = layout.visual_line_range(hit);
                    let offset = layout.line_offsets[run.line_i];
                    let expected_start = offset + run.glyphs.iter().map(|g| g.start).min().unwrap();
                    let expected_end = offset + run.glyphs.iter().map(|g| g.end).max().unwrap();
                    assert_eq!(
                        (start.byte_offset, end.byte_offset),
                        (expected_start, expected_end)
                    );
                    assert_eq!(start.affinity, TextAffinity::After);
                    assert_eq!(end.affinity, TextAffinity::Before);
                    for glyph in run.glyphs {
                        assert!(
                            start.byte_offset <= offset + glyph.start
                                && end.byte_offset >= offset + glyph.end
                        );
                    }
                    assert_eq!(layout.caret_position(start).y, run.line_top);
                    assert_eq!(layout.caret_position(end).y, run.line_top);
                }
            }
        }
    }

    #[test]
    fn visual_row_ranges_exclude_hard_endings_and_preserve_empty_rows_and_wrap_affinity() {
        let mut fonts = FontSystem::new();
        let content = "abc אבג\r\n\n";
        let layout = ShapedText::new(&mut fonts, content, 18., None);
        let first_end = content.find('\r').unwrap();
        for (offset, expected) in [
            (0, (0, first_end)),
            (first_end + 2, (first_end + 2, first_end + 2)),
            (content.len(), (content.len(), content.len())),
        ] {
            let range = layout.visual_line_range(TextPosition {
                byte_offset: offset,
                affinity: TextAffinity::After,
            });
            assert_eq!((range.0.byte_offset, range.1.byte_offset), expected);
        }
        let empty = ShapedText::new(&mut fonts, "", 18., None);
        let range = empty.visual_line_range(TextPosition::default());
        assert_eq!((range.0.byte_offset, range.1.byte_offset), (0, 0));
        let wrapped = ShapedText::new(&mut fonts, "abcdefghijklmnop", 18., Some(48.));
        let first = wrapped.visual_line_range(TextPosition::default());
        let second = wrapped.visual_line_range(TextPosition {
            affinity: TextAffinity::After,
            ..first.1
        });
        assert_eq!(first.1.byte_offset, second.0.byte_offset);
        assert!(wrapped.caret_position(first.1).y < wrapped.caret_position(second.0).y);
        assert_eq!(wrapped.visual_line_range(first.1), first);
    }

    #[test]
    fn wrapped_unicode_hit_caret_and_selection_share_visual_lines() {
        let mut fonts = FontSystem::new();
        let content = "aébcdefghijklmnop";
        let layout = ShapedText::new(&mut fonts, content, 18., Some(48.));
        let end = layout.visual_line_edge(TextPosition::default(), true);
        assert!(end.byte_offset > 0 && end.byte_offset < content.len());
        assert_eq!(end.affinity, TextAffinity::Before);
        let upstream = layout.caret_position(end);
        let next = TextPosition {
            affinity: TextAffinity::After,
            ..end
        };
        let downstream = layout.caret_position(next);
        assert_eq!(upstream.y, 0.);
        assert!(downstream.y > upstream.y);
        assert_eq!(
            layout.visual_line_edge(next, false).byte_offset,
            end.byte_offset
        );
        assert_eq!(layout.hit_position(f32::MAX, upstream.y + 1.), end);
        assert_eq!(layout.hit_position(-1., downstream.y + 1.), next);
        let selection = TextLayout::selection(&layout, 0..content.len());
        assert!(selection.len() > 1);
        assert!(selection.iter().all(|r| r.width > 0. && r.height > 0.));
        for rect in &selection {
            for x in [rect.x, rect.x + rect.width * 0.5, rect.x + rect.width] {
                let position = layout.hit_position(x, rect.y + 1.);
                assert!(
                    content
                        .grapheme_indices(true)
                        .any(|(i, _)| i == position.byte_offset)
                        || position.byte_offset == content.len()
                );
                assert!((layout.caret_position(position).y - rect.y).abs() < 0.1);
            }
        }
    }

    #[test]
    fn combining_interior_snaps_to_grapheme_and_crlf_edges_stay_on_line() {
        let mut fonts = FontSystem::new();
        let layout = ShapedText::new(&mut fonts, "e\u{301}x\r\n\n", 18., Some(200.));
        assert_eq!(TextLayout::caret(&layout, 1), TextLayout::caret(&layout, 0));
        assert_eq!(TextLayout::caret(&layout, 2), TextLayout::caret(&layout, 0));
        let end = layout.visual_line_edge(TextPosition::default(), true);
        assert_eq!(end.byte_offset, 4);
        assert_eq!(layout.caret_position(end).y, 0.);
        let final_line = TextPosition {
            byte_offset: 7,
            affinity: TextAffinity::After,
        };
        assert!(layout.caret_position(final_line).y > layout.line_height());
        assert_eq!(layout.visual_line_edge(final_line, false).byte_offset, 7);
    }

    #[test]
    fn bidi_visual_edges_stay_on_the_selected_wrapped_row() {
        let mut fonts = FontSystem::new();
        for content in ["אבגדהוזחטיכלמנסעפצקרשת", "abc אבג xyz אבג def"]
        {
            let layout = ShapedText::new(&mut fonts, content, 18., Some(70.));
            assert!(layout.size().1 > layout.line_height());
            for run in layout.buffer.layout_runs() {
                let position = layout.hit_position(run.line_w * 0.5, run.line_top + 1.);
                let start = layout.visual_line_edge(position, false);
                let end = layout.visual_line_edge(position, true);
                let left = layout.caret_position(start);
                let right = layout.caret_position(end);
                assert_eq!(left.y, run.line_top, "{content:?} {start:?}");
                assert_eq!(right.y, run.line_top, "{content:?} {end:?}");
                assert!(left.x <= right.x, "{content:?}: {left:?} {right:?}");
                assert!(content.is_char_boundary(start.byte_offset));
                assert!(content.is_char_boundary(end.byte_offset));
            }
        }
    }
}

#[cfg(test)]
mod line_height_tests {
    use super::*;
    use zgui::text_layout::{FontStyle, LineHeight, TextLayout, TextPosition};

    #[test]
    fn explicit_pitch_matches_carets_hit_selection_and_trailing_empty_line() {
        let mut fonts = FontSystem::new();
        for pitch in [8., 24., 40.] {
            let font = FontStyle {
                line_height: LineHeight::px(pitch),
                ..Default::default()
            };
            let layout = ShapedText::with_font(&mut fonts, "abc\ndef\n", 18., None, &font);
            assert_eq!(layout.line_height(), pitch);
            assert_eq!(layout.size().1, pitch * 3.);
            for (offset, row) in [(0, 0.), (4, 1.), (8, 2.)] {
                let caret = TextLayout::caret(&layout, offset);
                assert_eq!(caret.y, row * pitch);
                assert_eq!(caret.height, pitch);
                let hit = layout.hit_position(0., row * pitch + pitch * 0.5);
                assert_eq!(hit.byte_offset, offset);
                assert_eq!(layout.visual_line_edge(hit, false).byte_offset, offset);
            }
            let selection = layout.selection(0..7);
            assert_eq!(selection.len(), 2);
            assert_eq!(selection[0].height, pitch);
            assert_eq!(selection[1].y, pitch);
            assert_eq!(selection[1].height, pitch);
        }
    }

    #[test]
    fn custom_pitch_preserves_wrapping_and_visual_edges() {
        let mut fonts = FontSystem::new();
        let content = "alpha beta gamma delta";
        let normal = ShapedText::new(&mut fonts, content, 18., Some(60.));
        let font = FontStyle {
            line_height: LineHeight::px(37.),
            ..Default::default()
        };
        let spaced = ShapedText::with_font(&mut fonts, content, 18., Some(60.), &font);
        let normal_rows: Vec<_> = normal.buffer.layout_runs().collect();
        let spaced_rows: Vec<_> = spaced.buffer.layout_runs().collect();
        assert!(normal_rows.len() > 1);
        assert_eq!(normal_rows.len(), spaced_rows.len());
        assert_eq!(spaced.size().1, 37. * spaced_rows.len() as f32);
        for (index, (a, b)) in normal_rows.iter().zip(&spaced_rows).enumerate() {
            assert_eq!(a.line_w, b.line_w);
            assert_eq!(b.line_top, 37. * index as f32);
            let hit = spaced.hit_position(0., b.line_top + 1.);
            assert_eq!(spaced.caret_position(hit).y, b.line_top);
            let end = spaced.visual_line_edge(hit, true);
            assert_eq!(spaced.caret_position(end).y, b.line_top);
        }
        assert_eq!(
            normal
                .visual_line_edge(TextPosition::default(), true)
                .byte_offset,
            spaced
                .visual_line_edge(TextPosition::default(), true)
                .byte_offset
        );
    }

    #[test]
    fn software_shaping_uses_the_same_explicit_pitch() {
        let mut raster = CpuTextRaster::default();
        for pitch in [8., 40.] {
            let font = FontStyle {
                line_height: LineHeight::px(pitch),
                ..Default::default()
            };
            let images = raster.rasterize("M\nM", 18., 100., 100., &font);
            assert_eq!(images.len(), 2);
            assert_eq!(images[1].1 - images[0].1, pitch as i32);
            assert_eq!(images[0].2.data, images[1].2.data);
        }
    }

    #[test]
    fn default_measurement_includes_the_same_trailing_caret_row() {
        let mut fonts = FontSystem::new();
        let layout = ShapedText::new(&mut fonts, "text\n", 18., None);
        assert_eq!(
            super::super::measure_text(&mut fonts, "text\n", 18., None),
            layout.size()
        );
        assert_eq!(layout.size().1, layout.line_height() * 2.);
    }
}

#[cfg(test)]
mod cache_weight_tests {
    use super::*;
    use zgui::text_layout::TextLayout;

    #[test]
    fn native_accounting_includes_shape_and_layout_capacities() {
        let mut fonts = FontSystem::new();
        let mut layout = ShapedText::new(&mut fonts, "alpha beta\ngamma\n", 18., Some(45.));
        let weight = layout.cache_weight().unwrap();
        let rows: Vec<_> = layout.buffer.layout_runs().collect();
        let visible_glyph_bytes: usize = rows
            .iter()
            .map(|run| std::mem::size_of_val(run.glyphs))
            .sum();
        assert!(
            weight > std::mem::size_of::<ShapedText>() + layout.text.len() + visible_glyph_bytes
        );
        let prior_capacity = layout.line_offsets.capacity();
        layout.line_offsets.reserve(2048);
        assert_eq!(
            layout.cache_weight().unwrap() - weight,
            (layout.line_offsets.capacity() - prior_capacity) * std::mem::size_of::<usize>()
        );
        let longer = ShapedText::new(&mut fonts, "alpha beta gamma\n".repeat(100), 18., Some(45.));
        assert!(longer.cache_weight().unwrap() > weight * 10);
    }
}

#[cfg(test)]
mod letter_spacing_tests {
    use super::*;
    use zgui::text_layout::{FontStyle, LetterSpacing, TextLayout};

    fn font(spacing: f32) -> FontStyle {
        FontStyle {
            letter_spacing: LetterSpacing::px(spacing),
            ..Default::default()
        }
    }

    #[test]
    fn pixel_tracking_changes_glyph_advances_not_by_font_size_multiple() {
        let mut fonts = FontSystem::new();
        for size in [12., 24.] {
            let normal = ShapedText::with_font(&mut fonts, "MMMM", size, None, &font(0.));
            for spacing in [-1., 3.] {
                let tracked = ShapedText::with_font(&mut fonts, "MMMM", size, None, &font(spacing));
                assert!((tracked.size().0 - normal.size().0 - spacing * 4.).abs() < 0.01);
                assert!(
                    (TextLayout::caret(&tracked, 4).x
                        - TextLayout::caret(&normal, 4).x
                        - spacing * 4.)
                        .abs()
                        < 0.01
                );
            }
            assert!(attrs(&font(0.), size).letter_spacing_opt.is_none());
            assert_eq!(
                attrs(&font(3.), size).letter_spacing_opt.unwrap().0,
                3. / size
            );
        }
    }

    #[test]
    fn unicode_and_negative_tracking_keep_finite_geometry_and_grapheme_hits() {
        let mut fonts = FontSystem::new();
        for content in ["e\u{301}x", "abc אבג xyz", "אבגדה", "MMMM"] {
            for spacing in [-100., -1., 0., 3.] {
                let layout =
                    ShapedText::with_font(&mut fonts, content, 18., Some(200.), &font(spacing));
                let size = layout.size();
                assert!(size.0.is_finite() && size.1.is_finite());
                let boundaries: Vec<_> = content
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .chain([content.len()])
                    .collect();
                for offset in &boundaries {
                    let caret = TextLayout::caret(&layout, *offset);
                    assert!(caret.x.is_finite() && caret.y.is_finite() && caret.height.is_finite());
                    let hit = layout.hit_position(caret.x, caret.y + caret.height * 0.5);
                    assert!(
                        boundaries.contains(&hit.byte_offset),
                        "{content:?} spacing={spacing} hit={hit:?}"
                    );
                }
                for span in layout.selection(0..content.len()) {
                    assert!(span.x.is_finite() && span.width.is_finite() && span.width >= 0.);
                }
            }
        }
    }

    #[test]
    fn cpu_raster_uses_pixel_tracking_for_each_shaped_glyph() {
        let mut raster = CpuTextRaster::default();
        let normal = raster.rasterize("MMMM", 18., 200., 80., &font(0.));
        let spaced = raster.rasterize("MMMM", 18., 200., 80., &font(3.));
        assert_eq!(normal.len(), 4);
        assert_eq!(spaced.len(), normal.len());
        for (index, (a, b)) in normal.iter().zip(spaced.iter()).enumerate() {
            assert_eq!(b.0 - a.0, index as i32 * 3);
            assert_eq!(b.1, a.1);
        }
    }

    #[test]
    fn extreme_tracking_remains_finite() {
        let mut fonts = FontSystem::new();
        for spacing in [-f32::MAX, f32::MAX] {
            let layout = ShapedText::with_font(&mut fonts, "MMMM", 18., None, &font(spacing));
            assert!(
                layout.size().0.is_finite(),
                "spacing={spacing} size={:?}",
                layout.size()
            );
            assert!(TextLayout::caret(&layout, 4).x.is_finite());
        }
    }
}

/// Display-only shaping. Truncation keeps an original UTF-8 prefix, then shapes
/// the ellipsis using the preceding run; semantic text remains the full source.
pub fn rich_buffer(
    fonts: &mut FontSystem,
    rich: &zgui::rich_text::RichText,
    width: Option<f32>,
    height: Option<f32>,
) -> Buffer {
    use zgui::text_layout::TextOverflow;
    let options = rich.options();
    let Some(limit) = options
        .line_clamp
        .map(|n| n.get() as usize)
        .or((options.overflow == TextOverflow::Ellipsis).then_some(1))
    else {
        return rich_buffer_raw(fonts, rich, width, height, false);
    };
    let original = rich_buffer_raw(fonts, rich, width, None, false);
    let rows: Vec<_> = original
        .layout_runs()
        .map(|r| {
            (
                r.line_i,
                r.glyphs.iter().map(|g| g.start).min().unwrap_or(0),
                r.glyphs.iter().map(|g| g.end).max().unwrap_or(0),
                r.line_w,
            )
        })
        .collect();
    let overflow =
        rows.len() > limit || width.is_some_and(|w| rows.iter().take(limit).any(|r| r.3 > w));
    if !overflow {
        return original;
    }
    let row = rows
        .get(limit.saturating_sub(1))
        .or(rows.last())
        .copied()
        .unwrap_or((0, 0, 0, 0.));
    let offset: usize = original
        .lines
        .iter()
        .take(row.0)
        .map(|line| line.text().len() + line.ending().as_str().len())
        .sum();
    let start = (offset + row.1).min(rich.text().len());
    let raw_end = (offset + row.2).min(rich.text().len());
    let end = rich
        .text()
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(rich.text().len()))
        .take_while(|i| *i <= raw_end)
        .last()
        .unwrap_or(0);
    let make = |fonts: &mut FontSystem, cut: usize, ellipsis: bool| {
        let mut content = rich.text()[..cut].to_owned();
        let mut runs: Vec<_> = rich
            .runs()
            .iter()
            .take_while(|r| r.range.start < cut)
            .cloned()
            .map(|mut r| {
                r.range.end = r.range.end.min(cut);
                r
            })
            .collect();
        if ellipsis {
            let mut run = runs
                .last()
                .or(rich.runs().first())
                .cloned()
                .unwrap_or_default();
            content.push('…');
            run.range = cut..content.len();
            run.background = None;
            run.underline = None;
            run.strikethrough = None;
            runs.push(run);
        }
        if content.is_empty()
            && let Some(run) = rich.runs().first()
        {
            let mut buffer = Buffer::new_empty(Metrics::new(
                run.font_size,
                run.font.line_height.resolve(run.font_size),
            ));
            buffer.set_size(wrap_width(width), None);
            set_buffer_text(fonts, &mut buffer, "", &run.font, run.font_size);
            buffer.shape_until_scroll(fonts, false);
            return buffer;
        }
        let truncated = zgui::rich_text::RichText::new(content, runs).unwrap();
        rich_buffer_raw(fonts, &truncated, width, None, ellipsis)
    };
    if options.overflow == TextOverflow::Clip {
        return make(fonts, end, false);
    }
    let mut boundaries: Vec<_> = rich
        .text()
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|i| *i < end)
        .filter(|i| *i >= start)
        .collect();
    boundaries.push(end);
    let mut low = 0;
    let mut high = boundaries.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let probe = make(fonts, boundaries[mid], true);
        let fits = probe.layout_runs().count() <= limit
            && width.is_none_or(|w| probe.layout_runs().all(|r| r.line_w <= w + 0.001));
        if fits {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    make(fonts, boundaries[low.saturating_sub(1)], true)
}
/// Shape plain display text through the same overflow implementation as spans.
pub fn display_buffer(
    fonts: &mut FontSystem,
    text: &str,
    font: &zgui::text_layout::FontStyle,
    size: f32,
    width: Option<f32>,
    height: Option<f32>,
    options: zgui::text_layout::TextOptions,
) -> Buffer {
    let runs = if text.is_empty() {
        Vec::new()
    } else {
        vec![zgui::rich_text::TextRun {
            range: 0..text.len(),
            font: font.clone(),
            font_size: size,
            ..Default::default()
        }]
    };
    let rich = zgui::rich_text::RichText::new(text, runs)
        .unwrap()
        .with_options(options);
    rich_buffer(fonts, &rich, width, height)
}
/// Shared native mixed-run shaping used for measurement and rasterization.
fn rich_buffer_raw(
    fonts: &mut FontSystem,
    rich: &zgui::rich_text::RichText,
    width: Option<f32>,
    height: Option<f32>,
    ellipsis: bool,
) -> Buffer {
    let mut buffer = rich_buffer_unshaped(fonts, rich, ellipsis, Vec::new());
    buffer.set_size(wrap_width(width), height);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

/// `rich`'s lines and span attributes, not yet shaped or laid out.
/// Plain text's lines as `ShapedText::with_font` builds them, not yet shaped.
/// `recycled`: lines whose allocations (text, shapes, layout) the buffer's
/// lines reuse, as a buffer reuses its own when its text is set again.
pub(crate) fn plain_buffer_unshaped(
    fonts: &mut FontSystem,
    text: &str,
    font_size: f32,
    font: &zgui::text_layout::FontStyle,
    recycled: Vec<cosmic_text::BufferLine>,
) -> Buffer {
    let size = font_size.max(1.);
    let mut buffer = Buffer::new_empty(Metrics::new(size, font.line_height.resolve(size)));
    buffer.lines = recycled;
    set_buffer_text(fonts, &mut buffer, text, font, size);
    buffer
}
pub(crate) fn rich_buffer_unshaped(
    fonts: &mut FontSystem,
    rich: &zgui::rich_text::RichText,
    ellipsis: bool,
    recycled: Vec<cosmic_text::BufferLine>,
) -> Buffer {
    let base = rich.runs().first();
    let size = base.map_or(1., |run| run.font_size.max(1.));
    let line_height = base.map_or(1., |run| run.font.line_height.resolve(size));
    let mut buffer = Buffer::new_empty(Metrics::new(size, line_height));
    buffer.lines = recycled;
    let default_font = zgui::text_layout::FontStyle::default();
    let default_attrs = attrs(base.map_or(&default_font, |run| &run.font), size);
    let mut spans = Vec::new();
    // Shape only what changes shapes. cosmic-text's shaping cache keys on
    // every attribute of every span in a word, so colours (a streaming
    // fade-in animates them) or run boundaries between runs that differ only
    // in paint would reshape text each frame. Adjacent runs with the same
    // font and size share one span; glyphs find their run by byte offset
    // (`run_at`). The ellipsis run stays apart, marked by `usize::MAX`.
    let runs = rich.runs();
    let ellipsis_run = ellipsis.then(|| runs.len().saturating_sub(1));
    let mut index = 0;
    while index < runs.len() {
        let run = &runs[index];
        let mut end = index + 1;
        while end < runs.len()
            && Some(end) != ellipsis_run
            && Some(index) != ellipsis_run
            && runs[end].font == run.font
            && runs[end].font_size == run.font_size
            && runs[end].range.start == runs[end - 1].range.end
            // A padded run's room goes around it alone.
            && run.padding[0] == 0.
            && runs[end].padding[0] == 0.
        {
            end += 1;
        }
        let mut attributes = attrs(&run.font, run.font_size.max(1.));
        if run.padding[0] > 0. {
            attributes = attributes.padding(run.padding[0] / run.font_size.max(1.));
        }
        let attributes = attributes
            .metrics(Metrics::new(
                run.font_size.max(1.),
                run.font.line_height.resolve(run.font_size.max(1.)),
            ))
            .metadata(if Some(index) == ellipsis_run {
                usize::MAX
            } else {
                index
            });
        spans.extend(fallback_spans(
            fonts,
            &rich.text()[run.range.start..runs[end - 1].range.end],
            &run.font,
            attributes,
        ));
        index = end;
    }
    buffer.set_rich_text(
        spans,
        &default_attrs,
        Shaping::Advanced,
        alignment(base.map_or(&default_font, |run| &run.font)),
    );
    buffer
}

#[cfg(test)]
mod rich_tests {
    use super::*;
    use zgui::{
        rich_text::{RichText, TextRun},
        scene::Color,
        text_layout::FontStyle,
    };
    // Use the licensed repository font, with two separately registered family
    // aliases, so selection and feature tests never depend on host font installs.
    fn bundled_test_fonts() -> FontSystem {
        let mut db = cosmic_text::fontdb::Database::new();
        db.load_font_data(include_bytes!("../../../assets/DejaVuSans.ttf").to_vec());
        let face = db.faces().next().expect("bundled font").clone();
        for family in ["zgui test first", "zgui test second"] {
            let mut alias = face.clone();
            alias.families = vec![(family.into(), face.families[0].1)];
            db.push_face_info(alias);
        }
        FontSystem::new_with_locale_and_db("en-US".into(), db)
    }
    #[test]
    fn display_line_clamp_shapes_ellipsis_and_excludes_hidden_link_geometry() {
        use zgui::text_layout::{TextOptions, TextOverflow};
        let mut fonts = FontSystem::new();
        let text = "alpha βeta 👩‍💻 words continue across several wrapped lines";
        let rich = RichText::new(
            text,
            vec![TextRun {
                range: 0..text.len(),
                font_size: 20.,
                ..Default::default()
            }],
        )
        .unwrap()
        .with_options(TextOptions {
            overflow: TextOverflow::Ellipsis,
            line_clamp: std::num::NonZeroU32::new(2),
        });
        let buffer = rich_buffer(&mut fonts, &rich, Some(130.), None);
        assert_eq!(buffer.layout_runs().count(), 2);
        assert!(
            buffer
                .layout_runs()
                .flat_map(|row| row.glyphs)
                .any(|g| g.metadata == usize::MAX),
            "ellipsis must have actual shaped glyphs"
        );
        assert!(buffer.lines.iter().any(|line| line.text().ends_with('…')));
        assert!(buffer.layout_runs().all(|row| row.line_w <= 130.001));
        let layout = ShapedText::with_runs(&mut fonts, &rich, Some(130.));
        assert_eq!(layout.text(), text, "accessible source remains complete");
        assert!(layout.selection(text.len() - 5..text.len()).is_empty());
        let clipped = rich.clone().with_options(TextOptions {
            overflow: TextOverflow::Clip,
            line_clamp: std::num::NonZeroU32::new(1),
        });
        let clipped = rich_buffer(&mut fonts, &clipped, Some(130.), None);
        assert_eq!(clipped.layout_runs().count(), 1);
        assert!(!clipped.lines.iter().any(|line| line.text().contains('…')));
        let restored = rich.with_options(Default::default());
        assert!(
            rich_buffer(&mut fonts, &restored, Some(130.), None)
                .layout_runs()
                .count()
                > 2
        );
    }
    #[test]
    fn bundled_font_registration_and_alignment_use_real_native_geometry() {
        use zgui::text_layout::{FontFamily, TextAlign, TextLayout};
        assert!(FontData::new(Arc::<[u8]>::from(&b"invalid"[..])).is_err());
        let data = FontData::new(Arc::<[u8]>::from(
            &include_bytes!("../../../assets/DejaVuSans.ttf")[..],
        ))
        .unwrap();
        assert!(data.families().iter().any(|name| name == "DejaVu Sans"));
        let mut fonts = FontSystem::new_with_locale_and_db(
            "en-US".into(),
            cosmic_text::fontdb::Database::new(),
        );
        data.install(&mut fonts);
        let mut font = FontStyle {
            family: FontFamily::from("DejaVu Sans"),
            align: TextAlign::Left,
            ..Default::default()
        };
        let left = ShapedText::with_font(&mut fonts, "abc", 20., Some(200.), &font);
        font.align = TextAlign::Center;
        let center = ShapedText::with_font(&mut fonts, "abc", 20., Some(200.), &font);
        font.align = TextAlign::Right;
        let right = ShapedText::with_font(&mut fonts, "abc", 20., Some(200.), &font);
        let x = |layout: &ShapedText| TextLayout::caret(layout, 0).x;
        assert_eq!(left.size(), center.size());
        assert_eq!(left.size(), right.size());
        assert!((x(&center) - (200. - left.size().0) * 0.5).abs() < 0.01);
        assert!((x(&right) - (200. - left.size().0)).abs() < 0.01);
        assert!(
            right
                .buffer
                .layout_runs()
                .flat_map(|r| r.glyphs)
                .all(|g| g.glyph_id != 0)
        );
    }
    #[test]
    fn explicit_fallback_order_selects_real_fonts_before_platform_fallback() {
        use zgui::text_layout::FontFamily;
        let mut fonts = bundled_test_fonts();
        let first = fonts
            .db()
            .query(&cosmic_text::fontdb::Query {
                families: &[Family::Name("zgui test first")],
                ..Default::default()
            })
            .expect("test font");
        let second = fonts
            .db()
            .query(&cosmic_text::fontdb::Query {
                families: &[Family::Name("zgui test second")],
                ..Default::default()
            })
            .expect("test font");
        assert_ne!(
            first, second,
            "fallback families must have distinct face IDs"
        );
        let mut style = FontStyle {
            family: FontFamily::from("zgui nonexistent family"),
            fallbacks: vec![
                FontFamily::from("zgui test first"),
                FontFamily::from("zgui test second"),
            ]
            .into(),
            ..Default::default()
        };
        let layout = ShapedText::with_font(&mut fonts, "office e\u{301}", 20., None, &style);
        let glyphs = layout
            .buffer
            .layout_runs()
            .flat_map(|r| r.glyphs)
            .collect::<Vec<_>>();
        assert!(!glyphs.is_empty(), "fallback must shape actual glyphs");
        assert!(glyphs.iter().all(|g| g.font_id == first));
        style.fallbacks = vec![
            FontFamily::from("zgui test second"),
            FontFamily::from("zgui test first"),
        ]
        .into();
        let layout = ShapedText::with_font(&mut fonts, "office e\u{301}", 20., None, &style);
        let glyphs = layout
            .buffer
            .layout_runs()
            .flat_map(|r| r.glyphs)
            .collect::<Vec<_>>();
        assert!(
            !glyphs.is_empty(),
            "reordered fallback must shape actual glyphs"
        );
        assert!(glyphs.iter().all(|g| g.font_id == second));
    }
    #[test]
    fn opentype_features_change_actual_ligatures_and_share_canonical_keys() {
        use zgui::text_layout::{FontFamily, FontFeatures};
        let mut fonts = bundled_test_fonts();
        let mut style = FontStyle {
            family: FontFamily::from("DejaVu Sans"),
            ..Default::default()
        };
        let enabled = ShapedText::with_font(&mut fonts, "office ffi", 24., None, &style);
        let enabled_glyphs = enabled
            .buffer
            .layout_runs()
            .map(|r| r.glyphs.len())
            .sum::<usize>();
        style.features = FontFeatures::new([(*b"liga", 0), (*b"clig", 0)]);
        let disabled = ShapedText::with_font(&mut fonts, "office ffi", 24., None, &style);
        let disabled_glyphs = disabled
            .buffer
            .layout_runs()
            .map(|r| r.glyphs.len())
            .sum::<usize>();
        assert!(
            disabled_glyphs > enabled_glyphs,
            "font must expose real standard ligatures"
        );
        assert_eq!(
            style.features,
            FontFeatures::new([(*b"clig", 0), (*b"liga", 1), (*b"liga", 0)])
        );
        let rich = RichText::new(
            "office ffi",
            vec![TextRun {
                range: 0..10,
                font: style,
                font_size: 24.,
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(
            rich_buffer(&mut fonts, &rich, None, None)
                .layout_runs()
                .map(|r| r.glyphs.len())
                .sum::<usize>(),
            disabled_glyphs
        );
    }
    #[test]
    fn mixed_runs_shape_colors_metrics_and_wrapped_unicode_continuously() {
        let mut fonts = FontSystem::new();
        let content = "Hello العربية world e\u{301} end";
        let split = "Hello ".len();
        let rich = RichText::new(
            content,
            vec![
                TextRun {
                    range: 0..split,
                    font: FontStyle::default(),
                    font_size: 14.,
                    color: Color(255, 0, 0, 255),
                    ..Default::default()
                },
                TextRun {
                    range: split..content.len(),
                    font: FontStyle {
                        weight: 700,
                        ..Default::default()
                    },
                    font_size: 28.,
                    color: Color(0, 255, 0, 255),
                    ..Default::default()
                },
            ],
        )
        .unwrap();
        let wide = ShapedText::with_runs(&mut fonts, &rich, None);
        let narrow = ShapedText::with_runs(&mut fonts, &rich, Some(100.));
        assert!(narrow.size().1 > wide.size().1);
        assert!(wide.size().1 >= 28.);
        let buffer = rich_buffer(&mut fonts, &rich, None, None);
        let starts = line_starts(&buffer);
        let colors: Vec<_> = buffer
            .layout_runs()
            .flat_map(|r| {
                let start = starts[r.line_i];
                let rich = &rich;
                r.glyphs
                    .iter()
                    .map(move |g| rich.runs()[run_at(rich, start + g.start)].color)
            })
            .collect();
        assert!(colors.contains(&Color(255, 0, 0, 255)));
        assert!(colors.contains(&Color(0, 255, 0, 255)));
        assert!(
            buffer
                .layout_runs()
                .flat_map(|r| r.glyphs)
                .any(|g| g.font_size == 28.)
        );
        assert_eq!(narrow.text(), content);
    }
}

#[cfg(test)]
mod variation_axis_tests {
    //! A font's variation axes come from its file, which an application may
    //! not trust: an axis whose minimum is above its maximum shapes and draws
    //! like any other, where `f32::clamp` on its range would panic.
    use super::*;
    use zgui::text_layout::{FontFamily, FontStyle};

    /// The bundled font, given an `fvar` table with one `tag` axis.
    fn with_axis(tag: &[u8; 4], min: f32, default: f32, max: f32) -> Vec<u8> {
        let font: &[u8] = include_bytes!("../../../assets/DejaVuSans.ttf");
        let u16_at = |at: usize| usize::from(u16::from_be_bytes([font[at], font[at + 1]]));
        let u32_at = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
        // Version 1.0, axes at 16, one 20-byte axis, no named instances.
        let mut fvar: Vec<u8> = [1u16, 0, 16, 2, 1, 20, 0, 8]
            .iter()
            .flat_map(|field| field.to_be_bytes())
            .collect();
        fvar.extend(tag);
        for value in [min, default, max] {
            fvar.extend(((value * 65536.) as i32).to_be_bytes());
        }
        fvar.extend([0, 0, 1, 0]); // No flags; name 256.
        let mut tables: Vec<([u8; 4], &[u8])> = (0..u16_at(4))
            .map(|index| {
                let record = 12 + 16 * index;
                let (offset, len) = (u32_at(record + 8), u32_at(record + 12));
                (
                    font[record..record + 4].try_into().unwrap(),
                    &font[offset..offset + len],
                )
            })
            .collect();
        tables.push((*b"fvar", &fvar));
        tables.sort_by_key(|(tag, _)| *tag);
        let count = tables.len();
        let selector = count.ilog2() as usize;
        let mut out = font[..4].to_vec();
        for field in [
            count,
            16 << selector,
            selector,
            (count - (1 << selector)) * 16,
        ] {
            out.extend((field as u16).to_be_bytes());
        }
        let mut data = Vec::new();
        for (tag, table) in &tables {
            out.extend(tag);
            out.extend([0; 4]); // Checksums go unchecked.
            out.extend(((12 + 16 * count + data.len()) as u32).to_be_bytes());
            out.extend((table.len() as u32).to_be_bytes());
            data.extend(*table);
            data.resize(data.len().next_multiple_of(4), 0);
        }
        out.extend(data);
        out
    }

    /// Shapes "Hello" in `font` at `weight` through `FontData`, then draws
    /// each glyph as an image and as an outline.
    fn shape_and_draw(font: Vec<u8>, weight: u16) {
        let data = FontData::new(font).expect("a font with an fvar table");
        let mut fonts = FontSystem::new_with_locale_and_db(
            "en-US".into(),
            cosmic_text::fontdb::Database::new(),
        );
        data.install(&mut fonts);
        let style = FontStyle {
            family: FontFamily::from("DejaVu Sans"),
            weight,
            ..Default::default()
        };
        let shaped = ShapedText::with_font(&mut fonts, "Hello", 13., None, &style);
        let glyphs: Vec<_> = shaped
            .buffer
            .layout_runs()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| g.physical((0., 0.), 1.).cache_key)
            })
            .collect();
        assert_eq!(glyphs.len(), 5);
        let mut swash = cosmic_text::SwashCache::new();
        for glyph in glyphs {
            assert!(swash.get_image_uncached(&mut fonts, glyph).is_some());
            assert!(
                swash
                    .get_outline_commands_uncached(&mut fonts, glyph)
                    .is_some()
            );
        }
    }

    #[test]
    fn an_optical_size_axis_shapes_and_draws_whichever_way_its_range_runs() {
        shape_and_draw(with_axis(b"opsz", 17., 28., 96.), 400);
        shape_and_draw(with_axis(b"opsz", 96., 28., 17.), 400);
    }

    #[test]
    fn a_weight_axis_shapes_and_draws_whichever_way_its_range_runs() {
        shape_and_draw(with_axis(b"wght", 100., 400., 900.), 700);
        shape_and_draw(with_axis(b"wght", 900., 400., 100.), 700);
    }
}

/// A visual fragment of a run decoration, relative to the paragraph origin.
#[derive(Clone, Copy, Debug)]
pub struct RichDecoration {
    /// Its box, from the paragraph's origin.
    pub bounds: Rect,
    /// Its colour.
    pub color: zgui::scene::Color,
    /// Whether it is a background (else a line).
    pub background: bool,
    /// A background's corner radius.
    pub radius: f32,
    /// The styled run it decorates, to recolour it without re-shaping.
    pub run: usize,
    /// A line decoration: strikethrough rather than underline.
    pub strike: bool,
}
impl RichDecoration {
    /// This decoration's colour under `rich`, which has the same shape.
    pub fn color_in(&self, rich: &zgui::rich_text::RichText) -> zgui::scene::Color {
        let run = &rich.runs()[self.run];
        if self.background {
            return run.background.unwrap_or(self.color);
        }
        let line = if self.strike {
            run.strikethrough
        } else {
            run.underline
        };
        line.and_then(|d| d.color).unwrap_or(run.color)
    }
}
/// Where each of `buffer`'s lines starts in its source text.
pub fn line_starts(buffer: &Buffer) -> Vec<usize> {
    let mut offset = 0;
    buffer
        .lines
        .iter()
        .map(|line| {
            let start = offset;
            offset += line.text().len() + line.ending().as_str().len();
            start
        })
        .collect()
}
/// The rich run holding source byte `index` (the last run past the end,
/// where an ellipsis is appended).
pub fn run_at(rich: &zgui::rich_text::RichText, index: usize) -> usize {
    let runs = rich.runs();
    runs.partition_point(|run| run.range.end <= index)
        .min(runs.len().saturating_sub(1))
}
/// A plain text node's underline (`FontStyle::underline`): a line under
/// each laid-out line's glyphs, drawn in the text's colour.
pub fn plain_underlines<'a>(
    runs: impl Iterator<Item = cosmic_text::LayoutRun<'a>>,
    font_size: f32,
    underline: zgui::text_layout::TextUnderline,
) -> Vec<RichDecoration> {
    let below = underline.offset().unwrap_or(font_size * 0.1);
    runs.filter_map(|line| {
        let (left, right) = line
            .glyphs
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), g| {
                (l.min(g.x), r.max(g.x + g.w))
            });
        (right > left).then(|| RichDecoration {
            bounds: Rect::new(left, line.line_y + below, right - left, 1.),
            color: zgui::scene::Color(255, 255, 255, 255),
            background: false,
            radius: 0.,
            run: 0,
            strike: false,
        })
    })
    .collect()
}
/// Derive decoration geometry from shaped visual fragments, including bidi and wrapping.
pub fn rich_decorations(buffer: &Buffer, rich: &zgui::rich_text::RichText) -> Vec<RichDecoration> {
    rich_decorations_in(buffer.layout_runs(), &line_starts(buffer), rich)
}
/// `rich_decorations` over any laid-out runs, given where each line starts.
pub fn rich_decorations_in<'a>(
    runs: impl Iterator<Item = cosmic_text::LayoutRun<'a>>,
    starts: &[usize],
    rich: &zgui::rich_text::RichText,
) -> Vec<RichDecoration> {
    let mut output = Vec::new();
    for line in runs {
        let start = starts[line.line_i];
        for (index, run) in rich.runs().iter().enumerate() {
            if run.background.is_none() && run.underline.is_none() && run.strikethrough.is_none() {
                continue;
            }
            let mut spans: Vec<_> = line
                .glyphs
                .iter()
                .filter(|g| g.metadata != usize::MAX && run_at(rich, start + g.start) == index)
                .map(|g| (g.x, g.x + g.w))
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (left, right) in spans {
                if let Some(last) = merged.last_mut()
                    && left <= last.1 + 0.01
                {
                    last.1 = last.1.max(right);
                } else {
                    merged.push((left, right));
                }
            }
            for (left, right) in merged {
                if let Some(color) = run.background {
                    // A padded or rounded background hugs the text (CSS's
                    // content area, about 0.92 em above the baseline and
                    // 0.24 below) plus its padding; a plain one fills the line.
                    let bounds = if run.padding != [0.; 2] || run.radius > 0. {
                        let (above, below) = (run.font_size * 0.92, run.font_size * 0.24);
                        let top = line.line_y - above - run.padding[1];
                        Rect::new(left, top, right - left, above + below + 2. * run.padding[1])
                    } else {
                        Rect::new(left, line.line_top, right - left, line.line_height)
                    };
                    output.push(RichDecoration {
                        bounds,
                        color,
                        background: true,
                        radius: run.radius,
                        run: index,
                        strike: false,
                    });
                }
                for (decoration, y, strike) in [
                    (
                        run.underline,
                        line.line_y
                            + run
                                .underline
                                .and_then(|d| d.offset_px())
                                .unwrap_or(run.font_size * 0.1),
                        false,
                    ),
                    (run.strikethrough, line.line_y - run.font_size * 0.3, true),
                ] {
                    if let Some(style) = decoration {
                        output.push(RichDecoration {
                            bounds: Rect::new(left, y, right - left, style.thickness()),
                            color: style.color.unwrap_or(run.color),
                            background: false,
                            radius: 0.,
                            run: index,
                            strike,
                        });
                    }
                }
            }
        }
    }
    output
}
