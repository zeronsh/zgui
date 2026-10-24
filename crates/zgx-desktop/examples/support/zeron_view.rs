//! The transcript: user bubbles, streamed markdown under a fading veil, tool
//! groups with a connector tree, error chips and the hover metadata strip.
//! Layout and motion follow Zeron's `crates/ui/src/transcript.rs`,
//! `markdown/render.rs` and `notice.rs`; colours are white at graded alpha so
//! glass shows through.
//!
//! Every animation reads one page clock (`Anim`) that ticks on display frames
//! only while something moves, so a settled transcript costs nothing.

use super::markdown::{self as md, Align, Piece, Style, Token};
use super::model::{self, Block};
use super::rpc::Rpc;
use super::theme::{DANGER_MUTED, FADE_QUICK, Hover, Spec, alpha, hairline, ink, wash};
use std::{collections::HashMap, rc::Rc, sync::Arc};
use zgx::{
    compose::{Tasks, prelude::*},
    cursor::Cursor,
    input::InputEvent,
    reactive::Signal,
    rich_text::{RichText, TextMeasure, TextRun},
    scene::Color,
    text_layout::{FontStyle, LineHeight},
};

// Palette ---------------------------------------------------------------------

// Zeron's dark tokens (`theme.text`, `text_muted`, `text_faint`, `border`).
pub const TEXT: Color = super::theme::TEXT;
pub const BODY: Color = super::theme::TEXT;
pub const MUTED: Color = super::theme::TEXT_MUTED;
pub const HAIRLINE: Color = super::theme::BORDER;
pub const ACCENT: Color = super::theme::ACCENT;
pub const DANGER: Color = super::theme::DANGER;

pub fn white(alpha: u8) -> Color {
    Color(255, 255, 255, alpha)
}

pub fn fade(color: Color, alpha: f32) -> Color {
    Color(
        color.0,
        color.1,
        color.2,
        (f32::from(color.3) * alpha.clamp(0., 1.)).round() as u8,
    )
}

pub use super::theme::{Anim, icon, icon_data};

// Transcript -------------------------------------------------------------------

/// What the transcript view needs from the app.
#[derive(Clone)]
pub struct Ctx {
    pub anim: Anim,
    /// `None` in the offline markdown preview.
    pub rpc: Option<Rpc>,
    pub transcript: model::Shared,
    pub tasks: Tasks,
    pub toggles: Toggles,
}

impl Ctx {
    #[allow(dead_code)]
    pub fn new(anim: Anim, rpc: Option<Rpc>, transcript: model::Shared, tasks: Tasks) -> Self {
        Self {
            anim,
            rpc,
            transcript,
            tasks,
            toggles: Toggles::default(),
        }
    }
}

/// What the reader or the stream opened and closed, by part or entry, so a
/// row scrolled out of the virtualized transcript comes back as it was left.
#[derive(Clone, Default)]
pub struct Toggles(Rc<std::cell::RefCell<HashMap<String, bool>>>);

impl Toggles {
    pub fn get(&self, key: &str) -> Option<bool> {
        self.0.borrow().get(key).copied()
    }
    pub fn set(&self, key: &str, open: bool) {
        self.0.borrow_mut().insert(key.to_owned(), open);
    }
}

/// A row's kind, for its top gap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    User,
    Text,
    Group,
    Error,
}

impl RowKind {
    fn of(block: &Block) -> Self {
        match block {
            Block::User(_) => Self::User,
            Block::Text(_) => Self::Text,
            Block::Group(_) => Self::Group,
            Block::Error(_) => Self::Error,
        }
    }
}

/// Zeron's `top_gap_for`: nothing above the first row (the column pads it),
/// 16 at a turn start (an entry's first row), 12 to or from a tool group,
/// 8 else. Markdown blocks of one part sit 12 apart inside their part's view.
pub fn top_gap(previous: Option<(RowKind, bool)>, this: RowKind) -> f32 {
    match previous {
        None => 0.,
        Some((_, same_entry)) if !same_entry || this == RowKind::User => 16.,
        Some((before, _)) if before == RowKind::Group || this == RowKind::Group => 12.,
        Some(_) => 8.,
    }
}

