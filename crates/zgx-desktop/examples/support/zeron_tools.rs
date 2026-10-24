//! Tool groups and the working trailer, after Zeron's `crates/ui/src/
//! transcript.rs` (`render_tool_group`, `activity_rail`, `chip_header_row`,
//! `detail_body`, `render_working_trailer`), `roll_text.rs` and `loaders.rs`.
//!
//! A group is a quiet past-tense summary over an activity rail: each row
//! hangs off a trunk by a rounded elbow, rows arriving live stagger in (the
//! connector draws, then the content rises along the branch), and the fold,
//! the row details and the summary all move on Zeron's motion specs. Heights
//! are analytic, as in Zeron, so every fold tweens without measuring.

use super::markdown::{self as md, Change};
use super::model::{self, Block, PartKind, Tool};
use super::theme::{
    ACCENT, AmbientGuard, Anim, DANGER, EASE_OUT, EASE_OUT_EXPO, EASE_OUT_QUINT, GLYPH, GRADIENT,
    MONO, RESIZE, SANS, SUCCESS, Spec, TEXT, TEXT_FAINT, TEXT_MUTED, Tween, alpha, file_icon,
    gspin, hairline, icon_data, ink, mix,
};
use super::view::Ctx;
use serde_json::json;
use std::{cell::RefCell, collections::HashMap, f32::consts::FRAC_PI_2, rc::Rc, sync::Arc};
use zgx::{
    compose::prelude::*,
    input::InputEvent,
    reactive::Signal,
    rich_text::{RichText, TextMeasure, TextRun},
    scene::Color,
    text_layout::{FontStyle, LineHeight},
};

// Constants (transcript.rs:89-163) -------------------------------------------

const GUTTER: f32 = 48.;
const TEXT_GAP: f32 = 8.;
const TRUNK_X: f32 = 12.5;
const BEND: f32 = 6.;
const BRANCH_END_X: f32 = 28.;
const ICON_LEFT: f32 = 32.;
const ICON_SIZE: f32 = 16.;
const TEXT_SIZE: f32 = 12.;
const LINE: f32 = 18.;
/// Group header height.
pub const HEADER_HEIGHT: f32 = 26.;
/// A rail row's height (`TOOL_TREE_ROW_HEIGHT`).
pub const ROW_HEIGHT: f32 = 32.;
/// A subagent card row's height (`CHIP_HEIGHT`).
pub const CARD_ROW_HEIGHT: f32 = 38.;
const CARD_HEIGHT: f32 = 30.;
const CARD_HEADER_HEIGHT: f32 = 28.;
/// Padding above the first row of an open group.
pub const CHIPS_TOP_PAD: f32 = 2.;
const TOOL_FOLD: Spec = Spec(0.14, EASE_OUT);
const ROW_REVEAL: Spec = Spec(0.36, EASE_OUT_EXPO);
const CONNECTOR_REVEAL: Spec = Spec(0.48, EASE_OUT_QUINT);
const FIRST_ROW_DELAY: f32 = 0.090;
const ROW_STAGGER: f32 = 0.065;
const SHIMMER_PERIOD: f32 = 3.4;
const SHIMMER_HALF_WIDTH: f32 = 0.36;
/// A closed row detail stays mounted this long (`FOLD_TWEEN_WINDOW`).
const DETAIL_RETAIN: f32 = 0.4;
const OUTPUT_MAX_LINES: usize = 24;
const FULL_OUTPUT_MAX_LINES: usize = 400;
const OUTPUT_LINE_MAX_CHARS: usize = 8 * 1024;
const OUTPUT_BODY_PAD: f32 = 12.;
const DETAIL_SEPARATOR: f32 = 1.;
const AFFORDANCE_HEIGHT: f32 = 24.;
const CALL_WRAP_COLS: usize = 80;
const THOUGHT_WRAP_COLS: usize = 96;
const DIFF_LINE_HEIGHT: f32 = 21.;
const HUNK_HEADER_HEIGHT: f32 = 28.;
const DIFF_BOTTOM_PAD: f32 = 8.;
const DIFF_MAX_LINES: usize = 600;
const DIFF_CONTEXT: usize = 3;
const MARKER_WIDTH: f32 = 28.;
const ACCENT_BAR_WIDTH: f32 = 3.;
/// Gap between split segments of one block (agents leave the fold).
const SEGMENT_GAP: f32 = 10.;

// Group summary (proto view.rs:540-611, transcript.rs:1993) ------------------

/// What the summary needs of one member.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryItem {
    pub thought: bool,
    pub verb: &'static str,
    /// The file path for edits (unique paths count once).
    pub path: String,
    pub error: bool,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "Thought process · Ran 3 commands · edited 2 files · 1 failed".
pub fn summary(items: &[SummaryItem]) -> String {
    let mut segments: Vec<String> = Vec::new();
    match items.iter().filter(|i| i.thought).count() {
        0 => {}
        1 => segments.push("thought process".into()),
        n => segments.push(format!("thought {n} times")),
    }
    let calls: Vec<&SummaryItem> = items.iter().filter(|i| !i.thought).collect();
    if !calls.is_empty() {
        let (mut commands, mut reads, mut searches, mut fetches, mut todos, mut other) =
            (0, 0, 0, 0, 0, 0);
        let mut edited: Vec<&str> = Vec::new();
        let mut failed = 0;
        for call in &calls {
            if call.error {
                failed += 1;
            }
            match call.verb {
                "Run" => commands += 1,
                "Write" | "Edit" | "Patch" => {
                    let path = if call.verb == "Patch" && call.path.is_empty() {
                        "patch"
                    } else {
                        call.path.as_str()
                    };
                    if !edited.contains(&path) {
                        edited.push(path);
                    }
                }
                "Read" => reads += 1,
                "Search" | "Glob" | "Web" => searches += 1,
                "Fetch" => fetches += 1,
                "Todo" => todos += 1,
                _ => other += 1,
            }
        }
        let mut base: Vec<String> = Vec::new();
        if commands > 0 {
            base.push(format!("ran {}", plural(commands, "command", "commands")));
        }
        if !edited.is_empty() {
            base.push(format!("edited {}", plural(edited.len(), "file", "files")));
        }
        if reads > 0 {
            base.push(format!("read {}", plural(reads, "file", "files")));
        }
        if searches > 0 {
            base.push(format!("searched {}", plural(searches, "time", "times")));
        }
        if fetches > 0 {
            base.push(format!("fetched {}", plural(fetches, "page", "pages")));
        }
        if todos > 0 {
            base.push("updated todos".into());
        }
        if other > 0 {
            base.push(format!("called {}", plural(other, "tool", "tools")));
        }
        if base.is_empty() {
            base.push(plural(calls.len(), "tool", "tools"));
        }
        if failed > 0 {
            base.push(format!("{failed} failed"));
        }
        segments.push(base.join(" · "));
    }
    let mut text = segments.join(" · ");
    if let Some(first) = text.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    text
}

// Motion maths ------------------------------------------------------------------

/// The title sweep at `x` (0..1 across the title) for `phase` (0..1): a
/// linear tent of half-width 0.36, copies three title-widths apart.
pub fn shimmer_amount(x: f32, phase: f32) -> f32 {
    let primary = -2.5 + phase.clamp(0., 1.) * 6.;
    (-2..=2)
        .map(|copy| primary + copy as f32 * 3.)
        .map(|centre| (1. - (x - centre).abs() / SHIMMER_HALF_WIDTH).clamp(0., 1.))
        .fold(0., f32::max)
}

pub fn shimmer_phase(elapsed: f32) -> f32 {
    (elapsed.max(0.) / SHIMMER_PERIOD).fract()
}

/// One arrival's connector progress split into (incoming trunk, branch).
pub fn connector_parts(progress: f32, has_predecessor: bool) -> (f32, f32) {
    let progress = progress.clamp(0., 1.);
    let (incoming_start, incoming_end, branch_start) = if has_predecessor {
        (0.45, 0.72, 0.68)
    } else {
        (0., 0.62, 0.58)
    };
    let incoming = ((progress - incoming_start) / (incoming_end - incoming_start)).clamp(0., 1.);
    let branch = ((progress - branch_start) / (1. - branch_start)).clamp(0., 1.);
    (incoming, branch)
}

/// The trunk below a row grows with the NEXT row's arrival.
pub fn connector_continuation(next_progress: Option<f32>) -> f32 {
    next_progress.map_or(0., |p| (p / 0.45).clamp(0., 1.))
}

/// The elbow (a quadratic sampled at 24 steps) and the straight leg,
/// relative to the bend's top, revealed by arc length.
pub fn branch_points(progress: f32) -> Vec<(f32, f32)> {
    let mut path: Vec<(f32, f32)> = (0..=24)
        .map(|step| {
            let t = step as f32 / 24.;
            (BEND * t * t, BEND * (2. * t - t * t))
        })
        .collect();
    path.push((BRANCH_END_X - TRUNK_X, BEND));
    if progress >= 1. {
        return path;
    }
    let lengths: Vec<f32> = path
        .windows(2)
        .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
        .collect();
    let mut remaining = lengths.iter().sum::<f32>() * progress.clamp(0., 1.);
    let mut visible = vec![path[0]];
    for (pair, length) in path.windows(2).zip(lengths) {
        if remaining <= 0. {
            break;
        }
        let t = (remaining / length).min(1.);
        visible.push((
            pair[0].0 + (pair[1].0 - pair[0].0) * t,
            pair[0].1 + (pair[1].1 - pair[0].1) * t,
        ));
        remaining -= length;
    }
    visible
}

/// When a live row starts its entrance: rows that arrived with their group
/// wait 90 ms for the header; the model already staggers same-frame rows
/// 65 ms apart in `born`. History (`born == None`) never animates.
fn arrival_start(born: Option<f32>, head_born: Option<f32>, index: usize) -> Option<f32> {
    let born = born?;
    let with_group =
        head_born.is_some_and(|head| (born - (head + ROW_STAGGER * index as f32)).abs() < 1e-3);
    Some(born + if with_group { FIRST_ROW_DELAY } else { 0. })
}

// Roll text (roll_text.rs) --------------------------------------------------------

const ROLL_DURATION: f32 = 0.55;
const ROLL_STAGGER: f32 = 0.3;
const ROLL_TRAVEL_EM: f32 = 0.35;
const ROLL_TOTAL: f32 = ROLL_DURATION * (1. + ROLL_STAGGER);

