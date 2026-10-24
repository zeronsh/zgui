//! Zeron's left sidebar, after `crates/ui/src/shell.rs` (`render_chat_sidebar`,
//! `render_chat_row`, the titlebar cluster) and `shell/spaces.rs` (the
//! "Sessions" disclosure): the 38 px titlebar strip with the window
//! controls, compact 29 px session rows with
//! their status glyphs, harness marks and frosted monograms, the re-sort
//! glide, the "Star on GitHub" banner and the account footer.

use super::model::{self, ChatRow, Status};
use super::theme::{
    self, ACCENT, Anim, BG, BORDER, COLLAPSE, DANGER, FADE_QUICK, GLASS_HOVER, GLYPH, Hover, MONO,
    RESORT, TEXT, TEXT_FAINT, TEXT_MUTED, Tween, alpha, icon, icon_data, ink, mix, wash,
};
use super::{App, drag_region};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    f32::consts::FRAC_PI_2,
    rc::Rc,
    sync::Arc,
};
use zgx::{
    compose::prelude::*,
    input::InputEvent,
    reactive::Signal,
    scene::{Color, Insets},
};

// Metrics ----------------------------------------------------------------------

const SIDEBAR_WIDTH: f32 = 256.;
const TITLEBAR_HEIGHT: f32 = 38.;
/// Compact session rows (`sidebar_compact`).
const ROW_HEIGHT: f32 = 29.;
const LIST_GAP: f32 = 2.;
/// A row and its gap: the re-sort glide's step.
const ROW_PITCH: f32 = ROW_HEIGHT + LIST_GAP;
const DISCLOSURE_HEADER: f32 = 28.;
const DISCLOSURE_INSET: f32 = 4.;
const LIST_PAD_TOP: f32 = 4.;
const LIST_PAD_BOTTOM: f32 = 8.;
const FADE_BAND: f32 = 24.;
/// Titlebar cluster: buttons start at x 88, vertically centred on y 21.
const CLUSTER_X: f32 = 88.;
const CLUSTER_TOP: f32 = 4. + (TITLEBAR_HEIGHT - 4. - 24.) / 2.;
const GROUP_GAP: f32 = 8.;
const CONTROL_GAP: f32 = 2.;

/// `project_icon.rs` MONOGRAM_PALETTE, dark appearance.
const MONOGRAM_PALETTE: [u32; 8] = [
    0x94a3b8, 0x93c5fd, 0xc4b5fd, 0xfda4af, 0xfcd34d, 0x6ee7b7, 0x5eead4, 0xfdba74,
];

// Pure helpers -------------------------------------------------------------------

/// FNV-1a, 32 bit.
fn fnv1a(text: &str) -> u32 {
    text.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    })
}

/// A project's monogram tone, stable for its seed.
fn monogram_tone(seed: &str) -> Color {
    rgb(MONOGRAM_PALETTE[fnv1a(seed) as usize % MONOGRAM_PALETTE.len()])
}

/// `color` with its HSL lightness raised by `amount` (capped at .95), as
/// Zeron's monogram hover text.
fn lighten(color: Color, amount: f32) -> Color {
    let [r, g, b] = [color.0, color.1, color.2].map(|c| f32::from(c) / 255.);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.;
    let delta = max - min;
    let (hue, saturation) = if delta <= f32::EPSILON {
        (0., 0.)
    } else {
        let saturation = delta / (1. - (2. * lightness - 1.).abs());
        let hue = if max == r {
            ((g - b) / delta).rem_euclid(6.)
        } else if max == g {
            (b - r) / delta + 2.
        } else {
            (r - g) / delta + 4.
        };
        (hue / 6., saturation)
    };
    let lightness = (lightness + amount).min(0.95);
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let sector = hue * 6.;
    let x = chroma * (1. - (sector.rem_euclid(2.) - 1.).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.),
        1 => (x, chroma, 0.),
        2 => (0., chroma, x),
        3 => (0., x, chroma),
        4 => (x, 0., chroma),
        _ => (chroma, 0., x),
    };
    let m = lightness - chroma / 2.;
    let channel = |c: f32| ((c + m) * 255.).round().clamp(0., 255.) as u8;
    Color(channel(r), channel(g), channel(b), color.3)
}

