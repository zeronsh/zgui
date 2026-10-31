//! Tokens in styles.
//!
//! `.bg(tok::SURFACE_RAISED)` records that the background is the token
//! `surface.raised` (a [`Field`] and a [`TokenId`]) and writes a placeholder
//! (transparent, 0) so that the style reads as setting the property. When
//! the node is styled, each binding's value is read from the node's
//! [`ThemeScope`] (an array index) and written over the placeholder through
//! the ordinary setter. A later literal (`.bg(rgb(..))`) or a patch merged
//! over it replaces the binding, as it would a value.
use super::scope::ThemeScope;
use super::tokens::{ColorToken, FontToken, NumberToken, ShadowToken, TokenId};
use super::value::TokenValue;
use crate::scene::{BoxShadow, Color};
use crate::style::{Styled, Styles};
use crate::text_layout::FontFamily;
use std::sync::Arc;

/// A style property a token can set. Padding and margin carry the sides
/// (top 1, right 2, bottom 4, left 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// The background colour, `Styled::bg`.
    Background,
    /// The text colour, `Styled::text_color`.
    TextColor,
    /// The border colour, `Styled::border_color`.
    BorderColor,
    /// The corner radius of every corner, `Styled::rounded`.
    Radius,
    /// The border width, `Styled::border`.
    BorderWidth,
    /// Padding on the given sides, `Styled::p` (or `pt`, `pr`, `pb`, `pl`).
    Padding(u8),
    /// Margin on the given sides, `Styled::m` (or `mt`, `mr`, `mb`, `ml`).
    Margin(u8),
    /// The room between children, `Styled::gap`.
    Gap,
    /// The font size, `Styled::text_size`.
    TextSize,
    /// The line height, `Styled::line_height`; a value that is not a number gives
    /// `line_height_normal`.
    LineHeight,
    /// The letter spacing, `Styled::letter_spacing`.
    LetterSpacing,
    /// The font weight, held between 1 and 1000, `Styled::font_weight`.
    FontWeight,
    /// The font family, `Styled::font_family`.
    FontFamily,
    /// The box shadows, `Styled::shadows`; an empty list is no shadow.
    Shadow,
    /// `Styled::inset_shadows`.
    InsetShadow,
    /// `Styled::outline_color`.
    OutlineColor,
    /// The opacity, `Styled::opacity`; 1 when the value is not a number.
    Opacity,
    /// Backdrop blur (frosted glass).
    Blur,
    /// The width, `Styled::w`.
    Width,
    /// The height, `Styled::h`.
    Height,
    /// One of a shader fill's four parameters (a colour, as 0 to 1).
    ShaderParam(u8),
}
impl Field {
    /// Sides of padding or margin, else 15.
    fn sides(self) -> u8 {
        match self {
            Field::Padding(m) | Field::Margin(m) => m,
            _ => 15,
        }
    }
    /// Same property, whatever the sides.
    fn same(self, other: Field) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
    fn with_sides(self, sides: u8) -> Field {
        match self {
            Field::Padding(_) => Field::Padding(sides),
            Field::Margin(_) => Field::Margin(sides),
            other => other,
        }
    }
}

/// A style value or a token standing for one.
#[derive(Clone, Copy, Debug)]
pub enum Themed<T> {
    /// A value as written.
    Value(T),
    /// The token's value in the node's theme scope.
    Token(TokenId),
}
/// As derived, but inlined: restyling text compares inherited colours on
/// every run, and a call for an eight-byte comparison cost more than it.
impl<T: PartialEq> PartialEq for Themed<T> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Themed::Value(a), Themed::Value(b)) => a == b,
            (Themed::Token(a), Themed::Token(b)) => a == b,
            _ => false,
        }
    }
}
macro_rules! themed {
    ($t:ty: $($from:ty => $map:expr),* ; tokens $($tok:ty),*) => {
        $(impl From<$from> for Themed<$t> {
            fn from(value: $from) -> Self {
                #[allow(clippy::redundant_closure_call)]
                Themed::Value(($map)(value))
            }
        })*
        $(impl From<$tok> for Themed<$t> {
            fn from(token: $tok) -> Self {
                Themed::Token(token.into())
            }
        })*
    };
}
themed!(Color: Color => |c| c; tokens ColorToken, TokenId);
themed!(f32: f32 => |n| n; tokens NumberToken, TokenId);
themed!(u16: u16 => |n| n; tokens NumberToken, TokenId);
themed!(FontFamily:
    FontFamily => |f| f,
    &str => |s: &str| FontFamily::Named(s.into()),
    String => |s: String| FontFamily::Named(s.into()),
    Arc<str> => FontFamily::Named;
    tokens FontToken, TokenId);
