//! Extended retained layout options. Lengths are logical pixels or fractions of
//! the definite containing block; `Percent(1.0)` means 100%.
use crate::scene::Align;
/// A length: automatic, in logical pixels, or a fraction of the
/// containing block.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Sized by the layout (CSS `auto`).
    #[default]
    Auto,
    /// Logical pixels.
    Px(f32),
    /// A fraction of the containing block: `1.0` is 100%.
    Percent(f32),
}
impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}
/// How a view lays out its children, or that it takes no part in
/// layout (CSS `display`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    /// Flexbox, in a row or column (CSS `display: flex`).
    #[default]
    Flex,
    /// A grid of `columns` by `rows` equal tracks (CSS `display: grid`).
    Grid,
    /// Out of layout and interaction (CSS `display: none`).
    None,
}
/// Whether a flex container's children wrap onto more lines (CSS
/// `flex-wrap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexWrap {
    /// One line (`nowrap`).
    #[default]
    NoWrap,
    /// More lines as needed, each after the last (`wrap`).
    Wrap,
    /// More lines as needed, each before the last (`wrap-reverse`).
    Reverse,
}
/// How a container spaces its lines across its axis (CSS
/// `align-content`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentAlign {
    /// Packed at the start.
    #[default]
    Start,
    /// Packed in the centre.
    Center,
    /// Packed at the end.
    End,
    /// Stretched to fill the container.
    Stretch,
    /// Equal room between the lines, none at the edges.
    SpaceBetween,
    /// Equal room around each line, so half as much at the edges.
    SpaceAround,
    /// Equal room between the lines and at the edges.
    SpaceEvenly,
}
/// A length for each side, each unset unless given.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeLengths {
    /// The top side's.
    pub top: Option<Length>,
    /// The right side's.
    pub right: Option<Length>,
    /// The bottom side's.
    pub bottom: Option<Length>,
    /// The left side's.
    pub left: Option<Length>,
}
impl EdgeLengths {
    /// `value` on every side.
    pub fn all(value: Length) -> Self {
        Self {
            top: Some(value),
            right: Some(value),
            bottom: Some(value),
            left: Some(value),
        }
    }
    /// Take each side `other` sets, keeping the rest.
    pub fn merge(&mut self, other: Self) {
        if other.top.is_some() {
            self.top = other.top;
        }
        if other.right.is_some() {
            self.right = other.right;
        }
        if other.bottom.is_some() {
            self.bottom = other.bottom;
        }
        if other.left.is_some() {
            self.left = other.left;
        }
    }
}
/// Lengths at the start and end of a line (CSS's logical `-inline-start`
/// and `-inline-end`): left and right left to right, right and left right
/// to left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InlineEdges {
    /// At the line's start: the left, or the right right to left.
    pub start: Option<Length>,
    /// At the line's end: the right, or the left right to left.
    pub end: Option<Length>,
}
impl InlineEdges {
    /// Take each end `other` sets, keeping the rest.
    pub fn merge(&mut self, other: Self) {
        if other.start.is_some() {
            self.start = other.start;
        }
        if other.end.is_some() {
            self.end = other.end;
        }
    }
    /// Whether neither end is set.
    pub fn is_empty(self) -> bool {
        self.start.is_none() && self.end.is_none()
    }
}
/// Sparse options shared by scene layout and fluent style refinement. Unspecified
/// properties preserve ordinary retained-layout defaults. Grid tracks use equal
/// fractional shares, matching GPUI's `grid_cols`/`grid_rows` public surface.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutOptions {
    /// Flex, grid, or out of layout (CSS `display`).
    pub display: Option<Display>,
    /// `Some(false)` keeps its room in layout but paints none of it and
    /// takes no input (CSS `visibility: hidden`).
    pub visible: Option<bool>,
    /// Whether it clips what overflows across (CSS `overflow-x: hidden`);
    /// unset, the style's `clip`.
    pub clip_x: Option<bool>,
    /// Whether it clips what overflows down (CSS `overflow-y: hidden`);
    /// unset, the style's `clip`.
    pub clip_y: Option<bool>,
    /// Whether its children wrap onto more lines (CSS `flex-wrap`).
    pub wrap: Option<FlexWrap>,
    /// Lay its children out last first (CSS `row-reverse`,
    /// `column-reverse`).
    pub reverse: Option<bool>,
    /// Its size along its parent's main axis before growing or shrinking
    /// (CSS `flex-basis`).
    pub basis: Option<Length>,
    /// Its alignment across its parent's axis, overriding the parent's
    /// (CSS `align-self`).
    pub align_self: Option<Align>,
    /// How it spaces its lines across its axis (CSS `align-content`).
    pub align_content: Option<ContentAlign>,
    /// The room between columns (CSS `column-gap`); unset, the style's
    /// `gap`.
    pub gap_x: Option<Length>,
    /// The room between rows (CSS `row-gap`); unset, the style's `gap`.
    pub gap_y: Option<Length>,
    /// Width over height (CSS `aspect-ratio`); ignored unless finite and
    /// positive.
    pub aspect_ratio: Option<f32>,
    /// A grid's number of equal columns (at least 1).
    pub columns: Option<u16>,
    /// A grid's number of equal rows (at least 1).
    pub rows: Option<u16>,
    /// The grid column it starts at, from 1; negative counts from the end,
    /// 0 places it automatically (CSS `grid-column-start`).
    pub column_start: Option<i16>,
    /// The grid row it starts at, from 1; negative counts from the end, 0
    /// places it automatically (CSS `grid-row-start`).
    pub row_start: Option<i16>,
    /// How many grid columns it spans (at least 1).
    pub column_span: Option<u16>,
    /// How many grid rows it spans (at least 1).
    pub row_span: Option<u16>,
    /// Span every grid column (CSS `grid-column: 1 / -1`), over
    /// `column_start` and `column_span`.
    pub column_full: Option<bool>,
    /// Span every grid row (CSS `grid-row: 1 / -1`), over `row_start` and
    /// `row_span`.
    pub row_full: Option<bool>,
    /// Its insets from its containing block's edges (CSS `top`, `right`,
    /// `bottom`, `left`).
    pub inset: EdgeLengths,
    /// Its margins, over the style's (CSS `margin`).
    pub margin: EdgeLengths,
    /// Its padding, over the style's (CSS `padding`).
    pub padding: EdgeLengths,
    /// Insets at the start and end of a line, which follow the direction
    /// (`Scene::set_direction`).
    pub inset_inline: InlineEdges,
    /// Margins at the start and end of a line (CSS `margin-inline-start`
    /// and `margin-inline-end`).
    pub margin_inline: InlineEdges,
    /// Padding at the start and end of a line (CSS `padding-inline-start`
    /// and `padding-inline-end`).
    pub padding_inline: InlineEdges,
    /// The smallest width (CSS `min-width`); unset, the style's.
    pub min_width: Option<Length>,
    /// The largest width (CSS `max-width`); unset, the style's.
    pub max_width: Option<Length>,
    /// The smallest height (CSS `min-height`); unset, the style's.
    pub min_height: Option<Length>,
    /// The largest height (CSS `max-height`); unset, the style's.
    pub max_height: Option<Length>,
}
/// The grid lines a placement reaches at most, either way (`normalized`).
const GRID_LINES: i16 = 10_000;
impl LayoutOptions {
    /// Canonicalize nonfinite values at style ingress, preserving equal-write suppression.
    pub fn normalized(mut self) -> Self {
        fn finite(value: f32) -> f32 {
            if value.is_finite() { value } else { 0. }
        }
        fn length(value: &mut Option<Length>) {
            if let Some(inner) = value {
                *inner = match *inner {
                    Length::Px(v) => Length::Px(finite(v)),
                    Length::Percent(v) => Length::Percent(finite(v)),
                    Length::Auto => Length::Auto,
                };
            }
        }
        for value in [
            &mut self.basis,
            &mut self.gap_x,
            &mut self.gap_y,
            &mut self.min_width,
            &mut self.max_width,
            &mut self.min_height,
            &mut self.max_height,
        ] {
            length(value);
        }
        for edges in [&mut self.inset, &mut self.margin, &mut self.padding] {
            for value in [
                &mut edges.top,
                &mut edges.right,
                &mut edges.bottom,
                &mut edges.left,
            ] {
                length(value);
            }
        }
        for edges in [
            &mut self.inset_inline,
            &mut self.margin_inline,
            &mut self.padding_inline,
        ] {
            length(&mut edges.start);
            length(&mut edges.end);
        }
        if let Some(ratio) = &mut self.aspect_ratio {
            *ratio = finite(*ratio).max(0.);
        }
        if let Some(count) = &mut self.columns {
            *count = (*count).max(1);
        }
        if let Some(count) = &mut self.rows {
            *count = (*count).max(1);
        }
        // Grid lines a placement reaches, either way, as Firefox's: taffy
        // numbers lines in `i16`, and a start and span past them overflowed.
        for start in [&mut self.column_start, &mut self.row_start]
            .into_iter()
            .flatten()
        {
            *start = (*start).clamp(-GRID_LINES, GRID_LINES);
        }
        for span in [&mut self.column_span, &mut self.row_span]
            .into_iter()
            .flatten()
        {
            *span = (*span).min(GRID_LINES as u16);
        }
        self
    }
    /// Whether it sets nothing.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
    /// Take each option `other` sets, keeping the rest.
    pub fn merge(&mut self, other: Self) {
        macro_rules! merge{($($field:ident),* $(,)?)=>{$(if other.$field.is_some(){self.$field=other.$field;})*};}
        merge!(
            display,
            visible,
            wrap,
            reverse,
            clip_x,
            clip_y,
            basis,
            align_self,
            align_content,
            gap_x,
            gap_y,
            aspect_ratio,
            columns,
            rows,
            column_start,
            row_start,
            column_span,
            row_span,
            column_full,
            row_full,
            min_width,
            max_width,
            min_height,
            max_height
        );
        self.inset.merge(other.inset);
        self.margin.merge(other.margin);
        self.padding.merge(other.padding);
        self.inset_inline.merge(other.inset_inline);
        self.margin_inline.merge(other.margin_inline);
        self.padding_inline.merge(other.padding_inline);
    }
    /// Whether it sets anything at a line's start or end.
    pub fn has_inline(&self) -> bool {
        !(self.inset_inline.is_empty()
            && self.margin_inline.is_empty()
            && self.padding_inline.is_empty())
    }
}