fn gap(transcript: &model::Transcript, block: &Block, previous: Option<&Block>) -> f32 {
    let entry = |block: &Block| transcript.entry_of(block).map(|info| info.id);
    let previous = previous.map(|before| {
        let same = match (entry(before), entry(block)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        (RowKind::of(before), same)
    });
    top_gap(previous, RowKind::of(block))
}

/// The hover metadata lane under a row: 8 + 2 × 12, reserved whether or not
/// it shows, so revealing it never moves the list.
pub const STRIP: f32 = 32.;

/// User bubbles: 14 px text on 22 px lines, folded past five lines.
const USER_LINE: f32 = 22.;
const USER_LINES: usize = 5;
const USER_CHARS: usize = 400;
const USER_COLLAPSED_TEXT: f32 = USER_LINES as f32 * USER_LINE;
/// The folded body with its "..." line: the fold's resize endpoint.
const USER_COLLAPSED: f32 = USER_COLLAPSED_TEXT + USER_LINE;
/// The expander: `mt 8` over one 22 px line.
const USER_TOGGLE: f32 = 8. + USER_LINE;

/// Zeron's fold decision: more than five source lines, or measured taller
/// than five lines, or (not yet measured) longer than 400 characters.
pub fn user_needs_collapse(text: &str, measured: f32) -> bool {
    text.lines().count() > USER_LINES
        || measured > USER_COLLAPSED_TEXT + 0.5
        || (measured <= 0. && text.chars().count() > USER_CHARS)
}

/// Zeron's `user_resize_duration_ms`: travel-scaled, 220 ms up to 850 ms.
pub fn user_resize_ms(delta: f32) -> f32 {
    (220. + delta.max(0.) * 0.32).min(850.).round()
}

/// Zeron's `user_resize_spec`: ease-out, or ease-in-out past 500 px.
pub fn user_resize_spec(delta: f32) -> Spec {
    let curve = if delta > 500. {
        super::theme::EASE_IN_OUT
    } else {
        super::theme::EASE_OUT
    };
    Spec(user_resize_ms(delta) / 1000., curve)
}

/// Error chip text: 12 px on gpui's default φ line height.
const ERROR_LINE: f32 = 12. * 1.618;

/// Zeron's `single_line`: whitespace runs collapse to one space.
fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A block's height in a `width` column before it is mounted: its text
/// measured as the view sets it (by the prepared text layout uses, so this
/// is line-breaking arithmetic once shaped) and the chrome around it. The
/// transcript replaces it with the measured height once the row mounts.
pub fn estimate(
    ctx: &Ctx,
    measure: &TextMeasure,
    block: &Block,
    previous: Option<&Block>,
    width: f32,
) -> f32 {
    let lines = |text: &str, size: f32, line: f32, width: f32| -> f32 {
        if text.is_empty() {
            return line;
        }
        let font = FontStyle {
            line_height: LineHeight::px(line),
            ..FontStyle::default()
        };
        let run = TextRun {
            range: 0..text.len(),
            font,
            font_size: size,
            ..Default::default()
        };
        RichText::new(text, vec![run])
            .map_or(line, |rich| measure.measure(&rich, Some(width.max(1.))).1)
    };
    let transcript = ctx.transcript.borrow();
    let body = match block {
        Block::User(entry) => {
            let text = transcript
                .user_text
                .get(entry)
                .map(|t| t.with_untracked(String::clone))
                .unwrap_or_default();
            let open = ctx.toggles.get(&format!("user:{entry}")).unwrap_or(false);
            let body = lines(&text, 14., USER_LINE, width * 0.8 - 32.);
            let body = if user_needs_collapse(&text, body) {
                if open {
                    body + USER_TOGGLE
                } else {
                    USER_COLLAPSED + USER_TOGGLE
                }
            } else {
                body
            };
            // `py 10` around the body.
            body + 20.
        }
        Block::Text(key) => transcript.parts.get(key).map_or(0., |part| {
            let parsed = part.text.with_untracked(|t| md::parse(t));
            let mut height = 0.;
            for (i, block) in parsed.iter().enumerate() {
                height += match (i.checked_sub(1).map(|p| kind(&parsed[p])), kind(block)) {
                    (None, _) => 0.,
                    (Some(1), 1) => 4.,
                    _ => 12.,
                };
                height += match block {
                    md::Block::Para(run) => lines(&run.text, 14., 22., width),
                    md::Block::Heading(level, run) => match level {
                        1 => lines(&run.text, 19., 27., width),
                        2 => lines(&run.text, 16., 24., width),
                        3 => lines(&run.text, 15., 22., width),
                        _ => lines(&run.text, 14., 22., width),
                    },
                    md::Block::Item { depth, run, .. } => {
                        lines(&run.text, 14., 22., width - *depth as f32 * 26. - 26.)
                    }
                    // The 2 px rail, `pl 12`, `pr 10`.
                    md::Block::Quote(run) => lines(&run.text, 14., 22., width - 24.) + 12.,
                    // Inside the frame's hairlines.
                    md::Block::Code { run, .. } => {
                        2. + CODE_HEADER
                            + code_lines(&run.text) as f32 * CODE_LINE
                            + 2. * CODE_PAD_Y
                    }
                    // 46 px rows, each over its 1 px rule.
                    md::Block::Table { rows, .. } => (rows.len() + 1) as f32 * 47.,
                    md::Block::Rule => 1.,
                };
            }
            height
        }),
        Block::Error(key) => transcript.parts.get(key).map_or(0., |part| {
            let message = part.text.with_untracked(|text| single_line(text));
            // Wrapper `py 4`, frame `py 8` inside its hairlines, the 20 px
            // header and a 6 px gap.
            8. + 2. + 16. + 20. + 6. + lines(&message, 12., ERROR_LINE, width - 22.)
        }),
        Block::Group(key) => {
            let members = transcript
                .groups
                .get(key)
                .map_or(0, |m| m.with_untracked(Vec::len));
            let open = ctx.toggles.get(&format!("group:{key}")).unwrap_or(false);
            // Header, then the chips' top pad and 32 px rail rows.
            super::tools::HEADER_HEIGHT
                + if open {
                    super::tools::CHIPS_TOP_PAD + members as f32 * super::tools::ROW_HEIGHT
                } else {
                    0.
                }
        }
    };
    let strip = if transcript.strips.with_untracked(|s| s.contains(block)) {
        STRIP
    } else {
        0.
    };
    gap(&transcript, block, previous) + body + strip
}

/// One transcript row: its top gap (padding, so the gap is part of the
/// entry's hover area, as in Zeron), its content and, under an entry's
/// last row, the reserved metadata strip. Rows appear instantly: Zeron has
/// no entrance motion for messages; only the streaming veil fades text.
pub fn block(cx: &mut Context, ctx: &Ctx, block: &Block, previous: Option<Block>) -> View {
    let (gap, entry) = {
        let transcript = ctx.transcript.borrow();
        (
            gap(&transcript, block, previous.as_ref()),
            transcript.entry_of(block).map(|info| info.id),
        )
    };
    let view = match block {
        Block::User(entry) => user_bubble(cx, ctx, entry),
        Block::Text(key) => match ctx.transcript.borrow().parts.get(key).cloned() {
            Some(part) => markdown(cx, ctx, key, part),
            None => div(),
        },
        Block::Error(key) => error_chip(cx, ctx, key),
        Block::Group(key) => match ctx.transcript.borrow().groups.get(key).cloned() {
            Some(members) => super::tools::tool_group(cx, ctx, key, members),
            None => div(),
        },
    };
    let row = column()
        .w_full()
        .pt(gap)
        .child(view)
        .child(strip(ctx, block));
    match entry {
        Some(entry) => track_entry_hover(ctx, block.clone(), entry, row),
        None => row,
    }
}

/// Reveal the entry's strips while the pointer is over any of its rows.
/// Pointer enter and leave bubble from the node under the pointer, so moving
/// between two children of a row is a leave then an enter: the clear waits a
/// tick and is dropped if the pointer is still inside. Only the row that
/// owns the reveal clears it.
fn track_entry_hover(ctx: &Ctx, block: Block, entry: String, row: View) -> View {
    let hovered = ctx.transcript.borrow().hovered.clone();
    let tasks = ctx.tasks.clone();
    let inside = Rc::new(std::cell::Cell::new(false));
    row.on_event(move |event| match event.event {
        InputEvent::PointerEnter => {
            inside.set(true);
            let next = Some((block.clone(), entry.clone()));
            if hovered.with_untracked(|h| *h != next) {
                hovered.set(next);
            }
        }
        InputEvent::PointerLeave | InputEvent::PointerCancel => {
            inside.set(false);
            let (inside, hovered, block) = (inside.clone(), hovered.clone(), block.clone());
            // Spawned tasks run on the next executor tick, after this
            // event's enter (if any) has been routed.
            tasks.spawn(async move {
                if !inside.get()
                    && hovered.with_untracked(|h| h.as_ref().is_some_and(|(row, _)| *row == block))
                {
                    hovered.set(None);
                }
            });
        }
        _ => {}
    })
}

/// The reserved 32 px lane under a user bubble or a settled entry's last
/// row; its timestamp and copy button mount while the entry is hovered,
/// fading in over `FADE_QUICK`, and vanish at once on leave.
fn strip(ctx: &Ctx, block: &Block) -> View {
    let strips = ctx.transcript.borrow().strips.clone();
    let (ctx, block) = (ctx.clone(), block.clone());
    let key = block.clone();
    switch(
        move || strips.with(|s| s.contains(&key)),
        move |shown, _| {
            if !shown {
                return div();
            }
            let Some(info) = ctx.transcript.borrow().entry_of(&block) else {
                return div();
            };
            let user = info.role == "user";
            let hovered = ctx.transcript.borrow().hovered.clone();
            let ctx = ctx.clone();
            let entry = info.id.clone();
            row()
                .w_full()
                .h(STRIP)
                .pt(8.)
                .items_center()
                .when(user, |r| r.justify_end())
                .child(switch(
                    move || hovered.with(|h| h.as_ref().is_some_and(|(_, e)| *e == entry)),
                    move |on, cx| {
                        if on { metadata(cx, &ctx, &info) } else { div() }
                    },
                ))
        },
    )
    .w_full()
}

fn metadata(cx: &mut Context, ctx: &Ctx, info: &model::EntryInfo) -> View {
    let anim = ctx.anim.clone();
    let born = anim.now();
    anim.keep(FADE_QUICK.0 + 0.05);
    let stamp = info
        .created_ms
        .map(|ms| format_timestamp(ms, local_offset(ms)))
        .unwrap_or_default();
    let has_copy = ctx.transcript.borrow().copy_text(&info.id).is_some();
    let copy = has_copy.then(|| {
        let copied = cx.state(false);
        let (transcript, tasks, entry) =
            (ctx.transcript.clone(), ctx.tasks.clone(), info.id.clone());
        let on_copy = {
            let copied = copied.clone();
            move || {
                let text = transcript.borrow().copy_text(&entry);
                if let Some(text) = text
                    && let Ok(mut clipboard) = arboard::Clipboard::new()
                    && clipboard.set_text(text).is_ok()
                {
                    copied.set(true);
                    let copied = copied.clone();
                    tasks.spawn(async move {
                        zgx::timer::sleep(std::time::Duration::from_millis(1200)).await;
                        copied.set(false);
                    });
                }
            }
        };
        faded_button(
            cx,
            &ctx.anim,
            row()
                .size(24., 24.)
                .rounded(6.)
                .items_center()
                .justify_center()
                .cursor(Cursor::Pointer)
                .on_click(on_copy)
                .child(icon_switch(14., MUTED, move || {
                    if copied.get() { "check" } else { "copy" }
                })),
            || (Color(0, 0, 0, 0), ink(0.08)),
        )
    });
    row()
        .gap(8.)
        .items_center()
        .reactive_style(move || {
            let p = FADE_QUICK.at(anim.at(born + FADE_QUICK.0) - born);
            Styles::new().opacity(p)
        })
        .child(text(stamp).text_size(12.).text_color(alpha(MUTED, 0.55)))
        .children(copy)
}

/// A monochrome icon whose glyph follows `name`.
fn icon_switch(size: f32, color: Color, name: impl Fn() -> &'static str + 'static) -> View {
    svg_signal("icon", move || Arc::new(icon_data(name()).tinted(color))).size(size, size)
}

/// A quiet icon button: its background blends `base` → `over` over 150 ms
/// on hover (Zeron's `hover_blend`).
fn faded_button(
    cx: &mut Context,
    anim: &Anim,
    view: View,
    colors: impl Fn() -> (Color, Color) + 'static,
) -> View {
    let hover = Hover::new(cx, anim);
    let blend = hover.clone();
    hover.track(view.reactive_style(move || {
        let (base, over) = colors();
        Styles::new().bg(super::theme::mix(base, over, blend.get()))
    }))
}

// Timestamps ---------------------------------------------------------------------

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// (year, month 1-12, day) from days since 1970-01-01 (Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, usize, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as usize, day)
}