/// Byte lengths of the shared leading and trailing runs, never overlapping.
pub fn shared_ends(from: &str, to: &str) -> (usize, usize) {
    let prefix: usize = from
        .chars()
        .zip(to.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    let room = (from.len() - prefix).min(to.len() - prefix);
    let suffix = from[prefix..]
        .chars()
        .rev()
        .zip(to[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .scan(0, |sum, len| {
            *sum += len;
            Some(*sum)
        })
        .take_while(|sum| *sum <= room)
        .last()
        .unwrap_or(0);
    (prefix, suffix)
}

/// Start delays of the changed glyphs at `xs`, swept left to right across
/// 30% of the glyph duration.
pub fn roll_delays(xs: &[f32]) -> Vec<f32> {
    let count = xs.len();
    let min = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let spread =
        ROLL_DURATION * ROLL_STAGGER * count.saturating_sub(1) as f32 / count.max(1) as f32;
    xs.iter()
        .map(|x| {
            if max > min {
                spread * ((x - min) / (max - min)).clamp(0., 1.)
            } else {
                0.
            }
        })
        .collect()
}

#[derive(Clone, PartialEq)]
struct Roll {
    from: String,
    to: String,
    at: f32,
    generation: u32,
}

/// The ambient (30 Hz) clock, tracked until `end`.
fn ambient_at(anim: &Anim, end: f32) -> f32 {
    let now = anim.ambient_clock.with_untracked(|t| *t);
    if now >= end {
        now
    } else {
        anim.ambient_clock.get()
    }
}

fn shape_width(measure: &TextMeasure, text: &str, size: f32) -> f32 {
    if text.is_empty() {
        return 0.;
    }
    let font = FontStyle {
        family: SANS.into(),
        line_height: LineHeight::px((size * 1.5).round()),
        ..FontStyle::default()
    };
    let run = TextRun {
        range: 0..text.len(),
        font,
        font_size: size,
        ..Default::default()
    };
    RichText::new(text, vec![run]).map_or(0., |rich| measure.measure(&rich, None).0)
}

/// The x of each char boundary of `text` (a trailing sentinel keeps
/// trailing spaces counted), and the line's width.
fn glyph_xs(measure: &TextMeasure, text: &str, size: f32) -> (Vec<(usize, char, f32)>, f32) {
    let sentinel = shape_width(measure, "l", size);
    let xs = text
        .char_indices()
        .map(|(i, c)| {
            let x = if i == 0 {
                0.
            } else {
                shape_width(measure, &format!("{}l", &text[..i]), size) - sentinel
            };
            (i, c, x)
        })
        .collect();
    (xs, shape_width(measure, text, size))
}

/// A label that rolls when `text` changes: the shared ends stay (the tail
/// slides with the width), the changed middle rolls out upward and in from
/// below, staggered left to right, over Zeron's 550 ms quint; the width
/// glides on `RESIZE`. The first value never rolls. Rolls run on the 30 Hz
/// ambient clock, like Zeron's pulse lease.
pub fn roll_text(
    cx: &mut Context,
    anim: &Anim,
    text: impl Fn() -> String + 'static,
    size: f32,
    color: Color,
) -> View {
    let text: Rc<dyn Fn() -> String> = Rc::new(text);
    let line = (size * 1.5).round();
    let rest = text.clone();
    roll_core(cx, anim, text, size, Some(color), move |_| {
        let rest = rest.clone();
        text_signal(move || rest())
            .min_w(0.)
            .truncate()
            .text_size(size)
            .line_height(line)
            .text_color(color)
    })
}

/// [`roll_text`] with the caller's at-rest view (`color: None` inherits).
fn roll_core(
    cx: &mut Context,
    anim: &Anim,
    text: Rc<dyn Fn() -> String>,
    size: f32,
    color: Option<Color>,
    mut rest: impl FnMut(&mut Context) -> View + 'static,
) -> View {
    let initial = text();
    let roll = cx.state(Roll {
        from: initial.clone(),
        to: initial,
        at: f32::NEG_INFINITY,
        generation: 0,
    });
    let guard: Rc<RefCell<Option<AmbientGuard>>> = Rc::default();
    {
        let (roll, anim, guard) = (roll.clone(), anim.clone(), guard.clone());
        let effect = cx.runtime().effect(move || {
            let next = text();
            if roll.with_untracked(|r| r.to != next) {
                let now = anim.now();
                roll.update(|r| {
                    r.from = std::mem::replace(&mut r.to, next);
                    r.at = now;
                    r.generation += 1;
                });
                guard.borrow_mut().get_or_insert_with(|| anim.ambient());
            }
        });
        cx.retain(effect);
    }
    let measure = cx.text_measure();
    let anim = anim.clone();
    let select = {
        let (roll, anim) = (roll.clone(), anim.clone());
        move || {
            let (generation, at) = roll.with(|r| (r.generation, r.at));
            let end = at + ROLL_TOTAL;
            let live = generation > 0 && ambient_at(&anim, end) < end;
            if !live {
                guard.borrow_mut().take();
            }
            live.then_some(generation)
        }
    };
    switch(select, move |key, cx| match key {
        None => rest(cx),
        Some(_) => roll_glyphs(
            &anim,
            &measure,
            roll.with_untracked(Clone::clone),
            size,
            color,
        ),
    })
}

fn roll_glyphs(
    anim: &Anim,
    measure: &TextMeasure,
    roll: Roll,
    size: f32,
    color: Option<Color>,
) -> View {
    let line = (size * 1.5).round();
    let travel = size * ROLL_TRAVEL_EM;
    let room = travel * 2.;
    let halo = (size * 0.1 * 3.).ceil();
    let (prefix, suffix) = shared_ends(&roll.from, &roll.to);
    let (from_xs, from_width) = glyph_xs(measure, &roll.from, size);
    let (to_xs, to_width) = glyph_xs(measure, &roll.to, size);
    let x_at = |xs: &[(usize, char, f32)], width: f32, index: usize| {
        xs.iter()
            .find(|(i, _, _)| *i == index)
            .map_or(width, |(_, _, x)| *x)
    };
    let (from_len, to_len) = (roll.from.len(), roll.to.len());
    let suffix_to_x = x_at(&to_xs, to_width, to_len - suffix);
    let suffix_dx = x_at(&from_xs, from_width, from_len - suffix) - suffix_to_x;
    let start = roll.at;
    let elapsed = {
        let anim = anim.clone();
        move || ambient_at(&anim, start + ROLL_TOTAL) - start
    };
    let width =
        move |elapsed: f32| (from_width + (to_width - from_width) * RESIZE.at(elapsed)).ceil();
    let glyph = move |text_: String, left: f32| {
        let view = text(text_)
            .absolute()
            .left(halo + left)
            .top(room)
            .text_size(size)
            .line_height(line)
            .text_wrap(false);
        match color {
            Some(color) => view.text_color(color),
            None => view,
        }
    };
    let mut layer = div()
        .absolute()
        .left(-halo)
        .top(-room)
        .h(line + room * 2.)
        .overflow_hidden()
        .reactive_style({
            let elapsed = elapsed.clone();
            move || Styles::new().w(width(elapsed()) + halo * 2.)
        });
    if prefix > 0 {
        layer = layer.child(glyph(roll.to[..prefix].to_owned(), 0.));
    }
    if suffix > 0 {
        let elapsed = elapsed.clone();
        layer = layer.child(
            glyph(roll.to[to_len - suffix..].to_owned(), suffix_to_x).reactive_style(move || {
                let slide = RESIZE.at(elapsed());
                Styles::new().translate(suffix_dx * (1. - slide), 0.)
            }),
        );
    }
    // Outgoing glyphs rise out; incoming rise in from below.
    for (xs, len, outgoing) in [(&from_xs, from_len, true), (&to_xs, to_len, false)] {
        let changed: Vec<&(usize, char, f32)> = xs
            .iter()
            .filter(|(i, c, _)| *i >= prefix && *i < len - suffix && !c.is_whitespace())
            .collect();
        let delays = roll_delays(&changed.iter().map(|g| g.2).collect::<Vec<_>>());
        for (&(_, ch, x), delay) in changed.into_iter().zip(delays) {
            let elapsed = elapsed.clone();
            layer = layer.child(glyph(ch.to_string(), x).reactive_style(move || {
                let p = EASE_OUT_QUINT.eval((elapsed() - delay) / ROLL_DURATION);
                if outgoing {
                    Styles::new().translate(0., -travel * p).opacity(1. - p)
                } else {
                    Styles::new().translate(0., travel * (1. - p)).opacity(p)
                }
            }));
        }
    }
    div()
        .relative()
        .h(line)
        .min_w(0.)
        .reactive_style(move || Styles::new().w(width(elapsed())))
        .child(layer)
}

// Working trailer helpers (transcript.rs:2250-2320) ----------------------------

pub const FLAVOUR_WORDS: [&str; 21] = [
    "Zeroning",
    "Thinking",
    "Pondering",
    "Scheming",
    "Brewing",
    "Weaving",
    "Tinkering",
    "Musing",
    "Composing",
    "Sifting",
    "Untangling",
    "Distilling",
    "Sketching",
    "Plotting",
    "Riffing",
    "Combobulating",
    "Percolating",
    "Marinating",
    "Noodling",
    "Puzzling",
    "Conjuring",
];
const FLAVOUR_ROTATE_SECS: i64 = 7;

pub fn flavour_word(seed: u64, elapsed_secs: i64) -> &'static str {
    let step = (elapsed_secs.max(0) / FLAVOUR_ROTATE_SECS) as u64;
    FLAVOUR_WORDS[(seed.wrapping_add(step) % FLAVOUR_WORDS.len() as u64) as usize]
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x1_0000_01b3);
    }
    hash
}

/// The trailer seed for a chat: Zeron's 64-bit FNV-1a reduced mod 21, so
/// the word sequence matches Zeron's for the same chat id.
pub fn flavour_seed(chat_id: &str) -> u32 {
    (fnv1a(chat_id.as_bytes()) % FLAVOUR_WORDS.len() as u64) as u32
}

/// "42s", "3m 7s", "2h 5m", "1d 4h".
pub fn format_elapsed(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86_400 {
        format!("{}h {}m", secs / 3_600, (secs % 3_600) / 60)
    } else {
        format!("{}d {}h", secs / 86_400, (secs % 86_400) / 3_600)
    }
}

