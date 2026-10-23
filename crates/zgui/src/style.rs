//! Fluent, sparse styling for retained views.
//!
//! Dimensions, spacing, text sizes, blur and translations use logical pixels,
//! except explicit percentage dimension methods.
//! Style patches only replace explicitly supplied properties; chaining follows
//! last-write precedence, including individual padding and margin sides.
use crate::scene::{Align, BoxShadow, Color, Insets, Justify, Layout, LocalTransform, Transform};

/// Where a stroke (`Styled::stroke`) lies against the box's edge, as a
/// design tool aligns it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StrokeAlign {
    /// Within the box, over its content.
    #[default]
    Inside,
    /// Half within, half outside.
    Center,
    /// Wholly outside.
    Outside,
}
/// How image pixels fit the allocated padded content box. Alignment is centered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    /// Stretch independently along each axis (the default).
    #[default]
    Fill,
    /// Preserve aspect ratio and show the entire image.
    Contain,
    /// Preserve aspect ratio and crop to fill the content box.
    Cover,
    /// Keep intrinsic pixel dimensions, clipping excess content.
    None,
    /// Use intrinsic size unless shrinking is needed to contain the image.
    ScaleDown,
}

/// A set of style properties, each unset until a builder sets it: a
/// view's own, or a reusable patch merged over one ([`Styled::style`]).
#[derive(Debug, Default, PartialEq)]
pub struct Styles {
    pub(crate) layout_options: Option<std::sync::Arc<crate::layout::LayoutOptions>>,
    pub(crate) object_fit: Option<ObjectFit>,
    pub(crate) layout: Option<Layout>,
    pub(crate) absolute: Option<bool>,
    pub(crate) width: Option<f32>,
    pub(crate) height: Option<f32>,
    pub(crate) width_percent: Option<f32>,
    pub(crate) height_percent: Option<f32>,
    pub(crate) padding: Option<Insets>,
    pub(crate) margin: Option<Insets>,
    pub(crate) gap: Option<f32>,
    pub(crate) clip: Option<bool>,
    pub(crate) flex_grow: Option<f32>,
    pub(crate) flex_shrink: Option<f32>,
    pub(crate) align: Option<Align>,
    pub(crate) justify: Option<Justify>,
    pub(crate) text_wrap: Option<bool>,
    pub(crate) background: Option<Color>,
    pub(crate) cursor: Option<crate::cursor::Cursor>,
    pub(crate) text_color: Option<Color>,
    pub(crate) text_size: Option<f32>,
    pub(crate) line_height: Option<crate::text_layout::LineHeight>,
    pub(crate) font_family: Option<crate::text_layout::FontFamily>,
    pub(crate) text_align: Option<crate::text_layout::TextAlign>,
    pub(crate) font_weight: Option<u16>,
    pub(crate) italic: Option<bool>,
    pub(crate) radius: Option<f32>,
    pub(crate) border_width: Option<f32>,
    pub(crate) border_color: Option<Color>,
    pub(crate) opacity: Option<f32>,
    pub(crate) blur: Option<f32>,
    pub(crate) edge_fade: Option<f32>,
    pub(crate) filter_blur: Option<f32>,
    /// Colour filters (`Styled::brightness` and the rest).
    pub(crate) brightness: Option<f32>,
    pub(crate) contrast: Option<f32>,
    pub(crate) saturate: Option<f32>,
    /// Where `RareStyles::mask` lies over the node: animated every frame.
    pub(crate) mask_offset: Option<(f32, f32)>,
    /// `Styled::clip_inset`: animated every frame.
    pub(crate) clip_inset: Option<crate::scene::ClipInset>,
    pub(crate) transform: Option<Transform>,
    pub(crate) local_transform: TransformPatch,
    pub(crate) isolated: Option<bool>,
    pub(crate) padding_sides: u8,
    pub(crate) margin_sides: u8,
    /// Set (to none) by `bg`, `rounded` and `border`, so on most nodes.
    pub(crate) paint_background: Option<Option<crate::decoration::Background>>,
    pub(crate) corners: Option<Option<crate::decoration::Corners>>,
    pub(crate) border_edges: Option<Option<Insets>>,
    /// Properties few views set (see `RareStyles`).
    pub(crate) rare: Option<std::sync::Arc<RareStyles>>,
    /// Properties set by theme tokens (`.bg(tok::SURFACE_RAISED)`), resolved
    /// when the node is styled; see `crate::theme`.
    pub(crate) tokens: Option<std::sync::Arc<crate::theme::Bindings>>,
}
/// As derived, but copying the plain fields as one block: a derived clone
/// moves each of some fifty `Option`s on its own (a load and store per tag
/// and per value), which a style effect re-running every frame pays for.
impl Clone for Styles {
    #[inline]
    fn clone(&self) -> Self {
        // SAFETY: `copy` is a bitwise copy of `self`, its owning fields
        // aliasing `self`'s. Each of those is overwritten with its own clone
        // through `ptr::write`, which drops nothing (the aliases are not
        // owned); the rest are `Copy`. `ManuallyDrop` keeps a panicking clone
        // from dropping the aliases. The field list is checked below.
        let mut copy = std::mem::ManuallyDrop::new(unsafe { std::ptr::read(self) });
        unsafe {
            std::ptr::write(&mut copy.layout_options, self.layout_options.clone());
            std::ptr::write(&mut copy.font_family, self.font_family.clone());
            std::ptr::write(&mut copy.paint_background, self.paint_background.clone());
            std::ptr::write(&mut copy.rare, self.rare.clone());
            std::ptr::write(&mut copy.tokens, self.tokens.clone());
        }
        std::mem::ManuallyDrop::into_inner(copy)
    }
}
/// Fails to compile if a field that is not `Copy` is added to `Styles`
/// without `Clone for Styles` cloning it: every other field is copied here
/// with `..*styles`, which only `Copy` fields allow.
#[allow(dead_code)]
fn styles_clone_covers_every_owning_field(styles: &Styles) -> Styles {
    Styles {
        layout_options: None,
        font_family: None,
        paint_background: None,
        rare: None,
        tokens: None,
        ..*styles
    }
}
/// Scale, rotation, skew and origin, each set or not (see `Styled::scale`).
/// Inline rather than among `RareStyles`: animations rewrite it every frame,
/// and a bit per component keeps it a third the size of four `Option`s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TransformPatch {
    pub(crate) values: LocalTransform,
    pub(crate) set: u8,
}
impl TransformPatch {
    const SCALE: u8 = 1;
    const ROTATE: u8 = 2;
    const SKEW: u8 = 4;
    const ORIGIN: u8 = 8;
    /// `self` with what `over` sets laid over it.
    pub(crate) fn over(mut self, over: Self) -> Self {
        let pick = |bit: u8| over.set & bit != 0;
        if pick(Self::SCALE) {
            self.values.scale = over.values.scale;
        }
        if pick(Self::ROTATE) {
            self.values.rotate = over.values.rotate;
        }
        if pick(Self::SKEW) {
            self.values.skew = over.values.skew;
        }
        if pick(Self::ORIGIN) {
            self.values.origin = over.values.origin;
        }
        self.set |= over.set;
        self
    }
    /// `base` with the set components replaced.
    pub(crate) fn resolve(self, base: LocalTransform) -> LocalTransform {
        Self {
            values: base,
            set: 0,
        }
        .over(self)
        .values
    }
}
/// Properties few views set, kept apart and shared so that every `Styles`
/// (a view's, its node's style effect's and that effect's memo) stays small:
/// a `Styles` is copied and compared on every re-run.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RareStyles {
    pub(crate) min_width: Option<f32>,
    pub(crate) max_width: Option<f32>,
    pub(crate) min_height: Option<f32>,
    pub(crate) max_height: Option<f32>,
    pub(crate) fade_edges: Option<[f32; 2]>,
    pub(crate) fade_insets: Option<[f32; 2]>,
    pub(crate) fade_edges_x: Option<[f32; 2]>,
    pub(crate) fade_insets_x: Option<[f32; 2]>,
    pub(crate) fade_overflow: Option<bool>,
    pub(crate) snap: Option<bool>,
    /// `Styled::direction`: which way its lines run.
    pub(crate) direction: Option<crate::scene::Direction>,
    pub(crate) border_style: Option<crate::decoration::BorderStyle>,
    /// `Styled::border_over`.
    pub(crate) border_over: Option<bool>,
    /// `Styled::outline`, `outline_offset` (width and colour; offset).
    pub(crate) outline: Option<Option<(f32, Color)>>,
    pub(crate) outline_offset: Option<f32>,
    /// `Styled::shader_uniforms`.
    pub(crate) shader_uniforms: Option<crate::decoration::ShaderUniforms>,
    pub(crate) shadows: Option<Option<std::sync::Arc<[BoxShadow]>>>,
    /// `Styled::inset_shadows`.
    pub(crate) inset_shadows: Option<Option<std::sync::Arc<[BoxShadow]>>>,
    pub(crate) shadow: Option<Option<BoxShadow>>,
    pub(crate) letter_spacing: Option<crate::text_layout::LetterSpacing>,
    /// `Styled::text_underline`, `underline_offset`.
    pub(crate) underline: Option<bool>,
    pub(crate) underline_offset: Option<f32>,
    pub(crate) text_overflow: Option<crate::text_layout::TextOverflow>,
    pub(crate) line_clamp: Option<Option<std::num::NonZeroU32>>,
    pub(crate) font_features: Option<crate::text_layout::FontFeatures>,
    pub(crate) font_fallbacks: Option<std::sync::Arc<[crate::text_layout::FontFamily]>>,
    /// A mask image and the size it is drawn at (`Styled::mask`).
    pub(crate) mask: Option<Option<MaskImage>>,
    /// `Styled::relative`: a containing block for absolute descendants.
    pub(crate) relative: Option<bool>,
}
/// A mask image and the size (logical pixels) it is drawn at.
pub(crate) type MaskImage = (std::sync::Arc<crate::image::ImageData>, (f32, f32));
impl RareStyles {
    const NONE: Self = Self {
        min_width: None,
        max_width: None,
        min_height: None,
        max_height: None,
        fade_edges: None,
        fade_insets: None,
        fade_edges_x: None,
        fade_insets_x: None,
        fade_overflow: None,
        snap: None,
        direction: None,
        border_style: None,
        border_over: None,
        outline: None,
        outline_offset: None,
        shader_uniforms: None,
        shadows: None,
        inset_shadows: None,
        shadow: None,
        letter_spacing: None,
        underline: None,
        underline_offset: None,
        text_overflow: None,
        line_clamp: None,
        font_features: None,
        font_fallbacks: None,
        mask: None,
        relative: None,
    };
    /// Overlay explicitly supplied properties, preserving all others.
    fn merge(&mut self, other: &Self) {
        if other.min_width.is_some() {
            self.min_width = other.min_width;
        }
        if other.max_width.is_some() {
            self.max_width = other.max_width;
        }
        if other.min_height.is_some() {
            self.min_height = other.min_height;
        }
        if other.max_height.is_some() {
            self.max_height = other.max_height;
        }
        if other.fade_edges.is_some() {
            self.fade_edges = other.fade_edges;
        }
        if other.fade_insets.is_some() {
            self.fade_insets = other.fade_insets;
        }
        if other.fade_edges_x.is_some() {
            self.fade_edges_x = other.fade_edges_x;
        }
        if other.fade_insets_x.is_some() {
            self.fade_insets_x = other.fade_insets_x;
        }
        if other.fade_overflow.is_some() {
            self.fade_overflow = other.fade_overflow;
        }
        if other.snap.is_some() {
            self.snap = other.snap;
        }
        if other.direction.is_some() {
            self.direction = other.direction;
        }
        if other.border_over.is_some() {
            self.border_over = other.border_over;
        }
        if other.outline.is_some() {
            self.outline = other.outline;
        }
        if other.outline_offset.is_some() {
            self.outline_offset = other.outline_offset;
        }
        if other.shader_uniforms.is_some() {
            self.shader_uniforms = other.shader_uniforms;
        }
        if other.border_style.is_some() {
            self.border_style = other.border_style;
        }
        if other.shadows.is_some() {
            self.shadows = other.shadows.clone();
        }
        if other.inset_shadows.is_some() {
            self.inset_shadows = other.inset_shadows.clone();
        }
        if other.shadow.is_some() {
            self.shadow = other.shadow;
        }
        if other.letter_spacing.is_some() {
            self.letter_spacing = other.letter_spacing;
        }
        if other.underline.is_some() {
            self.underline = other.underline;
        }
        if other.underline_offset.is_some() {
            self.underline_offset = other.underline_offset;
        }
        if other.text_overflow.is_some() {
            self.text_overflow = other.text_overflow;
        }
        if other.line_clamp.is_some() {
            self.line_clamp = other.line_clamp;
        }
        if other.font_features.is_some() {
            self.font_features = other.font_features.clone();
        }
        if other.font_fallbacks.is_some() {
            self.font_fallbacks = other.font_fallbacks.clone();
        }
        if other.mask.is_some() {
            self.mask = other.mask.clone();
        }
        if other.relative.is_some() {
            self.relative = other.relative;
        }
    }
}
static NO_RARE: RareStyles = RareStyles::NONE;
impl Styles {
    pub(crate) fn rare(&self) -> &RareStyles {
        self.rare.as_deref().unwrap_or(&NO_RARE)
    }
    pub(crate) fn rare_mut(&mut self) -> &mut RareStyles {
        std::sync::Arc::make_mut(self.rare.get_or_insert_with(Default::default))
    }
    /// Whether any typography field (what text inherits) is set.
    pub(crate) fn sets_typography(&self) -> bool {
        self.rare().text_overflow.is_some()
            || self.rare().line_clamp.is_some()
            || self.text_color.is_some()
            || self.text_size.is_some()
            || self.text_wrap.is_some()
            || self.font_family.is_some()
            || self.rare().font_features.is_some()
            || self.text_align.is_some()
            || self.rare().font_fallbacks.is_some()
            || self.font_weight.is_some()
            || self.italic.is_some()
            || self.line_height.is_some()
            || self.rare().letter_spacing.is_some()
            || self.rare().underline.is_some()
            || self.rare().underline_offset.is_some()
    }
    pub(crate) fn layout_options_mut(&mut self) -> &mut crate::layout::LayoutOptions {
        std::sync::Arc::make_mut(self.layout_options.get_or_insert_with(Default::default))
    }
    fn clear_layout_spacing(&mut self, margin: bool, mask: u8) {
        let Some(options) = &mut self.layout_options else {
            return;
        };
        let options = std::sync::Arc::make_mut(options);
        let edges = if margin {
            &mut options.margin
        } else {
            &mut options.padding
        };
        if mask & 1 != 0 {
            edges.top = None;
        }
        if mask & 2 != 0 {
            edges.right = None;
        }
        if mask & 4 != 0 {
            edges.bottom = None;
        }
        if mask & 8 != 0 {
            edges.left = None;
        }
        if options.is_empty() {
            self.layout_options = None;
        }
    }
    /// No properties set.
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether any property is set by a theme token.
    pub fn has_tokens(&self) -> bool {
        self.tokens.is_some()
    }
    /// Overlay explicitly supplied properties, preserving all others.
    pub fn merge(&mut self, other: &Self) {
        if self.tokens.is_some() || other.tokens.is_some() {
            crate::theme::merge(self, other);
        }
        if other.padding.is_some() {
            self.clear_layout_spacing(
                false,
                if other.padding_sides == 0 {
                    15
                } else {
                    other.padding_sides
                },
            );
        }
        if other.margin.is_some() {
            self.clear_layout_spacing(
                true,
                if other.margin_sides == 0 {
                    15
                } else {
                    other.margin_sides
                },
            );
        }
        if let Some(options) = &mut self.layout_options
            && (other.gap.is_some()
                || other.clip.is_some()
                || other.rare().min_width.is_some()
                || other.rare().max_width.is_some()
                || other.rare().min_height.is_some()
                || other.rare().max_height.is_some()
                || other.layout.is_some())
        {
            let options = std::sync::Arc::make_mut(options);
            if other.gap.is_some() {
                options.gap_x = None;
                options.gap_y = None;
            }
            if other.clip.is_some() {
                options.clip_x = None;
                options.clip_y = None;
            }
            if other.rare().min_width.is_some() {
                options.min_width = None;
            }
            if other.rare().max_width.is_some() {
                options.max_width = None;
            }
            if other.rare().min_height.is_some() {
                options.min_height = None;
            }
            if other.rare().max_height.is_some() {
                options.max_height = None;
            }
            if other.layout.is_some() {
                options.reverse = None;
            }
        }
        if let Some(options) = &other.layout_options {
            if let Some(current) = &self.layout_options {
                let mut merged = **current;
                merged.merge(**options);
                if merged != **current {
                    self.layout_options = Some(std::sync::Arc::new(merged));
                }
            } else {
                self.layout_options = Some(options.clone());
            }
        }
        if let Some(rare) = &other.rare {
            match &mut self.rare {
                Some(own) => std::sync::Arc::make_mut(own).merge(rare),
                None => self.rare = Some(rare.clone()),
            }
        }
        if other.paint_background.is_some() {
            self.paint_background = other.paint_background.clone();
        }
        if other.corners.is_some() {
            self.corners = other.corners;
        }
        if other.border_edges.is_some() {
            self.border_edges = other.border_edges;
        }
        if other.object_fit.is_some() {
            self.object_fit = other.object_fit;
        }
        if other.absolute.is_some() {
            self.absolute = other.absolute;
        }
        if other.layout.is_some() {
            self.layout = other.layout;
        }
        if other.width.is_some() || other.width_percent.is_some() {
            self.width = other.width;
            self.width_percent = other.width_percent;
        }
        if other.height.is_some() || other.height_percent.is_some() {
            self.height = other.height;
            self.height_percent = other.height_percent;
        }
        if other.gap.is_some() {
            self.gap = other.gap;
        }
        if other.clip.is_some() {
            self.clip = other.clip;
        }
        if other.flex_grow.is_some() {
            self.flex_grow = other.flex_grow;
        }
        if other.flex_shrink.is_some() {
            self.flex_shrink = other.flex_shrink;
        }
        if other.align.is_some() {
            self.align = other.align;
        }
        if other.justify.is_some() {
            self.justify = other.justify;
        }
        if other.text_wrap.is_some() {
            self.text_wrap = other.text_wrap;
        }
        if other.cursor.is_some() {
            self.cursor = other.cursor;
        }
        if other.background.is_some() {
            self.background = other.background;
        }
        if other.text_color.is_some() {
            self.text_color = other.text_color;
        }
        if other.text_align.is_some() {
            self.text_align = other.text_align;
        }
        if other.font_family.is_some() {
            self.font_family = other.font_family.clone();
        }
        if other.font_weight.is_some() {
            self.font_weight = other.font_weight;
        }
        if other.italic.is_some() {
            self.italic = other.italic;
        }
        if other.text_size.is_some() {
            self.text_size = other.text_size;
        }
        if other.line_height.is_some() {
            self.line_height = other.line_height;
        }
        if other.radius.is_some() {
            self.radius = other.radius;
        }
        if other.border_width.is_some() {
            self.border_width = other.border_width;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        if other.opacity.is_some() {
            self.opacity = other.opacity;
        }
        if other.blur.is_some() {
            self.blur = other.blur;
        }
        if other.edge_fade.is_some() {
            self.edge_fade = other.edge_fade;
        }
        if other.filter_blur.is_some() {
            self.filter_blur = other.filter_blur;
        }
        if other.brightness.is_some() {
            self.brightness = other.brightness;
        }
        if other.contrast.is_some() {
            self.contrast = other.contrast;
        }
        if other.saturate.is_some() {
            self.saturate = other.saturate;
        }
        if other.mask_offset.is_some() {
            self.mask_offset = other.mask_offset;
        }
        if other.clip_inset.is_some() {
            self.clip_inset = other.clip_inset;
        }
        if other.transform.is_some() {
            self.transform = other.transform;
        }
        self.local_transform = self.local_transform.over(other.local_transform);
        if other.isolated.is_some() {
            self.isolated = other.isolated;
        }
        if let Some(value) = other.padding {
            if self.padding.is_some() && self.padding_sides == 0 {
                self.padding_sides = 15;
            }
            let mask = if other.padding_sides == 0 {
                15
            } else {
                other.padding_sides
            };
            merge_insets(
                self.padding.get_or_insert_with(Insets::default),
                value,
                mask,
            );
            self.padding_sides |= mask;
        }
        if let Some(value) = other.margin {
            if self.margin.is_some() && self.margin_sides == 0 {
                self.margin_sides = 15;
            }
            let mask = if other.margin_sides == 0 {
                15
            } else {
                other.margin_sides
            };
            merge_insets(self.margin.get_or_insert_with(Insets::default), value, mask);
            self.margin_sides |= mask;
        }
    }
}
pub(crate) fn merge_insets(target: &mut Insets, source: Insets, mask: u8) {
    if mask & 1 != 0 {
        target.top = source.top;
    }
    if mask & 2 != 0 {
        target.right = source.right;
    }
    if mask & 4 != 0 {
        target.bottom = source.bottom;
    }
    if mask & 8 != 0 {
        target.left = source.left;
    }
}
/// Take a setter's value; a theme token stands for a placeholder value the
/// setter writes as any other, and the property is bound to the token (its
/// value is written when the node is styled). A later value replaces the
/// binding. One path through the setter keeps chained setters moving the
/// style in place.
macro_rules! value_or_token {
    ($self:ident, $value:ident, $field:expr) => {
        let ($value, token) = crate::theme::split($value.into());
        if token.is_some() || $self.styles_mut().tokens.is_some() {
            $self = crate::theme::rebound($self, $field, token);
        }
    };
}
/// Shared fluent styling API for views and reusable style patches.
///
/// Colours, lengths, typography and shadows take a value or a theme token
/// (`.bg(tok::SURFACE_RAISED)`, `.p(tok::SPACE_3)`; see [`crate::theme`]).
pub trait Styled: Sized {
    /// The styles the builders set.
    fn styles_mut(&mut self) -> &mut Styles;
    /// Set `field` from the theme token `token` (what the typed setters do
    /// for tokens; scripts use it for `"$name"` values).
    fn token(
        mut self,
        field: crate::theme::Field,
        token: impl Into<crate::theme::TokenId>,
    ) -> Self {
        crate::theme::bind_in(self.styles_mut(), field, token.into());
        self
    }
    /// Set the colour `field` to `t` of the way from the token `from` to
    /// `to` (a colour or a token), both read from the node's theme: an
    /// animation between theme colours that follows theme changes.
    fn token_mix(
        mut self,
        field: crate::theme::Field,
        from: impl Into<crate::theme::TokenId>,
        to: impl Into<crate::theme::Themed<Color>>,
        t: f32,
    ) -> Self {
        crate::theme::bind_mix(self.styles_mut(), field, from.into(), to.into(), t);
        self
    }
    /// Restore flex display after `hidden` or `grid`, preserving its direction.
    fn flex(mut self) -> Self {
        self.styles_mut().layout_options_mut().display = Some(crate::layout::Display::Flex);
        self
    }
    /// Merge advanced layout options without changing unrelated style properties.
    fn layout_options(mut self, options: crate::layout::LayoutOptions) -> Self {
        self.styles_mut().layout_options_mut().merge(options);
        self
    }
    /// Lay the children out in a grid (CSS `display: grid`; see `grid_cols`).
    fn grid(mut self) -> Self {
        self.styles_mut().layout_options_mut().display = Some(crate::layout::Display::Grid);
        self
    }
    /// Remove the view from layout and interaction (CSS `display: none`); it
    /// keeps its component state. `flex` or `grid` shows it again.
    fn hidden(mut self) -> Self {
        self.styles_mut().layout_options_mut().display = Some(crate::layout::Display::None);
        self
    }
    /// Paint the view again after `invisible`.
    fn visible(mut self) -> Self {
        self.styles_mut().layout_options_mut().visible = Some(true);
        self
    }
    /// Keep the view's room in layout but paint none of it and take no
    /// input (CSS `visibility: hidden`).
    fn invisible(mut self) -> Self {
        self.styles_mut().layout_options_mut().visible = Some(false);
        self
    }
    /// Wrap the children onto more lines when they overflow the row or
    /// column (CSS `flex-wrap: wrap`).
    fn flex_wrap(mut self) -> Self {
        self.styles_mut().layout_options_mut().wrap = Some(crate::layout::FlexWrap::Wrap);
        self
    }
    /// Wrap, with each new line before the last (`flex-wrap: wrap-reverse`).
    fn flex_wrap_reverse(mut self) -> Self {
        self.styles_mut().layout_options_mut().wrap = Some(crate::layout::FlexWrap::Reverse);
        self
    }
    /// Keep the children on one line (`flex-wrap: nowrap`).
    fn flex_nowrap(mut self) -> Self {
        self.styles_mut().layout_options_mut().wrap = Some(crate::layout::FlexWrap::NoWrap);
        self
    }
    /// Lay the children out in a row, last first (`flex-direction:
    /// row-reverse`).
    fn flex_row_reverse(mut self) -> Self {
        self.styles_mut().layout = Some(Layout::Row);
        self.styles_mut().layout_options_mut().reverse = Some(true);
        self
    }
    /// Lay the children out in a column, last first (`flex-direction:
    /// column-reverse`).
    fn flex_col_reverse(mut self) -> Self {
        self.styles_mut().layout = Some(Layout::Column);
        self.styles_mut().layout_options_mut().reverse = Some(true);
        self
    }
    /// The size the view starts from along its parent's main axis before
    /// growing or shrinking (CSS `flex-basis`).
    fn flex_basis(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().basis = Some(value.into());
        self
    }
    /// Align this child across its parent's axis, overriding the parent's
    /// `items_*` (CSS `align-self`).
    fn align_self(mut self, value: Align) -> Self {
        self.styles_mut().layout_options_mut().align_self = Some(value);
        self
    }
    /// How a wrapping container spaces its lines across its axis (CSS
    /// `align-content`).
    fn align_content(mut self, value: crate::layout::ContentAlign) -> Self {
        self.styles_mut().layout_options_mut().align_content = Some(value);
        self
    }
    /// The room between columns, a length or percentage (CSS `column-gap`).
    fn gap_x(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().gap_x = Some(value.into());
        self
    }
    /// The room between rows, a length or percentage (CSS `row-gap`).
    fn gap_y(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().gap_y = Some(value.into());
        self
    }
    /// Width over height: the missing dimension follows from the other (CSS
    /// `aspect-ratio`).
    fn aspect_ratio(mut self, value: f32) -> Self {
        self.styles_mut().layout_options_mut().aspect_ratio = Some(value);
        self
    }
    /// A grid's number of equal columns (at least 1).
    fn grid_cols(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().columns = Some(value.max(1));
        self
    }
    /// A grid's number of equal rows (at least 1).
    fn grid_rows(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().rows = Some(value.max(1));
        self
    }
    /// The grid column this child starts at, from 1; negative counts from
    /// the end (CSS `grid-column-start`).
    fn col_start(mut self, value: i16) -> Self {
        self.styles_mut().layout_options_mut().column_start = Some(value);
        self
    }
    /// The grid row this child starts at, from 1; negative counts from the
    /// end (CSS `grid-row-start`).
    fn row_start(mut self, value: i16) -> Self {
        self.styles_mut().layout_options_mut().row_start = Some(value);
        self
    }
    /// How many grid columns this child spans (at least 1).
    fn col_span(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().column_span = Some(value.max(1));
        self
    }
    /// How many grid rows this child spans (at least 1).
    fn row_span(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().row_span = Some(value.max(1));
        self
    }
    /// Span every grid column (CSS `grid-column: 1 / -1`).
    fn col_span_full(mut self) -> Self {
        self.styles_mut().layout_options_mut().column_full = Some(true);
        self
    }
    /// Span every grid row (CSS `grid-row: 1 / -1`).
    fn row_span_full(mut self) -> Self {
        self.styles_mut().layout_options_mut().row_full = Some(true);
        self
    }
    /// The smallest width, as a percentage of the parent's content width.
    fn min_w_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().min_width =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// The largest width, as a percentage of the parent's content width.
    fn max_w_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().max_width =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// The smallest height, as a percentage of the parent's content height.
    fn min_h_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().min_height =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// The largest height, as a percentage of the parent's content height.
    fn max_h_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().max_height =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An `absolute` view's inset from its containing block's top edge.
    fn top(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.top = Some(value.into());
        self
    }
    /// An `absolute` view's inset from its containing block's right edge.
    fn right(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.right = Some(value.into());
        self
    }
    /// An `absolute` view's inset from its containing block's bottom edge.
    fn bottom(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.bottom = Some(value.into());
        self
    }
    /// An `absolute` view's inset from its containing block's left edge.
    fn left(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.left = Some(value.into());
        self
    }
    /// Padding on every side, as a percentage of the parent's content width
    /// (as CSS resolves percentage padding).
    fn p_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Left and right padding, as a percentage of the parent's content width.
    fn px_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Top and bottom padding, as a percentage of the parent's content width.
    fn py_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Top padding, as a percentage of the parent's content width.
    fn pt_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.top = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Right padding, as a percentage of the parent's content width.
    fn pr_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.right = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Bottom padding, as a percentage of the parent's content width.
    fn pb_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Left padding, as a percentage of the parent's content width.
    fn pl_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// A margin on every side, as a percentage of the parent's content width.
    fn m_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Automatic margins on every side: the view takes its parent's free
    /// space around it, centring it (CSS `margin: auto`).
    fn m_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Auto);
        options.margin.right = Some(crate::layout::Length::Auto);
        options.margin.bottom = Some(crate::layout::Length::Auto);
        options.margin.left = Some(crate::layout::Length::Auto);
        self
    }
    /// Left and right margins, as a percentage of the parent's content width.
    fn mx_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Automatic left and right margins: the view is centred across.
    fn mx_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Auto);
        options.margin.left = Some(crate::layout::Length::Auto);
        self
    }
    /// Top and bottom margins, as a percentage of the parent's content width.
    fn my_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Automatic top and bottom margins: centred down.
    fn my_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Auto);
        options.margin.bottom = Some(crate::layout::Length::Auto);
        self
    }
    /// A top margin, as a percentage of the parent's content width.
    fn mt_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic top margin: the free space goes above the view, pushing
    /// it to the bottom.
    fn mt_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Auto);
        self
    }
    /// A right margin, as a percentage of the parent's content width.
    fn mr_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic right margin: the free space goes after the view,
    /// pushing what follows it to the end.
    fn mr_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Auto);
        self
    }
    /// A bottom margin, as a percentage of the parent's content width.
    fn mb_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic bottom margin: the free space goes below the view.
    fn mb_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.bottom = Some(crate::layout::Length::Auto);
        self
    }
    /// A left margin, as a percentage of the parent's content width.
    fn ml_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic left margin: the free space goes before the view,
    /// pushing it to the end.
    fn ml_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.left = Some(crate::layout::Length::Auto);
        self
    }
    /// Take the view out of normal flow and position it against its
    /// containing block: the nearest ancestor that is `relative`, found
    /// without passing an `absolute` one; with none, its parent. `left`,
    /// `top`, `right` and `bottom` count from that block's padding box
    /// (inside its border), percentages of its size; without insets it
    /// sits at its parent's content origin plus margins. Absolute views do
    /// not contribute to their parent's intrinsic size or flex space, and
    /// still paint in their parent's place, under its clip.
    fn absolute(mut self) -> Self {
        self.styles_mut().absolute = Some(true);
        self
    }
    /// Make the view the containing block of its `absolute` descendants
    /// (CSS `position: relative`), and keep it in normal flow (undoing
    /// `absolute`). It changes nothing about its own layout.
    fn relative(mut self) -> Self {
        self.styles_mut().absolute = Some(false);
        self.styles_mut().rare_mut().relative = Some(true);
        self
    }
    /// Apply a reusable transformation while building a view or style patch.
    fn apply(self, transform: impl FnOnce(Self) -> Self) -> Self {
        transform(self)
    }
    /// Apply a transformation only when `condition` is true.
    ///
    /// This decision is made immediately, not reactively. Use a view's
    /// `reactive_style` method for styles that follow signal changes.
    fn when(self, condition: bool, transform: impl FnOnce(Self) -> Self) -> Self {
        if condition { transform(self) } else { self }
    }
    /// Apply a transformation with the contained value, if present.
    ///
    /// Like `when`, this runs while building the description, without creating
    /// a subscription. The closure is not called for `None`.
    fn when_some<T>(self, value: Option<T>, transform: impl FnOnce(Self, T) -> Self) -> Self {
        match value {
            Some(value) => transform(self, value),
            None => self,
        }
    }
    /// Merge `style`'s set properties over these.
    fn style(mut self, style: Styles) -> Self {
        self.styles_mut().merge(&style);
        self
    }
    /// Set centered image fitting within its padded content box. Not inherited.
    fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.styles_mut().object_fit = Some(fit);
        self
    }
    /// Fit the image inside its box, keeping its aspect ratio
    /// ([`ObjectFit::Contain`]).
    fn object_contain(self) -> Self {
        self.object_fit(ObjectFit::Contain)
    }
    /// Fill the box with the image, keeping its aspect ratio and cropping
    /// the rest ([`ObjectFit::Cover`]).
    fn object_cover(self) -> Self {
        self.object_fit(ObjectFit::Cover)
    }
    /// The width in points, or a theme token; replaces `w_percent`.
    fn w(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Width);
        self.styles_mut().width = Some(value);
        self.styles_mut().width_percent = None;
        self
    }
    /// The height in points, or a theme token; replaces `h_percent`.
    fn h(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Height);
        self.styles_mut().height = Some(value);
        self.styles_mut().height_percent = None;
        self
    }
    /// Width as a percentage of the parent's definite content width; 100 fills it.
    /// Indefinite parent widths use intrinsic sizing. Flex allocation and min/max
    /// constraints still apply. Negative and non-finite values normalize to zero.
    fn w_percent(mut self, percent: f32) -> Self {
        self.styles_mut().width = None;
        self.styles_mut().width_percent = Some(if percent.is_finite() {
            percent.max(0.) / 100.
        } else {
            0.
        });
        self
    }
    /// Height as a percentage of the parent's definite content height.
    /// Uses the same normalization and intrinsic fallback as `w_percent`.
    fn h_percent(mut self, percent: f32) -> Self {
        self.styles_mut().height = None;
        self.styles_mut().height_percent = Some(if percent.is_finite() {
            percent.max(0.) / 100.
        } else {
            0.
        });
        self
    }
    /// Fill the parent's definite content width, subject to flex and min/max rules.
    fn w_full(self) -> Self {
        self.w_percent(100.)
    }
    /// Fill the parent's definite content height, subject to flex and min/max rules.
    fn h_full(self) -> Self {
        self.h_percent(100.)
    }
    /// The smallest width in points (CSS `min-width`); replaces
    /// `min_w_percent`. `min_w(0.)` lets a flex child shrink below its
    /// content.
    fn min_w(mut self, value: f32) -> Self {
        if let Some(options) = &mut self.styles_mut().layout_options {
            std::sync::Arc::make_mut(options).min_width = None;
        }
        self.styles_mut().rare_mut().min_width = Some(value);
        self
    }
    /// The largest width in points (CSS `max-width`); replaces
    /// `max_w_percent`.
    fn max_w(mut self, value: f32) -> Self {
        if let Some(options) = &mut self.styles_mut().layout_options {
            std::sync::Arc::make_mut(options).max_width = None;
        }
        self.styles_mut().rare_mut().max_width = Some(value);
        self
    }
    /// The smallest height in points (CSS `min-height`); replaces
    /// `min_h_percent`.
    fn min_h(mut self, value: f32) -> Self {
        if let Some(options) = &mut self.styles_mut().layout_options {
            std::sync::Arc::make_mut(options).min_height = None;
        }
        self.styles_mut().rare_mut().min_height = Some(value);
        self
    }
    /// The largest height in points (CSS `max-height`); replaces
    /// `max_h_percent`.
    fn max_h(mut self, value: f32) -> Self {
        if let Some(options) = &mut self.styles_mut().layout_options {
            std::sync::Arc::make_mut(options).max_height = None;
        }
        self.styles_mut().rare_mut().max_height = Some(value);
        self
    }
    /// The room between children, both ways, in points or a theme token;
    /// replaces `gap_x` and `gap_y`.
    fn gap(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Gap);
        if let Some(options) = &mut self.styles_mut().layout_options {
            let options = std::sync::Arc::make_mut(options);
            options.gap_x = None;
            options.gap_y = None;
        }
        self.styles_mut().gap = Some(value);
        self
    }
    /// How much of its parent's free space this child takes, in proportion
    /// to its siblings' (CSS `flex-grow`).
    fn flex_grow(mut self, value: f32) -> Self {
        self.styles_mut().flex_grow = Some(value);
        self
    }
    /// How much this child gives up when its parent is short of space, in
    /// proportion to its siblings' (CSS `flex-shrink`; 1 unless set). It
    /// shrinks no smaller than its content unless `min_w`/`min_h` allow.
    fn flex_shrink(mut self, value: f32) -> Self {
        self.styles_mut().flex_shrink = Some(value);
        self
    }
    /// Fill the background with `brush` (a colour, or a gradient between two
    /// points) instead of the `bg` colour.
    fn bg_fill(mut self, brush: crate::canvas::Brush) -> Self {
        self.styles_mut().paint_background =
            Some(Some(crate::decoration::Background::Brush(brush)));
        self
    }
    /// Fill the background with a linear gradient through `stops`, at `angle`
    /// clockwise radians, its ends spanning the box (CSS `linear-gradient`).
    fn bg_gradient(
        mut self,
        angle: f32,
        stops: impl Into<std::sync::Arc<[crate::canvas::GradientStop]>>,
    ) -> Self {
        self.styles_mut().paint_background = Some(Some(crate::decoration::Background::Linear {
            angle,
            stops: stops.into(),
        }));
        self
    }
    /// A background image filling the view as `fit` places it (centred),
    /// over its `bg` colour, cut to its rounded corners, under its border
    /// and children. Set after `bg`, which clears it.
    fn bg_image(
        mut self,
        image: std::sync::Arc<crate::image::ImageData>,
        fit: crate::style::ObjectFit,
    ) -> Self {
        self.styles_mut().paint_background =
            Some(Some(crate::decoration::Background::Image { image, fit }));
        self
    }
    /// Fill the view with a shader (see `crate::decoration::FillShader`):
    /// a WGSL `fill` function run per pixel on the GPU, over its `bg`
    /// colour, cut to its rounded corners, under its border and children.
    /// Set after `bg`, which clears it.
    fn bg_shader(mut self, shader: std::sync::Arc<crate::decoration::FillShader>) -> Self {
        self.styles_mut().paint_background =
            Some(Some(crate::decoration::Background::Shader(shader)));
        self
    }
    /// The uniforms its `bg_shader` draws with: time and user parameters.
    /// Animating them per frame repaints the view alone (or set them with
    /// `Scene::set_shader_uniforms` directly, skipping style resolution).
    fn shader_uniforms(mut self, uniforms: crate::decoration::ShaderUniforms) -> Self {
        self.styles_mut().rare_mut().shader_uniforms = Some(uniforms);
        self
    }
    /// Round each corner by its own radius; replaces `rounded`.
    fn rounded_corners(mut self, corners: crate::decoration::Corners) -> Self {
        self.styles_mut().corners = Some(Some(corners));
        self
    }
    /// A border with its own width on each side; replaces `border`.
    fn border_edges(mut self, widths: Insets) -> Self {
        self.styles_mut().border_edges = Some(Some(widths));
        self
    }
    /// Paint the border over the view's content, as an inset stroke (a CSS
    /// outline drawn inside): a photo filling a framed thumbnail stays
    /// under its frame. Solid borders only; a dashed one paints with the fill.
    fn border_over(mut self, value: bool) -> Self {
        self.styles_mut().rare_mut().border_over = Some(value);
        self
    }
    /// An outline (CSS `outline`): a ring `width` wide outside the box at
    /// `outline_offset` (0 by default), its corners the box's radii grown
    /// alike, over the view's content, taking no room in layout and not cut
    /// by its own `overflow_hidden`.
    fn outline(mut self, width: f32, color: Color) -> Self {
        self.styles_mut().rare_mut().outline = Some(Some((width, color)));
        self
    }
    /// The outline's colour, a value or a theme token, keeping its width
    /// (1 point if none was set).
    fn outline_color(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::OutlineColor);
        let width = match self.styles_mut().rare_mut().outline {
            Some(Some((width, _))) => width,
            _ => 1.,
        };
        self.styles_mut().rare_mut().outline = Some(Some((width, value)));
        self
    }
    /// The outline's width, keeping its colour (the text colour's default
    /// black if none was set).
    fn outline_width(mut self, width: f32) -> Self {
        let color = match self.styles_mut().rare_mut().outline {
            Some(Some((_, color))) => color,
            _ => Color(0, 0, 0, 255),
        };
        self.styles_mut().rare_mut().outline = Some(Some((width.max(0.), color)));
        self
    }
    /// How far outside the box the outline starts (CSS `outline-offset`);
    /// negative draws it over the box's edge.
    fn outline_offset(mut self, offset: f32) -> Self {
        self.styles_mut().rare_mut().outline_offset = Some(offset);
        self
    }
    /// No outline.
    fn outline_none(mut self) -> Self {
        self.styles_mut().rare_mut().outline = Some(None);
        self
    }
    /// A design tool's stroke: `width` of `color` aligned inside the box (a
    /// border over its content), centred on its edge or outside it (an
    /// outline at offset `-width / 2` or 0).
    fn stroke(self, width: f32, color: Color, align: StrokeAlign) -> Self {
        match align {
            StrokeAlign::Inside => self.border(width).border_color(color).border_over(true),
            StrokeAlign::Center => self.outline(width, color).outline_offset(-width / 2.),
            StrokeAlign::Outside => self.outline(width, color).outline_offset(0.),
        }
    }
    /// Draw the border solid or dashed.
    fn border_style(mut self, style: crate::decoration::BorderStyle) -> Self {
        self.styles_mut().rare_mut().border_style = Some(style);
        self
    }
    /// Shadows under the box, in order (CSS `box-shadow`); replaces `shadow`.
    fn shadows(mut self, shadows: impl Into<std::sync::Arc<[BoxShadow]>>) -> Self {
        self.styles_mut().rare_mut().shadows = Some(Some(shadows.into()));
        self
    }
    /// Shadows inside the box (CSS's `inset`): see `Decoration::inset_shadows`.
    fn inset_shadows(mut self, shadows: impl Into<std::sync::Arc<[BoxShadow]>>) -> Self {
        self.styles_mut().rare_mut().inset_shadows = Some(Some(shadows.into()));
        self
    }
    /// The pointer's shape over the view.
    fn cursor(mut self, cursor: crate::cursor::Cursor) -> Self {
        self.styles_mut().cursor = Some(cursor);
        self
    }
    /// The background colour, or a theme token; clears `bg_fill`,
    /// `bg_gradient`, `bg_image` and `bg_shader`.
    fn bg(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Background);
        self.styles_mut().background = Some(value);
        self.styles_mut().paint_background = Some(None);
        self
    }
    /// The text colour, or a theme token. Inherited.
    fn text_color(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::TextColor);
        self.styles_mut().text_color = Some(value);
        self
    }
    /// Ordered explicit font families tried before platform fallback.
    fn font_fallbacks(
        mut self,
        families: impl Into<std::sync::Arc<[crate::text_layout::FontFamily]>>,
    ) -> Self {
        self.styles_mut().rare_mut().font_fallbacks = Some(families.into());
        self
    }
    /// How text past its last line or its width ends: cut, or with an
    /// ellipsis.
    fn text_overflow(mut self, overflow: crate::text_layout::TextOverflow) -> Self {
        self.styles_mut().rare_mut().text_overflow = Some(overflow);
        self
    }
    /// Clamp displayed visual lines; zero removes an inherited clamp.
    fn line_clamp(mut self, lines: u32) -> Self {
        self.styles_mut().rare_mut().line_clamp = Some(std::num::NonZeroU32::new(lines));
        self
    }
    /// One line, ending with an ellipsis if it doesn't fit.
    fn truncate(self) -> Self {
        self.line_clamp(1)
            .text_overflow(crate::text_layout::TextOverflow::Ellipsis)
    }
    /// How lines of text align in their box: at the line's start, left,
    /// centred or right. Inherited.
    fn text_align(mut self, alignment: crate::text_layout::TextAlign) -> Self {
        self.styles_mut().text_align = Some(alignment);
        self
    }
    /// Override inherited OpenType features; an empty set restores font defaults.
    fn font_features(mut self, features: crate::text_layout::FontFeatures) -> Self {
        self.styles_mut().rare_mut().font_features = Some(features);
        self
    }
    /// The font family, or a theme token. Inherited.
    fn font_family(
        mut self,
        family: impl Into<crate::theme::Themed<crate::text_layout::FontFamily>>,
    ) -> Self {
        value_or_token!(self, family, crate::theme::Field::FontFamily);
        self.styles_mut().font_family = Some(family);
        self
    }
    /// The font weight, from 1 to 1000 (400 regular, 700 bold), or a theme
    /// token. Inherited.
    fn font_weight(mut self, weight: impl Into<crate::theme::Themed<u16>>) -> Self {
        value_or_token!(self, weight, crate::theme::Field::FontWeight);
        self.styles_mut().font_weight = Some(weight.clamp(1, 1000));
        self
    }
    /// Bold text (weight 700).
    fn font_bold(self) -> Self {
        self.font_weight(700)
    }
    /// Italic text, or upright. Inherited.
    fn italic(mut self, italic: bool) -> Self {
        self.styles_mut().italic = Some(italic);
        self
    }
    /// The font size in points, or a theme token. Inherited.
    fn text_size(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::TextSize);
        self.styles_mut().text_size = Some(value);
        self
    }
    /// Set inherited line spacing in logical pixels. Invalid/non-positive values
    /// use normal spacing; positive values smaller than the font size are allowed.
    fn line_height(mut self, pixels: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, pixels, crate::theme::Field::LineHeight);
        self.styles_mut().line_height = Some(crate::text_layout::LineHeight::px(pixels));
        self
    }
    /// Reset inherited line spacing to the normal metric for each text size.
    fn line_height_normal(mut self) -> Self {
        self.styles_mut().line_height = Some(crate::text_layout::LineHeight::NORMAL);
        self
    }
    /// Inherited extra glyph advance in logical pixels. Zero resets inherited tracking.
    /// Non-finite values normalize to zero; finite values clamp to ±1,000,000 pixels.
    fn letter_spacing(mut self, pixels: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, pixels, crate::theme::Field::LetterSpacing);
        self.styles_mut().rare_mut().letter_spacing =
            Some(crate::text_layout::LetterSpacing::px(pixels));
        self
    }

    /// Underlined text, or not; inherited, like CSS's `text-decoration`
    /// (a hover style can underline a link's label).
    fn text_underline(mut self, on: bool) -> Self {
        self.styles_mut().rare_mut().underline = Some(on);
        self
    }
    /// How far below the baseline an underline sits (CSS's
    /// `text-underline-offset`; unset, a tenth of the font size). Inherited.
    fn underline_offset(mut self, px: f32) -> Self {
        self.styles_mut().rare_mut().underline_offset = px.is_finite().then_some(px);
        self
    }

    /// Round every corner by this radius in points, or a theme token;
    /// replaces `rounded_corners`.
    fn rounded(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Radius);
        self.styles_mut().radius = Some(value);
        self.styles_mut().corners = Some(None);
        self
    }
    /// A border this wide on every side, in points or a theme token;
    /// replaces `border_edges`.
    fn border(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::BorderWidth);
        self.styles_mut().border_width = Some(value);
        self.styles_mut().border_edges = Some(None);
        self
    }
    /// The border's colour, or a theme token.
    fn border_color(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::BorderColor);
        self.styles_mut().border_color = Some(value);
        self
    }
    /// The view's opacity, 0 to 1, or a theme token. The subtree composites
    /// item by item unless `isolated`.
    fn opacity(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Opacity);
        self.styles_mut().opacity = Some(value);
        self
    }
    /// Backdrop blur (CSS `backdrop-filter: blur()`): what lies behind the
    /// node, within its bounds, blurs by this sigma in logical pixels, as
    /// frosted glass; the node's own content stays sharp. For blurring the
    /// node's content itself, see `filter_blur`.
    fn blur(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Blur);
        self.styles_mut().blur = Some(value);
        self
    }
    /// Content blur (CSS `filter: blur()`): the node and its subtree blur by
    /// this sigma in logical pixels, spreading past its bounds, under its
    /// opacity. The subtree paints once into a cached layer; animating the
    /// sigma (a fade in from blurred text) only composites it again. Unlike
    /// `blur`, what lies behind is untouched.
    fn filter_blur(mut self, value: f32) -> Self {
        self.styles_mut().filter_blur = Some(value);
        self
    }
    /// CSS `filter: brightness()`: the view and its subtree, as a group,
    /// with colours scaled (1 leaves them, 0 is black, past 1 brighter), as
    /// a button brightens on hover. Applied as the GPU writes each colour:
    /// no layer, and animating it repaints only the view's area.
    fn brightness(mut self, value: f32) -> Self {
        self.styles_mut().brightness = Some(value);
        self
    }
    /// CSS `filter: contrast()`: colours pushed from (or, below 1, towards)
    /// mid grey.
    fn contrast(mut self, value: f32) -> Self {
        self.styles_mut().contrast = Some(value);
        self
    }
    /// CSS `filter: saturate()`: 0 is greyscale, past 1 more vivid.
    fn saturate(mut self, value: f32) -> Self {
        self.styles_mut().saturate = Some(value);
        self
    }
    /// Show the node's content through `image`'s alpha (CSS `mask-image`,
    /// no repeat, nearest sampling), drawn `size` (logical pixels) large at
    /// `mask_offset` from the node's origin; outside the image nothing shows.
    /// The subtree paints once into a cached layer, so moving the mask (a
    /// shimmer, a progress fill) only composites it again.
    fn mask(mut self, image: std::sync::Arc<crate::image::ImageData>, size: (f32, f32)) -> Self {
        self.styles_mut().rare_mut().mask = Some(Some((image, size)));
        self
    }
    /// Clip the view and its descendants to its box inset by these pixels
    /// (CSS `clip-path: inset(top right bottom left)`); negative ones reach
    /// past it, so shadows and rings aren't cut. Animating it composites
    /// again without layout. With `overflow_hidden` and rounded corners,
    /// the inset box is rounded alike.
    fn clip_inset(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.styles_mut().clip_inset = Some(crate::scene::ClipInset::px(top, right, bottom, left));
        self
    }
    /// `clip_inset` in percent of the view's own size (top and bottom of its
    /// height, left and right of its width), following it as it resizes:
    /// `clip_inset_percent(0., 0., 100., 0.)` closes it to its top edge.
    fn clip_inset_percent(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        use crate::layout::Length::Percent;
        self.styles_mut().clip_inset = Some(crate::scene::ClipInset {
            top: Percent(top / 100.),
            right: Percent(right / 100.),
            bottom: Percent(bottom / 100.),
            left: Percent(left / 100.),
        });
        self
    }
    /// `clip_inset` with each side's length (pixels or percent) as given.
    fn clip_inset_lengths(mut self, inset: crate::scene::ClipInset) -> Self {
        self.styles_mut().clip_inset = Some(inset);
        self
    }
    /// Where `mask`'s image lies, from the node's laid-out origin.
    fn mask_offset(mut self, x: f32, y: f32) -> Self {
        self.styles_mut().mask_offset = Some((x, y));
        self
    }
    /// Fade the view's top and bottom edges out over `value` points.
    fn edge_fade(mut self, value: f32) -> Self {
        self.styles_mut().edge_fade = Some(value);
        self
    }
    /// Render the subtree as a group into a cached layer, then composite it
    /// once: `isolated(true).opacity(0.5)` fades overlapping children
    /// together.
    fn isolated(mut self, value: bool) -> Self {
        self.styles_mut().isolated = Some(value);
        self
    }
    /// Wrap text at the content width (true), or keep each line whole.
    /// Inherited.
    fn text_wrap(mut self, value: bool) -> Self {
        self.styles_mut().text_wrap = Some(value);
        self
    }
    /// `w(width).h(height)`.
    fn size(self, width: f32, height: f32) -> Self {
        self.w(width).h(height)
    }
    /// Padding on every side, in points or a theme token.
    fn p(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(15));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            15,
        );
        style.padding_sides |= 15;
        style.clear_layout_spacing(false, 15);
        self
    }
    /// Left and right padding, in points or a theme token.
    fn px(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(10));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            10,
        );
        style.padding_sides |= 10;
        style.clear_layout_spacing(false, 10);
        self
    }
    /// Top and bottom padding, in points or a theme token.
    fn py(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(5));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            5,
        );
        style.padding_sides |= 5;
        style.clear_layout_spacing(false, 5);
        self
    }
    /// Top padding, in points or a theme token.
    fn pt(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(1));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            1,
        );
        style.padding_sides |= 1;
        style.clear_layout_spacing(false, 1);
        self
    }
    /// Right padding, in points or a theme token.
    fn pr(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(2));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            2,
        );
        style.padding_sides |= 2;
        style.clear_layout_spacing(false, 2);
        self
    }
    /// Bottom padding, in points or a theme token.
    fn pb(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(4));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            4,
        );
        style.padding_sides |= 4;
        style.clear_layout_spacing(false, 4);
        self
    }
    /// Left padding, in points or a theme token.
    fn pl(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(8));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.padding_sides = 15;
        }
        merge_insets(
            style.padding.get_or_insert_with(Insets::default),
            Insets::all(value),
            8,
        );
        style.padding_sides |= 8;
        style.clear_layout_spacing(false, 8);
        self
    }
    /// A margin on every side, in points or a theme token.
    fn m(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(15));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            15,
        );
        style.margin_sides |= 15;
        style.clear_layout_spacing(true, 15);
        self
    }
    /// Left and right margins, in points or a theme token.
    fn mx(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(10));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            10,
        );
        style.margin_sides |= 10;
        style.clear_layout_spacing(true, 10);
        self
    }
    /// Top and bottom margins, in points or a theme token.
    fn my(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(5));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            5,
        );
        style.margin_sides |= 5;
        style.clear_layout_spacing(true, 5);
        self
    }
    /// A top margin, in points or a theme token.
    fn mt(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(1));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            1,
        );
        style.margin_sides |= 1;
        style.clear_layout_spacing(true, 1);
        self
    }
    /// A right margin, in points or a theme token.
    fn mr(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(2));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            2,
        );
        style.margin_sides |= 2;
        style.clear_layout_spacing(true, 2);
        self
    }
    /// A bottom margin, in points or a theme token.
    fn mb(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(4));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            4,
        );
        style.margin_sides |= 4;
        style.clear_layout_spacing(true, 4);
        self
    }
    /// A left margin, in points or a theme token.
    fn ml(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(8));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.margin_sides = 15;
        }
        merge_insets(
            style.margin.get_or_insert_with(Insets::default),
            Insets::all(value),
            8,
        );
        style.margin_sides |= 8;
        style.clear_layout_spacing(true, 8);
        self
    }
    /// Lay the children out in a row, first first.
    fn flex_row(mut self) -> Self {
        if let Some(options) = &mut self.styles_mut().layout_options {
            std::sync::Arc::make_mut(options).reverse = None;
        }
        self.styles_mut().layout = Some(Layout::Row);
        self
    }
    /// Lay the children out in a column, first first.
    fn flex_col(mut self) -> Self {
        if let Some(options) = &mut self.styles_mut().layout_options {
            std::sync::Arc::make_mut(options).reverse = None;
        }
        self.styles_mut().layout = Some(Layout::Column);
        self
    }
    /// Stack the children in one area (see [`crate::compose::overlay`]).
    fn overlay(mut self) -> Self {
        self.styles_mut().layout = Some(Layout::Overlay);
        self
    }
    /// Align the children at the start across the axis (CSS `align-items:
    /// flex-start`).
    fn items_start(mut self) -> Self {
        self.styles_mut().align = Some(Align::Start);
        self
    }
    /// Centre the children across the axis.
    fn items_center(mut self) -> Self {
        self.styles_mut().align = Some(Align::Center);
        self
    }
    /// Align a row's children by their first lines' baselines.
    fn items_baseline(mut self) -> Self {
        self.styles_mut().align = Some(Align::Baseline);
        self
    }
    /// Align the children at the end across the axis.
    fn items_end(mut self) -> Self {
        self.styles_mut().align = Some(Align::End);
        self
    }
    /// Stretch the children across the axis, unless they size themselves on
    /// it (a column's default).
    fn items_stretch(mut self) -> Self {
        self.styles_mut().align = Some(Align::Stretch);
        self
    }
    /// Pack the children at the start of the axis.
    fn justify_start(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::Start);
        self
    }
    /// Centre the children along the axis.
    fn justify_center(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::Center);
        self
    }
    /// Pack the children at the end of the axis.
    fn justify_end(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::End);
        self
    }
    /// Spread the free space between the children, none at the ends.
    fn justify_between(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::SpaceBetween);
        self
    }
    /// Spread the free space evenly between the children and the ends.
    fn justify_evenly(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::SpaceEvenly);
        self
    }
    /// Spread the free space around each child: half as much at the ends as
    /// between.
    fn justify_around(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::SpaceAround);
        self
    }
    /// An overlay's children fill its height (its default, with
    /// `items_stretch` across). Along a row or column, the same as
    /// `justify_start`.
    fn justify_stretch(mut self) -> Self {
        self.styles_mut().justify = Some(Justify::Stretch);
        self
    }
    /// Fade content out over `top` and `bottom` bands at the edges of this
    /// node's clip (with `overflow_hidden` or a scroll view), per pixel.
    fn fade_edges(mut self, top: f32, bottom: f32) -> Self {
        self.styles_mut().rare_mut().fade_edges = Some([top, bottom]);
        self
    }
    /// End `fade_edges`' bands this far in from the clip's top and bottom
    /// edges, hiding content past them: a scroll view running on under a bar
    /// fades out by the bar's edge.
    fn fade_insets(mut self, top: f32, bottom: f32) -> Self {
        self.styles_mut().rare_mut().fade_insets = Some([top, bottom]);
        self
    }
    /// `fade_edges` at the left and right edges: a horizontally scrolling
    /// row fading the overflow at its right edge.
    fn fade_edges_x(mut self, left: f32, right: f32) -> Self {
        self.styles_mut().rare_mut().fade_edges_x = Some([left, right]);
        self
    }
    /// `fade_insets` at the left and right edges.
    fn fade_insets_x(mut self, left: f32, right: f32) -> Self {
        self.styles_mut().rare_mut().fade_insets_x = Some([left, right]);
        self
    }
    /// Fade an edge only as far as content runs past it: a scroll view at
    /// its top shows no top band, which grows to its width as the content
    /// scrolls under that edge.
    fn fade_overflow(mut self, value: bool) -> Self {
        self.styles_mut().rare_mut().fade_overflow = Some(value);
        self
    }
    /// Lay this node's edges on whole points after layout (percentages and
    /// ratios leave fractions, which blur a 1-point border across pixels);
    /// its size follows its rounded edges.
    fn snap(mut self, value: bool) -> Self {
        self.styles_mut().rare_mut().snap = Some(value);
        self
    }
    /// Which way lines run in it and what it holds (CSS's `direction`):
    /// right to left, rows run from the right, a column's children start at
    /// its right, text starts at the right, and the `ps`/`pe`, `ms`/`me`,
    /// `start`/`end` edges follow. `pl`, `ml` and `left` stay on the left.
    fn direction(mut self, value: crate::scene::Direction) -> Self {
        self.styles_mut().rare_mut().direction = Some(value);
        self
    }
    /// `direction(Direction::Rtl)`: Arabic, Hebrew, Persian, Urdu.
    fn rtl(self) -> Self {
        self.direction(crate::scene::Direction::Rtl)
    }
    /// Padding at the start of a line (CSS's `padding-inline-start`): the
    /// left left to right, the right right to left.
    fn ps(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().padding_inline.start = Some(value.into());
        self
    }
    /// Padding at the end of a line (`padding-inline-end`).
    fn pe(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().padding_inline.end = Some(value.into());
        self
    }
    /// A margin at the start of a line (`margin-inline-start`).
    fn ms(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().margin_inline.start = Some(value.into());
        self
    }
    /// A margin at the end of a line (`margin-inline-end`).
    fn me(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().margin_inline.end = Some(value.into());
        self
    }
    /// An absolute node's inset from the start of a line
    /// (`inset-inline-start`): `left` left to right, `right` right to left.
    fn start(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset_inline.start = Some(value.into());
        self
    }
    /// Its inset from the end of a line (`inset-inline-end`).
    fn end(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset_inline.end = Some(value.into());
        self
    }
    /// Fade all four edges (CSS order: top, right, bottom, left), each band
    /// independent; 0 leaves an edge sharp.
    fn fade_sides(self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.fade_edges(top, bottom).fade_edges_x(left, right)
    }
    /// Clip the subtree to the view's box, and to its rounded corners (CSS
    /// `overflow: hidden`).
    fn overflow_hidden(mut self) -> Self {
        self.styles_mut().clip = Some(true);
        if let Some(options) = &mut self.styles_mut().layout_options {
            let options = std::sync::Arc::make_mut(options);
            options.clip_x = None;
            options.clip_y = None;
        }
        self
    }
    /// Let the subtree paint past the view's box (the default).
    fn overflow_visible(mut self) -> Self {
        self.styles_mut().clip = Some(false);
        if let Some(options) = &mut self.styles_mut().layout_options {
            let options = std::sync::Arc::make_mut(options);
            options.clip_x = None;
            options.clip_y = None;
        }
        self
    }
    /// Clip the subtree at the view's left and right edges only.
    fn overflow_x_hidden(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_x = Some(true);
        self
    }
    /// Clip the subtree at the view's top and bottom edges only.
    fn overflow_y_hidden(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_y = Some(true);
        self
    }
    /// Let the subtree paint past the view's left and right edges.
    fn overflow_x_visible(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_x = Some(false);
        self
    }
    /// Let the subtree paint past the view's top and bottom edges.
    fn overflow_y_visible(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_y = Some(false);
        self
    }
    /// A 1-point border on every side.
    fn border_1(self) -> Self {
        self.border(1.)
    }
    /// `flex_grow(1.)`: take the parent's free space.
    fn grow(self) -> Self {
        self.flex_grow(1.)
    }
    /// `flex_shrink(0.)`: keep the view's size when its parent is short of
    /// space.
    fn shrink_0(self) -> Self {
        self.flex_shrink(0.)
    }
    /// One shadow under the box, or a theme token; replaces `shadows`.
    fn shadow(mut self, value: impl Into<crate::theme::Themed<BoxShadow>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Shadow);
        self.styles_mut().rare_mut().shadow = Some(Some(value));
        self.styles_mut().rare_mut().shadows = Some(None);
        self
    }
    /// No shadow.
    fn shadow_none(mut self) -> Self {
        self.styles_mut().rare_mut().shadow = Some(None);
        self.styles_mut().rare_mut().shadows = Some(None);
        self
    }
    /// Move the view by `x`, `y` points after layout, without moving its
    /// siblings: a compositor-only change.
    fn translate(mut self, x: f32, y: f32) -> Self {
        self.styles_mut().transform = Some(Transform { x, y });
        self
    }
    /// Scale uniformly about the transform origin, after layout: the
    /// subtree composites as a cached layer, so animating it repaints none
    /// of its content (see `Scene::set_local_transform`).
    fn scale(self, factor: f32) -> Self {
        self.scale_xy(factor, factor)
    }
    /// Scale by `x` across and `y` down about the transform origin, as
    /// `scale`.
    fn scale_xy(mut self, x: f32, y: f32) -> Self {
        let patch = &mut self.styles_mut().local_transform;
        patch.values.scale = (x, y);
        patch.set |= TransformPatch::SCALE;
        self
    }
    /// Rotate clockwise by `radians` about the transform origin, as `scale`.
    fn rotate(mut self, radians: f32) -> Self {
        let patch = &mut self.styles_mut().local_transform;
        patch.values.rotate = radians;
        patch.set |= TransformPatch::ROTATE;
        self
    }
    /// Skew by angles in radians, as CSS `skew(x, y)`.
    fn skew(mut self, x: f32, y: f32) -> Self {
        let patch = &mut self.styles_mut().local_transform;
        patch.values.skew = (x, y);
        patch.set |= TransformPatch::SKEW;
        self
    }
    /// The pivot of `scale`, `rotate` and `skew`, as fractions of the
    /// view's own size: (0.5, 0.5), its centre, by default.
    fn transform_origin(mut self, x: f32, y: f32) -> Self {
        let patch = &mut self.styles_mut().local_transform;
        patch.values.origin = (x, y);
        patch.set |= TransformPatch::ORIGIN;
        self
    }
}
impl Styled for Styles {
    fn styles_mut(&mut self) -> &mut Styles {
        self
    }
}
impl Styled for &mut Styles {
    fn styles_mut(&mut self) -> &mut Styles {
        self
    }
}
/// Opaque color from `0xRRGGBB`.
pub const fn rgb(value: u32) -> Color {
    Color((value >> 16) as u8, (value >> 8) as u8, value as u8, 255)
}
/// Color from `0xRRGGBBAA`, with alpha in the least significant byte.
pub const fn rgba(value: u32) -> Color {
    Color(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentage_patches_normalize_invalid_values_and_replace_pixel_axes() {
        let patch = Styles::new().size(80., 30.).w_percent(50.).h_full();
        assert_eq!(patch.width, None);
        assert_eq!(patch.height, None);
        assert_eq!(patch.width_percent, Some(0.5));
        assert_eq!(patch.height_percent, Some(1.));
        let pixels = patch.style(Styles::new().w(20.));
        assert_eq!(pixels.width, Some(20.));
        assert_eq!(pixels.width_percent, None);
        assert_eq!(pixels.height_percent, Some(1.));
        for invalid in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                Styles::new().w_percent(invalid),
                Styles::new().w_percent(0.)
            );
        }
        assert_eq!(Styles::new().w_percent(150.).width_percent, Some(1.5));
    }
    #[test]
    fn conditional_transformations_preserve_sparse_patch_edges() {
        fn horizontal<S: Styled>(style: S) -> S {
            style.px(12.)
        }

        let patch = Styles::new()
            .apply(horizontal)
            .when(false, |_| panic!("false branch must not run"))
            .when_some(None::<f32>, |_, _| panic!("missing value must not run"))
            .when(true, |style| style.pl(14.))
            .when_some(Some(String::from("owned")), |style, label| {
                assert_eq!(label, "owned");
                style.pr(16.)
            });
        assert_eq!(patch.padding_sides, 2 | 8);
        let result = Styles::new().p(8.).pt(3.).style(patch);
        assert_eq!(
            result.padding,
            Some(Insets {
                top: 3.,
                right: 16.,
                bottom: 8.,
                left: 14.,
            })
        );
    }
    #[test]
    fn patches_only_override_specified_properties() {
        let base = Styles::new()
            .size(120., 40.)
            .gap(8.)
            .bg(rgb(0x102030))
            .opacity(0.5);
        let result = base.clone().style(Styles::new().w(200.).border_1());
        assert_eq!(result.width, Some(200.));
        assert_eq!(result.height, base.height);
        assert_eq!(result.gap, base.gap);
        assert_eq!(result.background, base.background);
        assert_eq!(result.opacity, base.opacity);
        assert_eq!(result.border_width, Some(1.));
        assert_eq!(result.clone().style(Styles::new()), result);
    }
    #[test]
    fn side_spacing_obeys_chain_and_merge_precedence() {
        let style = Styles::new().p(8.).px(12.).pt(3.).m(4.).mb(9.);
        assert_eq!(
            style.padding,
            Some(Insets {
                top: 3.,
                right: 12.,
                bottom: 8.,
                left: 12.
            })
        );
        assert_eq!(
            style.margin,
            Some(Insets {
                top: 4.,
                right: 4.,
                bottom: 9.,
                left: 4.
            })
        );
        let merged = style.style(Styles::new().py(7.).ml(2.));
        assert_eq!(
            merged.padding,
            Some(Insets {
                top: 7.,
                right: 12.,
                bottom: 7.,
                left: 12.
            })
        );
        assert_eq!(
            merged.margin,
            Some(Insets {
                top: 4.,
                right: 4.,
                bottom: 9.,
                left: 2.
            })
        );
        assert_eq!(merged.p(1.).padding, Some(Insets::all(1.)));
    }
    #[test]
    fn effect_reset_and_color_order_are_explicit() {
        let shadow = BoxShadow {
            color: rgba(0x10203040),
            offset: Transform { x: 1., y: 2. },
            blur_radius: 4.,
            spread: 0.,
        };
        let style = Styles::new()
            .shadow(shadow)
            .overflow_hidden()
            .isolated(true)
            .style(
                Styles::new()
                    .shadow_none()
                    .overflow_visible()
                    .isolated(false),
            );
        assert_eq!(style.rare().shadow, Some(None));
        assert_eq!(style.clip, Some(false));
        assert_eq!(style.isolated, Some(false));
        assert_eq!(rgb(0x102030), Color(16, 32, 48, 255));
        assert_eq!(rgba(0x10203040), Color(16, 32, 48, 64));
    }
}