/// Zeron's `format_timestamp`: `%b %-d, %-I:%M %p` ("Jul 1, 3:45 PM") at a
/// UTC offset of `offset` seconds.
pub fn format_timestamp(ms: i64, offset: i64) -> String {
    let seconds = ms.div_euclid(1000) + offset;
    let (_, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    let (hour, minute) = (of_day / 3600, of_day % 3600 / 60);
    let (hour12, meridiem) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    format!(
        "{} {day}, {hour12}:{minute:02} {meridiem}",
        MONTHS[month - 1]
    )
}

/// The local zone's UTC offset (seconds) at `ms`, from `localtime_r`.
#[cfg(all(unix, target_pointer_width = "64"))]
fn local_offset(ms: i64) -> i64 {
    use std::ffi::{c_char, c_int, c_long};
    #[repr(C)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }
    // SAFETY: POSIX's `localtime_r` (time_t is i64 on 64-bit unix).
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let time = ms.div_euclid(1000);
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    // SAFETY: `localtime_r` writes a `struct tm` (whose 64-bit layout on
    // macOS and glibc matches `Tm`) into the buffer and returns it, or null.
    unsafe {
        if localtime_r(&time, tm.as_mut_ptr()).is_null() {
            0
        } else {
            tm.assume_init().gmtoff
        }
    }
}

#[cfg(not(all(unix, target_pointer_width = "64")))]
fn local_offset(_ms: i64) -> i64 {
    0
}

// User bubble ----------------------------------------------------------------------

/// A fold resize in flight: the body's height (including the "..." line)
/// eases `from` → `to` starting at `at`.
#[derive(Clone, Copy, PartialEq)]
struct Fold {
    from: f32,
    to: f32,
    at: f32,
    spec: Spec,
}

impl Fold {
    fn end(self) -> f32 {
        self.at + self.spec.0
    }
    fn value(self, now: f32) -> f32 {
        self.from + (self.to - self.from) * self.spec.at(now - self.at)
    }
}

/// Zeron's user bubble: `wash(.08)`, radius 16, `px 16 py 10`, right-aligned
/// and capped at 80% of the column. Past five lines the text is clipped to
/// five line boxes over a "..." line, with a "Show more" link; toggling eases
/// the clip's height over a travel-scaled duration.
fn user_bubble(cx: &mut Context, ctx: &Ctx, entry: &str) -> View {
    let text_ = ctx
        .transcript
        .borrow()
        .user_text
        .get(entry)
        .cloned()
        .unwrap_or_else(|| cx.state(String::new()));
    let memory = format!("user:{entry}");
    let expanded = cx.state(ctx.toggles.get(&memory).unwrap_or(false));
    // The full text's laid-out height, from a probe behind the clip.
    let measured = cx.state(0_f32);
    let fold = cx.state(None::<Fold>);
    let anim = ctx.anim.clone();
    let collapsible = {
        let (text_, measured) = (text_.clone(), measured.clone());
        move || text_.with(|t| user_needs_collapse(t, measured.get()))
    };
    let probe = {
        let measured = measured.clone();
        canvas(move |(_, height)| {
            if measured.with_untracked(|m| (m - height).abs() > 0.5) {
                measured.set(height);
            }
            zgx::canvas::Canvas::new()
        })
        .absolute()
        .left(0.)
        .top(0.)
        .right(0.)
        .bottom(0.)
    };
    let body = {
        let text_ = text_.clone();
        column()
            .w_full()
            .shrink_0()
            .child(
                text_signal(move || text_.get())
                    .w_full()
                    .text_wrap(true)
                    .text_size(14.)
                    .line_height(USER_LINE)
                    .text_color(TEXT),
            )
            .child(probe)
    };
    // The full text stays laid out behind the clip; only the clip's height
    // changes, so wrapping never shifts.
    let clip = {
        let (expanded, fold, anim, collapsible) = (
            expanded.clone(),
            fold.clone(),
            anim.clone(),
            collapsible.clone(),
        );
        column().w_full().child(body).reactive_style(move || {
            let open = expanded.get();
            let foldable = collapsible();
            if let Some(f) = fold.get()
                && foldable
            {
                let now = anim.at(f.end());
                if now < f.end() {
                    let ellipsis = if open { 0. } else { USER_LINE };
                    return Styles::new()
                        .h((f.value(now) - ellipsis).max(0.))
                        .overflow_hidden();
                }
            }
            if foldable && !open {
                Styles::new().h(USER_COLLAPSED_TEXT).overflow_hidden()
            } else {
                Styles::new()
            }
        })
    };
    let ellipsis = {
        let (expanded, collapsible) = (expanded.clone(), collapsible.clone());
        div()
            .h(USER_LINE)
            .text_size(14.)
            .line_height(USER_LINE)
            .text_color(TEXT)
            .child(text("..."))
            .reactive_style(move || {
                if collapsible() && !expanded.get() {
                    Styles::new()
                } else {
                    Styles::new().hidden()
                }
            })
    };
    let expander = {
        let hovered = cx.state(false);
        let (label, glyph, tint) = (expanded.clone(), expanded.clone(), hovered.clone());
        let on_toggle = {
            let (expanded, measured, fold, anim, toggles) = (
                expanded.clone(),
                measured.clone(),
                fold.clone(),
                anim.clone(),
                ctx.toggles.clone(),
            );
            move || {
                let now = anim.now();
                let full = measured.with_untracked(|m| *m).max(USER_COLLAPSED);
                let open = expanded.with_untracked(|e| *e);
                // From wherever the body is now, so a reversal doesn't jump.
                let from = match fold.with_untracked(|f| *f) {
                    Some(f) if now < f.end() => f.value(now),
                    _ if open => full,
                    _ => USER_COLLAPSED,
                };
                let to = if open { USER_COLLAPSED } else { full };
                let spec = user_resize_spec(full - USER_COLLAPSED);
                fold.set(Some(Fold {
                    from,
                    to,
                    at: now,
                    spec,
                }));
                expanded.set(!open);
                toggles.set(&memory, !open);
                anim.keep(spec.0 + 0.05);
            }
        };
        let collapsible = collapsible.clone();
        row()
            .mt(8.)
            .items_start()
            .reactive_style(move || {
                if collapsible() {
                    Styles::new()
                } else {
                    Styles::new().hidden()
                }
            })
            .child(
                row()
                    .gap(5.)
                    .items_center()
                    .text_size(14.)
                    .line_height(USER_LINE)
                    .cursor(Cursor::Pointer)
                    .on_click(on_toggle)
                    .on_event(move |event| match event.event {
                        InputEvent::PointerEnter => {
                            hovered.set(true);
                        }
                        InputEvent::PointerLeave | InputEvent::PointerCancel => {
                            hovered.set(false);
                        }
                        _ => {}
                    })
                    .reactive_style({
                        let tint = tint.clone();
                        move || Styles::new().text_color(if tint.get() { TEXT } else { MUTED })
                    })
                    .child(text_signal(move || {
                        if label.get() {
                            "Show less"
                        } else {
                            "Show more"
                        }
                        .into()
                    }))
                    .child(
                        svg_signal("expander", move || {
                            let name = if glyph.get() {
                                "alt-arrow-up"
                            } else {
                                "alt-arrow-down"
                            };
                            Arc::new(icon_data(name).tinted(if tint.get() { TEXT } else { MUTED }))
                        })
                        .size(12., 12.),
                    ),
            )
    };
    // `min_w 0` lets a long prompt wrap inside the 80% cap.
    row().w_full().justify_end().child(
        column()
            .min_w(0.)
            .max_w_percent(80.)
            .rounded(16.)
            .px(16.)
            .py(10.)
            .bg(wash(0.08))
            .text_size(14.)
            .line_height(USER_LINE)
            .text_color(TEXT)
            .child(clip)
            .child(ellipsis)
            .child(expander),
    )
}

// Error chip -------------------------------------------------------------------------

/// Zeron's transcript `ErrorChip` (`notice_chip` with the tile): a red-washed
/// card with a danger tile, "Error", a copy button and the wrapped message.
fn error_chip(cx: &mut Context, ctx: &Ctx, key: &str) -> View {
    let message = ctx
        .transcript
        .borrow()
        .parts
        .get(key)
        .map(|p| p.text.clone())
        .unwrap_or_else(|| cx.state(String::new()));
    let label = alpha(DANGER_MUTED, 0.8);
    let copy = {
        let message = message.clone();
        move || {
            let text = message.with_untracked(|m| single_line(m));
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text);
            }
        }
    };
    let header = row()
        .w_full()
        .gap(8.)
        .items_center()
        .child(
            row()
                .size(20., 20.)
                .shrink_0()
                .rounded(6.)
                .bg(alpha(DANGER, 0.12))
                .items_center()
                .justify_center()
                .child(icon("danger-triangle", 12., label)),
        )
        .child(text("Error").font_weight(500).text_color(label))
        .child(div().grow())
        .child(
            row()
                .size(20., 20.)
                .shrink_0()
                .rounded(6.)
                .items_center()
                .justify_center()
                .cursor(Cursor::Pointer)
                .hover(|s| s.bg(alpha(DANGER, 0.12)))
                .on_click(copy)
                .child(icon("copy", 12., label)),
        );
    column().w_full().py(4.).child(
        column()
            .w_full()
            .gap(6.)
            .rounded(10.)
            .border(1.)
            .border_color(alpha(DANGER, 0.16))
            .bg(alpha(DANGER, 0.05))
            .px(10.)
            .py(8.)
            .text_size(12.)
            .line_height(ERROR_LINE)
            .overflow_hidden()
            .child(header)
            .child(
                text_signal(move || message.with(|m| single_line(m)))
                    .w_full()
                    .min_w(0.)
                    .text_wrap(true)
                    .text_color(alpha(TEXT, 0.8)),
            ),
    )
}

