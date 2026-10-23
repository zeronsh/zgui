//! Themes as data: what a theme file holds and how it reads.
//!
//! ```json
//! {
//!     "name": "Solarized Dark",
//!     "type": "dark",
//!     "extends": "zgui-dark",
//!     "tokens": {
//!         "background": "#002b36",
//!         "surface": { "raised": "#073642", "overlay": "{surface.raised}" },
//!         "text.secondary": "alpha({text.primary}, 0.6)",
//!         "terminal.pane": "#00000040"
//!     }
//! }
//! ```
//!
//! `tokens` maps names (dotted, or nested in groups) to expressions (see
//! [`super::value`]). A theme that leaves a token out takes it from the theme
//! it `extends`, or else from the built-in theme of its `type`.
use super::json::{self, Json};
use super::tokens::{ColorToken, FontToken, NumberToken, ShadowToken, TokenId, TokenKind, token};
use super::value::{self, Expr, TokenValue};
use crate::scene::{BoxShadow, Color};
use crate::text_layout::FontFamily;
use std::sync::Arc;

/// Whether a theme is dark or light, and whether it is high-contrast. It
/// picks the built-in theme a theme falls back to, and tells apps which
/// way it leans (icons, images, syntax colours).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeKind {
    /// Light content on a dark background.
    #[default]
    Dark,
    /// Dark content on a light background.
    Light,
    /// High contrast on dark.
    HighContrast,
    /// High contrast on light.
    HighContrastLight,
}
impl ThemeKind {
    /// Whether it is `Dark` or `HighContrast`.
    pub fn is_dark(self) -> bool {
        matches!(self, ThemeKind::Dark | ThemeKind::HighContrast)
    }
    /// Its name in theme files: `dark`, `light`, `high-contrast` or
    /// `high-contrast-light`.
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeKind::Dark => "dark",
            ThemeKind::Light => "light",
            ThemeKind::HighContrast => "high-contrast",
            ThemeKind::HighContrastLight => "high-contrast-light",
        }
    }
    /// The kind a theme file's `type` names: one of `as_str`'s names or a VS Code
    /// one (`vs-dark`, `vs`, `hc`, `hc-black`, `hc-dark`, `hc-light`); `None` for
    /// anything else.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "dark" | "vs-dark" => ThemeKind::Dark,
            "light" | "vs" => ThemeKind::Light,
            "high-contrast" | "hc" | "hc-black" | "hc-dark" => ThemeKind::HighContrast,
            "high-contrast-light" | "hc-light" => ThemeKind::HighContrastLight,
            _ => return None,
        })
    }
}

/// A theme or theme file that could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeError(pub String);
impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ThemeError {}

/// A value given to a token: a literal, or another token to refer to.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenInput(pub(crate) Expr);
impl TokenInput {
    /// Parse a theme expression: `"#1a2230"`, `"{surface.raised}"`,
    /// `"alpha({text.primary}, 0.6)"`, `"over({surface.base}, {hover})"`.
    pub fn parse(text: &str) -> Result<Self, ThemeError> {
        value::parse(text, None).map(TokenInput).map_err(ThemeError)
    }
    /// Parse with the token's kind known (a bare word is a font, an easing).
    pub(crate) fn parse_for(id: TokenId, text: &str) -> Result<Self, ThemeError> {
        value::parse(text, super::tokens::standard_kind(id))
            .map(TokenInput)
            .map_err(|e| ThemeError(format!("`{id}`: {e}")))
    }
}
impl From<Color> for TokenInput {
    fn from(c: Color) -> Self {
        TokenInput(Expr::Value(TokenValue::Color(c)))
    }
}
impl From<f32> for TokenInput {
    fn from(n: f32) -> Self {
        TokenInput(Expr::Value(TokenValue::Number(n)))
    }
}
impl From<FontFamily> for TokenInput {
    fn from(f: FontFamily) -> Self {
        TokenInput(Expr::Value(TokenValue::Font(f)))
    }
}
impl From<BoxShadow> for TokenInput {
    fn from(s: BoxShadow) -> Self {
        TokenInput(Expr::Value(TokenValue::Shadow(Arc::from([s]))))
    }
}
impl From<Vec<BoxShadow>> for TokenInput {
    fn from(s: Vec<BoxShadow>) -> Self {
        TokenInput(Expr::Value(TokenValue::Shadow(s.into())))
    }
}
impl From<TokenValue> for TokenInput {
    fn from(v: TokenValue) -> Self {
        TokenInput(Expr::Value(v))
    }
}
macro_rules! input_ref {
    ($($t:ty),*) => {$(
        /// A reference: the token takes this token's value.
        impl From<$t> for TokenInput {
            fn from(t: $t) -> Self {
                TokenInput(Expr::Ref(t.into()))
            }
        }
    )*};
}
input_ref!(TokenId, ColorToken, NumberToken, FontToken, ShadowToken);

