// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};
use core::ops::Range;
use rangemap::RangeMap;
use smol_str::SmolStr;

use crate::{CacheKeyFlags, Metrics};

pub use fontdb::{Family, Stretch, Style, Weight};

/// Text color
#[derive(Clone, Copy, Debug, PartialOrd, Ord, Eq, Hash, PartialEq)]
pub struct Color(pub u32);

impl Color {
    /// Create new color with red, green, and blue components
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 0xFF)
    }

    /// Create new color with red, green, blue, and alpha components
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    /// Get a tuple over all of the attributes, in `(r, g, b, a)` order.
    #[inline]
    pub const fn as_rgba_tuple(self) -> (u8, u8, u8, u8) {
        (self.r(), self.g(), self.b(), self.a())
    }

    /// Get an array over all of the components, in `[r, g, b, a]` order.
    #[inline]
    pub const fn as_rgba(self) -> [u8; 4] {
        [self.r(), self.g(), self.b(), self.a()]
    }

    /// Get the red component
    #[inline]
    pub const fn r(&self) -> u8 {
        ((self.0 & 0x00_FF_00_00) >> 16) as u8
    }

    /// Get the green component
    #[inline]
    pub const fn g(&self) -> u8 {
        ((self.0 & 0x00_00_FF_00) >> 8) as u8
    }

    /// Get the blue component
    #[inline]
    pub const fn b(&self) -> u8 {
        (self.0 & 0x00_00_00_FF) as u8
    }

    /// Get the alpha component
    #[inline]
    pub const fn a(&self) -> u8 {
        ((self.0 & 0xFF_00_00_00) >> 24) as u8
    }
}

/// An owned version of [`Family`]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FamilyOwned {
    Name(SmolStr),
    Serif,
    SansSerif,
    Cursive,
    Fantasy,
    Monospace,
}

impl FamilyOwned {
    pub fn new(family: Family) -> Self {
        match family {
            Family::Name(name) => Self::Name(SmolStr::from(name)),
            Family::Serif => Self::Serif,
            Family::SansSerif => Self::SansSerif,
            Family::Cursive => Self::Cursive,
            Family::Fantasy => Self::Fantasy,
            Family::Monospace => Self::Monospace,
        }
    }

    pub fn as_family(&self) -> Family<'_> {
        match self {
            Self::Name(name) => Family::Name(name),
            Self::Serif => Family::Serif,
            Self::SansSerif => Family::SansSerif,
            Self::Cursive => Family::Cursive,
            Self::Fantasy => Family::Fantasy,
            Self::Monospace => Family::Monospace,
        }
    }
}

/// Metrics, but implementing Eq and Hash using u32 representation of f32
//TODO: what are the edge cases of this?
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CacheMetrics {
    font_size_bits: u32,
    line_height_bits: u32,
}

impl From<Metrics> for CacheMetrics {
    fn from(metrics: Metrics) -> Self {
        Self {
            font_size_bits: metrics.font_size.to_bits(),
            line_height_bits: metrics.line_height.to_bits(),
        }
    }
}

impl From<CacheMetrics> for Metrics {
    fn from(metrics: CacheMetrics) -> Self {
        Self {
            font_size: f32::from_bits(metrics.font_size_bits),
            line_height: f32::from_bits(metrics.line_height_bits),
        }
    }
}
/// A 4-byte `OpenType` feature tag identifier
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FeatureTag([u8; 4]);

impl FeatureTag {
    pub const fn new(tag: &[u8; 4]) -> Self {
        Self(*tag)
    }

    /// Kerning adjusts spacing between specific character pairs
    pub const KERNING: Self = Self::new(b"kern");
    /// Standard ligatures (fi, fl, etc.)
    pub const STANDARD_LIGATURES: Self = Self::new(b"liga");
    /// Contextual ligatures (context-dependent ligatures)
    pub const CONTEXTUAL_LIGATURES: Self = Self::new(b"clig");
    /// Contextual alternates (glyph substitutions based on context)
    pub const CONTEXTUAL_ALTERNATES: Self = Self::new(b"calt");
    /// Discretionary ligatures (optional stylistic ligatures)
    pub const DISCRETIONARY_LIGATURES: Self = Self::new(b"dlig");
    /// Small caps (lowercase to small capitals)
    pub const SMALL_CAPS: Self = Self::new(b"smcp");
    /// All small caps (uppercase and lowercase to small capitals)
    pub const ALL_SMALL_CAPS: Self = Self::new(b"c2sc");
    /// Stylistic Set 1 (font-specific alternate glyphs)
    pub const STYLISTIC_SET_1: Self = Self::new(b"ss01");
    /// Stylistic Set 2 (font-specific alternate glyphs)
    pub const STYLISTIC_SET_2: Self = Self::new(b"ss02");

