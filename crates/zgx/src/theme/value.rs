//! Token values and the expressions themes write them with.
//!
//! An expression is a literal (`"#1a2230"`, `16`, `"Geist"`), a reference to
//! another token (`"{surface.raised}"`) or a small derivation of colours:
//! `alpha(c, a)` (`c` with its opacity times `a`), `mix(a, b, t)` (`t` of the
//! way from `a` to `b`) and `over(base, wash)` (`wash` composited over
//! `base`). Expressions are parsed once, when a theme is loaded, and
//! evaluated once per token when a theme is applied, never while styling.
use super::tokens::{TokenId, TokenKind, token};
use crate::scene::{BoxShadow, Color, Transform};
use crate::text_layout::FontFamily;
use std::sync::Arc;

/// A resolved token value.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TokenValue {
    /// No theme defines the token and no app declared it.
    #[default]
    Unset,
    /// A colour.
    Color(Color),
    /// A number.
    Number(f32),
    /// A font family.
    Font(FontFamily),
    /// Box shadows.
    Shadow(Arc<[BoxShadow]>),
    /// Text, such as an easing curve.
    Text(Arc<str>),
}
impl TokenValue {
    /// The kind of value it is; `None` when unset.
    pub fn kind(&self) -> Option<TokenKind> {
        Some(match self {
            TokenValue::Unset => return None,
            TokenValue::Color(_) => TokenKind::Color,
            TokenValue::Number(_) => TokenKind::Number,
            TokenValue::Font(_) => TokenKind::Font,
            TokenValue::Shadow(_) => TokenKind::Shadow,
            TokenValue::Text(_) => TokenKind::Text,
        })
    }
    /// The colour, if it is one.
    pub fn color(&self) -> Option<Color> {
        match self {
            TokenValue::Color(c) => Some(*c),
            _ => None,
        }
    }
    /// The number, if it is one.
    pub fn number(&self) -> Option<f32> {
        match self {
            TokenValue::Number(n) => Some(*n),
            _ => None,
        }
    }
    /// The font family, if it is one; text reads as a CSS `font-family` list, of
    /// which the first family counts.
    pub fn font(&self) -> Option<FontFamily> {
        match self {
            TokenValue::Font(f) => Some(f.clone()),
            TokenValue::Text(t) => Some(font_family(t)),
            _ => None,
        }
    }
    /// The shadows, if it is a shadow value.
    pub fn shadows(&self) -> Option<Arc<[BoxShadow]>> {
        match self {
            TokenValue::Shadow(s) => Some(s.clone()),
            _ => None,
        }
    }
    /// The text, if it is text.
    pub fn text(&self) -> Option<Arc<str>> {
        match self {
            TokenValue::Text(t) => Some(t.clone()),
            _ => None,
        }
    }
}
impl std::fmt::Display for TokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenValue::Unset => f.write_str("unset"),
            TokenValue::Color(c) => f.write_str(&hex(*c)),
            TokenValue::Number(n) => write!(f, "{n}"),
            TokenValue::Font(family) => f.write_str(&family_name(family)),
            TokenValue::Shadow(shadows) if shadows.is_empty() => f.write_str("none"),
            TokenValue::Shadow(shadows) => {
                for (i, s) in shadows.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(
                        f,
                        "{} {} {} {} {}",
                        s.offset.x,
                        s.offset.y,
                        s.blur_radius * 2.,
                        s.spread,
                        hex(s.color)
                    )?;
                }
                Ok(())
            }
            TokenValue::Text(t) => f.write_str(t),
        }
    }
}
/// `#rrggbb`, or `#rrggbbaa` when translucent.
pub fn hex(Color(r, g, b, a): Color) -> String {
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}
/// A family name as themes write it: `"sans-serif"`, `"monospace"`, `"Geist"`.
pub fn family_name(family: &FontFamily) -> String {
    match family {
        FontFamily::SansSerif => "sans-serif".into(),
        FontFamily::Serif => "serif".into(),
        FontFamily::Monospace => "monospace".into(),
        FontFamily::Named(name) => name.to_string(),
    }
}
/// The family a theme's font names: the first of a CSS-style list, with the
/// generic names (`sans-serif`, `system-ui`, `monospace`, `serif`) mapped to
/// the platform's.
pub fn font_family(text: &str) -> FontFamily {
    let first = text.split(',').next().unwrap_or("").trim();
    let first = first.trim_matches(|c| c == '"' || c == '\'');
    match first.to_ascii_lowercase().as_str() {
        "" | "sans" | "sans-serif" | "system-ui" | "ui-sans-serif" | "-apple-system" => {
            FontFamily::SansSerif
        }
        "mono" | "monospace" | "ui-monospace" => FontFamily::Monospace,
        "serif" | "ui-serif" => FontFamily::Serif,
        _ => FontFamily::Named(first.into()),
    }
}

