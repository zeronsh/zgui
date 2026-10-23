//! Token names, their interned ids and the standard schema.
//!
//! A token is named once (`"surface.raised"`) and from then on handled by a
//! small integer id: styles store ids, scopes keep values in arrays indexed
//! by them, so resolving a token is an array index, never a string hash.
//! The standard tokens have fixed ids (the constants in [`tok`]); app tokens
//! (`"terminal.pane"`) get the next free id the first time they are named.
use std::sync::{Arc, OnceLock, RwLock};

/// An interned token name. Ids are process-wide: the same name has the same
/// id in every runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenId(pub(crate) u32);
impl TokenId {
    /// The token's name, as it was interned.
    pub fn name(self) -> Arc<str> {
        registry().read().unwrap().names[self.0 as usize].clone()
    }
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}
impl std::fmt::Display for TokenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

/// What a token holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// A colour.
    Color,
    /// Logical pixels, a weight, milliseconds: a plain number.
    Number,
    /// A font family.
    Font,
    /// A list of box shadows.
    Shadow,
    /// An easing curve, in anime.js syntax (`"cubicBezier(0.2, 0, 0, 1)"`).
    Text,
}

macro_rules! typed_tokens {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub TokenId);
        impl From<$name> for TokenId {
            fn from(token: $name) -> TokenId {
                token.0
            }
        }
    )*};
}
typed_tokens! {
    /// A colour token (`tok::TEXT_PRIMARY`): accepted wherever a colour is.
    ColorToken,
    /// A number token (`tok::RADIUS_MD`): accepted wherever a length or number is.
    NumberToken,
    /// A font family token (`tok::FONT_MONO`).
    FontToken,
    /// A shadow token (`tok::SHADOW_MD`).
    ShadowToken,
    /// An easing token (`tok::MOTION_EASE_STANDARD`).
    TextToken,
}

macro_rules! kind_type {
    (Color) => {
        ColorToken
    };
    (Number) => {
        NumberToken
    };
    (Font) => {
        FontToken
    };
    (Shadow) => {
        ShadowToken
    };
    (Text) => {
        TextToken
    };
}