// Markdown ---------------------------------------------------------------------

/// A streamed chunk: (source offset, arrival, fade seconds).
type Chunk = (usize, f32, f32);

/// Veil alpha for a source offset (Zeron's `veil.rs`): the chunk holding it
/// fades `1 - (1 - p)^1.6`, sped up by 30% per chunk beyond two still fading.
fn veil_alpha(chunks: &[Chunk], offset: usize, now: f32) -> f32 {
    let active = chunks
        .iter()
        .filter(|(_, born, fade)| now - born < *fade)
        .count();
    let boost = 1. + 0.3 * active.saturating_sub(2) as f32;
    match chunks.iter().rev().find(|(start, _, _)| *start <= offset) {
        Some((_, born, fade)) => {
            let p = ((now - born) * boost / fade).clamp(0., 1.);
            1. - (1. - p).powf(1.6)
        }
        None => 1.,
    }
}

/// Split `piece` at chunk boundaries and fade each part.
fn veiled(piece: &Piece, chunks: &[Chunk], now: f32, mut span: impl FnMut(String, f32)) {
    let end = piece.start + piece.text.len();
    let mut cuts: Vec<usize> = chunks
        .iter()
        .map(|(start, _, _)| *start)
        .filter(|start| *start > piece.start && *start < end)
        .collect();
    cuts.dedup();
    let mut from = piece.start;
    for cut in cuts.into_iter().chain([end]) {
        if cut > from
            && piece.text.is_char_boundary(from - piece.start)
            && piece.text.is_char_boundary(cut - piece.start)
        {
            span(
                piece.text[from - piece.start..cut - piece.start].to_owned(),
                veil_alpha(chunks, from, now),
            );
            from = cut;
        }
    }
}