themed!(BoxShadow: BoxShadow => |s| s; tokens ShadowToken, TokenId);

/// The token bindings of one `Styles`: each field's token, and for a colour
/// mid-way to another (an animation between theme colours), the blend.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Bindings(pub(crate) Vec<(Field, TokenId, Option<Mix>)>);

/// A token's colour `t` of the way to `to` (a colour or another token).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mix {
    pub(crate) to: Themed<Color>,
    pub(crate) t: f32,
}

/// Drop the bindings `field` (and its sides) replaces.
pub(crate) fn unbind(styles: &mut Styles, field: Field) {
    let Some(bindings) = &mut styles.fields_mut(crate::style::bit::TOKENS).tokens else {
        return;
    };
    let sides = field.sides();
    let bindings = Arc::make_mut(bindings);
    bindings.0.retain_mut(|(bound, _, _)| {
        if !bound.same(field) {
            return true;
        }
        let left = bound.sides() & !sides;
        *bound = bound.with_sides(left);
        left != 0
    });
    if bindings.0.is_empty() {
        styles.fields_mut(crate::style::bit::TOKENS).tokens = None;
    }
}
/// Bind `field` to `token`, writing a placeholder until it resolves.
pub(crate) fn bind_in(styles: &mut Styles, field: Field, token: TokenId) {
    apply(styles, field, &TokenValue::Unset);
    rebind(styles, field, Some(token));
}
/// Bind the colour `field` to `t` of the way from `from` to `to`, resolved
/// from the node's theme like any token.
pub(crate) fn bind_mix(
    styles: &mut Styles,
    field: Field,
    from: TokenId,
    to: Themed<Color>,
    t: f32,
) {
    bind_in(styles, field, from);
    if let Some((_, _, mix)) = styles
        .fields_mut(crate::style::bit::TOKENS)
        .tokens
        .as_mut()
        .and_then(|b| Arc::make_mut(b).0.last_mut())
    {
        *mix = Some(Mix { to, t });
    }
}
/// What a setter writes for a token until it resolves.
pub(crate) trait Placeholder {
    fn placeholder() -> Self;
}
impl Placeholder for Color {
    fn placeholder() -> Self {
        Color(0, 0, 0, 0)
    }
}
impl Placeholder for f32 {
    fn placeholder() -> Self {
        0.
    }
}
impl Placeholder for u16 {
    fn placeholder() -> Self {
        400
    }
}
impl Placeholder for FontFamily {
    fn placeholder() -> Self {
        FontFamily::default()
    }
}
impl Placeholder for BoxShadow {
    fn placeholder() -> Self {
        BoxShadow {
            color: Color(0, 0, 0, 0),
            offset: Default::default(),
            blur_radius: 0.,
            spread: 0.,
        }
    }
}
/// A setter's value, or a placeholder and the token it stands for.
#[inline]
pub(crate) fn split<T: Placeholder>(value: Themed<T>) -> (T, Option<TokenId>) {
    match value {
        Themed::Value(value) => (value, None),
        Themed::Token(token) => (T::placeholder(), Some(token)),
    }
}
/// `rebind` for a setter, taking and returning the style by value: out of
/// line, and without the style's address escaping, so that setters chained
/// without tokens still build the style in place.
#[inline(never)]
pub(crate) fn rebound<S: Styled>(mut style: S, field: Field, token: Option<TokenId>) -> S {
    rebind(style.styles_mut(), field, token);
    style
}
/// Replace `field`'s binding: drop what binds it, then bind it to `token`.
pub(crate) fn rebind(styles: &mut Styles, field: Field, token: Option<TokenId>) {
    unbind(styles, field);
    if let Some(token) = token {
        Arc::make_mut(
            styles
                .fields_mut(crate::style::bit::TOKENS)
                .tokens
                .get_or_insert_with(Default::default),
        )
        .0
        .push((field, token, None));
    }
}
/// Write `value` into `field` through its setter (which drops any binding
/// for it; resolution takes the bindings out first).
pub(crate) fn apply(styles: &mut Styles, field: Field, value: &TokenValue) {
    let color = || value.color().unwrap_or(Color(0, 0, 0, 0));
    let number = || value.number().unwrap_or(0.);
    let s = styles;
    match field {
        Field::Background => {
            s.bg(color());
        }
        Field::TextColor => {
            s.text_color(color());
        }
        Field::BorderColor => {
            s.border_color(color());
        }
        Field::Radius => {
            s.rounded(number());
        }
        Field::BorderWidth => {
            s.border(number());
        }
        Field::Padding(15) => {
            s.p(number());
        }
        Field::Padding(sides) => {
            let v = number();
            let s = if sides & 1 != 0 { s.pt(v) } else { s };
            let s = if sides & 2 != 0 { s.pr(v) } else { s };
            let s = if sides & 4 != 0 { s.pb(v) } else { s };
            if sides & 8 != 0 {
                s.pl(v);
            }
        }
        Field::Margin(15) => {
            s.m(number());
        }
        Field::Margin(sides) => {
            let v = number();
            let s = if sides & 1 != 0 { s.mt(v) } else { s };
            let s = if sides & 2 != 0 { s.mr(v) } else { s };
            let s = if sides & 4 != 0 { s.mb(v) } else { s };
            if sides & 8 != 0 {
                s.ml(v);
            }
        }
        Field::Gap => {
            s.gap(number());
        }
        Field::TextSize => {
            s.text_size(number());
        }
        Field::LineHeight => {
            match value.number() {
                Some(n) => s.line_height(n),
                None => s.line_height_normal(),
            };
        }
        Field::LetterSpacing => {
            s.letter_spacing(number());
        }
        Field::FontWeight => {
            s.font_weight(number().clamp(1., 1000.) as u16);
        }
        Field::FontFamily => {
            s.font_family(value.font().unwrap_or_default());
        }
        Field::Shadow => {
            match value.shadows() {
                Some(list) if !list.is_empty() => s.shadows(list),
                _ => s.shadow_none(),
            };
        }
        Field::InsetShadow => {
            s.inset_shadows(value.shadows().unwrap_or_default());
        }
        Field::OutlineColor => {
            s.outline_color(color());
        }
        Field::Opacity => {
            s.opacity(value.number().unwrap_or(1.));
        }
        Field::Blur => {
            s.blur(number());
        }
        Field::Width => {
            s.w(number());
        }
        Field::Height => {
            s.h(number());
        }
        Field::ShaderParam(slot) => {
            if let Some(c) = value.color() {
                let rare = s.rare_mut();
                let uniforms = (rare.cold.get_or_insert_default().shader_uniforms)
                    .get_or_insert_with(Default::default);
                uniforms.params[(slot & 3) as usize] = [
                    c.0 as f32 / 255.,
                    c.1 as f32 / 255.,
                    c.2 as f32 / 255.,
                    c.3 as f32 / 255.,
                ];
            }
        }
    }
}
/// A binding's value in `scope`, subscribing the running effect to the
/// tokens it reads.
fn value(scope: &ThemeScope, token: TokenId, mix: Option<Mix>) -> TokenValue {
    let value = scope.get(token);
    let Some(Mix { to, t }) = mix else {
        return value;
    };
    let to = match to {
        Themed::Value(c) => Some(c),
        Themed::Token(token) => scope.get(token).color(),
    };
    match (value.color(), to) {
        (Some(from), Some(to)) => TokenValue::Color(super::value::mix(from, to, t)),
        _ => value,
    }
}
/// Resolve `styles`' bindings from `scope`, subscribing the running effect
/// to each token read.
pub(crate) fn resolve(styles: &mut Styles, scope: &ThemeScope) {
    let Some(bindings) = styles.fields_mut(crate::style::bit::TOKENS).tokens.take() else {
        return;
    };
    for (field, token, mix) in &bindings.0 {
        apply(styles, *field, &value(scope, *token, *mix));
    }
    styles.fields_mut(crate::style::bit::TOKENS).tokens = Some(bindings);
}
/// Which sides of padding or margin (or, for other fields, whether) `styles`
/// sets `field`.
fn sets(styles: &Styles, field: Field) -> u8 {
    let all = |set: bool| if set { 15 } else { 0 };
    let sides = |set: bool, mask: u8| match (set, mask) {
        (false, _) => 0,
        (true, 0) => 15,
        (true, mask) => mask,
    };
    match field {
        Field::Background => all(styles.background.is_some()),
        Field::TextColor => all(styles.text_color.is_some()),
        Field::BorderColor => all(styles.border_color.is_some()),
        Field::Radius => all(styles.radius.is_some()),
        Field::BorderWidth => all(styles.border_width.is_some()),
        Field::Padding(_) => sides(styles.padding.is_some(), styles.padding_sides),
        Field::Margin(_) => sides(styles.margin.is_some(), styles.margin_sides),
        Field::Gap => all(styles.gap.is_some()),
        Field::TextSize => all(styles.text_size.is_some()),
        Field::LineHeight => all(styles.line_height.is_some()),
        Field::LetterSpacing => all(styles.rare().letter_spacing.is_some()),
        Field::FontWeight => all(styles.font_weight.is_some()),
        Field::FontFamily => all(styles.font_family.is_some()),
        Field::Shadow => all(styles.rare().shadow.is_some() || styles.rare().shadows.is_some()),
        Field::InsetShadow => all(styles.rare().inset_shadows.is_some()),
        Field::OutlineColor => all(styles.rare().outline.is_some()),
        Field::Opacity => all(styles.opacity.is_some()),
        Field::Blur => all(styles.overflow().blur.is_some()),
        Field::Width => all(styles.width.is_some() || styles.width_percent.is_some()),
        Field::Height => all(styles.height.is_some() || styles.height_percent.is_some()),
        // Set alongside the uniforms it lands in; nothing replaces it.
        Field::ShaderParam(_) => 0,
    }
}
/// `Styles::merge`'s part for bindings: what `other` sets replaces `target`'s
/// bindings for it, and `other`'s bindings come along.
pub(crate) fn merge(target: &mut Styles, other: &Styles) {
    if target.tokens.is_some() {
        let mut replaced = Vec::new();
        if let Some(bindings) = &target.tokens {
            for (field, _, _) in &bindings.0 {
                let mask = sets(other, *field) & field.sides();
                if mask != 0 {
                    replaced.push(field.with_sides(mask));
                }
            }
        }
        for field in replaced {
            unbind(target, field);
        }
    }
    if let Some(theirs) = &other.tokens {
        match &mut target.fields_mut(crate::style::bit::TOKENS).tokens {
            None => target.fields_mut(crate::style::bit::TOKENS).tokens = Some(theirs.clone()),
            Some(ours) => Arc::make_mut(ours).0.extend(theirs.0.iter().copied()),
        }
    }
}