    pub const fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Feature {
    pub tag: FeatureTag,
    pub value: u32,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct FontFeatures {
    pub features: Vec<Feature>,
}

impl FontFeatures {
    pub const fn new() -> Self {
        Self {
            features: Vec::new(),
        }
    }

    pub fn set(&mut self, tag: FeatureTag, value: u32) -> &mut Self {
        self.features.push(Feature { tag, value });
        self
    }

    /// Enable a feature (set to 1)
    pub fn enable(&mut self, tag: FeatureTag) -> &mut Self {
        self.set(tag, 1)
    }

    /// Disable a feature (set to 0)
    pub fn disable(&mut self, tag: FeatureTag) -> &mut Self {
        self.set(tag, 0)
    }
}

/// A wrapper for letter spacing to get around that f32 doesn't implement Eq and Hash
#[derive(Clone, Copy, Debug)]
pub struct LetterSpacing(pub f32);

impl PartialEq for LetterSpacing {
    fn eq(&self, other: &Self) -> bool {
        if self.0.is_nan() {
            other.0.is_nan()
        } else {
            self.0 == other.0
        }
    }
}

impl Eq for LetterSpacing {}

impl Hash for LetterSpacing {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        const CANONICAL_NAN_BITS: u32 = 0x7fc0_0000;

        let bits = if self.0.is_nan() {
            CANONICAL_NAN_BITS
        } else {
            // Add +0.0 to canonicalize -0.0 to +0.0
            (self.0 + 0.0).to_bits()
        };

        bits.hash(hasher);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UnderlineStyle {
    #[default]
    None,
    Single,
    Double,
    // TODO: Wavy
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TextDecoration {
    pub underline: UnderlineStyle,
    pub underline_color_opt: Option<Color>,
    pub strikethrough: bool,
    pub strikethrough_color_opt: Option<Color>,
    pub overline: bool,
    pub overline_color_opt: Option<Color>,
}

impl TextDecoration {
    pub const fn new() -> Self {
        Self {
            underline: UnderlineStyle::None,
            underline_color_opt: None,
            strikethrough: false,
            strikethrough_color_opt: None,
            overline: false,
            overline_color_opt: None,
        }
    }

    pub const fn has_decoration(&self) -> bool {
        !matches!(self.underline, UnderlineStyle::None) || self.strikethrough || self.overline
    }
}

/// Offset and thickness for a text decoration line, in EM units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecorationMetrics {
    /// Offset from baseline in EM units
    pub offset: f32,
    /// Thickness in EM units
    pub thickness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphDecorationData {
    /// The text decoration configuration from the user
    pub text_decoration: TextDecoration,
    /// Underline offset and thickness from the font
    pub underline_metrics: DecorationMetrics,
    /// Strikethrough offset and thickness from the font
    pub strikethrough_metrics: DecorationMetrics,
    /// Font ascent in EM units (ascent / upem).
    /// Used for overline positioning
    pub ascent: f32,
}

/// Text attributes
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Attrs<'a> {
    //TODO: should this be an option?
    pub color_opt: Option<Color>,
    pub family: Family<'a>,
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
    pub metadata: usize,
    pub cache_key_flags: CacheKeyFlags,
    pub metrics_opt: Option<CacheMetrics>,
    /// Letter spacing (tracking) in EM
    pub letter_spacing_opt: Option<LetterSpacing>,
    /// zgx: room added before a span's first glyph and after its last, in
    /// EM (CSS's inline padding: an inline code chip's)
    pub padding_opt: Option<LetterSpacing>,
    /// zgx: the optical size variable fonts shape and draw at (their
    /// `opsz` axis), in pixels: CSS's `font-optical-sizing: auto` sets it to
    /// the font size. 0 leaves the font's default.
    pub optical: u16,
    /// zgx: shown masked, a bullet for each character (a password), each
    /// bullet keeping its character's place in the text.
    pub masked: bool,
    pub font_features: FontFeatures,
    pub text_decoration: TextDecoration,
}

impl<'a> Attrs<'a> {
    /// Create a new set of attributes with sane defaults
    ///
    /// This defaults to a regular Sans-Serif font.
    pub const fn new() -> Self {
        Self {
            color_opt: None,
            family: Family::SansSerif,
            stretch: Stretch::Normal,
            style: Style::Normal,
            weight: Weight::NORMAL,
            metadata: 0,
            cache_key_flags: CacheKeyFlags::empty(),
            metrics_opt: None,
            letter_spacing_opt: None,
            padding_opt: None,
            optical: 0,
            masked: false,
            font_features: FontFeatures::new(),
            text_decoration: TextDecoration::new(),
        }
    }

    /// Set [Color]
    pub const fn color(mut self, color: Color) -> Self {
        self.color_opt = Some(color);
        self
    }

    /// Set [Family]
    pub const fn family(mut self, family: Family<'a>) -> Self {
        self.family = family;
        self
    }

    /// Set [Stretch]
    pub const fn stretch(mut self, stretch: Stretch) -> Self {
        self.stretch = stretch;
        self
    }

    /// Set [Style]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set [Weight]
    pub const fn weight(mut self, weight: Weight) -> Self {
        self.weight = weight;
        self
    }

    /// Set metadata
    pub const fn metadata(mut self, metadata: usize) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set [`CacheKeyFlags`]
    pub const fn cache_key_flags(mut self, cache_key_flags: CacheKeyFlags) -> Self {
        self.cache_key_flags = cache_key_flags;
        self
    }

    /// Set [`Metrics`], overriding values in buffer
    pub fn metrics(mut self, metrics: Metrics) -> Self {
        self.metrics_opt = Some(metrics.into());
        self
    }

    /// Set letter spacing (tracking) in EM
    pub const fn letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing_opt = Some(LetterSpacing(letter_spacing));
        self
    }

    /// zgx: show the text masked (see `masked`)
    pub const fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// zgx: set the optical size (pixels; 0 for the font's default)
    pub const fn optical(mut self, optical: u16) -> Self {
        self.optical = optical;
        self
    }

    /// zgx: set the span's inline padding (in EM)
    pub const fn padding(mut self, padding: f32) -> Self {
        self.padding_opt = Some(LetterSpacing(padding));
        self
    }

    /// Set [`FontFeatures`]
    pub fn font_features(mut self, font_features: FontFeatures) -> Self {
        self.font_features = font_features;
        self
    }

    pub const fn underline(mut self, style: UnderlineStyle) -> Self {
        self.text_decoration.underline = style;
        self
    }

    pub const fn underline_color(mut self, color: Color) -> Self {
        self.text_decoration.underline_color_opt = Some(color);
        self
    }

    pub const fn strikethrough(mut self) -> Self {
        self.text_decoration.strikethrough = true;
        self
    }

    pub const fn strikethrough_color(mut self, color: Color) -> Self {
        self.text_decoration.strikethrough_color_opt = Some(color);
        self
    }

    pub const fn overline(mut self) -> Self {
        self.text_decoration.overline = true;
        self
    }

    pub const fn overline_color(mut self, color: Color) -> Self {
        self.text_decoration.overline_color_opt = Some(color);
        self
    }

    /// Check if this set of attributes can be shaped with another
    pub fn compatible(&self, other: &Self) -> bool {
        self.family == other.family
            && self.stretch == other.stretch
            && self.style == other.style
            && self.weight == other.weight
    }
}

/// Font-specific part of [`Attrs`] to be used for matching
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FontMatchAttrs {
    family: FamilyOwned,
    stretch: Stretch,
    style: Style,
    weight: Weight,
}

impl FontMatchAttrs {
    /// zgx patch: whether `attrs` makes this key, without making one.
    pub(crate) fn matches(&self, attrs: &Attrs<'_>) -> bool {
        self.family.as_family() == attrs.family
            && self.stretch == attrs.stretch
            && self.style == attrs.style
            && self.weight == attrs.weight
    }
}

impl<'a> From<&Attrs<'a>> for FontMatchAttrs {
    fn from(attrs: &Attrs<'a>) -> Self {
        Self {
            family: FamilyOwned::new(attrs.family),
            stretch: attrs.stretch,
            style: attrs.style,
            weight: attrs.weight,
        }
    }
}

/// An owned version of [`Attrs`]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AttrsOwned {
    //TODO: should this be an option?
    pub color_opt: Option<Color>,
    pub family_owned: FamilyOwned,
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
    pub metadata: usize,
    pub cache_key_flags: CacheKeyFlags,
    pub metrics_opt: Option<CacheMetrics>,
    /// Letter spacing (tracking) in EM
    pub letter_spacing_opt: Option<LetterSpacing>,
    pub padding_opt: Option<LetterSpacing>,
    pub optical: u16,
    pub masked: bool,
    pub font_features: FontFeatures,
    pub text_decoration: TextDecoration,
}

impl AttrsOwned {
    pub fn new(attrs: &Attrs) -> Self {
        Self {
            color_opt: attrs.color_opt,
            family_owned: FamilyOwned::new(attrs.family),
            stretch: attrs.stretch,
            style: attrs.style,
            weight: attrs.weight,
            metadata: attrs.metadata,
            cache_key_flags: attrs.cache_key_flags,
            metrics_opt: attrs.metrics_opt,
            letter_spacing_opt: attrs.letter_spacing_opt,
            padding_opt: attrs.padding_opt,
            optical: attrs.optical,
            masked: attrs.masked,
            font_features: attrs.font_features.clone(),
            text_decoration: attrs.text_decoration,
        }
    }

