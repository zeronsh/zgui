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
    /// Names for the line here, between the tracks around it (CSS
    /// `[side]`), which items place themselves by (`GridLines`).
    Names(Vec<String>),
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
    /// Its named areas (CSS `grid-template-areas`).
    pub areas: Option<Arc<GridAreas>>,
    /// How items without a place fill it (CSS `grid-auto-flow`).
    pub flow: Option<GridFlow>,
}

/// A grid's named areas (CSS `grid-template-areas`): a string a row, a name
/// a cell, `.` an empty one. Each name covers a rectangle. Parse one with
/// `"head head\nside main".parse()`, a row a line.
#[derive(Clone, Debug, PartialEq)]
pub struct GridAreas {
    /// Each area, its lines from 1: rows, then columns, each start and end.
    pub(crate) areas: Vec<(String, [u16; 4])>,
}
impl FromStr for GridAreas {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let rows: Vec<Vec<&str>> = text
            .lines()
            .map(|row| row.split_whitespace().collect::<Vec<_>>())
            .filter(|row| !row.is_empty())
            .collect();
        let Some(width) = rows.first().map(Vec::len) else {
            return Err("areas need at least one row".into());
        };
        if let Some((at, row)) = rows.iter().enumerate().find(|(_, row)| row.len() != width) {
            return Err(format!(
                "areas: row {} has {} cells, the first {width}",
                at + 1,
                row.len()
            ));
        }
        let mut areas: Vec<(String, [u16; 4])> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            for (c, name) in row.iter().enumerate() {
                if name.chars().all(|c| c == '.') {
                    continue;
                }
                let (r, c) = (r as u16 + 1, c as u16 + 1);
                match areas.iter_mut().find(|(n, _)| n == name) {
                    Some((_, [r0, r1, c0, c1])) => {
                        *r0 = (*r0).min(r);
                        *r1 = (*r1).max(r + 1);
                        *c0 = (*c0).min(c);
                        *c1 = (*c1).max(c + 1);
                    }
                    None => areas.push((name.to_string(), [r, r + 1, c, c + 1])),
                }
            }
        }
        // Each name fills the box around its cells, as CSS requires.
        for (name, [r0, r1, c0, c1]) in &areas {
            for r in *r0..*r1 {
                for c in *c0..*c1 {
                    if rows[r as usize - 1][c as usize - 1] != name {
                        return Err(format!("areas: `{name}` isn't a rectangle"));
                    }
                }
            }
        }
        Ok(Self { areas })
    }
}

/// Where an item lies in its grid by name or line, over its numbered place
/// (`col_start` and the like): its column and its row, either left to the
/// grid's flow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridPlace {
    /// Its columns (CSS `grid-column`).
    pub column: Option<GridLines>,
    /// Its rows (CSS `grid-row`).
    pub row: Option<GridLines>,
}

/// How a grid places items without a place of their own (CSS
/// `grid-auto-flow`): along rows or columns, `dense` filling holes earlier
/// items left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum GridFlow {
    /// Along rows, each item after the last (CSS `row`).
    #[default]
    Row,
    /// Down columns (CSS `column`).
    Column,
    /// Along rows, filling holes earlier items left (CSS `row dense`).
    RowDense,
    /// Down columns, filling holes (CSS `column dense`).
    ColumnDense,
}
impl FromStr for GridFlow {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let dense = words.contains(&"dense");
        let column = words.contains(&"column");
        if words.is_empty()
            || words.len() > 2
            || words
                .iter()
                .any(|w| !matches!(*w, "row" | "column" | "dense"))
            || (column && words.contains(&"row"))
        {
            return Err(format!(
                "`{text}`: a grid's flow is \"row\", \"column\", \"dense\" or \"column dense\""
            ));
        }
        Ok(match (column, dense) {
            (false, false) => Self::Row,
            (true, false) => Self::Column,
            (false, true) => Self::RowDense,
            (true, true) => Self::ColumnDense,
        })
    }
}

