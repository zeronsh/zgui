//! Grid tracks of any size (CSS `grid-template-columns` and `-rows`): `220
//! 1fr auto`, `minmax(120, 1fr)`, `repeat(auto-fill, minmax(240, 1fr))`.
//! `grid_cols(4)` stays the plain way to say equal tracks. A bare number is
//! points, as everywhere in zgx; `px` is accepted too. Parsed as CSS parses
//! them, with its rules on what may repeat: a list it would reject is an
//! error here, not a layout that silently differs.
use std::{str::FromStr, sync::Arc};

/// How large a track is, or the limit of one (`TrackSize`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackBreadth {
    /// Points (CSS `220px`).
    Points(f32),
    /// A fraction of the grid's size: `1.0` is 100%.
    Percent(f32),
    /// A share of the room left (CSS `1fr`).
    Fr(f32),
    /// Sized by what it holds (CSS `auto`).
    Auto,
    /// As narrow as what it holds can be (CSS `min-content`).
    MinContent,
    /// As wide as what it holds would be (CSS `max-content`).
    MaxContent,
}
impl TrackBreadth {
    /// Whether it is a length or percentage, known without what the track
    /// holds (CSS's `<fixed-breadth>`).
    fn fixed(self) -> bool {
        matches!(self, Self::Points(_) | Self::Percent(_))
    }
}

/// A track's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackSize {
    /// One size (CSS `220px`, `1fr`, `auto`).
    Is(TrackBreadth),
    /// Between two sizes (CSS `minmax(120px, 1fr)`).
    MinMax(TrackBreadth, TrackBreadth),
    /// As `auto`, but no larger than a point or percentage limit (CSS
    /// `fit-content(300px)`).
    FitContent(TrackBreadth),
}
impl TrackSize {
    /// Whether an auto-repeated list may hold it (CSS's `<fixed-size>`): it
    /// has to say how many tracks fit before anything is measured.
    fn fixed(self) -> bool {
        match self {
            Self::Is(size) => size.fixed(),
            Self::MinMax(min, max) => min.fixed() || (max.fixed() && min != TrackBreadth::Auto),
            Self::FitContent(_) => false,
        }
    }
}

/// How often `repeat()` repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackRepeat {
    /// This many times.
    Times(u16),
    /// As many as fit, those left empty keeping their room (CSS `auto-fill`).
    AutoFill,
    /// As many as fit, those left empty collapsing (CSS `auto-fit`).
    AutoFit,
}

/// One entry of a track list.
#[derive(Clone, Debug, PartialEq)]
pub enum Track {
    /// A track.
    Size(TrackSize),
    /// Tracks repeated (CSS `repeat()`).
    Repeat(TrackRepeat, Vec<TrackSize>),
}

/// A grid's tracks along one axis: parse one with `"220 1fr auto".parse()`.
#[derive(Clone, Debug, PartialEq)]
pub struct GridTracks(pub Vec<Track>);

/// A grid's track lists, as a style holds them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridTemplate {
    /// Its columns (CSS `grid-template-columns`).
    pub columns: Option<Arc<GridTracks>>,
    /// Its rows (CSS `grid-template-rows`).
    pub rows: Option<Arc<GridTracks>>,
}

/// The tracks `repeat()` may produce at most, as Firefox's limit on lines
/// (`layout::GRID_LINES`).
const MOST_TRACKS: u16 = 10_000;

fn number(text: &str, what: &str) -> Result<f32, String> {
    let value: f32 = text
        .parse()
        .map_err(|_| format!("`{what}` is not a track size"))?;
    if !value.is_finite() || value < 0. {
        return Err(format!("`{what}`: a track can't be negative"));
    }
    Ok(value)
}
fn breadth(text: &str) -> Result<TrackBreadth, String> {
    Ok(match text {
        "auto" => TrackBreadth::Auto,
        "min-content" => TrackBreadth::MinContent,
        "max-content" => TrackBreadth::MaxContent,
        _ => {
            if let Some(share) = text.strip_suffix("fr") {
                TrackBreadth::Fr(number(share, text)?)
            } else if let Some(percent) = text.strip_suffix('%') {
                TrackBreadth::Percent(number(percent, text)? / 100.)
            } else {
                TrackBreadth::Points(number(text.strip_suffix("px").unwrap_or(text), text)?)
            }
        }
    })
}
/// `text`'s arguments if it is the call `name(…)`.
fn call<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
}
/// `text` split at `split` outside parentheses, each piece trimmed and none
/// empty.
fn pieces(text: &str, split: impl Fn(char) -> bool) -> Result<Vec<&str>, String> {
    let (mut depth, mut start, mut out) = (0i32, 0, Vec::new());
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if depth == 0 && split(c) => {
                out.push(&text[start..at]);
                start = at + c.len_utf8();
            }
            _ => {}
        }
        if depth < 0 {
            return Err(format!("`{text}`: a `)` with no `(`"));
        }
    }
    if depth != 0 {
        return Err(format!("`{text}`: a `(` with no `)`"));
    }
    out.push(&text[start..]);
    Ok(out
        .into_iter()
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .collect())
}
fn commas(text: &str, name: &str, count: usize) -> Result<Vec<String>, String> {
    let args = pieces(text, |c| c == ',')?;
    if args.len() != count {
        return Err(format!(
            "`{name}({text})`: it takes {count} argument{}",
            if count == 1 { "" } else { "s" }
        ));
    }
    Ok(args.into_iter().map(str::to_owned).collect())
}
fn size(text: &str) -> Result<TrackSize, String> {
    if let Some(args) = call(text, "minmax") {
        let args = commas(args, "minmax", 2)?;
        let (min, max) = (breadth(&args[0])?, breadth(&args[1])?);
        if matches!(min, TrackBreadth::Fr(_)) {
            return Err(format!("`{text}`: a minimum can't be a share of the room"));
        }
        return Ok(TrackSize::MinMax(min, max));
    }
    if let Some(args) = call(text, "fit-content") {
        let args = commas(args, "fit-content", 1)?;
        let limit = breadth(&args[0])?;
        if !limit.fixed() {
            return Err(format!("`{text}`: its limit is a length or percentage"));
        }
        return Ok(TrackSize::FitContent(limit));
    }
    breadth(text).map(TrackSize::Is)
}
fn repeat(args: &str) -> Result<Track, String> {
    let at = pieces(args, |c| c == ',')?;
    if at.len() != 2 {
        return Err(format!("`repeat({args})`: it takes a count and tracks"));
    }
    let tracks = pieces(at[1], char::is_whitespace)?
        .into_iter()
        .map(size)
        .collect::<Result<Vec<_>, _>>()?;
    let times = match at[0] {
        "auto-fill" => TrackRepeat::AutoFill,
        "auto-fit" => TrackRepeat::AutoFit,
        count => match count.parse::<u16>() {
            Ok(count) if (1..=MOST_TRACKS).contains(&count) => TrackRepeat::Times(count),
            _ => {
                return Err(format!(
                    "`repeat({args})`: its count is auto-fill, auto-fit or 1 to {MOST_TRACKS}"
                ));
            }
        },
    };
    if !matches!(times, TrackRepeat::Times(_)) && !tracks.iter().all(|track| track.fixed()) {
        return Err(format!(
            "`repeat({args})`: {} needs a fixed size to count what fits: \
             `minmax(240, 1fr)`, not `1fr` or `auto`",
            at[0]
        ));
    }
    Ok(Track::Repeat(times, tracks))
}