    pub fn as_attrs(&self) -> Attrs<'_> {
        Attrs {
            color_opt: self.color_opt,
            family: self.family_owned.as_family(),
            stretch: self.stretch,
            style: self.style,
            weight: self.weight,
            metadata: self.metadata,
            cache_key_flags: self.cache_key_flags,
            metrics_opt: self.metrics_opt,
            letter_spacing_opt: self.letter_spacing_opt,
            padding_opt: self.padding_opt,
            optical: self.optical,
            masked: self.masked,
            font_features: self.font_features.clone(),
            text_decoration: self.text_decoration,
        }
    }
}

/// List of text attributes to apply to a line
//TODO: have this clean up the spans when changes are made
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AttrsList {
    defaults: AttrsOwned,
    // Dropped by `Drop`, which most lists, with no spans, leave alone.
    pub(crate) spans: core::mem::ManuallyDrop<RangeMap<usize, AttrsOwned>>,
}

impl Drop for AttrsList {
    #[inline]
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // An empty map holds no memory: walking it is all a drop would do.
        if !self.spans.is_empty() {
            // SAFETY: dropped here alone, and never read again.
            unsafe { core::mem::ManuallyDrop::drop(&mut self.spans) };
        }
    }
}

impl AttrsList {
    /// Create a new attributes list with a set of default [Attrs]
    pub fn new(defaults: &Attrs) -> Self {
        Self {
            defaults: AttrsOwned::new(defaults),
            spans: core::mem::ManuallyDrop::new(RangeMap::new()),
        }
    }