/// Where an item lies along one of a grid's axes (CSS `grid-column` and
/// `grid-row`): `"2 / 4"`, `"2 / span 3"`, `"side / main"`, `"main"`
/// (an area or named line, start and end).
#[derive(Clone, Debug, PartialEq)]
pub struct GridLines {
    pub(crate) start: GridEdge,
    pub(crate) end: GridEdge,
}
/// One end of `GridLines`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GridEdge {
    Auto,
    /// A line by number, from 1; negative from the end.
    Line(i16),
    /// So many tracks from the other end.
    Span(u16),
    /// The line `name` (the `n`th so named; 0 the first), or an area's
    /// edge.
    Name(String, i16),
    /// To the `n`th line named `name` from the other end.
    NameSpan(String, u16),
}
impl GridLines {
    /// An area's: from its start line to its end (CSS `grid-area: main`).
    pub fn area(name: &str) -> Self {
        Self {
            start: GridEdge::Name(name.into(), 0),
            end: GridEdge::Name(name.into(), 0),
        }
    }
}
fn edge(text: &str) -> Result<GridEdge, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let number = |w: &str| w.parse::<i32>().ok();
    let name = |w: &str| {
        w.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '-')
            && !matches!(w, "auto" | "span")
            && number(w).is_none()
    };
    let bad = || format!("`{text}`: a grid line is a number, a name, `span` one, or `auto`");
    let line = |n: i32| -> Result<i16, String> {
        match i16::try_from(n) {
            Ok(0) | Err(_) => Err(format!(
                "`{text}`: line 0 doesn't exist; lines count from 1"
            )),
            Ok(n) => Ok(n),
        }
    };
    Ok(match words.as_slice() {
        ["auto"] => GridEdge::Auto,
        [w] if number(w).is_some() => GridEdge::Line(line(number(w).unwrap_or(0))?),
        [w] if name(w) => GridEdge::Name(w.to_string(), 0),
        [a, b] if name(a) && number(b).is_some() => {
            GridEdge::Name(a.to_string(), line(number(b).unwrap_or(0))?)
        }
        [a, b] if number(a).is_some() && name(b) => {
            GridEdge::Name(b.to_string(), line(number(a).unwrap_or(0))?)
        }
        ["span", n] if number(n).is_some_and(|n| n >= 1) => {
            GridEdge::Span(number(n).unwrap_or(1).min(i32::from(u16::MAX)) as u16)
        }
        ["span", w] if name(w) => GridEdge::NameSpan(w.to_string(), 1),
        _ => return Err(bad()),
    })
}
impl FromStr for GridLines {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let mut parts = text.split('/');
        let start = edge(parts.next().unwrap_or(""))?;
        let end = match parts.next() {
            Some(end) => edge(end)?,
            // CSS: a name alone ends at the same name; anything else, auto.
            None => match &start {
                GridEdge::Name(name, _) => GridEdge::Name(name.clone(), 0),
                _ => GridEdge::Auto,
            },
        };
        if parts.next().is_some() {
            return Err(format!("`{text}`: a start and an end, one `/` between"));
        }
        if matches!(
            (&start, &end),
            (
                GridEdge::Span(_) | GridEdge::NameSpan(..),
                GridEdge::Span(_) | GridEdge::NameSpan(..)
            )
        ) {
            return Err(format!("`{text}`: both ends can't span"));
        }
        Ok(Self { start, end })
    }
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
        // Line names in brackets (`[side main]`), the tracks between them.
        let mut rest = text;
        while !rest.is_empty() {
            let (between, names) = match rest.find('[') {
                Some(open) => {
                    let close = rest[open..]
                        .find(']')
                        .ok_or_else(|| format!("`{text}`: a `[` with no `]`"))?;
                    let names: Vec<String> = rest[open + 1..open + close]
                        .split_whitespace()
                        .map(String::from)
                        .collect();
                    let between = &rest[..open];
                    rest = &rest[open + close + 1..];
                    (between, Some(names))
                }
                None => (std::mem::take(&mut rest), None),
            };
            if between.contains(']') {
                return Err(format!("`{text}`: a `]` with no `[`"));
            }
            for piece in pieces(between, char::is_whitespace)? {
                if call(piece, "repeat").is_some() && piece.contains('[') {
                    return Err(format!(
                        "`{text}`: line names inside repeat() aren't supported"
                    ));
                }
                tracks.push(match call(piece, "repeat") {
                    Some(args) => repeat(args)?,
                    None => Track::Size(size(piece)?),
                });
            }
            if let Some(names) = names {
                if names.is_empty() {
                    return Err(format!("`{text}`: `[]` names no line"));
                }
                tracks.push(Track::Names(names));
            }
        }
        if !tracks.iter().any(|track| !matches!(track, Track::Names(_))) {
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
    fn line_names_sit_between_tracks() {
        assert_eq!(
            parse("[side] 200 [main start] 1fr [end]"),
            [
                Track::Names(vec!["side".into()]),
                Track::Size(Is(Points(200.))),
                Track::Names(vec!["main".into(), "start".into()]),
                Track::Size(Is(Fr(1.))),
                Track::Names(vec!["end".into()]),
            ]
        );
        for bad in [
            "[side 200",
            "side] 200",
            "[] 1fr",
            "[a]",
            "repeat(2, [a] 1fr)",
        ] {
            assert!(bad.parse::<GridTracks>().is_err(), "{bad}");
        }
    }
    #[test]
    fn areas_are_rectangles_of_names() {
        let areas: GridAreas = "head head\nside main\n. main".parse().unwrap();
        assert_eq!(
            areas.areas,
            [
                ("head".to_string(), [1, 2, 1, 3]),
                ("side".to_string(), [2, 3, 1, 2]),
                ("main".to_string(), [2, 4, 2, 3]),
            ]
        );
        assert!("a b\nb a".parse::<GridAreas>().is_err(), "not a rectangle");
        assert!(
            "a b\na".parse::<GridAreas>().is_err(),
            "rows of unequal width"
        );
        assert!("".parse::<GridAreas>().is_err());
    }
    #[test]
    fn grid_lines_parse_as_css_grid_column_does() {
        use GridEdge::*;
        let lines = |text: &str| {
            let l: GridLines = text.parse().unwrap();
            (l.start, l.end)
        };
        assert_eq!(lines("2 / 4"), (Line(2), Line(4)));
        assert_eq!(lines("2 / span 3"), (Line(2), Span(3)));
        assert_eq!(lines("-1"), (Line(-1), Auto));
        assert_eq!(
            lines("side / main"),
            (Name("side".into(), 0), Name("main".into(), 0))
        );
        assert_eq!(
            lines("main"),
            (Name("main".into(), 0), Name("main".into(), 0))
        );
        assert_eq!(
            lines("col 2 / span col"),
            (Name("col".into(), 2), NameSpan("col".into(), 1))
        );
        assert_eq!(lines("auto / span 2"), (Auto, Span(2)));
        for bad in ["0", "span 0", "span 1 / span 2", "1 / 2 / 3", "", "1.5"] {
            assert!(bad.parse::<GridLines>().is_err(), "{bad}");
        }
        assert_eq!("dense".parse(), Ok(GridFlow::RowDense));
        assert_eq!("column dense".parse(), Ok(GridFlow::ColumnDense));
        assert!("row column".parse::<GridFlow>().is_err());
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