macro_rules! schema {
    ($($(#[$doc:meta])* $konst:ident: $kind:ident = $name:literal $(=> $default:literal)?),* $(,)?) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[repr(u32)]
        enum Index { $($konst),* }
        /// The standard tokens: name, kind and, for the tokens zgui's own
        /// widgets declare, the default written in terms of the others.
        pub(crate) const STANDARD: &[(&str, TokenKind, Option<&str>)] = &[
            $(($name, TokenKind::$kind, schema!(@default $($default)?))),*
        ];
        /// The standard tokens, typed by what they hold. Every theme defines
        /// them (a theme that leaves one out takes it from the built-in theme
        /// of its type), so any app and widget can rely on them.
        pub mod tok {
            use super::*;
            $($(#[$doc])* pub const $konst: kind_type!($kind) = <kind_type!($kind)>::from_index(Index::$konst as u32);)*
        }
    };
    (@default $default:literal) => { Some($default) };
    (@default) => { None };
}

macro_rules! from_index {
    ($($name:ident),*) => {$(
        impl $name {
            const fn from_index(index: u32) -> Self {
                Self(TokenId(index))
            }
        }
    )*};
}
from_index!(ColorToken, NumberToken, FontToken, ShadowToken, TextToken);

schema! {
    /// The window's background, behind everything.
    BACKGROUND: Color = "background",
    /// Below the base: wells, code blocks, terminals.
    SURFACE_SUNKEN: Color = "surface.sunken",
    /// Panels, sidebars and cards at rest.
    SURFACE_BASE: Color = "surface.base",
    /// Controls and cards that stand out from the base.
    SURFACE_RAISED: Color = "surface.raised",
    /// Menus, popovers, modals and tooltips, over everything else.
    SURFACE_OVERLAY: Color = "surface.overlay",
    /// Body text.
    TEXT_PRIMARY: Color = "text.primary",
    /// Labels, descriptions and other supporting text.
    TEXT_SECONDARY: Color = "text.secondary",
    /// Hints, placeholders, timestamps.
    TEXT_TERTIARY: Color = "text.tertiary",
    /// Text of controls that cannot be used.
    TEXT_DISABLED: Color = "text.disabled",
    /// Borders of controls and cards.
    BORDER: Color = "border",
    /// Dividers, separators and hairlines.
    BORDER_SUBTLE: Color = "border.subtle",
    /// Borders that must be seen: a hovered input, a selected card.
    BORDER_STRONG: Color = "border.strong",
    /// The interactive colour: primary buttons, links, sliders, progress.
    ACCENT: Color = "accent",
    /// The accent while hovered.
    ACCENT_HOVER: Color = "accent.hover",
    /// Text and icons on the accent.
    ACCENT_TEXT: Color = "accent.text",
    /// Errors and destructive actions.
    DANGER: Color = "danger",
    /// Warnings.
    WARNING: Color = "warning",
    /// Success and completion.
    SUCCESS: Color = "success",
    /// Information.
    INFO: Color = "info",
    /// Selected text.
    SELECTION: Color = "selection",
    /// A selected item in a list, a sidebar, a tab strip.
    SELECTION_ITEM: Color = "selection.item",
    /// The ring around what has keyboard focus.
    FOCUS_RING: Color = "focus.ring",
    /// A translucent wash laid over a surface under the pointer.
    HOVER: Color = "hover",
    /// A translucent wash laid over a surface while pressed.
    PRESSED: Color = "pressed",
    /// A scrollbar's thumb at rest.
    SCROLLBAR_THUMB: Color = "scrollbar.thumb",
    /// The thumb under the pointer, dragged or focused.
    SCROLLBAR_THUMB_ACTIVE: Color = "scrollbar.thumb.active",
    /// A scrollbar's track.
    SCROLLBAR_TRACK: Color = "scrollbar.track",
    /// The colour laid over frosted glass (a backdrop blur).
    GLASS_TINT: Color = "glass.tint",
    /// The radius frosted glass blurs what is behind it by.
    GLASS_BLUR: Number = "glass.blur",
    /// The interface's typeface.
    FONT_SANS: Font = "font.sans",
    /// Code, terminals, numbers in columns.
    FONT_MONO: Font = "font.mono",
    /// The smallest font size, in points.
    FONT_SIZE_XS: Number = "font.size.xs",
    /// A small font size, in points.
    FONT_SIZE_SM: Number = "font.size.sm",
    /// Body text: what text is unless it says otherwise.
    FONT_SIZE_MD: Number = "font.size.md",
    /// A large font size, in points.
    FONT_SIZE_LG: Number = "font.size.lg",
    /// A larger font size, in points.
    FONT_SIZE_XL: Number = "font.size.xl",
    /// The largest font size, in points.
    FONT_SIZE_XXL: Number = "font.size.xxl",
    /// `FONT_SIZE_XS`'s line height, in points; each `FONT_LINE_*` is its
    /// size's.
    FONT_LINE_XS: Number = "font.line.xs",
    /// `FONT_SIZE_SM`'s line height.
    FONT_LINE_SM: Number = "font.line.sm",
    /// `FONT_SIZE_MD`'s line height.
    FONT_LINE_MD: Number = "font.line.md",
    /// `FONT_SIZE_LG`'s line height.
    FONT_LINE_LG: Number = "font.line.lg",
    /// `FONT_SIZE_XL`'s line height.
    FONT_LINE_XL: Number = "font.line.xl",
    /// `FONT_SIZE_XXL`'s line height.
    FONT_LINE_XXL: Number = "font.line.xxl",
    /// The regular font weight (400 built in).
    FONT_WEIGHT_REGULAR: Number = "font.weight.regular",
    /// The medium font weight (500 built in).
    FONT_WEIGHT_MEDIUM: Number = "font.weight.medium",
    /// The semibold font weight (600 built in).
    FONT_WEIGHT_SEMIBOLD: Number = "font.weight.semibold",
    /// The bold font weight (700 built in).
    FONT_WEIGHT_BOLD: Number = "font.weight.bold",
    /// The smallest corner radius.
    RADIUS_SM: Number = "radius.sm",
    /// A medium corner radius.
    RADIUS_MD: Number = "radius.md",
    /// A large corner radius.
    RADIUS_LG: Number = "radius.lg",
    /// The largest corner radius short of `RADIUS_FULL`.
    RADIUS_XL: Number = "radius.xl",
    /// A pill or a circle.
    RADIUS_FULL: Number = "radius.full",
    /// The spacing scale for padding, margins and gaps, step 1, the smallest.
    SPACE_1: Number = "space.1",
    /// Step 2 of the spacing scale.
    SPACE_2: Number = "space.2",
    /// Step 3 of the spacing scale.
    SPACE_3: Number = "space.3",
    /// Step 4 of the spacing scale.
    SPACE_4: Number = "space.4",
    /// Step 5 of the spacing scale.
    SPACE_5: Number = "space.5",
    /// Step 6 of the spacing scale.
    SPACE_6: Number = "space.6",
    /// Step 7 of the spacing scale.
    SPACE_7: Number = "space.7",
    /// Step 8 of the spacing scale.
    SPACE_8: Number = "space.8",
    /// The height of buttons, inputs and sliders.
    CONTROL_HEIGHT: Number = "control.height",
    /// A small shadow: controls.
    SHADOW_SM: Shadow = "shadow.sm",
    /// A medium shadow: cards and raised panels.
    SHADOW_MD: Shadow = "shadow.md",
    /// Overlays: menus, popovers, modals.
    SHADOW_LG: Shadow = "shadow.lg",
    /// A fast duration, in milliseconds.
    MOTION_DURATION_FAST: Number = "motion.duration.fast",
    /// A normal duration, in milliseconds.
    MOTION_DURATION_NORMAL: Number = "motion.duration.normal",
    /// A slow duration, in milliseconds.
    MOTION_DURATION_SLOW: Number = "motion.duration.slow",
    /// Easing for movement within the screen.
    MOTION_EASE_STANDARD: Text = "motion.ease.standard",
    /// Easing for what arrives (decelerates).
    MOTION_EASE_ENTER: Text = "motion.ease.enter",
    /// Easing for what leaves (accelerates).
    MOTION_EASE_EXIT: Text = "motion.ease.exit",
    // zgui's widgets' own tokens. They are declared like an app's, with
    // defaults in terms of the standard ones, so a theme can restyle a
    // widget without changing what the standard tokens mean.
    /// Buttons at rest.
    BUTTON_BG: Color = "button.bg" => "{surface.raised}",
    /// Buttons under the pointer.
    BUTTON_HOVER: Color = "button.hover" => "over({button.bg}, {hover})",
    /// Buttons while pressed.
    BUTTON_PRESSED: Color = "button.pressed" => "over({button.bg}, {pressed})",
    /// Text inputs, sliders and progress bars.
    INPUT_BG: Color = "input.bg" => "{surface.raised}",
    /// Menus' panels.
    MENU_BG: Color = "menu.bg" => "{surface.overlay}",
    /// A menu item under the pointer.
    MENU_HOVER: Color = "menu.hover" => "over({menu.bg}, {hover})",
    /// Popovers' and tooltips' panels (modals keep `surface.overlay`).
    POPOVER_BG: Color = "popover.bg" => "{surface.overlay}",
    /// What a modal dims the window with behind it.
    BACKDROP: Color = "backdrop" => "#00000082",
    /// Selected text while its window is in the background.
    SELECTION_INACTIVE: Color = "selection.inactive" => "alpha({text.secondary}, 0.25)",
    /// The colour of selected text's glyphs (CSS `::selection { color }`):
    /// transparent, the default, keeps each glyph's own colour.
    SELECTION_TEXT: Color = "selection.text" => "alpha({text.primary}, 0)",
    /// Selected text's glyphs while its window is in the background.
    SELECTION_TEXT_INACTIVE: Color = "selection.text.inactive" => "{selection.text}",
    /// The corner radius of selected text's highlight.
    SELECTION_RADIUS: Number = "selection.radius" => "0",
}

struct Registry {
    names: Vec<Arc<str>>,
    ids: std::collections::HashMap<Arc<str>, u32>,
}
fn registry() -> &'static RwLock<Registry> {
    static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = Registry {
            names: Vec::with_capacity(STANDARD.len() + 32),
            ids: Default::default(),
        };
        for (name, _, _) in STANDARD {
            let name: Arc<str> = (*name).into();
            registry
                .ids
                .insert(name.clone(), registry.names.len() as u32);
            registry.names.push(name);
        }
        RwLock::new(registry)
    })
}

/// The id of the token `name`, interning it the first time. A leading `$`
/// (the scripts' spelling) is ignored.
pub fn token(name: &str) -> TokenId {
    let name = name.strip_prefix('$').unwrap_or(name);
    if let Some(&id) = registry().read().unwrap().ids.get(name) {
        return TokenId(id);
    }
    let mut registry = registry().write().unwrap();
    if let Some(&id) = registry.ids.get(name) {
        return TokenId(id);
    }
    let id = registry.names.len() as u32;
    let name: Arc<str> = name.into();
    registry.ids.insert(name.clone(), id);
    registry.names.push(name);
    TokenId(id)
}
/// The id of `name` if it has been interned.
pub fn lookup(name: &str) -> Option<TokenId> {
    let name = name.strip_prefix('$').unwrap_or(name);
    registry()
        .read()
        .unwrap()
        .ids
        .get(name)
        .map(|&id| TokenId(id))
}
/// How many tokens are interned.
pub(crate) fn count() -> usize {
    registry().read().unwrap().names.len()
}
/// Every token interned so far: the standard ones, and those an app
/// declared or a style named, whether or not anything defines them
/// (`ThemeScope::is_defined` says).
pub fn interned() -> impl Iterator<Item = TokenId> {
    (0..count() as u32).map(TokenId)
}
/// The standard token's kind, or `None` for an app token.
pub fn standard_kind(id: TokenId) -> Option<TokenKind> {
    STANDARD.get(id.index()).map(|(_, kind, _)| *kind)
}
/// Every standard token, in order.
pub fn standard() -> impl Iterator<Item = (TokenId, &'static str, TokenKind)> {
    STANDARD
        .iter()
        .enumerate()
        .map(|(i, (name, kind, _))| (TokenId(i as u32), *name, *kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_ids_match_their_names() {
        assert_eq!(tok::BACKGROUND.0, token("background"));
        assert_eq!(tok::TEXT_SECONDARY.0, token("$text.secondary"));
        assert_eq!(tok::MENU_HOVER.0.name().as_ref(), "menu.hover");
        for (id, name, _) in standard() {
            assert_eq!(lookup(name), Some(id));
        }
        let app = token("test.tokens.app");
        assert!(app.index() >= STANDARD.len());
        assert_eq!(token("test.tokens.app"), app);
        assert_eq!(standard_kind(app), None);
    }
}