fn styled_span(text: String, style: Style, base: Color, alpha: f32) -> TextSpan {
    let span = text_span(text);
    match style {
        Style::Plain => span.text_color(fade(base, alpha)),
        Style::Bold => span.text_color(fade(TEXT, alpha)).font_weight(600),
        Style::Italic => span.text_color(fade(base, alpha)).italic(true),
        Style::BoldItalic => span
            .text_color(fade(TEXT, alpha))
            .font_weight(600)
            .italic(true),
        // The text keeps its colour under a muted 1 px line.
        Style::Strike => span.text_color(fade(base, alpha)).strikethrough({
            let mut line = zgx::rich_text::Decoration::new(1.);
            line.color = Some(fade(MUTED, alpha));
            line
        }),
        // Geist Mono at the surrounding size, accent on the accent wash. The
        // wash is not veiled (Zeron paints it under the text).
        Style::Code => span
            .font_family(super::theme::MONO)
            .text_color(fade(ACCENT, alpha))
            .background(super::theme::CODE_WASH),
        // Links keep the text colour over a muted underline.
        Style::Link => span.text_color(fade(TEXT, alpha)).underline({
            let mut line = zgx::rich_text::Decoration::new(1.);
            line.color = Some(fade(MUTED, alpha));
            line
        }),
    }
}

/// Spans for a run, veiled while its chunks fade.
fn run_spans(run: &md::Run, base: Color, chunks: &[Chunk], now: f32) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    for piece in md::inline(run) {
        veiled(&piece, chunks, now, |text, alpha| {
            spans.push(styled_span(text, piece.style, base, alpha))
        });
    }
    spans
}

/// The clock for text at `range` of a streaming part: tracked (so the view
/// redraws each frame) only while a chunk overlapping it is still fading.
/// Settled blocks of a long reply then recompute when text arrives, not on
/// every frame of every other block's fade.
fn veil_now(anim: &Anim, chunks: &[Chunk], range: std::ops::Range<usize>) -> f32 {
    let now = anim.clock.with_untracked(|now| *now);
    let fading = chunks
        .iter()
        .enumerate()
        .any(|(index, (start, born, fade))| {
            let end = chunks.get(index + 1).map_or(usize::MAX, |next| next.0);
            *start < range.end && end > range.start && now - born < *fade
        });
    if fading { anim.at(newest(chunks)) } else { now }
}
/// When the last chunk finishes fading: views track the clock until then.
fn newest(chunks: &[Chunk]) -> f32 {
    chunks
        .iter()
        .map(|(_, born, fade)| born + fade)
        .fold(f32::NEG_INFINITY, f32::max)
}

fn markdown(cx: &mut Context, ctx: &Ctx, key: &str, part: Rc<model::Part>) -> View {
    let blocks = cx.state(Vec::<md::Block>::new());
    {
        let (blocks, text_) = (blocks.clone(), part.text.clone());
        let effect = cx.runtime().effect(move || {
            let parsed = text_.with(|t| md::parse(t));
            blocks.set(parsed);
        });
        cx.retain(effect);
    }
    // A block that changes kind while streaming (a line that turns out to
    // start a table) gets a fresh view: key by index and kind.
    let keys = blocks.clone();
    let (ctx, part, key) = (ctx.clone(), part.clone(), key.to_owned());
    keyed(
        move || {
            keys.with(|b| {
                b.iter()
                    .enumerate()
                    .map(|(i, block)| (i, kind(block), i.checked_sub(1).map(|p| kind(&b[p]))))
                    .collect::<Vec<_>>()
            })
        },
        move |(index, this, before), cx| {
            // Zeron: 12 px between blocks, 4 px between list items.
            let gap = match before {
                None => 0.,
                Some(1) if this == 1 => 4.,
                Some(_) => 12.,
            };
            md_block(cx, &ctx, &key, &part, &blocks, index).mt(gap)
        },
    )
    .w_full()
}

