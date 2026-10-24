//! Themes: semantic tokens, themes as data, and live switching.
//!
//! Every zgx app and widget styles itself from a standard set of tokens
//! ([`tok`]): surfaces, text, borders, the accent, status colours, the
//! focus ring, hover and pressed washes, scrollbars, glass, typography,
//! radii, spacing, shadows and motion. A [`Theme`] gives them values (a
//! JSON file, a Luau table or Rust), so any theme styles any app, as any
//! VS Code theme styles any extension. Apps declare their own namespaced
//! tokens (`terminal.pane`) with defaults written in terms of the standard
//! ones ([`Themes::declare`]); a theme may still set them by name.
//!
//! ```
//! use zgx::{compose::{button, text}, style::Styled, theme::{tok, Themes}, widgets::Ui};
//! let mut ui = Ui::new(320., 200.);
//! let view = button()
//!     .bg(tok::SURFACE_RAISED)
//!     .rounded(tok::RADIUS_MD)
//!     .p(tok::SPACE_3)
//!     .child(text("Save").text_color(tok::TEXT_PRIMARY));
//! ui.mount(view);
//! ui.theme().set(zgx::theme::LIGHT).unwrap(); // repaints the button, nothing else
//! ```
//!
//! Costs: a style holds token ids, and resolving one indexes the node's
//! scope's array. A node subscribes to the tokens it uses and to nothing
//! else; nodes without tokens subscribe to nothing. Applying a theme
//! evaluates each token once and notifies only what reads tokens whose
//! values changed, so a colour change repaints those nodes and a metric
//! change re-lays out those nodes; nothing happens per frame.
mod bind;
mod file;
pub(crate) mod json;
mod scope;
mod tokens;
mod value;
mod vscode;

pub(crate) use bind::{Bindings, StyleTheme, bind_in, bind_mix, merge, rebound, resolve, split};
pub use bind::{Field, Themed};
pub use file::{Theme, ThemeError, ThemeKind, ThemeOverride, TokenInput};
pub(crate) use scope::Body;
pub use scope::{Appearance, DARK, LIGHT, SystemPreferences, ThemeReader, ThemeScope, Themes};
pub use tokens::{
    ColorToken, FontToken, NumberToken, ShadowToken, TextToken, TokenId, TokenKind, interned,
    lookup, standard, standard_kind, tok, token,
};
pub use value::{TokenValue, alpha, family_name, font_family, hex, mix, over, parse_color};