/// Zeron's `format_time_ago` for an age in seconds.
fn time_ago(age: i64) -> String {
    let seconds = age.max(0);
    if seconds < 60 {
        return "now".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let weeks = days / 7;
    if weeks < 5 {
        return format!("{weeks}w");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo");
    }
    format!("{}y", days / 365)
}

/// The FLIP diff of `shell.rs` `resort_offsets` for rows of one pitch: each
/// surviving key whose position moved, mapped to `old_y - new_y`.
fn resort_offsets(old: &[String], new: &[String], pitch: f32) -> HashMap<String, f32> {
    let before: HashMap<&str, usize> = old
        .iter()
        .enumerate()
        .map(|(index, key)| (key.as_str(), index))
        .collect();
    new.iter()
        .enumerate()
        .filter_map(|(index, key)| {
            let dy = (*before.get(key.as_str())? as f32 - index as f32) * pitch;
            (dy.abs() > 0.5).then(|| (key.clone(), dy))
        })
        .collect()
}

/// Whether the key order changed (not merely heights): `sidebar_key_order_changed`.
fn key_order_changed(old: &[String], new: &[String]) -> bool {
    old.len() != new.len() || old.iter().zip(new).any(|(a, b)| a != b)
}

/// The disclosure body's full height for `rows` compact rows.
fn body_height(rows: usize) -> f32 {
    DISCLOSURE_INSET + rows as f32 * ROW_HEIGHT + rows.saturating_sub(1) as f32 * LIST_GAP
}

/// The monogram's letter: the name's first character, uppercased.
fn monogram_letter(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string()
}

/// A project's monogram seed. Zeron seeds with the space's path; rows carry
/// the chat's cwd (falling back to the space path), so use the shortest cwd
/// among the project's rows, which is the checkout root unless every chat
/// runs in a worktree.
fn project_seed(rows: &[ChatRow], project: &str) -> String {
    rows.iter()
        .filter(|row| row.space_id.is_some() && row.project == project)
        .find_map(|row| row.space_path.as_deref())
        .unwrap_or(project)
        .to_owned()
}

/// A row's project name and monogram seed; chats outside a project read as
/// Zeron's "Home".
fn row_identity(rows: &[ChatRow], row: &ChatRow) -> (String, String) {
    match row.space_id {
        Some(_) => (row.project.clone(), project_seed(rows, &row.project)),
        None => ("Home".into(), "home".into()),
    }
}

// Shared bits --------------------------------------------------------------------

/// Buttons inside the titlebar's drag strip must not start a window drag.
fn no_drag(view: View) -> View {
    view.on_event(|event| {
        if matches!(event.event, InputEvent::PointerDown { .. }) {
            event.stop_propagation();
        }
    })
}

/// A 13 / 16 px monogram tile with its own frosted backdrop
/// (`project_icon.rs` `monogram`): tone at .08 (.24 lit), the letter in the
/// tone at .85 (lightened when lit).
fn monogram(name: &str, seed: &str, size: f32, lit: Rc<dyn Fn() -> bool>) -> View {
    let tone = monogram_tone(seed);
    let bright = lighten(tone, 0.12);
    div()
        .size(size, size)
        .shrink_0()
        .rounded(3.)
        .blur(16.)
        .items_center()
        .justify_center()
        .font_family(MONO)
        .font_weight(500)
        .reactive_style(move || {
            if lit() {
                Styles::new().bg(alpha(tone, 0.24)).text_color(bright)
            } else {
                Styles::new()
                    .bg(alpha(tone, 0.08))
                    .text_color(alpha(tone, 0.85))
            }
        })
        .child(
            text(monogram_letter(name))
                .w_full()
                .text_align(TextAlign::Center)
                .text_size(9.)
                .line_height(13.),
        )
}

/// Zeron's sidebar toggle glyph (`icons::sidebar_glyph`), from quads: an
/// 18×16 frame and the panel, scaled from a 24-unit box.
fn sidebar_glyph(open: f32, size: f32, color: Color) -> View {
    let s = size / 24.;
    let stroke = 1.75;
    div()
        .size(size, size)
        .shrink_0()
        .child(
            div()
                .absolute()
                .left((3. - stroke / 2.) * s)
                .top((4. - stroke / 2.) * s)
                .size((18. + stroke) * s, (16. + stroke) * s)
                .rounded((4. + stroke / 2.) * s)
                .border(stroke * s)
                .border_color(color),
        )
        .child(
            div()
                .absolute()
                .left(6.5 * s)
                .top(7.5 * s)
                .size((1.75 + (5.5 - 1.75) * open.clamp(0., 1.)) * s, 9. * s)
                .rounded(0.875 * s)
                .bg(color),
        )
}

/// A titlebar window-control button: 24×24, radius 6, its wash fading in
/// over 150 ms.
fn window_button(
    cx: &mut Context,
    anim: &Anim,
    glyph: View,
    on_click: impl FnMut() + 'static,
) -> View {
    let hover = Hover::new(cx, anim);
    let blend = hover.clone();
    let button = div()
        .size(24., 24.)
        .shrink_0()
        .rounded(6.)
        .items_center()
        .justify_center()
        .cursor(Cursor::Pointer)
        .reactive_style(move || {
            Styles::new().bg(mix(alpha(GLASS_HOVER, 0.), GLASS_HOVER, blend.get()))
        })
        .on_click(on_click)
        .child(glyph);
    no_drag(hover.track(button))
}

/// A disabled history button: the icon at 35%, no hover.
fn disabled_button(name: &str) -> View {
    no_drag(
        div()
            .size(24., 24.)
            .shrink_0()
            .items_center()
            .justify_center()
            .child(icon(name, 16., alpha(TEXT_MUTED, 0.35))),
    )
}

// Sidebar ------------------------------------------------------------------------

/// The re-sort glide's current epoch: rows that moved and their start
/// offsets, and rows new in this order (they fade in).
#[derive(Clone, Default, PartialEq)]
struct Resort {
    at: f32,
    offsets: HashMap<String, f32>,
    fresh: HashSet<String>,
}

pub fn sidebar(cx: &mut Context, state: &App) -> View {
    let anim = state.anim.clone();
    let visible: Rc<dyn Fn() -> Vec<String>> = {
        let rows = state.rows.clone();
        Rc::new(move || rows.with(|rows| rows.iter().map(|row| row.id.clone()).collect()))
    };

    // Re-sort glide: diff each new key order against the last one.
    let resort = cx.state(Resort::default());
    {
        let (visible, resort, anim) = (visible.clone(), resort.clone(), anim.clone());
        let previous: RefCell<Vec<String>> = RefCell::default();
        let effect = cx.runtime().effect(move || {
            let keys = visible();
            let mut previous = previous.borrow_mut();
            if *previous == keys {
                return;
            }
            // First fill never animates.
            if !previous.is_empty() && key_order_changed(&previous, &keys) {
                let offsets = resort_offsets(&previous, &keys, ROW_PITCH);
                let before: HashSet<&String> = previous.iter().collect();
                let fresh: HashSet<String> = keys
                    .iter()
                    .filter(|key| !before.contains(key))
                    .cloned()
                    .collect();
                if !offsets.is_empty() || !fresh.is_empty() {
                    resort.set(Resort {
                        at: anim.now(),
                        offsets,
                        fresh,
                    });
                    anim.keep(RESORT.0.max(FADE_QUICK.0) + 0.05);
                }
            }
            *previous = keys;
        });
        cx.retain(effect);
    }

    let titlebar = titlebar_strip(cx, state);
    let list = session_list(cx, state, &visible, &resort);
    let banner = star_banner(cx);
    let footer = footer(cx, &anim);

    column()
        .w(SIDEBAR_WIDTH)
        .h_full()
        .shrink_0()
        .bg(wash(0.05))
        .border_edges(Insets {
            top: 0.,
            right: 1.,
            bottom: 0.,
            left: 0.,
        })
        .border_color(BORDER)
        // Zeron's hairline rides a tone layer under the column, taking no
        // room from it: the rows keep the full 256.
        .border_over(true)
        .child(titlebar)
        // With the project filter hidden, Zeron keeps a 4 px spacer.
        .child(div().w_full().h(4.).shrink_0())
        .child(list)
        .child(banner)
        .child(footer)
}

// Titlebar -----------------------------------------------------------------------

/// The 38 px strip over the sidebar: a window drag region carrying the
/// control cluster (toggle, back / forward, +) and the "…" view options.
fn titlebar_strip(cx: &mut Context, state: &App) -> View {
    let anim = state.anim.clone();
    let toggle = window_button(cx, &anim, sidebar_glyph(1., 16., TEXT_MUTED), || {});
    let plus = {
        let selected = state.selected.clone();
        window_button(cx, &anim, icon("plus", 16., TEXT_MUTED), || {}).reactive_style(move || {
            if selected.with(Option::is_some) {
                Styles::new()
            } else {
                Styles::new().hidden()
            }
        })
    };
    // The view options snap their wash (no fade).
    let options = no_drag(
        div()
            .size(24., 24.)
            .rounded(6.)
            .items_center()
            .justify_center()
            .cursor(Cursor::Pointer)
            .hover(|s| s.bg(GLASS_HOVER))
            .on_click(|| {})
            .child(icon("more-horizontal", 16., TEXT_MUTED)),
    );
    let back_x = CLUSTER_X + 24. + GROUP_GAP;
    let forward_x = back_x + 24. + CONTROL_GAP;
    let plus_x = forward_x + 24. + GROUP_GAP;
    drag_region(
        div()
            .w_full()
            .h(TITLEBAR_HEIGHT)
            .shrink_0()
            .child(toggle.absolute().left(CLUSTER_X).top(CLUSTER_TOP))
            .child(
                disabled_button("arrow-left")
                    .absolute()
                    .left(back_x)
                    .top(CLUSTER_TOP),
            )
            .child(
                disabled_button("arrow-right")
                    .absolute()
                    .left(forward_x)
                    .top(CLUSTER_TOP),
            )
            .child(plus.absolute().left(plus_x).top(CLUSTER_TOP))
            .child(
                options
                    .absolute()
                    .left(SIDEBAR_WIDTH - 8. - 24.)
                    .top(CLUSTER_TOP),
            ),
    )
}

// Session list -------------------------------------------------------------------

fn session_list(
    cx: &mut Context,
    state: &App,
    visible: &Rc<dyn Fn() -> Vec<String>>,
    resort: &Signal<Resort>,
) -> View {
    let anim = state.anim.clone();
    let count = {
        let visible = visible.clone();
        move || visible().len()
    };
    // The disclosure's reveal, 0 closed .. 1 open.
    let disclosure = cx.state(Tween::settled(1.));
    let reveal = {
        let (disclosure, anim) = (disclosure.clone(), anim.clone());
        move || {
            let tween = disclosure.get();
            let end = tween.end(COLLAPSE);
            let now = anim.at(end);
            (tween.value(now, COLLAPSE), now < end, tween.target)
        }
    };
    let toggle = {
        let (disclosure, anim) = (disclosure.clone(), anim.clone());
        move || {
            let target = if disclosure.with_untracked(|t| t.target) > 0.5 {
                0.
            } else {
                1.
            };
            anim.retarget(&disclosure, target, COLLAPSE);
        }
    };
    let badge = {
        let (count, disclosure) = (count.clone(), disclosure.clone());
        row()
            .h(16.)
            .px(4.)
            .shrink_0()
            .items_center()
            .rounded(4.)
            .bg(ink(0.06))
            .reactive_style(move || {
                if disclosure.with(|t| t.target) < 0.5 {
                    Styles::new()
                } else {
                    Styles::new().hidden()
                }
            })
            .child(
                text_signal(move || count().to_string())
                    .font_family(MONO)
                    .text_size(10.)
                    .font_weight(500)
                    .text_color(alpha(TEXT_MUTED, 0.7)),
            )
    };
    let chevron = {
        let reveal = reveal.clone();
        let tint = alpha(TEXT_MUTED, 0.5);
        svg_signal("disclosure", move || {
            let (value, _, _) = reveal();
            Arc::new(
                icon_data("alt-arrow-right")
                    .tinted(tint)
                    .transformed(Affine::rotation(value * FRAC_PI_2)),
            )
        })
        .size(12., 12.)
        .shrink_0()
    };
    let header = row()
        .w_full()
        .h(DISCLOSURE_HEADER)
        .shrink_0()
        .px(8.)
        .gap(8.)
        .items_center()
        .cursor(Cursor::Pointer)
        .on_click(toggle)
        .child(
            text("Sessions")
                .text_size(12.)
                .font_weight(500)
                .text_color(alpha(TEXT_MUTED, 0.5)),
        )
        .child(badge)
        .child(div().grow())
        .child(chevron);
    let rows = {
        let (visible, state, resort) = (visible.clone(), state.clone(), resort.clone());
        keyed(
            move || visible(),
            move |id, cx| session_row(cx, &state, &resort, id),
        )
        .w_full()
        .gap(LIST_GAP)
    };
    let body = {
        let (reveal, count) = (reveal.clone(), count.clone());
        column()
            .w_full()
            .overflow_hidden()
            .pt(DISCLOSURE_INSET)
            .child(rows)
            .reactive_style(move || {
                let (value, moving, target) = reveal();
                if moving {
                    Styles::new()
                        .h(value * body_height(count()))
                        .opacity(0.35 + 0.65 * value)
                        .translate(0., -3. * (1. - value))
                } else if target < 0.5 {
                    Styles::new().h(0.).opacity(0.35)
                } else {
                    Styles::new()
                }
            })
    };
    let empty = {
        let count = count.clone();
        text("No sessions yet")
            .px(8.)
            .text_size(12.)
            .text_color(TEXT_FAINT)
            .reactive_style(move || {
                if count() == 0 {
                    Styles::new()
                } else {
                    Styles::new().hidden()
                }
            })
    };
    let section = {
        let count = count.clone();
        column()
            .w_full()
            .child(header)
            .child(body)
            .reactive_style(move || {
                if count() == 0 {
                    Styles::new().hidden()
                } else {
                    Styles::new()
                }
            })
    };

    // Edge fades only at edges with hidden overflow, as Zeron's EdgeFade:
    // the content's height is known (fixed rows), the viewport's is probed.
    let offset = cx.state(0_f32);
    let viewport = cx.state(0_f32);
    let probe = {
        let (viewport, tasks) = (viewport.clone(), cx.tasks());
        let seen = Rc::new(Cell::new(-1_f32));
        canvas(move |(_, height)| {
            if (seen.get() - height).abs() > 0.5 {
                seen.set(height);
                let viewport = viewport.clone();
                tasks.spawn(async move {
                    viewport.set(height);
                });
            }
            Canvas::new()
        })
        .absolute()
        .left(0.)
        .right(0.)
        .top(0.)
        .bottom(0.)
    };
    let scroller = {
        let (offset_now, reveal, count) = (offset.clone(), reveal.clone(), count.clone());
        scroll(offset)
            .w_full()
            .grow()
            .flex_shrink(1.)
            .min_h(0.)
            .reactive_style(move || {
                let n = count();
                let content = LIST_PAD_TOP
                    + LIST_PAD_BOTTOM
                    + if n == 0 {
                        16.
                    } else {
                        DISCLOSURE_HEADER + reveal().0 * body_height(n)
                    };
                let off = offset_now.get();
                let view = viewport.get();
                let top = if off > 0.5 { FADE_BAND } else { 0. };
                let bottom = if off + view < content - 0.5 {
                    FADE_BAND
                } else {
                    0.
                };
                Styles::new().fade_edges(top, bottom)
            })
            .child(
                column()
                    .w_full()
                    .px(8.)
                    .pt(LIST_PAD_TOP)
                    .pb(LIST_PAD_BOTTOM)
                    .child(empty)
                    .child(section),
            )
    };
    column()
        .w_full()
        .grow()
        .flex_shrink(1.)
        .min_h(0.)
        .child(probe)
        .child(scroller)
}

/// A compact session row (`render_chat_row`, compact): status slot, harness
/// mark, project monogram, title and the time / archive slot.
fn session_row(cx: &mut Context, state: &App, resort: &Signal<Resort>, id: String) -> View {
    let anim = state.anim.clone();
    let hover = Hover::new(cx, &anim);
    let subline = alpha(TEXT_MUTED, 0.5);
    let row_of = {
        let (rows, id) = (state.rows.clone(), id.clone());
        move || rows.with(|rows| rows.iter().find(|r| r.id == id).cloned())
    };
    let selected: Rc<dyn Fn() -> bool> = {
        let (selected, id) = (state.selected.clone(), id.clone());
        Rc::new(move || selected.with(|s| s.as_deref() == Some(id.as_str())))
    };

    let harness = {
        let row_of = row_of.clone();
        svg_signal("harness", move || {
            let harness = row_of().map(|r| r.harness()).unwrap_or_default();
            let (name, tint) = theme::harness_mark(&harness);
            Arc::new(icon_data(name).tinted(alpha(tint.unwrap_or(subline), 0.8)))
        })
        .size(13., 13.)
        .shrink_0()
    };
    let tile = {
        let (rows, id) = (state.rows.clone(), id.clone());
        let lit: Rc<dyn Fn() -> bool> = {
            let (selected, hover) = (selected.clone(), hover.clone());
            Rc::new(move || selected() || hover.on())
        };
        switch(
            move || {
                rows.with(|rows| {
                    rows.iter()
                        .find(|r| r.id == id)
                        .map(|row| row_identity(rows, row))
                        .unwrap_or_default()
                })
            },
            move |(name, seed), _| monogram(&name, &seed, 13., lit.clone()),
        )
        .size(13., 13.)
        .shrink_0()
    };
    let title = {
        let row_of = row_of.clone();
        text_signal(move || {
            row_of()
                .map(|r| r.title)
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| "New session".into())
        })
        .w(0.)
        .grow()
        .truncate()
    };
    let trailing = {
        let time = {
            let (row_of, now, hover) = (row_of.clone(), state.now.clone(), hover.clone());
            let shown = hover.clone();
            text_signal(move || {
                now.get();
                row_of()
                    .map(|r| time_ago(model::now_seconds() - r.updated))
                    .unwrap_or_default()
            })
            .text_size(11.)
            .line_height(14.)
            .text_color(subline)
            .reactive_style(move || {
                if shown.on() {
                    Styles::new().hidden()
                } else {
                    Styles::new()
                }
            })
        };
        let archive = {
            let shown = hover.clone();
            icon("archive-minimalistic", 13., TEXT_MUTED)
                .shrink_0()
                .cursor(Cursor::Pointer)
                .on_click(|| {})
                .reactive_style(move || {
                    if shown.on() {
                        Styles::new()
                    } else {
                        Styles::new().hidden()
                    }
                })
        };
        row()
            .min_w(30.)
            .h(14.)
            .shrink_0()
            .items_center()
            .justify_end()
            .child(time)
            .child(archive)
    };

    let style = {
        let (hover, selected, resort, anim, key) = (
            hover.clone(),
            selected.clone(),
            resort.clone(),
            anim.clone(),
            id.clone(),
        );
        move || {
            let mut style = if selected() {
                Styles::new().bg(wash(0.11)).text_color(TEXT)
            } else {
                let h = hover.get();
                Styles::new()
                    .bg(mix(alpha(GLASS_HOVER, 0.), GLASS_HOVER, h))
                    .text_color(mix(alpha(TEXT, 0.8), TEXT, h))
            };
            let (dy, fresh, at) =
                resort.with(|r| (r.offsets.get(&key).copied(), r.fresh.contains(&key), r.at));
            if let Some(dy) = dy {
                let now = anim.at(at + RESORT.0);
                style = style.translate(0., dy * (1. - RESORT.at(now - at)));
            }
            if fresh {
                let now = anim.at(at + FADE_QUICK.0);
                style = style.opacity(FADE_QUICK.at(now - at));
            }
            style
        }
    };
    let select = state.selected.clone();
    hover.track(
        row()
            .w_full()
            .h(ROW_HEIGHT)
            .shrink_0()
            .px(8.)
            .py(6.)
            .gap(4.)
            .items_center()
            .rounded(8.)
            .text_size(13.)
            .line_height(17.)
            .cursor(Cursor::Pointer)
            .reactive_style(style)
            .on_click(move || {
                select.set(Some(id.clone()));
            })
            .child(status_slot(state, row_of))
            .child(harness)
            .child(tile)
            .child(title)
            .child(trailing),
    )
}

