//! Animated values: numbers in a property's canonical unit (points,
//! radians, factors), RGBA colors, and strings with numbers in them, which
//! interpolate number by number as anime.js does.
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum V {
    Num(f64),
    /// Straight (not premultiplied) RGBA, 0..255 for color, 0..1 for alpha.
    Color([f64; 4]),
    /// Text around numbers: `pieces.len() == numbers.len() + 1`.
    Complex {
        pieces: Rc<[Rc<str>]>,
        numbers: Rc<[f64]>,
    },
    /// Anything else: shown as `to` once the tween ends.
    Discrete(crate::value::Value),
}
impl V {
    pub fn lerp(&self, to: &V, t: f64) -> V {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        match (self, to) {
            (V::Num(a), V::Num(b)) => V::Num(mix(*a, *b)),
            (V::Color(a), V::Color(b)) => V::Color([
                mix(a[0], b[0]),
                mix(a[1], b[1]),
                mix(a[2], b[2]),
                mix(a[3], b[3]),
            ]),
            (
                V::Complex {
                    pieces: p,
                    numbers: a,
                },
                V::Complex {
                    pieces: q,
                    numbers: b,
                },
            ) if p == q && a.len() == b.len() => V::Complex {
                pieces: p.clone(),
                numbers: a.iter().zip(b.iter()).map(|(a, b)| mix(*a, *b)).collect(),
            },
            _ if t >= 1. => to.clone(),
            _ => self.clone(),
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            V::Num(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Op {
    Set,
    Add,
    Sub,
    Mul,
}

/// A value as written: `100`, `"50%"`, `"+=1turn"`, `"#f80"`, `"10px 20px"`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Raw {
    pub op: Op,
    pub kind: RawKind,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RawKind {
    Num(f64, Option<Rc<str>>),
    Color([f64; 4]),
    Complex {
        pieces: Rc<[Rc<str>]>,
        numbers: Rc<[f64]>,
    },
    Other(crate::value::Value),
}

pub(crate) fn parse(value: &crate::value::Value) -> Raw {
    use crate::value::Value;
    match value {
        Value::Num(n) => Raw {
            op: Op::Set,
            kind: RawKind::Num(*n, None),
        },
        Value::Bool(b) => Raw {
            op: Op::Set,
            kind: RawKind::Num(*b as u8 as f64, None),
        },
        Value::Str(s) => parse_str(s),
        other => Raw {
            op: Op::Set,
            kind: RawKind::Other(other.clone()),
        },
    }
}

pub(crate) fn parse_str(text: &str) -> Raw {
    let trimmed = text.trim();
    let (op, rest) = match trimmed.get(..2) {
        Some("+=") => (Op::Add, &trimmed[2..]),
        Some("-=") => (Op::Sub, &trimmed[2..]),
        Some("*=") => (Op::Mul, &trimmed[2..]),
        _ => (Op::Set, trimmed),
    };
    let rest = rest.trim();
    if let Some(color) = parse_color(rest) {
        return Raw {
            op,
            kind: RawKind::Color(color),
        };
    }
    if let Some((n, unit)) = number_with_unit(rest) {
        return Raw {
            op,
            kind: RawKind::Num(n, unit),
        };
    }
    let (pieces, numbers) = decompose(rest);
    if numbers.is_empty() {
        return Raw {
            op,
            kind: RawKind::Other(crate::value::Value::Str(rest.into())),
        };
    }
    Raw {
        op,
        kind: RawKind::Complex { pieces, numbers },
    }
}

/// `"12.5px"` → (12.5, Some("px")); a bare number has no unit.
fn number_with_unit(text: &str) -> Option<(f64, Option<Rc<str>>)> {
    let end = number_end(text, 0)?;
    if end == 0 {
        return None;
    }
    let n: f64 = text[..end].parse().ok()?;
    let unit = text[end..].trim();
    if unit.is_empty() {
        return Some((n, None));
    }
    unit.chars()
        .all(|c| c.is_ascii_alphabetic() || c == '%')
        .then(|| (n, Some(unit.into())))
}
/// The end of a number starting at `start`, if one does.
fn number_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = start;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }
    let digits = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == digits || (i == digits + 1 && bytes[digits] == b'.') {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'-' || bytes[j] == b'+') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    Some(i)
}
/// Splits text into the pieces around its numbers.
pub(crate) fn decompose(text: &str) -> (Rc<[Rc<str>]>, Rc<[f64]>) {
    let mut pieces = Vec::new();
    let mut numbers = Vec::new();
    let mut piece_start = 0;
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let starts_number = bytes[i].is_ascii_digit()
            || ((bytes[i] == b'-' || bytes[i] == b'.')
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
                && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()));
        if starts_number && let Some(end) = number_end(text, i) {
            pieces.push(Rc::from(&text[piece_start..i]));
            numbers.push(text[i..end].parse().unwrap_or(0.));
            piece_start = end;
            i = end;
        } else {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    pieces.push(Rc::from(&text[piece_start..]));
    (pieces.into(), numbers.into())
}
pub(crate) fn compose(pieces: &[Rc<str>], numbers: &[f64]) -> String {
    let mut out = String::new();
    for (i, piece) in pieces.iter().enumerate() {
        out.push_str(piece);
        if let Some(n) = numbers.get(i) {
            out.push_str(&format_number(*n));
        }
    }
    out
}
pub(crate) fn format_number(n: f64) -> String {
    let rounded = (n * 1e4).round() / 1e4;
    if rounded.fract() == 0. && rounded.abs() < 1e15 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}
pub(crate) fn format_color(c: [f64; 4]) -> String {
    let byte = |v: f64| v.round().clamp(0., 255.) as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(c[0]),
        byte(c[1]),
        byte(c[2]),
        byte(c[3] * 255.)
    )
}

/// Hex (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `rgb()`/`rgba()`,
/// `hsl()`/`hsla()` and a few names.
pub(crate) fn parse_color(text: &str) -> Option<[f64; 4]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let mut digits = [0u8; 8];
        let mut len = 0;
        for c in hex.chars() {
            *digits.get_mut(len)? = c.to_digit(16)? as u8;
            len += 1;
        }
        let pair = |i: usize| (digits[i] * 16 + digits[i + 1]) as f64;
        let single = |i: usize| (digits[i] * 17) as f64;
        return Some(match len {
            3 => [single(0), single(1), single(2), 1.],
            4 => [single(0), single(1), single(2), single(3) / 255.],
            6 => [pair(0), pair(2), pair(4), 1.],
            8 => [pair(0), pair(2), pair(4), pair(6) / 255.],
            _ => return None,
        });
    }
    let lower = text.to_ascii_lowercase();
    let args = |prefix: &str| -> Option<Vec<(f64, bool)>> {
        let inner = lower
            .strip_prefix(prefix)?
            .trim()
            .strip_prefix('(')?
            .strip_suffix(')')?;
        inner
            .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| match s.strip_suffix('%') {
                Some(p) => p.parse().ok().map(|v: f64| (v, true)),
                None => s.trim_end_matches("deg").parse().ok().map(|v| (v, false)),
            })
            .collect()
    };
    let alpha = |v: Option<&(f64, bool)>| v.map_or(1., |(a, pct)| if *pct { a / 100. } else { *a });
    for prefix in ["rgba", "rgb"] {
        if let Some(v) = args(prefix) {
            if v.len() < 3 {
                return None;
            }
            let channel = |(n, pct): (f64, bool)| if pct { n * 2.55 } else { n };
            return Some([channel(v[0]), channel(v[1]), channel(v[2]), alpha(v.get(3))]);
        }
    }
    for prefix in ["hsla", "hsl"] {
        if let Some(v) = args(prefix) {
            if v.len() < 3 {
                return None;
            }
            let (h, s, l) = (v[0].0.rem_euclid(360.) / 360., v[1].0 / 100., v[2].0 / 100.);
            let [r, g, b] = hsl_to_rgb(h, s, l);
            return Some([r * 255., g * 255., b * 255., alpha(v.get(3))]);
        }
    }
    Some(match lower.as_str() {
        "transparent" => [0., 0., 0., 0.],
        "black" => [0., 0., 0., 1.],
        "white" => [255., 255., 255., 1.],
        "red" => [255., 0., 0., 1.],
        "green" => [0., 128., 0., 1.],
        "lime" => [0., 255., 0., 1.],
        "blue" => [0., 0., 255., 1.],
        "yellow" => [255., 255., 0., 1.],
        "cyan" | "aqua" => [0., 255., 255., 1.],
        "magenta" | "fuchsia" => [255., 0., 255., 1.],
        "orange" => [255., 165., 0., 1.],
        "purple" => [128., 0., 128., 1.],
        "gray" | "grey" => [128., 128., 128., 1.],
        _ => return None,
    })
}
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s == 0. {
        return [l, l, l];
    }
    let q = if l < 0.5 { l * (1. + s) } else { l + s - l * s };
    let p = 2. * l - q;
    let hue = |mut t: f64| {
        if t < 0. {
            t += 1.;
        }
        if t > 1. {
            t -= 1.;
        }
        if t < 1. / 6. {
            p + (q - p) * 6. * t
        } else if t < 0.5 {
            q
        } else if t < 2. / 3. {
            p + (q - p) * (2. / 3. - t) * 6.
        } else {
            p
        }
    };
    [hue(h + 1. / 3.), hue(h), hue(h - 1. / 3.)]
}