/// A node's style patches' tokens, for its style effect: the base and
/// variant patches are resolved in place, and re-resolved when a token they
/// use changes.
pub(crate) struct StyleTheme {
    scope: std::rc::Rc<ThemeScope>,
    /// Whether the base or a variant uses tokens.
    themed: bool,
    /// The scope's epoch when `fresh` was last cleared.
    epoch: u32,
    /// Which patches (bit 0 the base, then hover, active, focus, disabled)
    /// hold the values of `epoch`.
    fresh: u8,
}
impl StyleTheme {
    pub(crate) fn new(
        scope: std::rc::Rc<ThemeScope>,
        base: &Styles,
        variants: &[Option<Box<Styles>>],
    ) -> Self {
        let themed = base.tokens.is_some()
            || variants
                .iter()
                .any(|v| v.as_ref().is_some_and(|v| v.tokens.is_some()));
        Self {
            scope,
            themed,
            epoch: 0,
            fresh: 0,
        }
    }
    /// Resolve the tokens of the base and of the variants in effect
    /// (`active`: hover, active, focus, disabled), subscribing to them; a
    /// variant not in effect is resolved when it comes into effect. Whether
    /// any patch took new values.
    pub(crate) fn refresh(
        &mut self,
        base: &mut Styles,
        variants: &mut [Option<Box<Styles>>; 4],
        active: [bool; 4],
    ) -> bool {
        if !self.themed {
            return false;
        }
        let epoch = self.scope.revision() as u32;
        if epoch != self.epoch {
            self.epoch = epoch;
            self.fresh = 0;
        }
        let mut changed = self.visit(0, base);
        for (i, (variant, on)) in variants.iter_mut().zip(active).enumerate() {
            if on && let Some(variant) = variant {
                changed |= self.visit(i + 1, variant);
            }
        }
        changed
    }
    /// Resolve `patch` if the scope changed since it last was; otherwise
    /// only read its tokens, to keep the subscriptions.
    fn visit(&mut self, slot: usize, patch: &mut Styles) -> bool {
        let Some(bindings) = patch.fields_mut(crate::style::bit::TOKENS).tokens.take() else {
            return false;
        };
        let stale = self.fresh & (1 << slot) == 0;
        for (field, token, mix) in &bindings.0 {
            if stale {
                apply(patch, *field, &value(&self.scope, *token, *mix));
            } else {
                self.scope.track(*token);
                if let Some(Mix {
                    to: Themed::Token(to),
                    ..
                }) = mix
                {
                    self.scope.track(*to);
                }
            }
        }
        self.fresh |= 1 << slot;
        patch.fields_mut(crate::style::bit::TOKENS).tokens = Some(bindings);
        stale
    }
    /// Resolve a dynamic style's tokens, if it has any.
    #[inline]
    pub(crate) fn resolve(&self, produced: &mut Styles) {
        if produced.tokens.is_some() {
            resolve(produced, &self.scope);
        }
    }
    /// Account for a variant made after the style mounted.
    pub(crate) fn include(&mut self, variant: &Styles) {
        self.themed |= variant.tokens.is_some();
    }
    /// A patch (`slot`: 0 the base, then hover, active, focus, disabled) was
    /// replaced: its tokens resolve when it next comes into effect.
    pub(crate) fn replaced(&mut self, slot: usize) {
        self.fresh &= !(1 << slot);
    }
    /// Whether the base or a variant uses tokens.
    pub(crate) fn themed(&self) -> bool {
        self.themed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Themes, tok};
    #[test]
    fn later_values_replace_bindings_side_by_side() {
        let s = Styles::new().p(tok::SPACE_3).px(4.);
        assert_eq!(
            s.tokens.as_ref().unwrap().0,
            vec![(Field::Padding(5), tok::SPACE_3.0, None)]
        );
        let s = Styles::new().bg(tok::ACCENT).bg(Color(1, 2, 3, 255));
        assert!(s.tokens.is_none());
        assert_eq!(s.background, Some(Color(1, 2, 3, 255)));
        let mut base = Styles::new().bg(tok::BUTTON_BG).rounded(tok::RADIUS_MD);
        base.merge(&Styles::new().bg(Color(9, 9, 9, 255)));
        assert_eq!(
            base.tokens.as_ref().unwrap().0,
            vec![(Field::Radius, tok::RADIUS_MD.0, None)]
        );
        let mut base = Styles::new().bg(Color(9, 9, 9, 255));
        base.merge(&Styles::new().bg(tok::ACCENT));
        assert_eq!(
            base.tokens.as_ref().unwrap().0,
            vec![(Field::Background, tok::ACCENT.0, None)]
        );
    }
    #[test]
    fn bindings_resolve_from_the_scope() {
        let runtime = crate::reactive::Runtime::new();
        let themes = Themes::of(&runtime);
        let mut s = Styles::new()
            .bg(tok::SURFACE_RAISED)
            .p(tok::SPACE_2)
            .text_size(tok::FONT_SIZE_SM)
            .font_family(tok::FONT_MONO)
            .shadow(tok::SHADOW_MD);
        resolve(&mut s, themes.root());
        assert_eq!(s.background, Some(Color(35, 46, 64, 255)));
        assert_eq!(s.padding.unwrap().left, 8.);
        assert_eq!(s.text_size, Some(13.));
        assert_eq!(s.font_family, Some(FontFamily::Monospace));
        assert!(s.rare().shadows.as_ref().unwrap().as_ref().unwrap().len() == 1);
        assert!(s.tokens.is_some(), "bindings stay for the next resolution");
    }
    #[test]
    fn blends_resolve_between_theme_colours() {
        let runtime = crate::reactive::Runtime::new();
        let themes = Themes::of(&runtime);
        let scope = themes.root();
        let (a, b) = (
            scope.color(tok::SURFACE_RAISED),
            scope.color(tok::TEXT_PRIMARY),
        );
        let mut s = Styles::new().token_mix(
            Field::TextColor,
            tok::SURFACE_RAISED,
            tok::TEXT_PRIMARY,
            0.5,
        );
        resolve(&mut s, scope);
        assert_eq!(s.text_color, Some(crate::theme::value::mix(a, b, 0.5)));
        let white = Color(255, 255, 255, 255);
        let mut s = Styles::new().token_mix(Field::Background, tok::SURFACE_RAISED, white, 1.);
        resolve(&mut s, scope);
        assert_eq!(s.background, Some(white));
        let s = s.bg(Color(1, 2, 3, 255));
        assert!(s.tokens.is_none(), "a later value replaces the blend");
    }
}