/// The 13×13 status slot: the mini spinner while working, else a 6 px dot.
fn status_slot(state: &App, row_of: impl Fn() -> Option<ChatRow> + 'static) -> View {
    let (statuses, anim) = (state.statuses.clone(), state.anim.clone());
    switch(
        move || {
            let id = row_of().map(|r| r.id).unwrap_or_default();
            statuses.with(|s| s.get(&id).copied().unwrap_or(Status::Idle))
        },
        move |status, cx| {
            let glyph = match status {
                Status::Working => {
                    // Ambient frames only while a spinner is mounted.
                    cx.retain(anim.ambient());
                    mini_spinner(&anim)
                }
                Status::AwaitingInput => dot(alpha(ACCENT, 0.6)),
                Status::Errored => dot(alpha(DANGER, 0.65)),
                Status::Idle => dot(ink(0.14)),
            };
            div()
                .size(13., 13.)
                .items_center()
                .justify_center()
                .child(glyph)
        },
    )
    .size(13., 13.)
    .shrink_0()
}

fn dot(color: Color) -> View {
    div().size(6., 6.).rounded(3.).bg(color)
}

/// Zeron's mini glyph spinner (`loaders.rs`): 2×3 round 2 px cells, 1 px
/// apart, rows tinted light / mid / deep, a snake chasing round the ring.
fn mini_spinner(anim: &Anim) -> View {
    /// Clockwise ring index of each (row, col), top-left first.
    const RING: [[usize; 2]; 3] = [[0, 1], [5, 2], [4, 3]];
    let mut rows = Vec::new();
    for (r, ring) in RING.iter().enumerate() {
        let mut cells = Vec::new();
        for &index in ring {
            let anim = anim.clone();
            let phase = index as f32 / 6.;
            cells.push(
                div()
                    .size(2., 2.)
                    .rounded(1.)
                    .bg(GLYPH[r])
                    .reactive_style(move || {
                        let t = anim.ambient_clock.get();
                        Styles::new().opacity(theme::gspin(t / 0.75 + phase, 0.1))
                    }),
            );
        }
        rows.push(row().gap(1.).children(cells));
    }
    column().gap(1.).shrink_0().children(rows)
}

