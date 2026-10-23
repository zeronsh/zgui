//! Extended retained layout options. Lengths are logical pixels or fractions of
//! the definite containing block; `Percent(1.0)` means 100%.
use crate::scene::Align;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
}
impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Flex,
    Grid,
    None,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    Reverse,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeLengths {
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
    pub left: Option<Length>,
}
impl EdgeLengths {
    pub fn all(value: Length) -> Self {
        Self {
            top: Some(value),
            right: Some(value),
            bottom: Some(value),
            left: Some(value),
        }
    }
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
    pub start: Option<Length>,
    pub end: Option<Length>,
}
impl InlineEdges {
    pub fn merge(&mut self, other: Self) {
        if other.start.is_some() {
            self.start = other.start;
        }
        if other.end.is_some() {
            self.end = other.end;
        }
    }
    pub fn is_empty(self) -> bool {
        self.start.is_none() && self.end.is_none()
    }
}
/// Sparse options shared by scene layout and fluent style refinement. Unspecified
/// properties preserve ordinary retained-layout defaults. Grid tracks use equal
/// fractional shares, matching GPUI's `grid_cols`/`grid_rows` public surface.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutOptions {
    pub display: Option<Display>,
    pub visible: Option<bool>,
    pub clip_x: Option<bool>,
    pub clip_y: Option<bool>,
    pub wrap: Option<FlexWrap>,
    pub reverse: Option<bool>,
    pub basis: Option<Length>,
    pub align_self: Option<Align>,
    pub align_content: Option<ContentAlign>,
    pub gap_x: Option<Length>,
    pub gap_y: Option<Length>,
    pub aspect_ratio: Option<f32>,
    pub columns: Option<u16>,
    pub rows: Option<u16>,
    pub column_start: Option<i16>,
    pub row_start: Option<i16>,
    pub column_span: Option<u16>,
    pub row_span: Option<u16>,
    pub column_full: Option<bool>,
    pub row_full: Option<bool>,
    pub inset: EdgeLengths,
    pub margin: EdgeLengths,
    pub padding: EdgeLengths,
    /// Insets, margins and padding at the start and end of a line, which
    /// follow the direction (`Scene::set_direction`).
    pub inset_inline: InlineEdges,
    pub margin_inline: InlineEdges,
    pub padding_inline: InlineEdges,
    pub min_width: Option<Length>,
    pub max_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_height: Option<Length>,
}
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
        self
    }
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
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