fn kind(block: &md::Block) -> u8 {
    match block {
        md::Block::Para(_) => 0,
        md::Block::Heading(level, _) => 10 + level,
        md::Block::Item { .. } => 1,
        md::Block::Quote(_) => 2,
        md::Block::Code { .. } => 3,
        md::Block::Table { .. } => 4,
        md::Block::Rule => 5,
    }
}

fn md_block(
    cx: &mut Context,
    ctx: &Ctx,
    key: &str,
    part: &Rc<model::Part>,
    blocks: &Signal<Vec<md::Block>>,
    index: usize,
) -> View {
    let this = {
        let blocks = blocks.clone();
        move || blocks.with(|b| b.get(index).cloned())
    };
    let chunks = part.chunks.clone();
    let anim = ctx.anim.clone();
    let spans_of = {
        let (chunks, anim) = (chunks.clone(), anim.clone());
        move |run: &md::Run, base: Color| {
            chunks.with(|c| {
                let now = veil_now(&anim, c, run.start..run.start + run.text.len());
                run_spans(run, base, c, now)
            })
        }
    };
    match this() {
        Some(md::Block::Code { lang, .. }) => {
            let fit = format!("fit:{key}:{index}");
            code_block(cx, ctx, fit, lang, this, chunks)
        }
        Some(md::Block::Table { .. }) => table(cx, this, spans_of),
        // A bare 1 px rule: the 12 px block gaps space it.
        Some(md::Block::Rule) => div().w_full().h(1.).bg(HAIRLINE),
        Some(md::Block::Heading(level, _)) => rich_text_signal(move || match this() {
            Some(md::Block::Heading(_, run)) => spans_of(&run, TEXT),
            _ => Vec::new(),
        })
        .w_full()
        .text_wrap(true)
        .text_size(match level {
            1 => 19.,
            2 => 16.,
            3 => 15.,
            _ => 14.,
        })
        .line_height(match level {
            1 => 27.,
            2 => 24.,
            _ => 22.,
        })
        .font_weight(600),
        Some(md::Block::Item { depth, marker, .. }) => {
            // The transcript has no task checkboxes: a task item keeps the
            // accent disc and reads its "[ ] " / "[x] " literally.
            let body = rich_text_signal({
                let (spans_of, chunks, anim) = (spans_of.clone(), chunks.clone(), anim.clone());
                move || match this() {
                    Some(md::Block::Item { run, task, .. }) => {
                        let mut spans = Vec::new();
                        if let Some(done) = task {
                            let start = run.start.saturating_sub(4);
                            let alpha = chunks.with(|c| {
                                let now = veil_now(&anim, c, start..run.start);
                                veil_alpha(c, start, now)
                            });
                            spans.push(
                                text_span(if done { "[x] " } else { "[ ] " })
                                    .text_color(fade(BODY, alpha)),
                            );
                        }
                        spans.extend(spans_of(&run, BODY));
                        spans
                    }
                    _ => Vec::new(),
                }
            })
            .w(0.)
            .grow()
            .text_wrap(true)
            .text_size(14.)
            .line_height(22.);
            // A 5 px accent disc or an accent "N.", in an 18 px column
            // centred on the first line.
            let marker_view = match marker.as_str() {
                "•" | "" => row()
                    .w(18.)
                    .h(22.)
                    .shrink_0()
                    .items_center()
                    .child(div().size(5., 5.).ml(1.).rounded(2.5).bg(ACCENT)),
                _ => text(marker)
                    .min_w(18.)
                    .shrink_0()
                    .text_size(14.)
                    .line_height(22.)
                    .text_color(ACCENT),
            };
            row()
                .w_full()
                .gap(8.)
                .items_start()
                .pl(depth as f32 * 26.)
                .child(marker_view)
                .child(body)
        }
        Some(md::Block::Quote(_)) => row()
            .w_full()
            .rounded_corners(zgx::decoration::Corners {
                top_left: 0.,
                top_right: 6.,
                bottom_right: 6.,
                bottom_left: 0.,
            })
            .bg(Color(139, 124, 246, 13))
            .border_edges(zgx::scene::Insets {
                top: 0.,
                right: 0.,
                bottom: 0.,
                left: 2.,
            })
            .border_color(Color(139, 124, 246, 153))
            .pl(12.)
            .pr(10.)
            .py(6.)
            .child(
                rich_text_signal(move || match this() {
                    Some(md::Block::Quote(run)) => spans_of(&run, TEXT),
                    _ => Vec::new(),
                })
                .w_full()
                .text_wrap(true)
                .text_size(14.)
                .line_height(22.),
            ),
        _ => rich_text_signal(move || match this() {
            Some(md::Block::Para(run)) => spans_of(&run, BODY),
            _ => Vec::new(),
        })
        .w_full()
        .text_wrap(true)
        .text_size(14.)
        .line_height(22.),
    }
}

/// Zeron's runtime syntax palette.
fn token_color(token: Token) -> Color {
    match token {
        Token::Text => Color(232, 232, 234, 255),
        Token::Keyword => Color(139, 124, 246, 255),
        Token::String => Color(52, 211, 153, 255),
        Token::Number => Color(250, 204, 21, 255),
        Token::Comment => Color(146, 146, 154, 255),
        Token::Function => Color(96, 165, 250, 255),
        Token::Type => Color(192, 132, 252, 255),
        Token::Punct => Color(161, 161, 170, 255),
    }
}

/// Code block metrics (Zeron's `CODE_HEADER_HEIGHT`, `CODE_PADDING_*` and
/// 12.5 px Geist Mono on 18 px lines).
const CODE_HEADER: f32 = 28.;
const CODE_LINE: f32 = 18.;
const CODE_PAD_Y: f32 = 10.;

/// The body's lines: a trailing newline (a fence still streaming) is not a
/// line of its own.
fn code_lines(code: &str) -> usize {
    code.trim_end_matches('\n').lines().count().max(1)
}