// Banner and footer ----------------------------------------------------------------

/// "Star on GitHub", until dismissed.
fn star_banner(cx: &mut Context) -> View {
    let dismissed = cx.state(false);
    let shown = dismissed.clone();
    switch(
        move || shown.get(),
        move |gone, _| {
            if gone {
                return div();
            }
            let (close, visit) = (dismissed.clone(), dismissed.clone());
            let dismiss = div()
                .size(18., 18.)
                .shrink_0()
                .rounded(4.)
                .items_center()
                .justify_center()
                .cursor(Cursor::Pointer)
                .hover(|s| s.bg(alpha(ACCENT, 0.22)))
                .on_click(move || {
                    close.set(true);
                })
                .child(icon("close", 10., ACCENT));
            row()
                .mx(8.)
                .pl(8.)
                .pr(4.)
                .py(4.)
                .gap(6.)
                .items_center()
                .rounded(6.)
                .bg(alpha(ACCENT, 0.22))
                .hover(|s| s.bg(alpha(ACCENT, 0.16)))
                .text_size(11.)
                .font_weight(500)
                .text_color(ACCENT)
                .cursor(Cursor::Pointer)
                .on_click(move || {
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open")
                        .arg("https://github.com/zeronsh/comet")
                        .spawn();
                    visit.set(true);
                })
                .child(icon("star-bold", 12., ACCENT).shrink_0().translate(0., 0.5))
                .child(text("Star on GitHub").w(0.).grow().truncate())
                .child(dismiss)
        },
    )
    .w_full()
    .shrink_0()
}