    /// Get the default [Attrs]
    pub fn defaults(&self) -> Attrs<'_> {
        self.defaults.as_attrs()
    }

    /// Get the current attribute spans
    pub fn spans(&self) -> Vec<(&Range<usize>, &AttrsOwned)> {
        self.spans_iter().collect()
    }

    /// Get an iterator over the current attribute spans
    pub fn spans_iter(&self) -> impl Iterator<Item = (&Range<usize>, &AttrsOwned)> + '_ {
        self.spans.iter()
    }

    /// Clear the current attribute spans
    pub fn clear_spans(&mut self) {
        self.spans.clear();
    }

    /// Add an attribute span, removes any previous matching parts of spans
    pub fn add_span(&mut self, range: Range<usize>, attrs: &Attrs) {
        //do not support 1..1 or 2..1 even if by accident.
        if range.is_empty() {
            return;
        }

        self.spans.insert(range, AttrsOwned::new(attrs));
    }

    /// zgx: whether the defaults or any span set padding.
    pub fn has_padding(&self) -> bool {
        self.defaults.padding_opt.is_some()
            || self.spans.iter().any(|(_, attrs)| attrs.padding_opt.is_some())
    }

    /// zgx: the defaults, borrowed (`defaults` copies them out).
    pub(crate) fn defaults_owned(&self) -> &AttrsOwned {
        &self.defaults
    }

    /// Get the attribute span for an index
    ///
    /// This returns a span that contains the index
    pub fn get_span(&self, index: usize) -> Attrs<'_> {
        // zgx patch: the defaults are copied out only when no span holds
        // the index (they were copied out for every lookup).
        self.spans
            .get(&index)
            .map_or_else(|| self.defaults.as_attrs(), AttrsOwned::as_attrs)
    }

    /// Split attributes list at an offset
    #[allow(clippy::missing_panics_doc)]
    pub fn split_off(&mut self, index: usize) -> Self {
        let mut new = Self::new(&self.defaults.as_attrs());
        let mut removes = Vec::new();

        //get the keys we need to remove or fix.
        for span in self.spans.iter() {
            if span.0.end <= index {
                continue;
            }

            if span.0.start >= index {
                removes.push((span.0.clone(), false));
            } else {
                removes.push((span.0.clone(), true));
            }
        }

        for (key, resize) in removes {
            let (range, attrs) = self
                .spans
                .get_key_value(&key.start)
                .map(|v| (v.0.clone(), v.1.clone()))
                .expect("attrs span not found");
            self.spans.remove(key);

            if resize {
                new.spans.insert(0..range.end - index, attrs.clone());
                self.spans.insert(range.start..index, attrs);
            } else {
                new.spans
                    .insert(range.start - index..range.end - index, attrs);
            }
        }
        new
    }

    /// Resets the attributes with new defaults.
    pub(crate) fn reset(mut self, default: &Attrs) -> Self {
        self.defaults = AttrsOwned::new(default);
        self.spans.clear();
        self
    }
}