/// Zeron's code block: a 10 px rounded frame on `ink(.035)` with a 28 px
/// header (language; fit toggle and copy button) over the highlighted body,
/// which scrolls sideways unless fit is on.
fn code_block(
    cx: &mut Context,
    ctx: &Ctx,
    fit_key: String,
    lang: String,
    this: impl Fn() -> Option<md::Block> + Clone + 'static,
    chunks: Signal<Vec<Chunk>>,
) -> View {
    let copied = cx.state(false);
    let fit = cx.state(ctx.toggles.get(&fit_key).unwrap_or(false));
    let anim = ctx.anim.clone();
    // The highlighted source, wrapped or not.
    let code = {
        let (this, lang) = (this.clone(), lang.clone());
        Rc::new(move |wrap: bool| {
            let (this, lang, chunks, anim) =
                (this.clone(), lang.clone(), chunks.clone(), anim.clone());
            rich_text_signal(move || {
                let Some(md::Block::Code { run, .. }) = this() else {
                    return Vec::new();
                };
                let source = run.text.trim_end_matches('\n');
                let now =
                    chunks.with(|c| veil_now(&anim, c, run.start..run.start + run.text.len()));
                let mut spans = Vec::new();
                chunks.with(|chunks| {
                    for (range, token) in md::highlight(source, &lang) {
                        let piece = Piece {
                            text: source[range.clone()].to_owned(),
                            start: run.start + range.start,
                            style: Style::Plain,
                        };
                        veiled(&piece, chunks, now, |text, alpha| {
                            spans.push(text_span(text).text_color(fade(token_color(token), alpha)));
                        });
                    }
                });
                spans
            })
            .text_wrap(wrap)
            .font_family(super::theme::MONO)
            .text_size(12.5)
            .line_height(CODE_LINE)
        })
    };
    let copy = {
        let (this, copied) = (this.clone(), copied.clone());
        let tasks = ctx.tasks.clone();
        move || {
            if let Some(md::Block::Code { run, .. }) = this()
                && let Ok(mut clipboard) = arboard::Clipboard::new()
                && clipboard
                    .set_text(run.text.trim_end_matches('\n').to_owned())
                    .is_ok()
            {
                copied.set(true);
                let copied = copied.clone();
                tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(1200)).await;
                    copied.set(false);
                });
            }
        }
    };
    let fit_toggle = {
        let (state, shade, toggles) = (fit.clone(), fit.clone(), ctx.toggles.clone());
        faded_button(
            cx,
            &ctx.anim,
            row()
                .size(22., 22.)
                .rounded(6.)
                .items_center()
                .justify_center()
                .cursor(Cursor::Pointer)
                .on_click(move || {
                    let on = !state.with_untracked(|f| *f);
                    state.set(on);
                    toggles.set(&fit_key, on);
                })
                .child(icon("wrap-text", 13., MUTED)),
            move || {
                if shade.get() {
                    (ink(0.09), ink(0.13))
                } else {
                    (Color(0, 0, 0, 0), ink(0.08))
                }
            },
        )
    };
    let copy_button = {
        let (glyph, label) = (copied.clone(), copied.clone());
        faded_button(
            cx,
            &ctx.anim,
            row()
                .h(22.)
                .px(6.)
                .gap(4.)
                .rounded(5.)
                .items_center()
                .cursor(Cursor::Pointer)
                .text_size(10.5)
                .text_color(MUTED)
                .on_click(copy)
                .child(icon_switch(12., MUTED, move || {
                    if glyph.get() { "check" } else { "copy" }
                }))
                // Only the copied state carries a label.
                .child(text("Copied").reactive_style(move || {
                    if label.get() {
                        Styles::new()
                    } else {
                        Styles::new().hidden()
                    }
                })),
            || (Color(0, 0, 0, 0), ink(0.08)),
        )
    };
    let label = lang
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    let header = row()
        .w_full()
        .h(CODE_HEADER)
        .shrink_0()
        .pl(12.)
        .pr(5.)
        .items_center()
        .justify_between()
        .bg(ink(0.02))
        .border_edges(zgx::scene::Insets {
            top: 0.,
            right: 0.,
            bottom: 1.,
            left: 0.,
        })
        .border_color(HAIRLINE)
        .child(text(label).min_w(0.).text_size(11.).text_color(MUTED))
        .child(
            row()
                .shrink_0()
                .gap(2.)
                .items_center()
                .child(fit_toggle)
                .child(copy_button),
        );
    let body = {
        let fit = fit.clone();
        switch(
            move || fit.get(),
            move |fit, cx| {
                if fit {
                    return column()
                        .w_full()
                        .min_w(0.)
                        .overflow_hidden()
                        .px(12.)
                        .py(CODE_PAD_Y)
                        .child(code(true).w_full().min_w(0.));
                }
                // Scroll views have a default height: size this one to its lines.
                let lines = this.clone();
                scroll_x(cx.state(0_f32))
                    .w_full()
                    .reactive_style(move || {
                        let count = match lines() {
                            Some(md::Block::Code { run, .. }) => code_lines(&run.text),
                            _ => 1,
                        };
                        Styles::new().h(count as f32 * CODE_LINE + 2. * CODE_PAD_Y)
                    })
                    .child(column().px(12.).py(CODE_PAD_Y).child(code(false)))
            },
        )
        .w_full()
    };
    column()
        .w_full()
        .min_w(0.)
        .rounded(10.)
        .bg(ink(0.035))
        .border(1.)
        .border_color(HAIRLINE)
        .overflow_hidden()
        .child(header)
        .child(body)
}

fn row_view() -> View {
    row()
}

/// Zeron's column sizing: each column's natural width is max(content, 48)
/// + 24; widths share the table in that proportion.
fn column_weights(header: &[md::Run], rows: &[Vec<md::Run>]) -> Vec<f32> {
    (0..header.len())
        .map(|i| {
            let longest = std::iter::once(header.get(i))
                .chain(rows.iter().map(|row| row.get(i)))
                .flatten()
                .map(|run| run.text.chars().count())
                .max()
                .unwrap_or(4);
            (longest as f32 * 7.5).clamp(48., 420.) + 24.
        })
        .collect()
}