/// A shadow whose colour is an expression (`"0 8 24 {shadow.color}"`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShadowExpr {
    pub x: f32,
    pub y: f32,
    /// CSS's blur radius (twice the Gaussian sigma zgx draws with).
    pub blur: f32,
    pub spread: f32,
    pub color: Expr,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expr {
    Value(TokenValue),
    Ref(TokenId),
    Alpha(Box<Expr>, f32),
    Mix(Box<Expr>, Box<Expr>, f32),
    Over(Box<Expr>, Box<Expr>),
    Shadows(Arc<[ShadowExpr]>),
}
impl Expr {
    /// Evaluate with `lookup` resolving references.
    pub(crate) fn eval(&self, lookup: &mut dyn FnMut(TokenId) -> TokenValue) -> TokenValue {
        match self {
            Expr::Value(v) => v.clone(),
            Expr::Ref(id) => lookup(*id),
            Expr::Alpha(e, a) => match e.eval(lookup) {
                TokenValue::Color(c) => TokenValue::Color(alpha(c, *a)),
                _ => TokenValue::Unset,
            },
            Expr::Mix(a, b, t) => match (a.eval(lookup), b.eval(lookup)) {
                (TokenValue::Color(a), TokenValue::Color(b)) => TokenValue::Color(mix(a, b, *t)),
                (TokenValue::Number(a), TokenValue::Number(b)) => {
                    TokenValue::Number(a + (b - a) * t)
                }
                _ => TokenValue::Unset,
            },
            Expr::Over(a, b) => match (a.eval(lookup), b.eval(lookup)) {
                (TokenValue::Color(a), TokenValue::Color(b)) => TokenValue::Color(over(a, b)),
                _ => TokenValue::Unset,
            },
            Expr::Shadows(list) => {
                let mut out = Vec::with_capacity(list.len());
                for s in list.iter() {
                    let TokenValue::Color(color) = s.color.eval(lookup) else {
                        return TokenValue::Unset;
                    };
                    out.push(BoxShadow {
                        color,
                        offset: Transform { x: s.x, y: s.y },
                        blur_radius: s.blur / 2.,
                        spread: s.spread,
                    });
                }
                TokenValue::Shadow(out.into())
            }
        }
    }
}

/// `c` with its opacity multiplied by `a`.
pub fn alpha(Color(r, g, b, a): Color, by: f32) -> Color {
    Color(r, g, b, (a as f32 * by.clamp(0., 1.)).round() as u8)
}
/// `t` of the way from `a` to `b`, per channel (straight alpha).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color(
        lerp(a.0, b.0),
        lerp(a.1, b.1),
        lerp(a.2, b.2),
        lerp(a.3, b.3),
    )
}
/// `wash` composited over `base` (source over, straight alpha).
pub fn over(base: Color, wash: Color) -> Color {
    let sa = wash.3 as f32 / 255.;
    let da = base.3 as f32 / 255.;
    let out = sa + da * (1. - sa);
    if out <= 0. {
        return Color(0, 0, 0, 0);
    }
    let channel = |s: u8, d: u8| {
        ((s as f32 * sa + d as f32 * da * (1. - sa)) / out)
            .round()
            .clamp(0., 255.) as u8
    };
    Color(
        channel(wash.0, base.0),
        channel(wash.1, base.1),
        channel(wash.2, base.2),
        (out * 255.).round() as u8,
    )
}

/// Parse a CSS colour: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`,
/// `rgba()`, `transparent`, `black`, `white`.
pub fn parse_color(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let digit = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
        let pair = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        return match hex.len() {
            3 | 4 => Some(Color(
                digit(0)? * 17,
                digit(1)? * 17,
                digit(2)? * 17,
                if hex.len() == 4 { digit(3)? * 17 } else { 255 },
            )),
            6 | 8 => Some(Color(
                pair(0)?,
                pair(2)?,
                pair(4)?,
                if hex.len() == 8 { pair(6)? } else { 255 },
            )),
            _ => None,
        };
    }
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "transparent" => return Some(Color(0, 0, 0, 0)),
        "black" => return Some(Color(0, 0, 0, 255)),
        "white" => return Some(Color(255, 255, 255, 255)),
        _ => {}
    }
    let inner = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split([',', ' ', '/'])
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let channel = |p: &str| -> Option<u8> {
        Some(match p.strip_suffix('%') {
            Some(p) => (p.parse::<f32>().ok()? * 2.55).round().clamp(0., 255.) as u8,
            None => p.parse::<f32>().ok()?.round().clamp(0., 255.) as u8,
        })
    };
    let alpha = match parts.get(3) {
        None => 255,
        Some(p) => match p.strip_suffix('%') {
            Some(p) => (p.parse::<f32>().ok()? * 2.55).round().clamp(0., 255.) as u8,
            None => (p.parse::<f32>().ok()? * 255.).round().clamp(0., 255.) as u8,
        },
    };
    Some(Color(
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
        alpha,
    ))
}