impl FromStr for GridTracks {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let mut tracks = Vec::new();
        for piece in pieces(text, char::is_whitespace)? {
            tracks.push(match call(piece, "repeat") {
                Some(args) => repeat(args)?,
                None => Track::Size(size(piece)?),
            });
        }
        if tracks.is_empty() {
            return Err("a track list needs at least one track".into());
        }
        let auto = |track: &&Track| {
            matches!(
                track,
                Track::Repeat(TrackRepeat::AutoFill | TrackRepeat::AutoFit, _)
            )
        };
        if tracks.iter().filter(auto).count() > 1 {
            return Err(format!("`{text}`: one auto-fill or auto-fit at most"));
        }
        if tracks.iter().any(|t| auto(&t))
            && tracks.iter().any(|track| {
                matches!(track, Track::Size(size) if !size.fixed())
                    || matches!(track, Track::Repeat(TrackRepeat::Times(_), sizes)
                        if !sizes.iter().all(|size| size.fixed()))
            })
        {
            return Err(format!(
                "`{text}`: beside auto-fill or auto-fit every track needs a fixed size"
            ));
        }
        Ok(Self(tracks))
    }
}

#[cfg(test)]
mod tests {
    use super::{TrackBreadth::*, TrackRepeat::*, TrackSize::*, *};

    fn parse(text: &str) -> Vec<Track> {
        text.parse::<GridTracks>().unwrap().0
    }
    fn error(text: &str) -> String {
        text.parse::<GridTracks>().unwrap_err()
    }

    #[test]
    fn track_lists_parse_as_css_writes_them() {
        assert_eq!(
            parse("220 1fr auto 25% 40px min-content max-content"),
            [
                Is(Points(220.)),
                Is(Fr(1.)),
                Is(Auto),
                Is(Percent(0.25)),
                Is(Points(40.)),
                Is(MinContent),
                Is(MaxContent),
            ]
            .map(Track::Size)
        );
        assert_eq!(
            parse("minmax(120, 1fr) fit-content(300)"),
            [MinMax(Points(120.), Fr(1.)), FitContent(Points(300.))].map(Track::Size)
        );
        assert_eq!(
            parse("repeat(auto-fill, minmax(240, 1fr))"),
            [Track::Repeat(AutoFill, vec![MinMax(Points(240.), Fr(1.))])]
        );
        assert_eq!(
            parse("48 repeat(3, 1fr 2fr)"),
            [
                Track::Size(Is(Points(48.))),
                Track::Repeat(Times(3), vec![Is(Fr(1.)), Is(Fr(2.))])
            ]
        );
    }

    /// What CSS rejects, so a list means here what it means there.
    #[test]
    fn what_css_rejects_is_an_error_saying_why() {
        for (text, says) in [
            ("", "at least one"),
            ("1fx", "not a track size"),
            ("-10", "negative"),
            ("minmax(1fr, 2fr)", "minimum"),
            ("minmax(10)", "2 arguments"),
            ("fit-content(1fr)", "length or percentage"),
            ("repeat(0, 1fr)", "count"),
            ("repeat(3)", "count and tracks"),
            ("repeat(auto-fill, 1fr)", "fixed size"),
            ("repeat(auto-fit, auto)", "fixed size"),
            ("repeat(auto-fill, 100) repeat(auto-fit, 100)", "at most"),
            ("repeat(auto-fill, 100) 1fr", "fixed size"),
            ("minmax(10, 1fr", "no `)`"),
            ("10)", "no `(`"),
        ] {
            let error = error(text);
            assert!(error.contains(says), "`{text}`: {error}");
        }
    }
}