fn table(
    cx: &mut Context,
    this: impl Fn() -> Option<md::Block> + Clone + 'static,
    spans_of: impl Fn(&md::Run, Color) -> Vec<TextSpan> + Clone + 'static,
) -> View {
    // Rebuilt when the column count or widths change as the table streams.
    let shape = {
        let this = this.clone();
        move || match this() {
            Some(md::Block::Table {
                aligns,
                header,
                rows,
            }) => Some((
                aligns,
                column_weights(&header, &rows)
                    .iter()
                    .map(|w| *w as u32)
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        }
    };
    let _ = cx;
    switch(shape, move |shape, _| {
        let Some((aligns, weights)) = shape else {
            return div();
        };
        let columns = weights.len();
        let row_count = {
            let this = this.clone();
            move || match this() {
                Some(md::Block::Table { rows, .. }) => rows.len(),
                _ => 0,
            }
        };
        let cell = {
            let (this, spans_of) = (this.clone(), spans_of.clone());
            move |row: Option<usize>, column: usize| {
                let (this, spans_of) = (this.clone(), spans_of.clone());
                let align = aligns.get(column).copied().unwrap_or(Align::Left);
                // Alignment by the cell's justification: the text keeps its
                // natural width inside it, wrapping only when too long.
                let text_ = rich_text_signal(move || {
                    let Some(md::Block::Table { header, rows, .. }) = this() else {
                        return Vec::new();
                    };
                    let run = match row {
                        None => header.get(column).cloned(),
                        Some(row) => rows.get(row).and_then(|r| r.get(column)).cloned(),
                    };
                    run.map_or_else(Vec::new, |run| {
                        spans_of(&run, if row.is_none() { TEXT } else { BODY })
                    })
                })
                .min_w(0.)
                .text_wrap(true)
                .text_size(14.)
                .line_height(22.)
                .when(row.is_none(), |c| c.font_weight(700));
                let cell = row_view()
                    .w(0.)
                    .flex_grow(weights.get(column).copied().unwrap_or(72) as f32)
                    .p(12.)
                    .child(text_);
                match align {
                    Align::Left => cell.justify_start(),
                    Align::Center => cell.justify_center(),
                    Align::Right => cell.justify_end(),
                }
            }
        };
        // Frameless, as in Zeron: only 1 px rules between rows.
        let rule = || zgx::scene::Insets {
            top: 0.,
            right: 0.,
            bottom: 1.,
            left: 0.,
        };
        let header_row = {
            let cell = cell.clone();
            row()
                .w_full()
                .border_edges(rule())
                .border_color(hairline(0.10))
                .children((0..columns).map(move |c| cell(None, c)))
        };
        let body = keyed(
            move || (0..row_count()).collect::<Vec<_>>(),
            move |r, _| {
                let cell = cell.clone();
                row()
                    .w_full()
                    .border_edges(rule())
                    .border_color(hairline(0.10))
                    .children((0..columns).map(move |c| cell(Some(r), c)))
            },
        )
        .w_full();
        column().w_full().child(header_row).child(body)
    })
    .w_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_follow_top_gap_for() {
        use RowKind::*;
        assert_eq!(top_gap(None, User), 0.);
        assert_eq!(top_gap(None, Text), 0.);
        // Turn starts: a user bubble, or an entry's first row.
        assert_eq!(top_gap(Some((Text, false)), User), 16.);
        assert_eq!(top_gap(Some((User, false)), Text), 16.);
        assert_eq!(top_gap(Some((User, false)), Group), 16.);
        // Within an entry: 12 around tool groups, 8 else.
        assert_eq!(top_gap(Some((Text, true)), Group), 12.);
        assert_eq!(top_gap(Some((Group, true)), Text), 12.);
        assert_eq!(top_gap(Some((Group, true)), Error), 12.);
        assert_eq!(top_gap(Some((Text, true)), Text), 8.);
        assert_eq!(top_gap(Some((Text, true)), Error), 8.);
    }

    #[test]
    fn timestamps_format_like_zeron() {
        // 2024-07-01T15:45:00Z.
        let ms = 1_719_848_700_000;
        assert_eq!(format_timestamp(ms, 0), "Jul 1, 3:45 PM");
        assert_eq!(format_timestamp(ms + 59_999, 0), "Jul 1, 3:45 PM");
        // UTC-7 is still the morning; UTC+9 is past midnight.
        assert_eq!(format_timestamp(ms, -7 * 3600), "Jul 1, 8:45 AM");
        assert_eq!(format_timestamp(ms, 9 * 3600), "Jul 2, 12:45 AM");
        // Noon and midnight read 12.
        assert_eq!(format_timestamp(1_704_110_400_000, 0), "Jan 1, 12:00 PM");
        assert_eq!(format_timestamp(1_709_164_805_000, 0), "Feb 29, 12:00 AM");
        assert_eq!(format_timestamp(0, 0), "Jan 1, 12:00 AM");
        assert_eq!(format_timestamp(-1, 0), "Dec 31, 11:59 PM");
    }

    #[test]
    fn user_bubbles_collapse_past_five_lines() {
        assert!(!user_needs_collapse("a\nb\nc\nd\ne", 0.));
        assert!(user_needs_collapse("a\nb\nc\nd\ne\nf", 0.));
        // Unmeasured: the 400 character proxy.
        assert!(!user_needs_collapse(&"x".repeat(400), 0.));
        assert!(user_needs_collapse(&"x".repeat(401), 0.));
        // Measured: the height decides.
        assert!(!user_needs_collapse(&"x".repeat(900), 110.));
        assert!(user_needs_collapse("short", 132.));
        // Source lines always fold, however short they lay out.
        assert!(user_needs_collapse("1\n2\n3\n4\n5\n6", 50.));
    }

    #[test]
    fn fold_resize_scales_with_travel() {
        assert_eq!(user_resize_ms(0.), 220.);
        assert_eq!(user_resize_ms(-40.), 220.);
        assert_eq!(user_resize_ms(100.), 252.);
        assert_eq!(user_resize_ms(2000.), 850.);
        assert_eq!(user_resize_spec(500.).1, super::super::theme::EASE_OUT);
        assert_eq!(user_resize_spec(501.).1, super::super::theme::EASE_IN_OUT);
        assert!((user_resize_spec(100.).0 - 0.252).abs() < 1e-6);
        let fold = Fold {
            from: 132.,
            to: 532.,
            at: 1.,
            spec: user_resize_spec(400.),
        };
        assert_eq!(fold.value(0.5), 132.);
        assert_eq!(fold.value(fold.end()), 532.);
    }

    #[test]
    fn code_bodies_drop_a_trailing_newline() {
        assert_eq!(code_lines(""), 1);
        assert_eq!(code_lines("a\nb"), 2);
        assert_eq!(code_lines("a\nb\n"), 2);
    }

    #[test]
    fn error_messages_are_one_line() {
        assert_eq!(
            single_line("  exit 1\n\tstderr:  boom "),
            "exit 1 stderr: boom"
        );
    }
}