/// A number, optionally with a `px` or `ms` unit.
fn parse_number(text: &str) -> Option<f32> {
    let text = text.trim();
    let text = text
        .strip_suffix("px")
        .or_else(|| text.strip_suffix("ms"))
        .unwrap_or(text);
    text.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// Split `a, b, c` at top-level commas.
fn split_args(text: &str) -> Vec<&str> {
    let (mut depth, mut start, mut out) = (0, 0, Vec::new());
    for (i, c) in text.char_indices() {
        match c {
            '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out
}

/// Parse a string expression. `kind`, when known (standard tokens), picks
/// how a bare literal reads (a font name, an easing, a shadow).
pub(crate) fn parse(text: &str, kind: Option<TokenKind>) -> Result<Expr, String> {
    let text = text.trim();
    if let Some(name) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}'))
        && !name.contains(['{', '}'])
    {
        let name = name.trim();
        if name.is_empty() {
            return Err("an empty reference `{}`".into());
        }
        return Ok(Expr::Ref(token(name)));
    }
    if let Some(name) = text.strip_prefix('$')
        && !name.is_empty()
        && !name.contains([' ', '(', ','])
    {
        return Ok(Expr::Ref(token(name)));
    }
    for function in ["alpha", "mix", "over"] {
        if let Some(inner) = text
            .strip_prefix(function)
            .and_then(|t| t.trim_start().strip_prefix('('))
            .and_then(|t| t.strip_suffix(')'))
        {
            let args = split_args(inner);
            let color = |i: usize| -> Result<Box<Expr>, String> {
                let arg = args
                    .get(i)
                    .ok_or_else(|| format!("`{function}` needs more arguments"))?;
                parse(arg, Some(TokenKind::Color)).map(Box::new)
            };
            let amount = |i: usize| -> Result<f32, String> {
                let arg = args
                    .get(i)
                    .ok_or_else(|| format!("`{function}` needs more arguments"))?;
                let arg = arg.trim();
                match arg.strip_suffix('%') {
                    Some(p) => parse_number(p).map(|p| p / 100.),
                    None => parse_number(arg),
                }
                .ok_or_else(|| format!("`{arg}` is not a number"))
            };
            return Ok(match (function, args.len()) {
                ("alpha", 2) => Expr::Alpha(color(0)?, amount(1)?),
                ("mix", 3) => Expr::Mix(color(0)?, color(1)?, amount(2)?),
                ("mix", 2) => Expr::Mix(color(0)?, color(1)?, 0.5),
                ("over", 2) => Expr::Over(color(0)?, color(1)?),
                _ => {
                    return Err(format!(
                        "`{function}` takes {} arguments",
                        match function {
                            "alpha" => "(colour, amount)",
                            "mix" => "(a, b, amount)",
                            _ => "(base, wash)",
                        }
                    ));
                }
            });
        }
    }
    match kind {
        Some(TokenKind::Color) => parse_color(text)
            .map(|c| Expr::Value(TokenValue::Color(c)))
            .ok_or_else(|| format!("`{text}` is not a colour")),
        Some(TokenKind::Number) => parse_number(text)
            .map(|n| Expr::Value(TokenValue::Number(n)))
            .ok_or_else(|| format!("`{text}` is not a number")),
        Some(TokenKind::Font) => Ok(Expr::Value(TokenValue::Font(font_family(text)))),
        Some(TokenKind::Shadow) => parse_shadows(text),
        Some(TokenKind::Text) => Ok(Expr::Value(TokenValue::Text(text.into()))),
        None => {
            if let Some(c) = parse_color(text) {
                Ok(Expr::Value(TokenValue::Color(c)))
            } else if let Some(n) = parse_number(text) {
                Ok(Expr::Value(TokenValue::Number(n)))
            } else if text == "none"
                || text.starts_with(|c: char| c.is_ascii_digit() || c == '-')
                    && text.split_whitespace().count() >= 3
            {
                parse_shadows(text)
            } else {
                Ok(Expr::Value(TokenValue::Text(text.into())))
            }
        }
    }
}

/// CSS `box-shadow`: `none`, or comma-separated `x y blur? spread? colour`.
pub(crate) fn parse_shadows(text: &str) -> Result<Expr, String> {
    if text.trim() == "none" {
        return Ok(Expr::Shadows(Arc::from([])));
    }
    let mut list = Vec::new();
    for part in split_args(text) {
        list.push(parse_shadow(part)?);
    }
    Ok(Expr::Shadows(list.into()))
}
fn parse_shadow(text: &str) -> Result<ShadowExpr, String> {
    // The colour is the last word, or a function or reference at the end.
    let text = text.trim();
    let split = if text.ends_with(')') || text.ends_with('}') {
        let open = if text.ends_with(')') { '(' } else { '{' };
        let start = text.rfind(open).unwrap_or(0);
        text[..start].rfind(' ').map_or(0, |i| i + 1)
    } else {
        text.rfind(' ').map_or(0, |i| i + 1)
    };
    let (lengths, color) = text.split_at(split);
    let numbers: Vec<f32> = lengths
        .split_whitespace()
        .map(|n| parse_number(n).ok_or_else(|| format!("`{n}` in a shadow is not a length")))
        .collect::<Result<_, _>>()?;
    if numbers.len() < 2 || numbers.len() > 4 {
        return Err(format!("`{text}`: a shadow is `x y blur? spread? colour`"));
    }
    Ok(ShadowExpr {
        x: numbers[0],
        y: numbers[1],
        blur: numbers.get(2).copied().unwrap_or(0.),
        spread: numbers.get(3).copied().unwrap_or(0.),
        color: parse(color, Some(TokenKind::Color))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tok;
    #[test]
    fn colours_parse_in_css_forms() {
        assert_eq!(parse_color("#fff"), Some(Color(255, 255, 255, 255)));
        assert_eq!(parse_color("#10141c"), Some(Color(16, 20, 28, 255)));
        assert_eq!(parse_color("#38629eaa"), Some(Color(56, 98, 158, 170)));
        assert_eq!(parse_color("rgba(0, 0, 0, 0.5)"), Some(Color(0, 0, 0, 128)));
        assert_eq!(parse_color("rgb(10 20 30)"), Some(Color(10, 20, 30, 255)));
        assert_eq!(parse_color("transparent"), Some(Color(0, 0, 0, 0)));
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("Geist"), None);
    }
    #[test]
    fn expressions_parse_references_and_derivations() {
        assert_eq!(
            parse("{surface.raised}", None).unwrap(),
            Expr::Ref(tok::SURFACE_RAISED.0)
        );
        let Expr::Alpha(inner, a) = parse("alpha({text.primary}, 0.6)", None).unwrap() else {
            panic!()
        };
        assert_eq!(*inner, Expr::Ref(tok::TEXT_PRIMARY.0));
        assert_eq!(a, 0.6);
        assert!(matches!(
            parse("mix({accent}, #ffffff, 25%)", None).unwrap(),
            Expr::Mix(_, _, t) if t == 0.25
        ));
        assert!(parse("alpha({accent})", None).is_err());
        assert_eq!(
            parse("Geist, sans-serif", Some(TokenKind::Font)).unwrap(),
            Expr::Value(TokenValue::Font(FontFamily::Named("Geist".into())))
        );
        assert_eq!(
            parse("12px", Some(TokenKind::Number)).unwrap(),
            Expr::Value(TokenValue::Number(12.))
        );
        let Expr::Shadows(list) = parse("0 8 24 0 alpha({background}, 0.5)", None).unwrap() else {
            panic!()
        };
        assert_eq!((list[0].y, list[0].blur), (8., 24.));
        assert!(matches!(list[0].color, Expr::Alpha(..)));
    }
    #[test]
    fn derivations_compose_colours() {
        assert_eq!(
            alpha(Color(255, 255, 255, 255), 0.5),
            Color(255, 255, 255, 128)
        );
        assert_eq!(
            mix(Color(0, 0, 0, 255), Color(255, 255, 255, 255), 0.5).0,
            128
        );
        // The default dark theme's hover wash over its raised surface is
        // exactly the hover colour zgx's widgets always had.
        assert_eq!(
            over(Color(35, 46, 64, 255), Color(59, 82, 112, 128)),
            Color(47, 64, 88, 255)
        );
        assert_eq!(
            over(Color(0, 0, 0, 0), Color(0, 0, 0, 0)),
            Color(0, 0, 0, 0)
        );
    }
}