/// The account row: the "Local" account trigger, then Pull requests and
/// Settings.
fn footer(cx: &mut Context, anim: &Anim) -> View {
    let hover = Hover::new(cx, anim);
    let account = {
        let blend = hover.clone();
        let avatar = div()
            .size(16., 16.)
            .shrink_0()
            .rounded(8.)
            .bg(TEXT)
            .items_center()
            .justify_center()
            .child(
                text("L")
                    .w_full()
                    .text_align(TextAlign::Center)
                    .font_family(MONO)
                    .text_size(10.)
                    .line_height(16.)
                    .font_weight(600)
                    .text_color(BG),
            );
        hover.track(
            row()
                .h(28.)
                .w(0.)
                .grow()
                .min_w(0.)
                .px(8.)
                .gap(8.)
                .items_center()
                .rounded(8.)
                .text_size(13.)
                .font_weight(500)
                .cursor(Cursor::Pointer)
                .reactive_style(move || {
                    let h = blend.get();
                    Styles::new()
                        .bg(mix(alpha(GLASS_HOVER, 0.), alpha(GLASS_HOVER, 0.8), h))
                        .text_color(mix(alpha(TEXT, 0.8), TEXT, h))
                })
                .on_click(|| {})
                .child(avatar)
                .child(text("Local").line_height(17.).w(0.).grow().truncate()),
        )
    };
    let pulls = footer_button(cx, anim, "pull-request");
    let settings = footer_button(cx, anim, "settings");
    column().w_full().shrink_0().p(8.).child(
        row()
            .w_full()
            .h(28.)
            .gap(4.)
            .items_center()
            .justify_between()
            .child(account)
            .child(row().gap(2.).shrink_0().child(pulls).child(settings)),
    )
}