/// Converts a length to points. `percent_of` is what 100% is.
pub(crate) fn length(n: f64, unit: Option<&str>, percent_of: f64) -> f64 {
    match unit {
        Some("%") => n / 100. * percent_of,
        Some("rem" | "em") => n * 16.,
        Some("vw" | "vh") => n / 100. * percent_of,
        _ => n,
    }
}
/// Converts an angle to radians; a bare number is degrees, as in anime.js.
pub(crate) fn angle(n: f64, unit: Option<&str>) -> f64 {
    match unit {
        Some("rad") => n,
        Some("turn") => n * std::f64::consts::TAU,
        Some("grad") => n * std::f64::consts::PI / 200.,
        _ => n.to_radians(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_units_relatives_colors_and_complex_values() {
        assert_eq!(
            parse_str("100px"),
            Raw {
                op: Op::Set,
                kind: RawKind::Num(100., Some("px".into()))
            }
        );
        assert_eq!(
            parse_str("+=1.5turn"),
            Raw {
                op: Op::Add,
                kind: RawKind::Num(1.5, Some("turn".into()))
            }
        );
        assert_eq!(
            parse_str("-=-20"),
            Raw {
                op: Op::Sub,
                kind: RawKind::Num(-20., None)
            }
        );
        assert_eq!(
            parse_str("*=2"),
            Raw {
                op: Op::Mul,
                kind: RawKind::Num(2., None)
            }
        );
        assert_eq!(parse_color("#f80"), Some([255., 136., 0., 1.]));
        assert_eq!(
            parse_color("#ff880080").map(|c| (c[3] * 255.).round()),
            Some(128.)
        );
        assert_eq!(
            parse_color("rgba(10, 20, 30, .5)"),
            Some([10., 20., 30., 0.5])
        );
        let hsl = parse_color("hsl(120, 100%, 50%)").unwrap();
        assert!((hsl[1] - 255.).abs() < 1e-9 && hsl[0].abs() < 1e-9);
        let RawKind::Complex { pieces, numbers } = parse_str("translate(10px, -2.5e1px) $5").kind
        else {
            panic!()
        };
        assert_eq!(&*numbers, &[10., -25., 5.]);
        assert_eq!(compose(&pieces, &[1., 2., 3.]), "translate(1px, 2px) $3");
        assert!(matches!(parse_str("bold").kind, RawKind::Other(_)));
    }
    #[test]
    fn interpolates_by_kind() {
        assert_eq!(V::Num(0.).lerp(&V::Num(10.), 0.25), V::Num(2.5));
        assert_eq!(
            V::Color([0., 0., 0., 0.]).lerp(&V::Color([100., 200., 50., 1.]), 0.5),
            V::Color([50., 100., 25., 0.5])
        );
        assert!((angle(0.5, Some("turn")) - std::f64::consts::PI).abs() < 1e-12);
        assert_eq!(length(50., Some("%"), 300.), 150.);
    }
}