fn format_kb(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

// Details (transcript.rs:760-960, 2154) ---------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
struct Lines {
    lines: Vec<String>,
    more: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct DiffLine {
    kind: Change,
    old: Option<u32>,
    new: Option<u32>,
    text: String,
}

#[derive(Clone, Debug, PartialEq)]
struct Hunk {
    header: String,
    lines: Vec<DiffLine>,
}

type StyledLine = Vec<(String, md::Style)>;

#[derive(Clone, Debug, PartialEq)]
enum Body {
    Output(Lines),
    Thought(Vec<StyledLine>, usize),
    Diff(Vec<Hunk>),
    Stats(Vec<(String, u64, u64)>),
}

#[derive(Clone, Debug, PartialEq)]
struct Affordance {
    blob: String,
    label: String,
    loading: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
struct Detail {
    invocation: Option<Lines>,
    body: Option<Body>,
    affordance: Option<Affordance>,
}

fn lines_height(lines: usize, more: usize) -> f32 {
    (lines + usize::from(more > 0)) as f32 * LINE + OUTPUT_BODY_PAD
}

fn body_height(body: &Body) -> f32 {
    match body {
        Body::Output(l) => lines_height(l.lines.len(), l.more),
        Body::Thought(lines, more) => lines_height(lines.len(), *more),
        Body::Stats(stats) => stats.len() as f32 * LINE + OUTPUT_BODY_PAD,
        Body::Diff(hunks) => {
            hunks
                .iter()
                .map(|h| HUNK_HEADER_HEIGHT + h.lines.len() as f32 * DIFF_LINE_HEIGHT)
                .sum::<f32>()
                + DIFF_BOTTOM_PAD
        }
    }
}

impl Detail {
    fn expandable(&self) -> bool {
        self.invocation.is_some() || self.body.is_some()
    }
    /// Height an open detail adds under the row header.
    fn extra(&self) -> f32 {
        if !self.expandable() {
            return 0.;
        }
        self.invocation.as_ref().map_or(0., |l| {
            DETAIL_SEPARATOR + lines_height(l.lines.len(), l.more)
        }) + self
            .body
            .as_ref()
            .map_or(0., |b| DETAIL_SEPARATOR + body_height(b))
            + if self.affordance.is_some() {
                AFFORDANCE_HEIGHT
            } else {
                0.
            }
    }
}

fn wrap_cols(line: &str, cols: usize) -> Vec<String> {
    if line.chars().count() <= cols {
        return vec![line.to_owned()];
    }
    line.chars()
        .collect::<Vec<_>>()
        .chunks(cols)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn capped(mut lines: Vec<String>, cap: usize) -> Option<Lines> {
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    let more = lines.len().saturating_sub(cap);
    lines.truncate(cap);
    Some(Lines { lines, more })
}

/// The full invocation, hard-wrapped at 80 columns.
fn call_lines(text: &str) -> Option<Lines> {
    capped(
        text.lines()
            .flat_map(|l| wrap_cols(l, CALL_WRAP_COLS))
            .collect(),
        OUTPUT_MAX_LINES,
    )
}

fn output_line(line: &str) -> String {
    match line.char_indices().nth(OUTPUT_LINE_MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_owned(),
    }
}

fn output_lines(text: &str, cap: usize) -> Option<Lines> {
    capped(text.lines().map(output_line).collect(), cap)
}

/// What a call was, in full: its command, path, pattern or query.
fn invocation_text(tool: &Tool) -> Option<String> {
    if tool.detail.trim().is_empty() {
        (tool.verb == "Patch").then(|| "workspace".to_owned())
    } else {
        Some(tool.detail.clone())
    }
}

/// Word-wrap styled pieces at `budget` chars; continuation lines indent.
fn wrap_styled(
    pieces: &[(String, md::Style)],
    lead: &str,
    indent: usize,
    out: &mut Vec<StyledLine>,
) {
    let budget = THOUGHT_WRAP_COLS.saturating_sub(indent).max(16);
    // Tokens glue across style boundaries so a styled word never splits.
    let mut tokens: Vec<StyledLine> = Vec::new();
    let mut in_token = false;
    for (text, style) in pieces {
        for (ix, segment) in text.split('\n').enumerate() {
            if ix > 0 {
                in_token = false;
            }
            let mut pos = 0;
            while pos < segment.len() {
                let rest = &segment[pos..];
                let ws = rest.chars().next().is_some_and(char::is_whitespace);
                let end = rest
                    .char_indices()
                    .find(|(_, c)| c.is_whitespace() != ws)
                    .map_or(segment.len(), |(i, _)| pos + i);
                if ws {
                    in_token = false;
                } else {
                    if !in_token {
                        tokens.push(Vec::new());
                        in_token = true;
                    }
                    let token = tokens.last_mut().expect("token");
                    match token.last_mut() {
                        Some((t, s)) if s == style => t.push_str(&segment[pos..end]),
                        _ => token.push((segment[pos..end].to_owned(), *style)),
                    }
                }
                pos = end;
            }
        }
    }
    let mut line: StyledLine = vec![(lead.to_owned(), md::Style::Plain)];
    let mut len = 0;
    let continuation = " ".repeat(indent);
    let flush = |line: &mut StyledLine, out: &mut Vec<StyledLine>| {
        out.push(std::mem::replace(
            line,
            vec![(continuation.clone(), md::Style::Plain)],
        ));
    };
    for token in tokens {
        let token_len: usize = token.iter().map(|(t, _)| t.chars().count()).sum();
        if len > 0 && len + 1 + token_len > budget {
            flush(&mut line, out);
            len = 0;
        }
        if len > 0 {
            line.push((" ".into(), md::Style::Plain));
            len += 1;
        }
        // An overlong token hard-splits at the budget.
        for (text, style) in token {
            let mut chars = text.chars().peekable();
            while chars.peek().is_some() {
                let chunk: String = chars
                    .by_ref()
                    .take(budget.saturating_sub(len).max(1))
                    .collect();
                len += chunk.chars().count();
                line.push((chunk, style));
                if len >= budget && chars.peek().is_some() {
                    flush(&mut line, out);
                    len = 0;
                }
            }
        }
    }
    out.push(line);
}

/// A thought's markdown flattened into styled lines (transcript.rs:430),
/// wrapped at 96 columns and capped at 24 lines (a live thought keeps its
/// tail).
fn thought_lines(text: &str, live: bool) -> (Vec<StyledLine>, usize) {
    let mut out: Vec<StyledLine> = Vec::new();
    let pieces = |run: &md::Run, bold: bool| -> Vec<(String, md::Style)> {
        md::inline(run)
            .into_iter()
            .map(|p| {
                let style = match (bold, p.style) {
                    (true, md::Style::Plain) => md::Style::Bold,
                    (true, md::Style::Italic) => md::Style::BoldItalic,
                    (_, style) => style,
                };
                (p.text, style)
            })
            .collect()
    };
    for block in md::parse(text) {
        if !out.is_empty() {
            out.push(Vec::new());
        }
        match &block {
            md::Block::Para(run) => wrap_styled(&pieces(run, false), "", 0, &mut out),
            md::Block::Heading(_, run) => wrap_styled(&pieces(run, true), "", 0, &mut out),
            md::Block::Quote(run) => wrap_styled(&pieces(run, false), "│ ", 2, &mut out),
            md::Block::Item {
                depth,
                marker,
                task,
                run,
            } => {
                let mark = match task {
                    Some(true) => "[x]".to_owned(),
                    Some(false) => "[ ]".to_owned(),
                    None => marker.clone(),
                };
                let lead = format!("{}{mark} ", "  ".repeat(*depth));
                let indent = lead.chars().count();
                wrap_styled(&pieces(run, false), &lead, indent, &mut out);
            }
            md::Block::Code { run, .. } => {
                for line in run.text.trim_end_matches('\n').lines() {
                    for chunk in wrap_cols(line, THOUGHT_WRAP_COLS) {
                        out.push(vec![(chunk, md::Style::Code)]);
                    }
                }
            }
            md::Block::Table { header, rows, .. } => {
                for cells in std::iter::once(header).chain(rows) {
                    let text = cells
                        .iter()
                        .map(|c| c.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    wrap_styled(&[(text, md::Style::Plain)], "", 0, &mut out);
                }
            }
            md::Block::Rule => out.push(vec![("———".into(), md::Style::Plain)]),
        }
    }
    let blank = |l: &StyledLine| l.iter().all(|(t, _)| t.trim().is_empty());
    while out.last().is_some_and(blank) {
        out.pop();
    }
    let more = out.len().saturating_sub(OUTPUT_MAX_LINES);
    if more > 0 {
        if live {
            out.drain(..more);
            while out.first().is_some_and(blank) {
                out.remove(0);
            }
        } else {
            out.truncate(OUTPUT_MAX_LINES);
        }
    }
    (out, more)
}

/// Unified hunks with three lines of context and dual line numbers.
fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    let full: Vec<(Change, String)> = md::line_diff(old, new, usize::MAX)
        .into_iter()
        .flatten()
        .collect();
    if full.iter().all(|(c, _)| *c == Change::Same) {
        return Vec::new();
    }
    let (mut o, mut n) = (0_u32, 0_u32);
    let numbered: Vec<DiffLine> = full
        .into_iter()
        .map(|(kind, text)| {
            let (old_no, new_no) = match kind {
                Change::Same => {
                    o += 1;
                    n += 1;
                    (Some(o), Some(n))
                }
                Change::Removed => {
                    o += 1;
                    (Some(o), None)
                }
                Change::Added => {
                    n += 1;
                    (None, Some(n))
                }
            };
            DiffLine {
                kind,
                old: old_no,
                new: new_no,
                text,
            }
        })
        .collect();
    let mut keep = vec![false; numbered.len()];
    for (i, line) in numbered.iter().enumerate() {
        if line.kind != Change::Same {
            let from = i.saturating_sub(DIFF_CONTEXT);
            let to = (i + DIFF_CONTEXT).min(numbered.len() - 1);
            keep[from..=to].iter_mut().for_each(|k| *k = true);
        }
    }
    let mut hunks = Vec::new();
    let mut budget = DIFF_MAX_LINES;
    let (mut old_before, mut new_before) = (0_u32, 0_u32);
    let mut i = 0;
    while i < numbered.len() && budget > 0 {
        if !keep[i] {
            old_before += u32::from(numbered[i].old.is_some());
            new_before += u32::from(numbered[i].new.is_some());
            i += 1;
            continue;
        }
        let mut lines = Vec::new();
        while i < numbered.len() && keep[i] {
            old_before += u32::from(numbered[i].old.is_some());
            new_before += u32::from(numbered[i].new.is_some());
            if budget > 0 {
                lines.push(numbered[i].clone());
                budget -= 1;
            }
            i += 1;
        }
        let old_count = lines.iter().filter(|l| l.old.is_some()).count() as u32;
        let new_count = lines.iter().filter(|l| l.new.is_some()).count() as u32;
        let old_start = lines.iter().find_map(|l| l.old).unwrap_or(old_before);
        let new_start = lines.iter().find_map(|l| l.new).unwrap_or(new_before);
        hunks.push(Hunk {
            header: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
            lines,
        });
    }
    hunks
}

#[derive(Clone, Debug, PartialEq)]
enum Fetch {
    Loading,
    Ready(String),
    Failed,
}

/// Sidecar blobs fetched for a row, with the order they were requested in
/// (the most recently requested ready blob is the one shown).
#[derive(Clone, Debug, PartialEq, Default)]
struct Fetches {
    blobs: Vec<(String, Fetch, u64)>,
    next: u64,
}

impl Fetches {
    fn get(&self, blob: &str) -> Option<&(String, Fetch, u64)> {
        self.blobs.iter().find(|(b, _, _)| b == blob)
    }
    fn set(&mut self, blob: &str, fetch: Fetch) {
        self.next += 1;
        let order = self.next;
        match self.blobs.iter_mut().find(|(b, _, _)| b == blob) {
            Some(entry) => {
                entry.1 = fetch;
                entry.2 = order;
            }
            None => self.blobs.push((blob.to_owned(), fetch, order)),
        }
    }
}

fn blob_body(text: &str, is_diff: bool) -> Option<Body> {
    if is_diff {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let new = value.get("newText")?.as_str()?;
        let old = value.get("oldText").and_then(|t| t.as_str()).unwrap_or("");
        let hunks = hunks(old, new);
        return (!hunks.is_empty()).then_some(Body::Diff(hunks));
    }
    output_lines(text, FULL_OUTPUT_MAX_LINES).map(Body::Output)
}

fn compute_detail(tool: &Tool, think: bool, text: &str, live: bool, fetches: &Fetches) -> Detail {
    if think {
        let (lines, more) = thought_lines(text, live);
        return Detail {
            body: (!lines.is_empty()).then_some(Body::Thought(lines, more)),
            ..Detail::default()
        };
    }
    let invocation = invocation_text(tool).and_then(|t| call_lines(&t));
    let refs = [tool.diff_ref.as_deref(), tool.output_ref.as_deref()];
    let shown = refs
        .iter()
        .flatten()
        .filter_map(|r| match fetches.get(r) {
            Some((_, Fetch::Ready(text), order)) => Some((*order, *r, text.as_str())),
            _ => None,
        })
        .max_by_key(|(order, _, _)| *order);
    let body = match shown {
        Some((_, blob, text)) => blob_body(text, Some(blob) == tool.diff_ref.as_deref()),
        None => None,
    }
    .or_else(|| {
        if let Some((_, old, new)) = &tool.diff {
            let hunks = hunks(old.as_deref().unwrap_or(""), new);
            if !hunks.is_empty() {
                return Some(Body::Diff(hunks));
            }
        }
        if let Some((added, removed)) = tool.stats {
            let path = if tool.detail.is_empty() {
                "workspace".to_owned()
            } else {
                tool.detail.clone()
            };
            return Some(Body::Stats(vec![(path, added, removed)]));
        }
        output_lines(&tool.output, OUTPUT_MAX_LINES).map(Body::Output)
    });
    let shown_ref = shown.map(|(_, r, _)| r);
    let candidates = [
        (tool.diff_ref.as_deref(), "diff", None),
        (tool.output_ref.as_deref(), "output", tool.output_bytes),
    ];
    let mut affordance = None;
    for (blob, what, bytes) in candidates {
        let Some(blob) = blob else { continue };
        let (label, loading) = match fetches.get(blob).map(|(_, f, _)| f) {
            Some(Fetch::Ready(_)) => {
                if shown_ref == Some(blob) {
                    continue;
                }
                (format!("Show full {what}"), false)
            }
            Some(Fetch::Loading) => (format!("Loading full {what}…"), true),
            Some(Fetch::Failed) => (format!("Couldn't load full {what} — tap to retry"), false),
            None => match bytes {
                Some(b) => (format!("Show full {what} ({})", format_kb(b)), false),
                None => (format!("Show full {what}"), false),
            },
        };
        affordance = Some(Affordance {
            blob: blob.to_owned(),
            label,
            loading,
        });
        break;
    }
    Detail {
        invocation,
        body,
        affordance,
    }
}

// Group model -------------------------------------------------------------------

fn part_of(ctx: &Ctx, key: &str) -> Option<Rc<model::Part>> {
    ctx.transcript
        .try_borrow()
        .ok()
        .and_then(|t| t.parts.get(key).cloned())
}

fn is_agent(part: &model::Part) -> bool {
    part.kind == PartKind::Tool && part.tool.with_untracked(|t| t.verb == "Agent")
}

type DetailCache = Option<(Tool, String, bool, Fetches, Rc<Detail>)>;

/// One member's state, kept by its group so the group's analytic height can
/// read every row's height without the rows being mounted.
struct RowState {
    key: String,
    part: Rc<model::Part>,
    think: bool,
    /// When its entrance starts; `None` appears settled.
    start: Option<f32>,
    /// The detail's open progress (0..1 over `TOOL_FOLD`).
    open: Signal<Tween>,
    /// The reader's choice; `None` follows the default (a live thought).
    user: Signal<Option<bool>>,
    fetches: Signal<Fetches>,
    hover: Signal<bool>,
    cache: RefCell<DetailCache>,
}

impl RowState {
    fn detail(&self, live: bool) -> Rc<Detail> {
        self.part.tool.with(|tool| {
            self.part.text.with(|text| {
                self.fetches.with(|fetches| {
                    let text = if self.think { text.as_str() } else { "" };
                    let mut cache = self.cache.borrow_mut();
                    if let Some((t, x, l, f, detail)) = cache.as_ref()
                        && t == tool
                        && x == text
                        && *l == live
                        && f == fetches
                    {
                        return detail.clone();
                    }
                    let detail = Rc::new(compute_detail(tool, self.think, text, live, fetches));
                    *cache = Some((
                        tool.clone(),
                        text.to_owned(),
                        live,
                        fetches.clone(),
                        detail.clone(),
                    ));
                    detail
                })
            })
        })
    }
    fn reveal(&self, anim: &Anim) -> f32 {
        self.start
            .map_or(1., |s| ROW_REVEAL.at(anim.at(s + ROW_REVEAL.0) - s))
    }
    fn connector(&self, anim: &Anim) -> f32 {
        self.start.map_or(1., |s| {
            CONNECTOR_REVEAL.at(anim.at(s + CONNECTOR_REVEAL.0) - s)
        })
    }
    fn failed(&self) -> bool {
        !self.think && self.part.tool.with(|t| t.error)
    }
}

/// One fold (or one run of subagent cards) of a transcript block.
struct Group {
    ctx: Ctx,
    /// The transcript block this segment belongs to.
    block_key: String,
    block_members: Signal<Vec<String>>,
    members: Signal<Vec<String>>,
    /// Rail rows under a fold, or subagent cards.
    rail: bool,
    head_born: Option<f32>,
    streaming: Signal<bool>,
    blocks: Signal<Vec<Block>>,
    /// Row state signals are created on demand, outside any mount context.
    runtime: zgx::reactive::Runtime,
    states: RefCell<HashMap<String, Rc<RowState>>>,
}

impl Group {
    fn base(&self) -> f32 {
        if self.rail {
            ROW_HEIGHT
        } else {
            CARD_ROW_HEIGHT
        }
    }
    fn state(&self, key: &str) -> Option<Rc<RowState>> {
        if let Some(state) = self.states.borrow().get(key) {
            return Some(state.clone());
        }
        let part = part_of(&self.ctx, key)?;
        let think = part.kind == PartKind::Reasoning;
        let index = self
            .members
            .with_untracked(|m| m.iter().position(|k| k == key))
            .unwrap_or(0);
        let start = if self.rail {
            arrival_start(part.born, self.head_born, index)
        } else {
            None
        };
        let anim = &self.ctx.anim;
        if let Some(start) = start {
            let left = start + CONNECTOR_REVEAL.0 + 0.05 - anim.now();
            if left > 0. {
                anim.keep(left);
            }
        }
        let user = self.ctx.toggles.get(&format!("tool:{key}"));
        let open = user.unwrap_or(think && self.live_part(key, false));
        let runtime = &self.runtime;
        let state = Rc::new(RowState {
            key: key.to_owned(),
            part,
            think,
            start,
            open: runtime.signal(Tween::settled(if open { 1. } else { 0. })),
            user: runtime.signal(user),
            fetches: runtime.signal(Fetches::default()),
            hover: runtime.signal(false),
            cache: RefCell::new(None),
        });
        self.states
            .borrow_mut()
            .insert(key.to_owned(), state.clone());
        Some(state)
    }
    /// The block is the streaming tail of the transcript.
    fn live_tail(&self, track: bool) -> bool {
        let block = Block::Group(self.block_key.clone());
        if track {
            self.streaming.get() && self.blocks.with(|b| b.last() == Some(&block))
        } else {
            self.streaming.with_untracked(|s| *s)
                && self.blocks.with_untracked(|b| b.last() == Some(&block))
        }
    }
    /// `key` is the entry's last part and still streaming.
    fn live_part(&self, key: &str, track: bool) -> bool {
        self.live_tail(track)
            && if track {
                self.block_members
                    .with(|m| m.last().map(String::as_str) == Some(key))
            } else {
                self.block_members
                    .with_untracked(|m| m.last().map(String::as_str) == Some(key))
            }
    }
    /// Zeron's `auto_open`: streaming, and this fold holds the entry's last part.
    fn auto_open(&self) -> bool {
        self.live_tail(true)
            && self
                .block_members
                .with(|block| self.members.with(|m| m.last() == block.last()))
    }
    fn row_height(&self, state: &RowState) -> f32 {
        let live = state.think && self.live_part(&state.key, true);
        let extra = state.detail(live).extra();
        self.base() + extra * self.ctx.anim.tween(&state.open, TOOL_FOLD)
    }
    fn summary(&self) -> String {
        let items: Vec<SummaryItem> = self.members.with(|keys| {
            keys.iter()
                .filter_map(|key| self.state(key))
                .map(|s| {
                    s.part.tool.with(|t| SummaryItem {
                        thought: s.think,
                        verb: t.verb,
                        path: t.detail.clone(),
                        error: t.error,
                    })
                })
                .collect()
        });
        summary(&items)
    }
    /// Anything still running: a live thought or an unresolved call.
    fn unresolved(&self) -> bool {
        self.streaming.get()
            && self.members.with(|keys| {
                keys.iter().filter_map(|key| self.state(key)).any(|s| {
                    if s.think {
                        self.live_part(&s.key, true)
                    } else {
                        s.part.tool.with(|t| !t.resolved)
                    }
                })
            })
    }
}

// Tool groups ---------------------------------------------------------------------

/// A transcript block of consecutive tool calls and thoughts. Subagent calls
/// leave the fold (Zeron splits them into their own always-open rows), so a
/// block renders as one or more segments.
pub fn tool_group(cx: &mut Context, ctx: &Ctx, key: &str, members: Signal<Vec<String>>) -> View {
    let (streaming, blocks) = {
        let transcript = ctx.transcript.borrow();
        (transcript.streaming.clone(), transcript.blocks.clone())
    };
    let segments = cx.state(Vec::<(String, bool)>::new());
    let segment_members: Rc<RefCell<HashMap<String, Signal<Vec<String>>>>> = Rc::default();
    {
        let (ctx, members, segments, segment_members, runtime) = (
            ctx.clone(),
            members.clone(),
            segments.clone(),
            segment_members.clone(),
            cx.runtime(),
        );
        let effect = cx.runtime().effect(move || {
            let mut runs: Vec<(String, bool, Vec<String>)> = Vec::new();
            members.with(|keys| {
                for key in keys {
                    let agent = part_of(&ctx, key).is_some_and(|p| is_agent(&p));
                    match runs.last_mut() {
                        Some((_, a, run)) if *a == agent => run.push(key.clone()),
                        _ => runs.push((key.clone(), agent, vec![key.clone()])),
                    }
                }
            });
            let mut map = segment_members.borrow_mut();
            for (head, _, run) in &runs {
                match map.get(head) {
                    Some(signal) => {
                        signal.set(run.clone());
                    }
                    None => {
                        map.insert(head.clone(), runtime.signal(run.clone()));
                    }
                }
            }
            drop(map);
            segments.set(
                runs.into_iter()
                    .map(|(head, agent, _)| (head, agent))
                    .collect(),
            );
        });
        cx.retain(effect);
    }
    let (ctx, block_key) = (ctx.clone(), key.to_owned());
    keyed(
        move || segments.get(),
        move |(head, agent), cx| {
            let Some(seg) = segment_members.borrow().get(&head).cloned() else {
                return div();
            };
            let head_born = part_of(&ctx, &head).and_then(|p| p.born);
            let group = Rc::new(Group {
                ctx: ctx.clone(),
                block_key: block_key.clone(),
                block_members: members.clone(),
                members: seg,
                rail: !agent,
                head_born,
                streaming: streaming.clone(),
                blocks: blocks.clone(),
                runtime: cx.runtime(),
                states: RefCell::new(HashMap::new()),
            });
            let view = if agent {
                card_rows(group)
            } else {
                fold_group(cx, group, &head)
            };
            if head == block_key {
                view
            } else {
                view.mt(SEGMENT_GAP)
            }
        },
    )
    .w_full()
}

/// Subagent calls: always-open cards, no header, no rail.
fn card_rows(group: Rc<Group>) -> View {
    let keys = group.members.clone();
    keyed(
        move || keys.get(),
        move |key, cx| match group.state(&key) {
            Some(state) => tool_row(cx, group.clone(), state),
            None => div(),
        },
    )
    .w_full()
}

/// The summary header over a folding body of rail rows.
fn fold_group(cx: &mut Context, group: Rc<Group>, head: &str) -> View {
    let anim = group.ctx.anim.clone();
    let user_key = format!("group-user:{head}");
    let memory = format!("group:{head}");
    let user = cx.state(group.ctx.toggles.get(&user_key));
    // Rows still arriving hold the fold open (Zeron's `arrival_pending`).
    let arrival_pending = {
        let (group, anim) = (group.clone(), anim.clone());
        move || {
            let end = group.members.with(|keys| {
                keys.iter()
                    .filter_map(|k| group.state(k))
                    .filter_map(|s| s.start)
                    .map(|s| s + CONNECTOR_REVEAL.0)
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            anim.at(end) < end
        }
    };
    let open_now = {
        let (group, user) = (group.clone(), user.clone());
        move || {
            user.get()
                .unwrap_or_else(|| group.auto_open() || arrival_pending())
        }
    };
    let initial = open_now();
    let fold = cx.state(Tween::settled(if initial { 1. } else { 0. }));
    {
        let (fold, anim, toggles, memory) = (
            fold.clone(),
            anim.clone(),
            group.ctx.toggles.clone(),
            memory.clone(),
        );
        // Opens with the stream; closes once the stream moves past it
        // (unless the reader chose), on the same 140 ms tween.
        let effect = cx.runtime().effect(move || {
            let open = open_now();
            anim.retarget(&fold, if open { 1. } else { 0. }, TOOL_FOLD);
            toggles.set(&memory, open);
        });
        cx.retain(effect);
    }
    let toggle = {
        let (fold, user, toggles) = (fold.clone(), user.clone(), group.ctx.toggles.clone());
        move || {
            let open = fold.with_untracked(|t| t.target > 0.5);
            user.set(Some(!open));
            toggles.set(&user_key, !open);
        }
    };
    // The shimmer runs while the fold is live, from when it first appeared.
    let origin = group.head_born.unwrap_or_else(|| anim.now());
    let active = {
        let group = group.clone();
        move || group.auto_open() || group.unresolved()
    };
    {
        let (anim, active) = (anim.clone(), active.clone());
        let guard: RefCell<Option<AmbientGuard>> = RefCell::new(None);
        let effect = cx.runtime().effect(move || {
            let on = active();
            let mut slot = guard.borrow_mut();
            match (on, slot.is_some()) {
                (true, false) => *slot = Some(anim.ambient()),
                (false, true) => *slot = None,
                _ => {}
            }
        });
        cx.retain(effect);
    }
    let hovered = cx.state(false);
    let title = {
        let text: Rc<dyn Fn() -> String> = {
            let group = group.clone();
            Rc::new(move || group.summary())
        };
        let (rest_text, anim2, hovered2, measure) = (
            text.clone(),
            anim.clone(),
            hovered.clone(),
            cx.text_measure(),
        );
        let centres: Rc<RefCell<(String, Vec<f32>)>> = Rc::default();
        roll_core(cx, &anim, text, TEXT_SIZE, None, move |_| {
            let (text, anim, hovered, active, measure, centres) = (
                rest_text.clone(),
                anim2.clone(),
                hovered2.clone(),
                active.clone(),
                measure.clone(),
                centres.clone(),
            );
            rich_text_signal(move || {
                let summary = text();
                if !active() {
                    return vec![text_span(summary)];
                }
                let phase = shimmer_phase(anim.ambient_clock.get() - origin);
                let base = if hovered.get() { TEXT } else { TEXT_MUTED };
                let mut cache = centres.borrow_mut();
                if cache.0 != summary {
                    let (xs, width) = glyph_xs(&measure, &summary, TEXT_SIZE);
                    let n = xs.len();
                    let centres = (0..n)
                        .map(|i| {
                            let right = xs.get(i + 1).map_or(width, |g| g.2);
                            if width > 0. {
                                (xs[i].2 + right) / 2. / width
                            } else {
                                i as f32 / (n.max(2) - 1) as f32
                            }
                        })
                        .collect();
                    *cache = (summary.clone(), centres);
                }
                summary
                    .chars()
                    .zip(cache.1.iter())
                    .map(|(ch, x)| {
                        let amount = shimmer_amount(*x, phase);
                        let color = if amount > 0.001 {
                            mix(TEXT_MUTED, TEXT, amount)
                        } else {
                            base
                        };
                        text_span(ch.to_string()).text_color(color)
                    })
                    .collect()
            })
            .min_w(0.)
            .text_wrap(false)
            .truncate()
        })
    };
    let chevron = {
        let (fold, anim) = (fold.clone(), anim.clone());
        svg_signal("disclosure", move || {
            let p = anim.tween(&fold, TOOL_FOLD);
            Arc::new(
                icon_data("alt-arrow-down")
                    .tinted(TEXT_MUTED)
                    .transformed(Affine::rotation(-FRAC_PI_2 * (1. - p))),
            )
        })
        .size(14., 14.)
        .absolute()
        .left(TRUNK_X - 7.)
        .top(2.)
    };
    let header = row()
        .w_full()
        .h(HEADER_HEIGHT)
        .shrink_0()
        .gap(6.)
        .pr(4.)
        .items_center()
        .text_size(TEXT_SIZE)
        .line_height(LINE)
        .cursor(Cursor::Pointer)
        .reactive_style({
            let hovered = hovered.clone();
            move || Styles::new().text_color(if hovered.get() { TEXT } else { TEXT_MUTED })
        })
        .on_event({
            let hovered = hovered.clone();
            move |event| match event.event {
                InputEvent::PointerEnter => {
                    hovered.set(true);
                }
                InputEvent::PointerLeave | InputEvent::PointerCancel => {
                    hovered.set(false);
                }
                _ => {}
            }
        })
        .on_click(toggle)
        .child(div().size(22., 18.).shrink_0().relative().child(chevron))
        .child(
            row()
                .min_w(0.)
                .h(LINE)
                .items_center()
                .overflow_hidden()
                .child(title),
        );
    // A new group's header is clipped open over 360 ms expo (no fade).
    let head_born = group.head_born;
    if let Some(born) = head_born {
        let left = born + ROW_REVEAL.0 + 0.05 - anim.now();
        if left > 0. {
            anim.keep(left);
        }
    }
    let header = div()
        .w_full()
        .shrink_0()
        .overflow_hidden()
        .reactive_style({
            let anim = anim.clone();
            move || {
                let reveal = head_born.map_or(1., |b| ROW_REVEAL.at(anim.at(b + ROW_REVEAL.0) - b));
                Styles::new().h(HEADER_HEIGHT * reveal)
            }
        })
        .child(header);
    let body_height = {
        let (group, fold, anim) = (group.clone(), fold.clone(), anim.clone());
        move || {
            let v = anim.tween(&fold, TOOL_FOLD);
            if v <= 0. {
                return 0.;
            }
            let rows: f32 = group.members.with(|keys| {
                keys.iter()
                    .filter_map(|k| group.state(k))
                    .map(|s| group.row_height(&s) * s.reveal(&anim))
                    .sum()
            });
            (CHIPS_TOP_PAD + rows) * v
        }
    };
    let mounted = {
        let (fold, anim) = (fold.clone(), anim.clone());
        move || {
            let t = fold.get();
            let end = t.end(TOOL_FOLD);
            t.target > 0.5 || anim.at(end) < end
        }
    };
    let rows = {
        let group = group.clone();
        switch(mounted, move |mounted, _| {
            if !mounted {
                return div();
            }
            let (keys, group) = (group.members.clone(), group.clone());
            column().w_full().pt(CHIPS_TOP_PAD).child(
                keyed(
                    move || keys.get(),
                    move |key, cx| match group.state(&key) {
                        Some(state) => tool_row(cx, group.clone(), state),
                        None => div(),
                    },
                )
                .w_full(),
            )
        })
        .w_full()
    };
    let body = div()
        .w_full()
        .shrink_0()
        .overflow_hidden()
        .reactive_style(move || Styles::new().h(body_height()))
        .child(rows);
    column()
        .w_full()
        .font_family(SANS)
        .child(header)
        .child(body)
}

// Rows ----------------------------------------------------------------------------

fn tool_icon(tool: &Tool, think: bool) -> &'static str {
    if think {
        return "thought";
    }
    match tool.verb {
        "Run" => "terminal",
        "Read" | "Patch" => "document",
        "Write" => "document-add",
        "Edit" => "pen",
        "Search" => "magnifer",
        "Glob" => "folder-with-files",
        "Fetch" | "Web" => "global",
        "Todo" => "checklist",
        "Agent" => "bot",
        _ => "widget",
    }
}

fn file_badge_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|c| !c.is_empty())
        .unwrap_or(path)
}

/// A clockwise ribbon contour one pixel wide; nonzero fill unions the trunk
/// and the elbow so their joint is not blended twice.
fn ribbon(path: &mut PathBuilder, points: &[(f32, f32)]) -> bool {
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for p in points {
        if pts
            .last()
            .is_none_or(|q: &(f32, f32)| (p.0 - q.0).hypot(p.1 - q.1) > 1e-3)
        {
            pts.push(*p);
        }
    }
    if pts.len() < 2 {
        return false;
    }
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for (i, p) in pts.iter().enumerate() {
        let a = pts[i.saturating_sub(1)];
        let b = pts[(i + 1).min(pts.len() - 1)];
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = dx.hypot(dy).max(0.0001);
        let normal = (-dy / length * 0.5, dx / length * 0.5);
        left.push((p.0 + normal.0, p.1 + normal.1));
        right.push((p.0 - normal.0, p.1 - normal.1));
    }
    path.move_to(left[0].0, left[0].1);
    for p in left.iter().skip(1).chain(right.iter().rev()) {
        path.line_to(p.0, p.1);
    }
    path.close();
    true
}

/// The activity rail of one row (transcript.rs:9732): the trunk down to the
/// bend (and on to the row's bottom as the next row arrives), the rounded
/// elbow and the leg to the icon, revealed by arc length.
fn rail_drawing(
    height: f32,
    incoming: f32,
    branch: f32,
    continues: bool,
    continuation: f32,
) -> Canvas {
    let mut canvas = Canvas::new();
    let mut path = Path::builder();
    let bend_y = ROW_HEIGHT / 2. - BEND;
    let mut any = false;
    if incoming > 0. {
        let mut bottom = bend_y * incoming;
        if incoming >= 1. && continues && continuation > 0. {
            bottom = bend_y + (height - bend_y).max(0.) * continuation;
        }
        any |= ribbon(&mut path, &[(TRUNK_X, 0.), (TRUNK_X, bottom)]);
    }
    if branch > 0. {
        let points: Vec<(f32, f32)> = branch_points(branch)
            .into_iter()
            .map(|(x, y)| (TRUNK_X + x, bend_y + y))
            .collect();
        any |= ribbon(&mut path, &points);
    }
    if any && let Ok(path) = path.build() {
        canvas.fill_rule(path, hairline(0.12), FillRule::Winding);
    }
    canvas
}

/// Zeron's 2×3 mini glyph spinner: brightness chases round the ring.
fn mini_spinner(anim: &Anim) -> View {
    const RING: [[usize; 2]; 3] = [[0, 1], [5, 2], [4, 3]];
    column().gap(1.).shrink_0().children((0..3).map(|r| {
        let anim = anim.clone();
        row().gap(1.).children((0..2).map(move |c| {
            let anim = anim.clone();
            let phase = RING[r][c] as f32 / 6.;
            div()
                .size(2., 2.)
                .rounded(1.)
                .bg(GLYPH[r])
                .reactive_style(move || {
                    Styles::new().opacity(gspin(anim.ambient_clock.get() / 0.75 + phase, 0.1))
                })
        }))
    }))
}

/// One member: the rail (or a subagent card) and its header, with the
/// expandable detail under it.
fn tool_row(cx: &mut Context, group: Rc<Group>, state: Rc<RowState>) -> View {
    let anim = group.ctx.anim.clone();
    let rail = group.rail;
    let base = group.base();
    let think = state.think;
    let key = state.key.clone();
    let tool = state.part.tool.clone();
    let live = {
        let (group, key) = (group.clone(), key.clone());
        move || think && group.live_part(&key, true)
    };
    let detail = {
        let (state, live) = (state.clone(), live.clone());
        move || state.detail(live())
    };
    // A streaming thought opens; it folds when its turn moves on.
    {
        let (state, anim, live) = (state.clone(), anim.clone(), live.clone());
        let effect = cx.runtime().effect(move || {
            let open = state.user.get().unwrap_or_else(&live);
            if state.open.with_untracked(|t| (t.target > 0.5) != open) {
                anim.retarget(&state.open, if open { 1. } else { 0. }, TOOL_FOLD);
                anim.keep(DETAIL_RETAIN + 0.05);
            }
        });
        cx.retain(effect);
    }
    let height = {
        let (group, state) = (group.clone(), state.clone());
        move || group.row_height(&state)
    };
    let position = {
        let (group, key) = (group.clone(), key.clone());
        move || {
            group
                .members
                .with(|m| match m.iter().position(|k| *k == key) {
                    Some(i) => (i > 0, i + 1 < m.len(), m.get(i + 1).cloned()),
                    None => (false, false, None),
                })
        }
    };
    let branch = {
        let (state, anim, position) = (state.clone(), anim.clone(), position.clone());
        move || connector_parts(state.connector(&anim), position().0).1
    };
    let toggle = {
        let (state, anim, toggles, detail) = (
            state.clone(),
            anim.clone(),
            group.ctx.toggles.clone(),
            detail.clone(),
        );
        move || {
            if !detail().expandable() {
                return;
            }
            let open = state.open.with_untracked(|t| t.target > 0.5);
            state.user.set(Some(!open));
            toggles.set(&format!("tool:{}", state.key), !open);
            anim.keep(DETAIL_RETAIN + 0.05);
        }
    };
    let expandable = {
        let detail = detail.clone();
        move || detail().expandable()
    };
    let failed = {
        let state = state.clone();
        move || state.failed()
    };
    // Group-hover text: label, detail and chevron brighten on hover.
    let lit = {
        let (state, expandable, failed) = (state.clone(), expandable.clone(), failed.clone());
        move || rail && state.hover.get() && expandable() && !failed()
    };
    let label = text(if think {
        "Thought process"
    } else {
        tool.with_untracked(|t| t.verb)
    })
    .shrink_0()
    .h(LINE)
    .when(!rail, |l| l.font_weight(500))
    .reactive_style({
        let (failed, lit) = (failed.clone(), lit.clone());
        move || {
            Styles::new().text_color(if failed() {
                DANGER
            } else if lit() {
                TEXT
            } else {
                TEXT_MUTED
            })
        }
    });
    let file = tool.with_untracked(|t| t.file && !t.detail.is_empty());
    let detail_view = if think {
        div().hidden()
    } else if file {
        let path = tool.with_untracked(|t| t.detail.clone());
        let name = file_badge_name(&path).to_owned();
        // The frosted file badge: polychrome icon in a dark well, the name.
        row()
            .min_w(0.)
            .h(22.)
            .items_center()
            .overflow_hidden()
            .gap(6.)
            .rounded(5.)
            .blur(16.)
            .bg(ink(0.06))
            .hover(|s| s.bg(ink(0.1)))
            .pl(1.)
            .pr(6.)
            .reactive_style({
                let (failed, lit) = (failed.clone(), lit.clone());
                move || {
                    Styles::new().text_color(if failed() {
                        DANGER
                    } else if lit() {
                        TEXT
                    } else {
                        alpha(TEXT, 0.85)
                    })
                }
            })
            .child(
                row()
                    .size(20., 20.)
                    .shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded(4.)
                    .bg(Color(0, 0, 0, 82))
                    .child(file_icon(&path, 14.)),
            )
            .child(text(name).min_w(0.).truncate())
    } else {
        let (tool, failed, lit) = (tool.clone(), failed.clone(), lit.clone());
        let shown = tool.clone();
        text_signal(move || tool.with(|t| invocation_text(t).unwrap_or_default()))
            .min_w(0.)
            .h(LINE)
            .truncate()
            .when(!rail, |d| d.grow())
            .reactive_style(move || {
                let empty = shown.with(|t| invocation_text(t).is_none());
                let styles = Styles::new().text_color(if failed() {
                    DANGER
                } else if !rail {
                    alpha(TEXT, 0.85)
                } else if lit() {
                    TEXT
                } else {
                    TEXT_MUTED
                });
                if rail && empty {
                    styles.hidden()
                } else {
                    styles.flex()
                }
            })
    };
    // A running subagent pulses the mini glyph spinner.
    let spinner = if tool.with_untracked(|t| t.verb == "Agent") && !think {
        let running = {
            let (tool, streaming) = (tool.clone(), group.streaming.clone());
            move || streaming.get() && tool.with(|t| !t.resolved)
        };
        {
            let (anim, running) = (anim.clone(), running.clone());
            let guard: RefCell<Option<AmbientGuard>> = RefCell::new(None);
            let effect = cx.runtime().effect(move || {
                let on = running();
                let mut slot = guard.borrow_mut();
                match (on, slot.is_some()) {
                    (true, false) => *slot = Some(anim.ambient()),
                    (false, true) => *slot = None,
                    _ => {}
                }
            });
            cx.retain(effect);
        }
        mini_spinner(&anim).reactive_style(move || {
            if running() {
                Styles::new().flex()
            } else {
                Styles::new().hidden()
            }
        })
    } else {
        div().hidden()
    };
    let chevron = {
        let (state, lit_state, failed) = (state.clone(), state.clone(), failed.clone());
        let expandable = expandable.clone();
        row()
            .size(18., 18.)
            .shrink_0()
            .items_center()
            .justify_center()
            .when(!rail, |t| t.rounded(5.).bg(ink(0.06)))
            .reactive_style(move || {
                if !expandable() {
                    return Styles::new().hidden();
                }
                let shown = !rail || lit_state.hover.get();
                Styles::new().flex().opacity(if shown { 1. } else { 0. })
            })
            .child(
                svg_signal("caret", move || {
                    let open = state.open.with(|t| t.target > 0.5);
                    let color = if rail && state.hover.get() {
                        if failed() { DANGER } else { TEXT }
                    } else {
                        TEXT_FAINT
                    };
                    let name = if open {
                        "alt-arrow-down"
                    } else {
                        "alt-arrow-right"
                    };
                    Arc::new(icon_data(name).tinted(color))
                })
                .size(12., 12.),
            )
    };
    let header = row()
        .h(if rail {
            CARD_HEIGHT
        } else {
            CARD_HEADER_HEIGHT
        })
        .w_full()
        .min_w(0.)
        .shrink_0()
        .items_center()
        .gap(8.)
        .px(if rail { 0. } else { 8. })
        .text_size(TEXT_SIZE)
        .line_height(LINE)
        .reactive_style({
            let expandable = expandable.clone();
            move || {
                if expandable() {
                    Styles::new().cursor(Cursor::Pointer)
                } else {
                    Styles::new().cursor(Cursor::Default)
                }
            }
        })
        .on_event({
            let hover = state.hover.clone();
            move |event| match event.event {
                InputEvent::PointerEnter => {
                    hover.set(true);
                }
                InputEvent::PointerLeave | InputEvent::PointerCancel => {
                    hover.set(false);
                }
                _ => {}
            }
        })
        .on_click(toggle)
        .when(!rail, |h| {
            h.child(
                row()
                    .size(18., 18.)
                    .shrink_0()
                    .rounded(5.)
                    .bg(ink(0.08))
                    .items_center()
                    .justify_center()
                    .child(super::theme::icon(
                        tool_icon(&tool.with_untracked(Clone::clone), think),
                        12.,
                        TEXT_MUTED,
                    )),
            )
        })
        .child(label)
        .child(detail_view)
        .child(spinner)
        .child(chevron);
    // The detail stays mounted while it closes, then a little longer.
    let panel = {
        let (state, anim, detail) = (state.clone(), anim.clone(), detail.clone());
        let fetch_state = state.clone();
        let ctx = group.ctx.clone();
        switch(
            move || {
                let t = state.open.get();
                let end = t.at + DETAIL_RETAIN;
                let mounted = t.target > 0.5 || anim.at(end) < end;
                mounted.then(&detail)
            },
            move |detail, _| match detail {
                Some(detail) => detail_panel(&detail, rail, &ctx, &fetch_state),
                None => div(),
            },
        )
        .w_full()
    };
    // Zeron: the card is `row height − base + 30`, centred by `my`.
    let card_height_offset = base - CARD_HEIGHT;
    let card = column()
        .min_w(0.)
        .grow()
        .overflow_hidden()
        .my((base - CARD_HEIGHT) / 2.)
        .when(rail, |c| c.ml(TEXT_GAP))
        .when(!rail, |c| {
            c.rounded(9.)
                .border(1.)
                .border_color(hairline(0.07))
                .bg(ink(0.03))
        })
        .reactive_style({
            let (height, branch) = (height.clone(), branch.clone());
            move || {
                let styles = Styles::new().h(height() - card_height_offset);
                if !rail {
                    return styles;
                }
                // Content rises along the branch as it draws.
                let c = branch();
                styles.opacity(c).translate(0., 4. * (1. - c))
            }
        })
        .child(header)
        .child(panel);
    let mut line = row().w_full().shrink_0().items_stretch().reactive_style({
        let height = height.clone();
        move || Styles::new().h(height())
    });
    if rail {
        let canvas_view = {
            let (state, anim, group, position) =
                (state.clone(), anim.clone(), group.clone(), position.clone());
            canvas(move |(_, h)| {
                let (has_predecessor, continues, next) = position();
                let (incoming, branch) = connector_parts(state.connector(&anim), has_predecessor);
                let continuation = connector_continuation(
                    next.and_then(|k| group.state(&k))
                        .map(|s| s.connector(&anim)),
                );
                rail_drawing(h, incoming, branch, continues, continuation)
            })
            .w(GUTTER)
            .grow()
        };
        let icon_view = {
            let (state, tool) = (state.clone(), tool.clone());
            svg_signal("tool", move || {
                let color = if state.failed() { DANGER } else { TEXT_MUTED };
                Arc::new(icon_data(tool.with(|t| tool_icon(t, think))).tinted(color))
            })
            .size(ICON_SIZE, ICON_SIZE)
            .absolute()
            .left(ICON_LEFT)
            .top(ROW_HEIGHT / 2. - ICON_SIZE / 2.)
            .reactive_style({
                let branch = branch.clone();
                move || Styles::new().opacity(branch())
            })
        };
        // The gutter spans the whole row, expanded detail included, so the
        // trunk can continue alongside it.
        line = line.child(
            column()
                .w(GUTTER)
                .shrink_0()
                .relative()
                .reactive_style({
                    let height = height.clone();
                    move || Styles::new().h(height())
                })
                .child(canvas_view)
                .child(icon_view),
        );
    }
    line = line.child(card);
    if let Some(start) = state.start
        && anim.now() < start + CONNECTOR_REVEAL.0
    {
        anim.keep(start + CONNECTOR_REVEAL.0 + 0.05 - anim.now());
    }
    // A new row is clipped open over 360 ms expo.
    div()
        .w_full()
        .shrink_0()
        .overflow_hidden()
        .reactive_style({
            let (state, anim) = (state.clone(), anim.clone());
            move || Styles::new().h(height() * state.reveal(&anim))
        })
        .child(line)
}

// Detail bodies (transcript.rs:9093) ----------------------------------------------

fn line_row(child: View, height: f32) -> View {
    row()
        .h(height)
        .w_full()
        .min_w(0.)
        .shrink_0()
        .items_center()
        .overflow_hidden()
        .child(child.shrink_0().text_wrap(false).line_height(height))
}

fn more_row(more: usize) -> View {
    line_row(
        text(format!("… {more} more lines"))
            .text_size(TEXT_SIZE)
            .text_color(TEXT_FAINT),
        LINE,
    )
}

fn lines_view(lines: &Lines) -> View {
    column()
        .w_full()
        .min_w(0.)
        .shrink_0()
        .py(6.)
        .font_family(MONO)
        .text_size(TEXT_SIZE)
        .children(lines.lines.iter().map(|l| {
            if l.is_empty() {
                row().h(LINE).shrink_0()
            } else {
                line_row(text(l.clone()).text_color(TEXT_FAINT), LINE)
            }
        }))
        .when(lines.more > 0, |c| c.child(more_row(lines.more)))
}

fn thought_view(lines: &[StyledLine], more: usize) -> View {
    column()
        .w_full()
        .min_w(0.)
        .shrink_0()
        .py(6.)
        .font_family(SANS)
        .text_size(TEXT_SIZE)
        .children(lines.iter().map(|line| {
            if line.iter().all(|(t, _)| t.trim().is_empty()) {
                return row().h(LINE).shrink_0();
            }
            let spans: Vec<TextSpan> = line
                .iter()
                .filter(|(t, _)| !t.is_empty())
                .map(|(t, style)| {
                    let span = text_span(t.clone()).text_color(TEXT_FAINT);
                    match style {
                        md::Style::Plain => span,
                        md::Style::Bold => span.font_weight(600),
                        md::Style::Italic => span.italic(true),
                        md::Style::BoldItalic => span.font_weight(600).italic(true),
                        md::Style::Strike => span.strikethrough(Decoration::new(1.)),
                        md::Style::Code => span.font_family(MONO),
                        md::Style::Link => span.underline(Decoration::new(1.)),
                    }
                })
                .collect();
            line_row(rich_text_signal(move || spans.clone()), LINE)
        }))
        .when(more > 0, |c| c.child(more_row(more)))
}

fn stats_view(stats: &[(String, u64, u64)]) -> View {
    column()
        .w_full()
        .min_w(0.)
        .shrink_0()
        .py(6.)
        .font_family(MONO)
        .text_size(TEXT_SIZE)
        .children(stats.iter().map(|(path, added, removed)| {
            row()
                .h(LINE)
                .w_full()
                .min_w(0.)
                .shrink_0()
                .items_center()
                .gap(8.)
                .child(file_icon(path, 14.).shrink_0())
                .child(
                    row().min_w(0.).grow().overflow_hidden().child(
                        text(path.clone())
                            .text_color(TEXT_FAINT)
                            .text_wrap(false)
                            .shrink_0(),
                    ),
                )
                .child(text(format!("+{added}")).text_color(SUCCESS).shrink_0())
                .child(text(format!("−{removed}")).text_color(DANGER).shrink_0())
        }))
}

/// changes.rs `gutter_width`: digits of the largest line number.
fn gutter_width(hunks: &[Hunk]) -> f32 {
    let max = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .map(|l| l.old.unwrap_or(0).max(l.new.unwrap_or(0)))
        .max()
        .unwrap_or(0)
        .max(1);
    let digits = max.ilog10() + 1;
    (digits as f32 * 6.6 + 8. + 6.).max(36.)
}

fn diff_view(hunks: &[Hunk]) -> View {
    let gutter_px = gutter_width(hunks);
    let gutter = move |no: Option<u32>, color: Color| {
        row()
            .w(gutter_px)
            .h(DIFF_LINE_HEIGHT)
            .shrink_0()
            .justify_end()
            .items_center()
            .pr(8.)
            .child(
                text(no.map(|n| n.to_string()).unwrap_or_default())
                    .font_family(MONO)
                    .text_size(11.)
                    .line_height(DIFF_LINE_HEIGHT)
                    .text_color(color),
            )
    };
    let mut body = column().w_full().min_w(0.).shrink_0().pb(DIFF_BOTTOM_PAD);
    for hunk in hunks {
        body = body.child(
            row()
                .h(HUNK_HEADER_HEIGHT)
                .w_full()
                .shrink_0()
                .items_center()
                .px(16.)
                .overflow_hidden()
                .bg(alpha(ACCENT, 0.08))
                .child(
                    text(hunk.header.clone())
                        .font_family(MONO)
                        .text_size(11.)
                        .text_color(TEXT_FAINT)
                        .text_wrap(false)
                        .shrink_0(),
                ),
        );
        for line in &hunk.lines {
            let faint = alpha(TEXT_FAINT, 0.8);
            let (marker, marker_color, wash, accent, number) = match line.kind {
                Change::Added => (
                    "+",
                    SUCCESS,
                    Some(alpha(SUCCESS, 0.055)),
                    Some(alpha(SUCCESS, 0.55)),
                    alpha(SUCCESS, 0.9),
                ),
                Change::Removed => (
                    "−",
                    DANGER,
                    Some(alpha(DANGER, 0.055)),
                    Some(alpha(DANGER, 0.55)),
                    alpha(DANGER, 0.9),
                ),
                Change::Same => ("·", alpha(TEXT_FAINT, 0.5), None, None, faint),
            };
            let mut view = row()
                .h(DIFF_LINE_HEIGHT)
                .w_full()
                .min_w(0.)
                .shrink_0()
                .items_start()
                .child(
                    div()
                        .w(ACCENT_BAR_WIDTH)
                        .h(DIFF_LINE_HEIGHT)
                        .shrink_0()
                        .when_some(accent, |d, c| d.bg(c)),
                )
                .child(gutter(
                    line.old,
                    if line.kind == Change::Removed {
                        number
                    } else {
                        faint
                    },
                ))
                .child(gutter(
                    line.new,
                    if line.kind == Change::Added {
                        number
                    } else {
                        faint
                    },
                ))
                .child(
                    row()
                        .w(MARKER_WIDTH)
                        .h(DIFF_LINE_HEIGHT)
                        .shrink_0()
                        .justify_center()
                        .items_center()
                        .child(
                            text(marker)
                                .font_family(MONO)
                                .text_size(12.)
                                .line_height(DIFF_LINE_HEIGHT)
                                .text_color(marker_color),
                        ),
                )
                .child(
                    row()
                        .grow()
                        .min_w(0.)
                        .h(DIFF_LINE_HEIGHT)
                        .items_center()
                        .overflow_hidden()
                        .pl(12.)
                        .child(
                            text(line.text.clone())
                                .font_family(MONO)
                                .text_size(12.)
                                .line_height(DIFF_LINE_HEIGHT)
                                .text_color(alpha(TEXT, 0.92))
                                .text_wrap(false)
                                .shrink_0(),
                        ),
                );
            if let Some(wash) = wash {
                view = view.bg(wash);
            }
            body = body.child(view);
        }
    }
    body
}

/// Request a sidecar blob (or re-show one already fetched).
fn fetch_blob(ctx: &Ctx, state: &RowState, blob: &str) {
    let ready = state
        .fetches
        .with_untracked(|f| matches!(f.get(blob), Some((_, Fetch::Ready(_), _))));
    if ready {
        // Already here: the click makes it the shown one.
        let text = state.fetches.with_untracked(|f| match f.get(blob) {
            Some((_, Fetch::Ready(text), _)) => text.clone(),
            _ => String::new(),
        });
        state.fetches.update(|f| f.set(blob, Fetch::Ready(text)));
        return;
    }
    let Some(rpc) = ctx.rpc.clone() else {
        return;
    };
    state.fetches.update(|f| f.set(blob, Fetch::Loading));
    let (fetches, blob) = (state.fetches.clone(), blob.to_owned());
    ctx.tasks.spawn(async move {
        let reply = rpc.call("FetchToolBlob", json!({ "blobRef": blob })).await;
        let fetch = match reply {
            Ok(reply) => match reply.get("text").and_then(|t| t.as_str()) {
                Some(text) => Fetch::Ready(text.to_owned()),
                None => Fetch::Failed,
            },
            Err(_) => Fetch::Failed,
        };
        fetches.update(|f| {
            // Keep the request's order: the reader asked for this one last.
            if let Some(entry) = f.blobs.iter_mut().find(|(b, _, _)| *b == blob) {
                entry.1 = fetch;
            }
        });
    });
}

/// Invocation, then output / diff / stats / thought, then the fetch
/// affordance; text aligns with the label (no horizontal padding).
fn detail_panel(detail: &Detail, rail: bool, ctx: &Ctx, state: &Rc<RowState>) -> View {
    let separator = || {
        div()
            .h(DETAIL_SEPARATOR)
            .w_full()
            .shrink_0()
            .when(!rail, |d| d.bg(hairline(0.06)))
    };
    let mut panel = column().w_full().min_w(0.).shrink_0().overflow_hidden();
    if let Some(invocation) = &detail.invocation {
        panel = panel.child(separator()).child(lines_view(invocation));
    }
    if let Some(body) = &detail.body {
        panel = panel.child(separator()).child(match body {
            Body::Output(lines) => lines_view(lines),
            Body::Thought(lines, more) => thought_view(lines, *more),
            Body::Diff(hunks) => diff_view(hunks),
            Body::Stats(stats) => stats_view(stats),
        });
    }
    if let Some(affordance) = &detail.affordance {
        let mut view = row()
            .h(AFFORDANCE_HEIGHT)
            .shrink_0()
            .items_center()
            .text_size(TEXT_SIZE)
            .text_color(TEXT_FAINT)
            .child(text(affordance.label.clone()));
        if !affordance.loading {
            let (ctx, state, blob) = (ctx.clone(), state.clone(), affordance.blob.clone());
            view = view
                .cursor(Cursor::Pointer)
                .hover(|s| s.text_color(TEXT_MUTED))
                .on_click(move || fetch_blob(&ctx, &state, &blob));
        }
        panel = panel.child(view);
    }
    panel
}

// Working trailer (transcript.rs:7013) ----------------------------------------------

/// Zeron's gradient spinner: 3×3 round cells tinted per row, a linear pulse
/// travelling upward every 750 ms.
fn gradient_spinner(anim: &Anim) -> View {
    const CELL: f32 = 2.5;
    column().gap(CELL / 2.).shrink_0().children((0..3).map(|r| {
        let anim = anim.clone();
        row().gap(CELL / 2.).children((0..3).map(move |c| {
            let anim = anim.clone();
            let phase = (2. - r as f32 + (c as f32 - 1.).abs()) / 4.;
            div()
                .size(CELL, CELL)
                .rounded(CELL / 2.)
                .bg(GRADIENT[r])
                .reactive_style(move || {
                    Styles::new().opacity(gspin(anim.ambient_clock.get() / 0.75 + phase, 0.1))
                })
        }))
    }))
}

/// "Thinking… 12s" under a live turn: the gradient spinner, a flavour word
/// that turns every 7 s and a timer whose digits roll. `seed` picks the
/// chat's word sequence (pass [`flavour_seed`] of the chat id).
pub fn trailer(
    cx: &mut Context,
    anim: &Anim,
    working: impl Fn() -> bool + Clone + 'static,
    seed: u32,
) -> View {
    let started = cx.state(None::<f32>);
    let guard = Rc::new(RefCell::new(None::<AmbientGuard>));
    {
        let (anim, started, guard) = (anim.clone(), started.clone(), guard.clone());
        let effect = cx.runtime().effect(move || {
            let live = working();
            let mut slot = guard.borrow_mut();
            if live && slot.is_none() {
                *slot = Some(anim.ambient());
                started.set(Some(anim.now()));
            } else if !live && slot.is_some() {
                *slot = None;
                started.set(None);
            }
        });
        cx.retain(effect);
    }
    let anim = anim.clone();
    // Keyed by the turn's start, so a new turn's labels appear fresh.
    switch(
        move || started.get().map(f32::to_bits),
        move |turn, cx| match turn {
            Some(bits) => working_row(cx, &anim, f32::from_bits(bits), seed),
            None => div(),
        },
    )
    .w_full()
}

fn working_row(cx: &mut Context, anim: &Anim, at: f32, seed: u32) -> View {
    let elapsed = {
        let anim = anim.clone();
        move || (anim.ambient_clock.get() - at).max(0.) as i64
    };
    let word = {
        let elapsed = elapsed.clone();
        roll_text(
            cx,
            anim,
            move || format!("{}…", flavour_word(u64::from(seed), elapsed())),
            12.,
            TEXT_MUTED,
        )
    };
    let timer = roll_text(cx, anim, move || format_elapsed(elapsed()), 11., TEXT_FAINT);
    row()
        .pt(16.)
        .gap(8.)
        .items_center()
        .text_size(11.)
        .font_family(SANS)
        .child(gradient_spinner(anim))
        .child(word)
        .child(div().translate(0., 1.).child(timer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(verb: &'static str, path: &str, error: bool) -> SummaryItem {
        SummaryItem {
            thought: false,
            verb,
            path: path.into(),
            error,
        }
    }

    fn thought() -> SummaryItem {
        SummaryItem {
            thought: true,
            verb: "",
            path: String::new(),
            error: false,
        }
    }

    #[test]
    fn summaries_follow_zeron() {
        assert_eq!(
            summary(&[
                item("Run", "ls", false),
                item("Run", "pwd", false),
                item("Run", "make", true),
                item("Edit", "a.rs", false),
                item("Write", "b.rs", false),
                item("Edit", "a.rs", false),
            ]),
            "Ran 3 commands · edited 2 files · 1 failed"
        );
        assert_eq!(summary(&[thought()]), "Thought process");
        assert_eq!(
            summary(&[thought(), thought(), item("Read", "x", false)]),
            "Thought 2 times · read 1 file"
        );
        assert_eq!(
            summary(&[
                item("Search", "p", false),
                item("Glob", "*", false),
                item("Web", "q", false),
                item("Fetch", "u", false),
                item("Todo", "", false),
                item("MCP", "s · t", false),
                item("Agent", "d", false),
            ]),
            "Searched 3 times · fetched 1 page · updated todos · called 2 tools"
        );
        assert_eq!(summary(&[item("Patch", "", false)]), "Edited 1 file");
        assert_eq!(summary(&[item("Search", "p", false)]), "Searched 1 time");
    }

    #[test]
    fn shimmer_is_a_linear_tent_with_copies() {
        // phase 0.5: the primary peak sits at x = 0.5.
        assert!((shimmer_amount(0.5, 0.5) - 1.).abs() < 1e-6);
        assert!((shimmer_amount(0.5 + 0.18, 0.5) - 0.5).abs() < 1e-5);
        assert_eq!(shimmer_amount(0.5 + 0.36, 0.5), 0.);
        // No seam: the end of one sweep matches the start of the next.
        for i in 0..=10 {
            let x = i as f32 / 10.;
            assert!((shimmer_amount(x, 0.) - shimmer_amount(x, 1.)).abs() < 1e-5);
        }
        // A copy three widths behind the primary reaches the title.
        assert!(shimmer_amount(0.5, 0.0) > 0. || shimmer_amount(0.5, 0.5) > 0.);
        assert!((shimmer_phase(3.4 * 1.25) - 0.25).abs() < 1e-5);
    }

    #[test]
    fn connector_parts_split_incoming_and_branch() {
        assert_eq!(connector_parts(0., false), (0., 0.));
        assert_eq!(connector_parts(1., false), (1., 1.));
        let (incoming, branch) = connector_parts(0.31, false);
        assert!((incoming - 0.5).abs() < 1e-5 && branch == 0.);
        let (incoming, branch) = connector_parts(0.45, true);
        assert_eq!((incoming, branch), (0., 0.));
        let (incoming, branch) = connector_parts(0.84, true);
        assert!(incoming == 1. && (branch - 0.5).abs() < 1e-5);
        assert_eq!(connector_continuation(None), 0.);
        assert!((connector_continuation(Some(0.225)) - 0.5).abs() < 1e-5);
        assert_eq!(connector_continuation(Some(0.9)), 1.);
    }

    #[test]
    fn elbow_samples_reveal_by_arc_length() {
        let full = branch_points(1.);
        assert_eq!(full.len(), 26);
        assert_eq!(full[0], (0., 0.));
        assert!((full[24].0 - 6.).abs() < 1e-5 && (full[24].1 - 6.).abs() < 1e-5);
        assert_eq!(full[25], (15.5, 6.));
        assert_eq!(branch_points(0.), vec![(0., 0.)]);
        let length = |points: &[(f32, f32)]| {
            points
                .windows(2)
                .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
                .sum::<f32>()
        };
        let total = length(&full);
        let half = branch_points(0.5);
        assert!((length(&half) - total / 2.).abs() < 1e-3);
    }

    #[test]
    fn roll_keeps_shared_ends() {
        assert_eq!(shared_ends("Opus 5.5", "Opus 5.1"), (7, 0));
        assert_eq!(shared_ends("feature/a", "fix/a"), (1, 2));
        assert_eq!(shared_ends("main", "main-2"), (4, 0));
        assert_eq!(shared_ends("aa", "aaa"), (2, 0));
        assert_eq!(shared_ends("ab", "b"), (0, 1));
        assert_eq!(shared_ends("9s", "10s"), (0, 1));
        let delays = roll_delays(&[0., 5., 10.]);
        assert_eq!(delays[0], 0.);
        let spread = ROLL_DURATION * ROLL_STAGGER * 2. / 3.;
        assert!((delays[2] - spread).abs() < 1e-6);
        assert!((ROLL_TOTAL - 0.715).abs() < 1e-6);
    }

    #[test]
    fn elapsed_and_flavour() {
        assert_eq!(format_elapsed(0), "0s");
        assert_eq!(format_elapsed(59), "59s");
        assert_eq!(format_elapsed(61), "1m 1s");
        assert_eq!(format_elapsed(3_725), "1h 2m");
        assert_eq!(format_elapsed(90_000), "1d 1h");
        assert_eq!(flavour_word(0, 0), "Zeroning");
        assert_eq!(flavour_word(0, 6), "Zeroning");
        assert_eq!(flavour_word(0, 7), "Thinking");
        assert_eq!(flavour_word(20, 7), "Zeroning");
        let seed = flavour_seed("chat-1");
        assert!(seed < 21);
        assert_eq!(
            flavour_word(u64::from(seed), 14),
            flavour_word(fnv1a(b"chat-1"), 14)
        );
    }

    #[test]
    fn arrivals_stagger_after_the_header() {
        assert_eq!(arrival_start(None, Some(1.), 0), None);
        assert_eq!(
            arrival_start(Some(1.), Some(1.), 0),
            Some(1. + FIRST_ROW_DELAY)
        );
        assert_eq!(
            arrival_start(Some(1. + ROW_STAGGER), Some(1.), 1),
            Some(1. + ROW_STAGGER + FIRST_ROW_DELAY)
        );
        // A later arrival into an existing group has no header delay.
        assert_eq!(arrival_start(Some(4.), Some(1.), 1), Some(4.));
    }

    #[test]
    fn details_are_capped_and_analytic() {
        let output: String = (0..30).map(|i| format!("line {i}\n")).collect();
        let tool = Tool {
            verb: "Run",
            detail: "cargo test".into(),
            output,
            output_ref: Some("blob".into()),
            output_bytes: Some(5000),
            ..Tool::default()
        };
        let detail = compute_detail(&tool, false, "", false, &Fetches::default());
        let Some(Body::Output(lines)) = &detail.body else {
            panic!("output body")
        };
        assert_eq!((lines.lines.len(), lines.more), (24, 6));
        assert_eq!(
            detail.affordance.as_ref().map(|a| a.label.as_str()),
            Some("Show full output (5 KB)")
        );
        // invocation 1 + 18 + 12, output 1 + 25 × 18 + 12, affordance 24.
        assert_eq!(detail.extra(), 31. + 463. + 24.);
        let wrapped = call_lines(&"x".repeat(170)).unwrap();
        assert_eq!(wrapped.lines.len(), 3);
    }

    #[test]
    fn hunks_number_lines() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\n";
        let found = hunks(old, new);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].header, "@@ -2,7 +2,7 @@");
        assert_eq!(found[0].lines.len(), 8);
        assert!(hunks(old, old).is_empty());
    }

    #[test]
    fn thoughts_wrap_and_keep_the_live_tail() {
        let text: String = (0..40).map(|i| format!("para {i}\n\n")).collect();
        let (lines, more) = thought_lines(&text, false);
        assert_eq!(lines.len(), 24);
        assert!(more > 0);
        let (live, _) = thought_lines(&text, true);
        assert!(live.last().unwrap().iter().any(|(t, _)| t.contains("39")));
        let long = "word ".repeat(40);
        let (lines, _) = thought_lines(&long, false);
        assert!(lines.len() >= 2);
        assert!(
            lines
                .iter()
                .all(|l| l.iter().map(|(t, _)| t.chars().count()).sum::<usize>() <= 96)
        );
    }
}