/// One token a theme sets: its expression and the source it was read from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub id: TokenId,
    pub source: Arc<str>,
    pub expr: Arc<Expr>,
}

/// A theme: a name, a type, an optional base and the tokens it sets.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Its name, which other themes' `extends` and scopes refer to it by.
    pub name: String,
    /// Dark or light, and whether high-contrast; picks the built-in theme it falls
    /// back to.
    pub kind: ThemeKind,
    /// The theme this one starts from; by default, the built-in theme of
    /// its kind.
    pub extends: Option<String>,
    pub(crate) tokens: Vec<Entry>,
}
impl Theme {
    /// A theme named `name`, of kind `kind`, that sets no tokens and starts from
    /// its kind's built-in theme.
    pub fn new(name: impl Into<String>, kind: ThemeKind) -> Self {
        Self {
            name: name.into(),
            kind,
            extends: None,
            tokens: Vec::new(),
        }
    }
    /// Start from `base` and override only what this theme sets.
    pub fn extending(mut self, base: impl Into<String>) -> Self {
        self.extends = Some(base.into());
        self
    }
    /// Set `token`; a later value for the same token replaces it.
    pub fn with(mut self, token: impl Into<TokenId>, value: impl Into<TokenInput>) -> Self {
        let id = token.into();
        let expr = value.into().0;
        let source = describe(&expr);
        self.insert(id, source.into(), expr);
        self
    }
    /// Set `token` from a theme expression (`"alpha({accent}, 0.2)"`).
    pub fn with_expr(mut self, token: impl Into<TokenId>, text: &str) -> Result<Self, ThemeError> {
        let id = token.into();
        let input = TokenInput::parse_for(id, text)?;
        self.insert(id, text.into(), input.0);
        Ok(self)
    }
    fn insert(&mut self, id: TokenId, source: Arc<str>, expr: Expr) {
        let entry = Entry {
            id,
            source,
            expr: Arc::new(expr),
        };
        match self.tokens.iter_mut().find(|e| e.id == id) {
            Some(existing) => *existing = entry,
            None => self.tokens.push(entry),
        }
    }
    /// The expression this theme itself gives `token`, as written.
    pub fn get(&self, token: impl Into<TokenId>) -> Option<&str> {
        let id = token.into();
        self.tokens.iter().find(|e| e.id == id).map(|e| &*e.source)
    }
    /// The tokens this theme sets, as written, in order.
    pub fn tokens(&self) -> impl Iterator<Item = (TokenId, &str)> {
        self.tokens.iter().map(|e| (e.id, &*e.source))
    }
    /// Read a zgui theme file (JSON, comments and trailing commas allowed).
    pub fn from_json(text: &str) -> Result<Self, ThemeError> {
        Self::from_json_named(text, None)
    }
    /// As `from_json`, naming the theme `fallback` if the file does not.
    pub(crate) fn from_json_named(text: &str, fallback: Option<&str>) -> Result<Self, ThemeError> {
        let root = json::parse(text).map_err(ThemeError)?;
        Self::from_value(&root, fallback)
    }
    pub(crate) fn from_value(root: &Json, fallback: Option<&str>) -> Result<Self, ThemeError> {
        let Json::Object(_) = root else {
            return Err(ThemeError("a theme is a JSON object".into()));
        };
        let name = match root.get("name") {
            Some(Json::String(name)) => name.clone(),
            Some(_) => return Err(ThemeError("`name` must be a string".into())),
            None => fallback
                .map(str::to_owned)
                .ok_or_else(|| ThemeError("a theme needs a `name`".into()))?,
        };
        let kind = match root.get("type") {
            None => ThemeKind::Dark,
            Some(Json::String(kind)) => ThemeKind::parse(kind).ok_or_else(|| {
                ThemeError(format!(
                    "`type` is \"dark\", \"light\", \"high-contrast\" or \"high-contrast-light\", not \"{kind}\""
                ))
            })?,
            Some(_) => return Err(ThemeError("`type` must be a string".into())),
        };
        let mut theme = Theme::new(name, kind);
        match root.get("extends") {
            None | Some(Json::Null) => {}
            Some(Json::String(base)) => theme.extends = Some(base.clone()),
            Some(_) => return Err(ThemeError("`extends` must be a theme's name".into())),
        }
        match root.get("tokens") {
            None => {}
            Some(tokens @ Json::Object(_)) => theme.read_tokens("", tokens)?,
            Some(_) => return Err(ThemeError("`tokens` must be an object".into())),
        }
        Ok(theme)
    }
    fn read_tokens(&mut self, prefix: &str, group: &Json) -> Result<(), ThemeError> {
        let Json::Object(members) = group else {
            return Ok(());
        };
        for (key, value) in members {
            let name = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            let id = token(&name);
            let kind = super::tokens::standard_kind(id);
            match value {
                Json::String(text) => {
                    let input = TokenInput::parse_for(id, text)?;
                    self.insert(id, text.as_str().into(), input.0);
                }
                Json::Number(n) => {
                    if kind.is_some_and(|k| k != TokenKind::Number) {
                        return Err(ThemeError(format!("`{name}` is not a number token")));
                    }
                    let expr = Expr::Value(TokenValue::Number(*n as f32));
                    self.insert(id, n.to_string().into(), expr);
                }
                Json::Array(items) => {
                    // A list of shadows, as strings or objects.
                    let mut parts = Vec::new();
                    for item in items {
                        parts.push(match item {
                            Json::String(s) => s.clone(),
                            item @ Json::Object(_) => shadow_object(&name, item)?,
                            _ => {
                                return Err(ThemeError(format!(
                                    "`{name}`: a shadow list holds strings or objects"
                                )));
                            }
                        });
                    }
                    let text = if parts.is_empty() {
                        "none".to_owned()
                    } else {
                        parts.join(", ")
                    };
                    let expr = value::parse_shadows(&text)
                        .map_err(|e| ThemeError(format!("`{name}`: {e}")))?;
                    self.insert(id, text.into(), expr);
                }
                // A shadow written as an object: only shadow fields, an offset among them.
                group @ Json::Object(fields)
                    if kind == Some(TokenKind::Shadow)
                        || kind.is_none()
                            && fields.iter().any(|(k, _)| k == "x" || k == "y")
                            && fields.iter().all(|(k, _)| {
                                matches!(k.as_str(), "x" | "y" | "blur" | "spread" | "color")
                            }) =>
                {
                    let text = shadow_object(&name, group)?;
                    let expr = value::parse_shadows(&text)
                        .map_err(|e| ThemeError(format!("`{name}`: {e}")))?;
                    self.insert(id, text.into(), expr);
                }
                group @ Json::Object(_) => self.read_tokens(&name, group)?,
                Json::Bool(_) | Json::Null => {
                    return Err(ThemeError(format!(
                        "`{name}` must be a string, a number or a group of tokens"
                    )));
                }
            }
        }
        Ok(())
    }
    /// The theme as a zgui theme file.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out.push_str(&format!("\t\"name\": {},\n", json::quote(&self.name)));
        out.push_str(&format!("\t\"type\": \"{}\",\n", self.kind.as_str()));
        if let Some(base) = &self.extends {
            out.push_str(&format!("\t\"extends\": {},\n", json::quote(base)));
        }
        out.push_str("\t\"tokens\": {");
        for (i, entry) in self.tokens.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            let value = match &*entry.expr {
                Expr::Value(TokenValue::Number(n)) => n.to_string(),
                _ => json::quote(&entry.source),
            };
            out.push_str(&format!("\t\t{}: {value}", json::quote(&entry.id.name())));
        }
        out.push_str("\n\t}\n}\n");
        out
    }
}
/// `{ "x": 0, "y": 8, "blur": 24, "spread": 0, "color": "#0006" }` as CSS.
fn shadow_object(name: &str, object: &Json) -> Result<String, ThemeError> {
    let number = |key: &str| match object.get(key) {
        None => Ok(0.),
        Some(Json::Number(n)) => Ok(*n),
        Some(_) => Err(ThemeError(format!("`{name}.{key}` must be a number"))),
    };
    let color = object
        .get("color")
        .and_then(Json::as_str)
        .unwrap_or("#00000059");
    Ok(format!(
        "{} {} {} {} {color}",
        number("x")?,
        number("y")?,
        number("blur")?,
        number("spread")?
    ))
}
/// How a literal expression is written in a theme file.
fn describe(expr: &Expr) -> String {
    match expr {
        Expr::Value(v) => v.to_string(),
        Expr::Ref(id) => format!("{{{id}}}"),
        Expr::Alpha(e, a) => format!("alpha({}, {a})", describe(e)),
        Expr::Mix(a, b, t) => format!("mix({}, {}, {t})", describe(a), describe(b)),
        Expr::Over(a, b) => format!("over({}, {})", describe(a), describe(b)),
        Expr::Shadows(list) if list.is_empty() => "none".into(),
        Expr::Shadows(list) => list
            .iter()
            .map(|s| {
                format!(
                    "{} {} {} {} {}",
                    s.x,
                    s.y,
                    s.blur,
                    s.spread,
                    describe(&s.color)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// What a scope changes about the theme it is in (see `ThemeScope::child`):
/// another theme for a subtree (a light card in a dark app), some tokens
/// (a subtree with its own accent), or both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeOverride {
    pub(crate) theme: Option<String>,
    pub(crate) tokens: Vec<(TokenId, Arc<Expr>)>,
    /// The system's text size does not apply in the subtree.
    pub(crate) fixed_text_size: bool,
}
impl ThemeOverride {
    /// An override that changes nothing.
    pub fn new() -> Self {
        Self::default()
    }
    /// Keep the theme's font sizes in the subtree whatever the system's
    /// text size (`Themes::set_system_text_scale`): a terminal's fixed grid,
    /// a canvas laid out in points.
    pub fn fixed_text_size(mut self) -> Self {
        self.fixed_text_size = true;
        self
    }
    /// Use the theme `name` in the subtree.
    pub fn theme(mut self, name: impl Into<String>) -> Self {
        self.theme = Some(name.into());
        self
    }
    /// Give `token` this value in the subtree. Tokens derived from it (a
    /// focus ring that is `{accent}`) follow.
    pub fn with(mut self, token: impl Into<TokenId>, value: impl Into<TokenInput>) -> Self {
        let id = token.into();
        let expr = Arc::new(value.into().0);
        match self.tokens.iter_mut().find(|(t, _)| *t == id) {
            Some(existing) => existing.1 = expr,
            None => self.tokens.push((id, expr)),
        }
        self
    }
    /// Give `token` a theme expression in the subtree.
    pub fn with_expr(self, token: impl Into<TokenId>, text: &str) -> Result<Self, ThemeError> {
        let id = token.into();
        let input = TokenInput::parse_for(id, text)?;
        Ok(self.with(id, input))
    }
    /// Whether it changes neither the theme nor any token.
    pub fn is_empty(&self) -> bool {
        self.theme.is_none() && self.tokens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tok;
    #[test]
    fn theme_files_read_nested_groups_and_round_trip() {
        let theme = Theme::from_json(
            r##"{
                "name": "Test",
                "type": "light",
                "extends": "zgui-light",
                "tokens": {
                    "accent": "#e5484d",
                    "text": { "secondary": "alpha({text.primary}, 0.6)" },
                    "radius.md": 8,
                    "shadow.md": [{ "y": 4, "blur": 12, "color": "#0003" }],
                    "app.badge": "{accent}",
                }
            }"##,
        )
        .unwrap();
        assert_eq!(theme.name, "Test");
        assert_eq!(theme.kind, ThemeKind::Light);
        assert_eq!(theme.extends.as_deref(), Some("zgui-light"));
        assert_eq!(theme.get(tok::ACCENT), Some("#e5484d"));
        assert_eq!(
            theme.get(tok::TEXT_SECONDARY),
            Some("alpha({text.primary}, 0.6)")
        );
        assert_eq!(theme.get(token("app.badge")), Some("{accent}"));
        let again = Theme::from_json(&theme.to_json()).unwrap();
        assert_eq!(again.tokens().count(), theme.tokens().count());
        assert_eq!(again.get(tok::RADIUS_MD), Some("8"));
    }
    #[test]
    fn theme_files_report_bad_values() {
        let error = Theme::from_json(r#"{ "name": "Bad", "tokens": { "accent": "blue-ish" } }"#)
            .unwrap_err();
        assert!(error.0.contains("accent"), "{error}");
        assert!(Theme::from_json(r#"{ "tokens": {} }"#).is_err());
        assert!(Theme::from_json(r#"{ "name": "x", "type": "sepia" }"#).is_err());
        assert!(
            Theme::from_json(r##"{ "name": "x", "tokens": { "radius.md": "#fff" } }"##).is_err()
        );
    }
}