/// A 28×28 footer action: its icon fades muted → text, its wash fades in.
fn footer_button(cx: &mut Context, anim: &Anim, name: &'static str) -> View {
    let hover = Hover::new(cx, anim);
    let (tint, wash_in) = (hover.clone(), hover.clone());
    let glyph = svg_signal(name, move || {
        Arc::new(icon_data(name).tinted(mix(TEXT_MUTED, TEXT, tint.get())))
    })
    .size(15., 15.);
    hover.track(
        div()
            .size(28., 28.)
            .shrink_0()
            .rounded(8.)
            .items_center()
            .justify_center()
            .cursor(Cursor::Pointer)
            .reactive_style(move || {
                Styles::new().bg(mix(alpha(GLASS_HOVER, 0.), GLASS_HOVER, wash_in.get()))
            })
            .on_click(|| {})
            .child(glyph),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_reference_vectors() {
        assert_eq!(fnv1a(""), 0x811c_9dc5);
        assert_eq!(fnv1a("a"), 0xe40c_292c);
        assert_eq!(fnv1a("foobar"), 0xbf9c_f968);
    }

    #[test]
    fn monogram_tone_is_palette_entry_by_hash() {
        for seed in ["home", "/Users/w/zeron", "/tmp/x", ""] {
            let expected = rgb(MONOGRAM_PALETTE[fnv1a(seed) as usize % 8]);
            assert_eq!(monogram_tone(seed), expected);
        }
        // "a" hashes to 0xe40c292c, which is 4 mod 8: amber.
        assert_eq!(monogram_tone("a"), rgb(0xfcd34d));
    }

    #[test]
    fn lighten_raises_hsl_lightness() {
        // slate 0x94a3b8: hsl(215, 20%, 65%) → 77%.
        let light = lighten(rgb(0x94a3b8), 0.12);
        assert!((i32::from(light.0) - 185).abs() <= 1, "{light:?}");
        assert!((i32::from(light.1) - 195).abs() <= 1, "{light:?}");
        assert!((i32::from(light.2) - 208).abs() <= 1, "{light:?}");
        // Grey stays grey; lightness caps at .95.
        assert_eq!(lighten(Color(128, 128, 128, 255), 0.12).0, 159);
        assert_eq!(
            lighten(Color(250, 250, 250, 7), 0.12),
            Color(242, 242, 242, 7)
        );
    }

    #[test]
    fn time_ago_matches_zeron_format() {
        assert_eq!(time_ago(-7200), "now");
        assert_eq!(time_ago(0), "now");
        assert_eq!(time_ago(59), "now");
        assert_eq!(time_ago(60), "1m");
        assert_eq!(time_ago(59 * 60), "59m");
        assert_eq!(time_ago(3600), "1h");
        assert_eq!(time_ago(23 * 3600 + 3599), "23h");
        assert_eq!(time_ago(24 * 3600), "1d");
        assert_eq!(time_ago(6 * 86_400), "6d");
        assert_eq!(time_ago(7 * 86_400), "1w");
        assert_eq!(time_ago(30 * 86_400), "4w");
        assert_eq!(time_ago(35 * 86_400), "1mo");
        assert_eq!(time_ago(400 * 86_400), "1y");
    }

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_owned()).collect()
    }

    #[test]
    fn resort_offsets_glide_from_old_rows() {
        // "c" jumps to the top: it starts two rows down, the others one up.
        let offsets = resort_offsets(&keys(&["a", "b", "c"]), &keys(&["c", "a", "b"]), ROW_PITCH);
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets["c"], 2. * ROW_PITCH);
        assert_eq!(offsets["a"], -ROW_PITCH);
        assert_eq!(offsets["b"], -ROW_PITCH);
        // Unmoved rows don't glide; new rows have no offset.
        let offsets = resort_offsets(&keys(&["a", "b"]), &keys(&["a", "b", "n"]), ROW_PITCH);
        assert!(offsets.is_empty());
        let offsets = resort_offsets(&keys(&["a", "b"]), &keys(&["n", "a", "b"]), ROW_PITCH);
        assert_eq!(offsets["a"], -ROW_PITCH);
        assert!(!offsets.contains_key("n"));
        // Removal: survivors below slide up from their old place.
        let offsets = resort_offsets(&keys(&["a", "b", "c"]), &keys(&["a", "c"]), ROW_PITCH);
        assert_eq!(offsets["c"], ROW_PITCH);
    }

    #[test]
    fn key_order_change_ignores_identical_orders() {
        assert!(!key_order_changed(&keys(&["a", "b"]), &keys(&["a", "b"])));
        assert!(key_order_changed(&keys(&["a", "b"]), &keys(&["b", "a"])));
        assert!(key_order_changed(&keys(&["a"]), &keys(&["a", "b"])));
    }

    #[test]
    fn body_height_counts_inset_rows_and_gaps() {
        assert_eq!(body_height(0), 4.);
        assert_eq!(body_height(1), 4. + 29.);
        assert_eq!(body_height(3), 4. + 3. * 29. + 2. * 2.);
    }

    #[test]
    fn monogram_letter_uppercases_first_char() {
        assert_eq!(monogram_letter("  zeron"), "Z");
        assert_eq!(monogram_letter("ärger"), "Ä");
        assert_eq!(monogram_letter(""), "?");
    }
}
