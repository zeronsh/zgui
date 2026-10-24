//! Zeron's docked composer: the frosted pill (compact single row or the
//! expanded two-row layout, morphing between them), its text input, attach
//! and send / stop buttons, the combined model + effort chip with its frosted
//! picker card, and the checkout footer under it.
//! Follows Zeron's `crates/ui/src/composer.rs`, `pickers.rs`,
//! `pickers/compact.rs`, `popover.rs` and `frost.rs`.

use super::model::ChatRow;
use super::theme::{
    self, ACCENT, Anim, BG, BORDER, COLLAPSE, COMPOSER_BORDER, DANGER, EASE_OUT_QUINT, GLASS_HOVER,
    Hover, MENU_IN, MENU_OUT, RESIZE, SURFACE_TINT, Spec, TEXT, TEXT_FAINT, TEXT_MUTED, Tween,
    alpha, hairline, ink, mix, wash,
};
use super::{App, Models};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    f32::consts::{FRAC_PI_2, PI, TAU},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use zgx::{
    compose::{Tasks, prelude::*},
    input::{EventContext, EventPhase, InputEvent, Key, PointerButton},
    reactive::{Runtime, Signal},
    rich_text::{RichText, TextMeasure, TextRun},
    scene::{BoxShadow, Color, Insets, Transform},
    text_layout::{FontStyle, LineHeight},
    timer::sleep,
};

// Metrics (composer.rs constants) ------------------------------------------------

/// The docked pill's corner radius (26 less the dock's 4).
pub const RADIUS: f32 = 22.;
/// Compact pill, border box: a 47 px row plus the two hairlines.
pub const COMPACT_TOTAL_HEIGHT: f32 = 49.;
const COMPACT_ROW: f32 = 47.;
const PILL_BORDER_V: f32 = 2.;
/// Input text: Geist 14 at `leading-relaxed`.
pub const INPUT_TEXT_SIZE: f32 = 14.;
pub const INPUT_LINE_HEIGHT: f32 = 22.75;
/// Expanded textarea box: `pt-4 pb-1`, clamped 60..260 when docked.
const TEXTAREA_PAD_V: f32 = 20.;
const TEXTAREA_MIN: f32 = 60.;
const TEXTAREA_MAX: f32 = 260.;
/// Expanded actions row: 2 + 32 + 8.
const ACTIONS_ROW: f32 = 42.;
const ACTIONS_BOTTOM_PAD: f32 = 8.;
/// Below this compact input width the composer always expands.
const MIN_COMPACT_INPUT_WIDTH: f32 = 200.;
/// Expanded collapses only once the text is this much under capacity.
const COLLAPSE_HYSTERESIS: f32 = 32.;
/// Overflow fade band at the input's top and bottom edges.
const INPUT_FADE_BAND: f32 = 12.;
/// Compact row: attach (8 + 28), input padding (8 + 8), send group (8 + 28 + 8).
const COMPACT_FIXED: f32 = 8. + 28. + 16. + 8. + 28. + 8.;
/// Send / attach centre: 25 px above the expanded bottom, 24.5 in compact.
const CLUSTER_Y_DELTA: f32 =
    ACTIONS_BOTTOM_PAD + 16. + PILL_BORDER_V / 2. - COMPACT_TOTAL_HEIGHT / 2.;
/// The model chip: 248 px at most, 45% of the pill's width.
const CHIP_MAX: f32 = 248.;
const CHIP_SHARE: f32 = 0.45;
/// The compact model card (`pickers/compact.rs`, `popover.rs`).
const CARD_WIDTH: f32 = 256.;
const CARD_RADIUS: f32 = 12.;
const CARD_GAP: f32 = 6.;
const CARD_INSET: f32 = 4.;
const MENU_GAP: f32 = 2.;
/// Concentric with the card: inside its 1 px border and 4 px inset.
const MENU_ITEM_RADIUS: f32 = CARD_RADIUS - 1. - CARD_INSET;
const MENU_TRAVEL: f32 = 4.;
const MENU_BLUR: f32 = 16.;
/// `menu_geometry`: the room above the trigger less 14, at most 640.
const MENU_MARGIN: f32 = 14.;
const MENU_MAX_HEIGHT: f32 = 640.;
/// One-line model rows plus the 2 px gap; about seven show at once.
const COMPACT_ROW_HEIGHT: f32 = 32.;
const COMPACT_LIST_ROWS: f32 = 7.;
/// A 28 px rail under a 44x32 pill thumb overhanging it by 2 px.
const SLIDER_HEIGHT: f32 = 36.;
const RAIL_HEIGHT: f32 = 28.;
const RAIL_TOP: f32 = (SLIDER_HEIGHT - RAIL_HEIGHT) / 2.;
const THUMB_WIDTH: f32 = 44.;
const THUMB_HEIGHT: f32 = 32.;
const THUMB_INSET: f32 = THUMB_WIDTH / 2.;
const FILL_PAST: f32 = RAIL_HEIGHT / 2.;
/// The header's two-line title; 36 with no effort ladder.
const HEADER_HEIGHT: f32 = 48.;
const HEADER_HEIGHT_SINGLE: f32 = 36.;
const FAST_BUTTON_WIDTH: f32 = 32.;
const SLIDER_TOP: f32 = 2.;
const SLIDER_BOTTOM: f32 = 4.;
const EFFORT_BLOCK: f32 = SLIDER_TOP + SLIDER_HEIGHT + SLIDER_BOTTOM;
const OPTIONS_GAP: f32 = 4.;
const CARD_BORDERS: f32 = 2.;
/// The model list's back + provider strip (40) and search row (40).
const LIST_HEADER: f32 = 80.;
/// Option rows (26 plus a 2 gap) all show up to this many, then scroll.
const OPTIONS_MAX_ROWS: usize = 6;
const OPTION_ROW: f32 = 26.;
/// The nested option menu beside its row.
const SUBMENU_WIDTH: f32 = 232.;
/// Option ids Cursor uses for an effort ladder.
const EFFORT_OPTION_IDS: [&str; 3] = ["effort", "reasoning", "reasoning_effort"];
/// Scritto's roll: 550 ms per glyph, staggered over 30% more.
const ROLL: Spec = Spec(0.55, EASE_OUT_QUINT);
const ROLL_STAGGER: f32 = 0.3;
const ROLL_TRAVEL_EM: f32 = 0.35;
/// The footer slot: 24 tall, pulled 8 into the container's bottom padding.
pub const FOOTER_HEIGHT: f32 = 24.;
/// The composer container: `max-w 768`, `px 16`, `pb 16`, gap 8.
pub const CONTAINER_MAX_WIDTH: f32 = 768.;
const CONTAINER_PAD: f32 = 16.;
const CONTAINER_GAP: f32 = 8.;

// Pure layout --------------------------------------------------------------------

/// Compact <-> expanded with hysteresis (`composer_flip`): a newline or a too
/// narrow pill always expands; compact expands once the text is wider than
/// `capacity`; expanded collapses only when it is `COLLAPSE_HYSTERESIS`
/// narrower than that.
pub fn composer_flip(expanded: bool, text_width: f32, capacity: f32, has_newline: bool) -> bool {
    if has_newline || capacity < MIN_COMPACT_INPUT_WIDTH {
        return true;
    }
    if expanded {
        text_width >= capacity - COLLAPSE_HYSTERESIS
    } else {
        text_width > capacity
    }
}

/// The compact input's text capacity for a pill `inner` wide (inside its
/// hairlines) and a model chip `chip` wide: the input width less 8. Unknown
/// before the first measure, when the composer stays compact.
pub fn compact_capacity(inner: f32, chip: f32) -> f32 {
    if inner <= 0. {
        return f32::MAX;
    }
    let chip = chip.min(CHIP_SHARE * (inner + PILL_BORDER_V));
    inner - COMPACT_FIXED - chip - 8.
}

/// Docked expanded height for a content height: the textarea box clamps to
/// 60..260, then the actions row and the hairlines (104..304).
pub fn expanded_height(content: f32) -> f32 {
    (content + TEXTAREA_PAD_V).clamp(TEXTAREA_MIN, TEXTAREA_MAX) + ACTIONS_ROW + PILL_BORDER_V
}

/// The pill's resting height in a mode.
pub fn pill_target(expanded: bool, content: f32) -> f32 {
    if expanded {
        expanded_height(content)
    } else {
        COMPACT_TOTAL_HEIGHT
    }
}

/// The whole dock (`dock`) for a pill height: the gap, the 24 px footer slot
/// pulled 8 into the 16 px bottom padding.
#[allow(dead_code)]
pub fn stack_height(pill: f32) -> f32 {
    pill + CONTAINER_GAP + FOOTER_HEIGHT - 8. + CONTAINER_PAD
}

/// Whether the settled expanded input scrolls (its box hit the 260 cap).
fn input_overflows(expanded: bool, content: f32) -> bool {
    expanded && content + TEXTAREA_PAD_V > TEXTAREA_MAX + 0.5
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Expanded text top padding across a flip: 12 (compact) to 16.
fn morph_text_pad(progress: f32) -> f32 {
    lerp(12., 16., progress)
}

/// The shared outer inset of attach and send: compact 8, expanded 12.
fn morph_cluster_inset(expanded: bool, progress: f32) -> f32 {
    if expanded {
        lerp(8., 12., progress)
    } else {
        lerp(12., 8., progress)
    }
}

/// The controls' 0.5 px centring delta, decaying over a flip.
fn morph_cluster_dy(progress: f32) -> f32 {
    CLUSTER_Y_DELTA * (1. - progress)
}

/// Collapse glide: the compact text starts where the expanded text sat (17
/// below the old top) and walks down to its resting row.
fn collapse_text_glide(from: f32, progress: f32) -> f32 {
    (from - 53.).max(0.) * (1. - progress)
}

/// The model chip's width: padding, mark, label, the effort suffix, then
/// the 13 px fast mark.
pub fn chip_width(label: f32, suffix: Option<f32>, fast: bool) -> f32 {
    let mut width = 2. * 6. + 16. + 6. + label.ceil();
    if let Some(suffix) = suffix {
        width += 6. + suffix.ceil();
    }
    if fast {
        width += 6. + 13.;
    }
    width.min(CHIP_MAX)
}

/// Zeron's `reasoning_label` for a wire level.
pub fn effort_label(level: &str) -> String {
    match level {
        "minimal" => "Minimal".into(),
        "low" => "Low".into(),
        "medium" => "Medium".into(),
        "high" => "High".into(),
        "xhigh" | "x-high" | "x_high" => "X-High".into(),
        "max" => "Max".into(),
        "ultra" => "Ultra".into(),
        "ultracode" => "Ultracode".into(),
        "ultrathink" => "Ultrathink".into(),
        other => title_case(other),
    }
}

/// `clamp_reasoning`: an offered pick stands; anything else heals to High,
/// then Medium, then the ladder's first. No ladder (not loaded): as picked.
fn effective_effort(explicit: Option<String>, ladder: &[String]) -> Option<String> {
    if ladder.is_empty() {
        return explicit;
    }
    match explicit {
        Some(level) if ladder.contains(&level) => Some(level),
        _ => ["high", "medium"]
            .into_iter()
            .find(|l| ladder.iter().any(|x| x == l))
            .map(str::to_owned)
            .or_else(|| ladder.first().cloned()),
    }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

// Text measurement ------------------------------------------------------------------

fn measure(
    measure: &TextMeasure,
    text: &str,
    size: f32,
    weight: u16,
    line: f32,
    wrap: Option<f32>,
) -> (f32, f32) {
    if text.is_empty() {
        return (0., line);
    }
    let run = TextRun {
        range: 0..text.len(),
        font: FontStyle {
            family: FontFamily::from(theme::SANS),
            weight,
            line_height: LineHeight::px(line),
            ..FontStyle::default()
        },
        font_size: size,
        ..Default::default()
    };
    RichText::new(text, vec![run]).map_or((0., line), |rich| measure.measure(&rich, wrap))
}

/// The draft's height wrapped at `width`, line by line so a trailing
/// newline counts.
fn content_height(m: &TextMeasure, draft: &str, width: f32) -> f32 {
    draft
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                INPUT_LINE_HEIGHT
            } else {
                measure(
                    m,
                    line,
                    INPUT_TEXT_SIZE,
                    400,
                    INPUT_LINE_HEIGHT,
                    Some(width.max(1.)),
                )
                .1
                .max(INPUT_LINE_HEIGHT)
            }
        })
        .sum()
}

// Footer -------------------------------------------------------------------------

/// Under the composer: checkout and branch, as in Zeron's session footer.
pub fn footer(chat: impl Fn() -> Option<ChatRow> + Clone + 'static) -> View {
    let ink_ = alpha(TEXT_MUTED, 0.6);
    let label = |glyph: &'static str, value: View| {
        row()
            .h(20.)
            .px(8.)
            .gap(6.)
            .min_w(0.)
            .items_center()
            .text_size(12.)
            .font_weight(500)
            .text_color(ink_)
            .child(theme::icon(glyph, 12., ink_).shrink_0())
            .child(value)
    };
    row()
        .w_full()
        .h(FOOTER_HEIGHT)
        .px(10.)
        .gap(4.)
        .items_center()
        .child(
            label("folder", text("Local checkout").truncate())
                .max_w(160.)
                .shrink_0(),
        )
        .child(label(
            "git-branch",
            text_signal(move || {
                chat()
                    .and_then(|c| c.branch)
                    .unwrap_or_else(|| "No ref".into())
            })
            .min_w(0.)
            .truncate(),
        ))
}

// Composer -------------------------------------------------------------------------

/// The pill, its error caption, and its live (animated) height.
struct Parts {
    pill: View,
    notice: View,
    height: Signal<f32>,
}

/// The whole docked composer: the container (`w_full`, max 768, `px 16`,
/// `pb 16`, gap 8) holding the frosted pill and the footer slot. Returns
/// the pill's live height (49 compact, 104..304 expanded, animated); the
/// whole dock is `stack_height(height)` tall.
#[allow(dead_code)]
pub fn dock(
    cx: &mut Context,
    state: &App,
    id: &str,
    working: impl Fn() -> bool + Clone + 'static,
    chat: impl Fn() -> Option<ChatRow> + Clone + 'static,
) -> (View, Signal<f32>) {
    let parts = build(cx, state, id, working);
    let view = column()
        .w_full()
        .max_w(CONTAINER_MAX_WIDTH)
        .shrink_0()
        .px(CONTAINER_PAD)
        .pb(CONTAINER_PAD)
        .gap(CONTAINER_GAP)
        .child(parts.notice)
        .child(parts.pill)
        .child(div().w_full().h(FOOTER_HEIGHT).mb(-8.).child(footer(chat)));
    (view, parts.height)
}

/// The pill alone (plus its error caption), as before; `composer_with_height`
/// also returns its live height.
#[allow(dead_code)]
pub fn composer(
    cx: &mut Context,
    state: &App,
    id: &str,
    working: impl Fn() -> bool + Clone + 'static,
) -> View {
    composer_with_height(cx, state, id, working).0
}

#[allow(dead_code)]
pub fn composer_with_height(
    cx: &mut Context,
    state: &App,
    id: &str,
    working: impl Fn() -> bool + Clone + 'static,
) -> (View, Signal<f32>) {
    let parts = build(cx, state, id, working);
    let view = column()
        .w_full()
        .max_w(CONTAINER_MAX_WIDTH - 2. * CONTAINER_PAD)
        .gap(CONTAINER_GAP)
        .child(parts.notice)
        .child(parts.pill);
    (view, parts.height)
}

fn build(
    cx: &mut Context,
    state: &App,
    id: &str,
    working: impl Fn() -> bool + Clone + 'static,
) -> Parts {
    let anim = state.anim.clone();
    let draft = cx.state(String::new());
    let sending = cx.state(false);
    let notice = cx.state(None::<String>);
    let send = {
        let (state, id, draft, sending, notice, working) = (
            state.clone(),
            id.to_owned(),
            draft.clone(),
            sending.clone(),
            notice.clone(),
            working.clone(),
        );
        let tasks = cx.tasks();
        Rc::new(move || {
            let prompt = draft.with_untracked(|d| d.trim().to_owned());
            if prompt.is_empty() || sending.with_untracked(|s| *s) {
                return;
            }
            let Some(chat) = state
                .rows
                .with_untracked(|rows| rows.iter().find(|r| r.id == id).cloned())
            else {
                return;
            };
            let busy = working();
            let (rpc, draft, sending, notice) = (
                state.rpc.clone(),
                draft.clone(),
                sending.clone(),
                notice.clone(),
            );
            sending.set(true);
            tasks.spawn(async move {
                let result = if busy {
                    // Mid-turn: queue it to run when the turn ends, as Zeron does.
                    rpc.call(
                        "QueueMessage",
                        json!({ "chatId": chat.id, "text": prompt, "holdForTurnEnd": true }),
                    )
                    .await
                } else {
                    let config = &chat.config;
                    rpc.call(
                        "QueueCommand",
                        json!({ "chatId": chat.id, "command": {
                            "kind": "run",
                            "messageId": uuid::Uuid::new_v4().to_string(),
                            "request": {
                                "prompt": prompt,
                                "harness": chat.harness(),
                                "model": config.get("model").cloned().unwrap_or(Value::Null),
                                "reasoning": config.get("reasoning").cloned().unwrap_or(Value::Null),
                                "modelOptions": config.get("modelOptions").cloned().unwrap_or_else(|| json!({})),
                                "cwd": chat.cwd.clone().unwrap_or_else(|| "~".into()),
                                "sandbox": config.get("sandbox").cloned().unwrap_or_else(|| json!("workspace-write")),
                                "autoApprove": false,
                                "resume": null,
                            }
                        }}),
                    )
                    .await
                };
                sending.set(false);
                match result {
                    Ok(_) => {
                        draft.set(String::new());
                        notice.set(None);
                    }
                    Err(error) => {
                        notice.set(Some(format!("Couldn't send: {error}")));
                    }
                }
            });
        })
    };
    let interrupt = {
        let (rpc, id) = (state.rpc.clone(), id.to_owned());
        let tasks = cx.tasks();
        move || {
            let (rpc, id) = (rpc.clone(), id.clone());
            tasks.spawn(async move {
                let _ = rpc
                    .call(
                        "QueueCommand",
                        json!({ "chatId": id, "command": { "kind": "interrupt" } }),
                    )
                    .await;
            });
        }
    };
    // Test hook: ZERON_SEND=<text> sends it through this composer once.
    if let Ok(text_) = std::env::var("ZERON_SEND") {
        static SENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SENT.swap(true, std::sync::atomic::Ordering::Relaxed) {
            let (draft, send) = (draft.clone(), send.clone());
            cx.tasks().spawn(async move {
                sleep(Duration::from_millis(2500)).await;
                println!("ZERON_SEND {text_:?}");
                draft.set(text_);
                send();
            });
        }
    }

    // Layout state. `inner` is the pill's width inside its hairlines, read
    // back from layout; the chip's target width comes from the picker.
    let inner = cx.state(0_f32);
    let expanded = cx.state(false);
    let content = cx.state(INPUT_LINE_HEIGHT);
    let height_tween = cx.state(Tween::settled(COMPACT_TOTAL_HEIGHT));
    let height = cx.state(COMPACT_TOTAL_HEIGHT);
    // The flip morph's progress (settled at 1) and the height it left from.
    let flip = cx.state(Tween::settled(1.));
    let flip_from = Rc::new(Cell::new(COMPACT_TOTAL_HEIGHT));
    let chip = Chip {
        target: cx.state(0_f32),
        tween: cx.state(Tween::settled(0.)),
        inner: inner.clone(),
        anim: anim.clone(),
    };

    // Mode and target height: hysteresis on the draft's width against the
    // compact capacity; every target change over half a pixel morphs the
    // height over 180 ms, except before the first measure.
    {
        let (inner, expanded, content, height_tween, height, flip, flip_from, chip, draft, anim) = (
            inner.clone(),
            expanded.clone(),
            content.clone(),
            height_tween.clone(),
            height.clone(),
            flip.clone(),
            flip_from.clone(),
            chip.clone(),
            draft.clone(),
            anim.clone(),
        );
        let m = cx.text_measure();
        let measured = Cell::new(false);
        let effect = cx.runtime().effect(move || {
            let width = inner.get();
            let chip_w = chip.target.get();
            let (text_width, newline, lines) = draft.with(|d| {
                let newline = d.contains('\n');
                let text_width = if newline {
                    0.
                } else {
                    measure(&m, d, INPUT_TEXT_SIZE, 400, INPUT_LINE_HEIGHT, None).0
                };
                (
                    text_width,
                    newline,
                    content_height(&m, d, (width - 32.).max(1.)),
                )
            });
            if content.with_untracked(|c| (c - lines).abs() > 0.01) {
                content.set(lines);
            }
            let was = expanded.with_untracked(|e| *e);
            let next = composer_flip(was, text_width, compact_capacity(width, chip_w), newline);
            // No motion until the pill has been measured once.
            let snap = width <= 0. || !measured.replace(true);
            if next != was {
                expanded.set(next);
                if !snap {
                    flip_from.set(height.with_untracked(|h| *h));
                    flip.set(Tween {
                        from: 0.,
                        target: 1.,
                        at: anim.now(),
                    });
                    anim.keep(COLLAPSE.0 + 0.05);
                }
            }
            let target = pill_target(next, lines);
            if height_tween.with_untracked(|t| (t.target - target).abs() > 0.5) {
                if snap {
                    height_tween.set(Tween::settled(target));
                } else {
                    anim.retarget(&height_tween, target, COLLAPSE);
                }
            }
        });
        cx.retain(effect);
    }
    // The live height, ticking only while it moves.
    {
        let (height_tween, height, anim) = (height_tween.clone(), height.clone(), anim.clone());
        let effect = cx.runtime().effect(move || {
            let value = anim.tween(&height_tween, COLLAPSE);
            if height.with_untracked(|h| (h - value).abs() > 0.01) {
                height.set(value);
            }
        });
        cx.retain(effect);
    }

    // Reads the pill's inner width back from layout.
    let probe = {
        let inner = inner.clone();
        canvas(move |(width, _)| {
            if inner.with_untracked(|w| (w - width).abs() > 0.5) {
                inner.set(width);
            }
            Canvas::new()
        })
        .absolute()
        .left(0.)
        .right(0.)
        .top(0.)
        .h(1.)
    };

    let placeholder = {
        let draft = draft.clone();
        text("Do anything…")
            .text_size(INPUT_TEXT_SIZE)
            .line_height(INPUT_LINE_HEIGHT)
            .text_color(TEXT_FAINT)
            .truncate()
            .reactive_style(move || {
                if draft.with(String::is_empty) {
                    Styles::new().flex()
                } else {
                    Styles::new().hidden()
                }
            })
    };
    let input = {
        let (send, expanded, content) = (send.clone(), expanded.clone(), content.clone());
        text_area("Message", draft.clone())
            .w_full()
            .h_full()
            .text_wrap(true)
            .font_family(theme::SANS)
            .text_size(INPUT_TEXT_SIZE)
            .line_height(INPUT_LINE_HEIGHT)
            .text_color(TEXT)
            .bg(alpha(TEXT, 0.))
            .border(0.)
            .rounded(0.)
            .p(0.)
            .overflow_hidden()
            .focus(|s| s)
            // A 12 px fade at both edges, only while the settled box scrolls.
            .reactive_style(move || {
                let band = if input_overflows(expanded.get(), content.get()) {
                    INPUT_FADE_BAND
                } else {
                    0.
                };
                Styles::new().fade_edges(band, band)
            })
            .on_event(move |event| {
                if event.phase == EventPhase::Capture
                    && matches!(
                        event.event,
                        InputEvent::KeyDown { key: Key::Enter, modifiers, .. } if !modifiers.shift
                    )
                {
                    event.prevent_default();
                    event.stop_propagation();
                    send();
                }
            })
    };
    // The text box: expanded, the textarea under the pill's top edge
    // (`px 16`, `pt` 12 -> 16, `pb 4`); compact, the middle of the bottom
    // row, 45 px in from the outer left, gliding down after a collapse.
    let text_box = {
        let (inner, expanded, height, flip, flip_from, chip, anim) = (
            inner.clone(),
            expanded.clone(),
            height.clone(),
            flip.clone(),
            flip_from.clone(),
            chip.clone(),
            anim.clone(),
        );
        overlay()
            .absolute()
            .child(placeholder)
            .child(input)
            .reactive_style(move || {
                let width = inner.get();
                let h = height.get() - PILL_BORDER_V;
                let progress = anim.tween(&flip, COLLAPSE);
                if expanded.get() {
                    let pad = morph_text_pad(progress);
                    let textarea = (h - ACTIONS_ROW).max(0.);
                    Styles::new()
                        .left(16.)
                        .top(pad)
                        .w((width - 32.).max(0.))
                        .h((textarea - pad - 4.).max(0.))
                } else {
                    let inset = morph_cluster_inset(false, progress);
                    let chip_w = chip.now();
                    let glide = collapse_text_glide(flip_from.get(), progress);
                    Styles::new()
                        .left(inset + 36.)
                        .top(h - COMPACT_ROW + (COMPACT_ROW - INPUT_LINE_HEIGHT) / 2. - glide)
                        .w((width - 2. * inset - 80. - chip_w).max(0.))
                        .h(INPUT_LINE_HEIGHT)
                }
            })
    };

    // Attach: a 28 px circle, paperclip at 18, washing in on hover.
    let attach = {
        let hover = Hover::new(cx, &anim);
        let blend = hover.clone();
        hover.track(
            button()
                .size(28., 28.)
                .p(0.)
                .rounded(14.)
                .border(0.)
                .items_center()
                .justify_center()
                .shrink_0()
                .cursor(Cursor::Pointer)
                .hover(|s| s)
                .focus(|s| s)
                .reactive_style(move || Styles::new().bg(ink(0.10 * blend.get())))
                .child(theme::icon("paperclip", 18., TEXT_MUTED))
                .on_click(|| {}),
        )
    };
    // Send / queue / stop: a 28 px light circle; the arrow dims to 35% and
    // goes inert while there's nothing to send, a dark square stops a turn.
    let action = {
        let (w1, w2, w3, w4) = (
            working.clone(),
            working.clone(),
            working.clone(),
            working.clone(),
        );
        let (d1, d2, d3, d4) = (draft.clone(), draft.clone(), draft.clone(), draft.clone());
        let empty = |draft: &Signal<String>| draft.with(|d| d.trim().is_empty());
        let arrow = theme::icon("arrow-up", 14., BG).reactive_style(move || {
            if w1() && empty(&d1) {
                Styles::new().hidden()
            } else {
                Styles::new().flex()
            }
        });
        let square = div()
            .size(11., 11.)
            .rounded(3.)
            .bg(BG)
            .reactive_style(move || {
                if w2() && empty(&d2) {
                    Styles::new().flex()
                } else {
                    Styles::new().hidden()
                }
            });
        button()
            .size(28., 28.)
            .p(0.)
            .rounded(14.)
            .border(0.)
            .items_center()
            .justify_center()
            .shrink_0()
            .bg(TEXT)
            .cursor(Cursor::Pointer)
            .focus(|s| s)
            .hover(|s| s.opacity(0.85))
            .disabled_style(|s| s.opacity(0.35))
            .disabled_when(move || !w3() && empty(&d3))
            .child(arrow)
            .child(square)
            .on_click(move || {
                if w4() && empty(&d4) {
                    interrupt();
                } else {
                    send();
                }
            })
    };
    // The model chip's slot: at most 45% of the pill.
    let picker = {
        let inner = inner.clone();
        row()
            .min_w(0.)
            .items_center()
            .reactive_style(move || {
                let width = inner.get();
                Styles::new().max_w(if width > 0. {
                    CHIP_SHARE * (width + PILL_BORDER_V)
                } else {
                    CHIP_MAX
                })
            })
            .child(picker_for(state, id, chip.clone()))
    };
    // The controls, pinned to the pill's stationary bottom edge: one 47 px
    // row compact, the 42 px actions row expanded; insets 8 <-> 12.
    let controls = {
        let (expanded, flip, anim) = (expanded.clone(), flip.clone(), anim.clone());
        row()
            .absolute()
            .left(0.)
            .right(0.)
            .bottom(0.)
            .items_center()
            .reactive_style(move || {
                let progress = anim.tween(&flip, COLLAPSE);
                let open = expanded.get();
                let inset = morph_cluster_inset(open, progress);
                let dy = morph_cluster_dy(progress);
                if open {
                    Styles::new()
                        .h(ACTIONS_ROW)
                        .pt(2.)
                        .pb(ACTIONS_BOTTOM_PAD)
                        .px(inset)
                        .translate(0., dy)
                } else {
                    Styles::new()
                        .h(COMPACT_ROW)
                        .pt(0.)
                        .pb(0.)
                        .px(inset)
                        .translate(0., -dy)
                }
            })
            .child(attach)
            .child(div().grow().min_w(0.).h(1.))
            .child(picker)
            .child(row().pl(8.).items_center().shrink_0().child(action))
    };
    // Zeron's frosted pill: radius 22, a 16 px backdrop blur over a faint
    // tint, a cool hairline; no shadow, no focus ring. The bottom edge stays
    // put (the dock is bottom-anchored), so only the height animates.
    let pill = {
        let height = height.clone();
        div()
            .w_full()
            .shrink_0()
            .relative()
            .rounded(RADIUS)
            .blur(16.)
            .bg(SURFACE_TINT)
            .border(1.)
            .border_color(COMPOSER_BORDER)
            .overflow_hidden()
            .reactive_style(move || Styles::new().h(height.get()))
            .child(probe)
            .child(controls)
            .child(text_box)
    };
    let notice_view = {
        let (shown, message, dismiss) = (notice.clone(), notice.clone(), notice.clone());
        row()
            .mx(8.)
            .mt(6.)
            .gap(6.)
            .items_center()
            .text_size(11.)
            .line_height(14.)
            .text_color(TEXT_FAINT)
            .cursor(Cursor::Pointer)
            .reactive_style(move || {
                if shown.with(Option::is_some) {
                    Styles::new().flex()
                } else {
                    Styles::new().hidden()
                }
            })
            .child(div().size(5., 5.).rounded(2.5).bg(DANGER).shrink_0())
            .child(
                text_signal(move || message.get().unwrap_or_default())
                    .min_w(0.)
                    .truncate(),
            )
            .on_click(move || {
                dismiss.set(None);
            })
    };
    Parts {
        pill,
        notice: notice_view,
        height,
    }
}

/// The model chip's width: the target the picker measures and its glide.
#[derive(Clone)]
struct Chip {
    target: Signal<f32>,
    tween: Signal<Tween>,
    /// The pill's inner width, for the 45% cap.
    inner: Signal<f32>,
    anim: Anim,
}

impl Chip {
    /// The gliding width, capped at 45% of the pill.
    fn now(&self) -> f32 {
        let width = self.anim.tween(&self.tween, RESIZE);
        let inner = self.inner.get();
        if inner > 0. {
            width.min(CHIP_SHARE * (inner + PILL_BORDER_V))
        } else {
            width
        }
    }
    fn retarget(&self, width: f32) {
        if self.target.with_untracked(|t| (t - width).abs() > 0.01) {
            self.target.set(width);
        }
        if self.tween.with_untracked(|t| t.target <= 0.) {
            self.tween.set(Tween::settled(width));
        } else {
            self.anim.retarget(&self.tween, width, RESIZE);
        }
    }
}

/// Set fields of the chat's config. `setChatConfig` replaces the whole
/// config, so the rest is copied from the chat row.
fn set_config(state: &App, id: &str, chat: &ChatRow, changes: &[(&str, Value)]) {
    let mut config = chat.config.clone();
    if !config.is_object() {
        config = default_config(chat);
    }
    for (key, value) in changes {
        config[*key] = value.clone();
    }
    send_config(state, id, config);
}

fn default_config(chat: &ChatRow) -> Value {
    json!({ "harness": chat.harness(), "modelOptions": {}, "sandbox": "workspace-write" })
}

/// `Mutate setChatConfig` with a whole config.
fn send_config(state: &App, id: &str, config: Value) {
    let (rpc, id) = (state.rpc.clone(), id.to_owned());
    state.tasks.spawn(async move {
        if let Err(error) = rpc
            .call(
                "Mutate",
                json!({ "op": "setChatConfig", "chatId": id, "config": config }),
            )
            .await
        {
            eprintln!("zeron: updating the chat config failed: {error}");
        }
    });
}

// Model catalog ------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OptionChoice {
    id: String,
    label: String,
}

/// A model option as `ListModels` sends it (`zeron_proto::ModelOption`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ModelOption {
    id: String,
    label: String,
    choices: Vec<OptionChoice>,
    default: String,
}

/// A catalog model (`zeron_proto::Model`).
#[derive(Clone, Debug, PartialEq)]
struct ModelInfo {
    id: String,
    label: String,
    description: Option<String>,
    levels: Vec<String>,
    options: Vec<ModelOption>,
}

fn text_at(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn parse_models(list: &Value) -> Vec<ModelInfo> {
    let array = |value: &Value, key: &str| -> Vec<Value> {
        value
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = text_at(m, "id")?;
            let options = array(m, "options")
                .iter()
                .filter_map(|o| {
                    let choices = array(o, "choices")
                        .iter()
                        .filter_map(|c| {
                            let id = text_at(c, "id")?;
                            Some(OptionChoice {
                                label: text_at(c, "label").unwrap_or_else(|| id.clone()),
                                id,
                            })
                        })
                        .collect();
                    Some(ModelOption {
                        id: text_at(o, "id")?,
                        label: text_at(o, "label").unwrap_or_default(),
                        default: text_at(o, "defaultChoice").unwrap_or_default(),
                        choices,
                    })
                })
                .collect();
            Some(ModelInfo {
                label: text_at(m, "label").unwrap_or_else(|| id.clone()),
                description: text_at(m, "description"),
                levels: array(m, "reasoningLevels")
                    .iter()
                    .filter_map(|l| l.as_str().map(str::to_owned))
                    .collect(),
                options,
                id,
            })
        })
        .collect()
}

thread_local! {
    /// The full catalog per harness (options included), beside `App::models`.
    static CATALOGS: RefCell<HashMap<String, Signal<Vec<ModelInfo>>>> =
        RefCell::new(HashMap::new());
    /// Starred models per harness. Zeron keeps them in its local defaults;
    /// the clone has no store, so they last the session.
    static FAVORITES: RefCell<HashMap<String, Signal<Vec<String>>>> =
        RefCell::new(HashMap::new());
}

fn models_for(state: &App, cx: &mut Context, harness: &str) -> (Models, Signal<Vec<ModelInfo>>) {
    let cached = state.models.borrow().get(harness).cloned();
    let catalog = CATALOGS.with(|c| c.borrow().get(harness).cloned());
    if let (Some(models), Some(catalog)) = (cached, catalog) {
        return (models, catalog);
    }
    let models = cx.state(Vec::new());
    let catalog = cx.state(Vec::new());
    state
        .models
        .borrow_mut()
        .insert(harness.to_owned(), models.clone());
    CATALOGS.with(|c| c.borrow_mut().insert(harness.to_owned(), catalog.clone()));
    let (rpc, out, full, harness) = (
        state.rpc.clone(),
        models.clone(),
        catalog.clone(),
        harness.to_owned(),
    );
    state.tasks.spawn(async move {
        if let Ok(list) = rpc
            .call("ListModels", json!({ "harness": harness, "force": false }))
            .await
        {
            let parsed = parse_models(&list);
            out.set(
                parsed
                    .iter()
                    .map(|m| (m.id.clone(), m.label.clone(), m.levels.clone()))
                    .collect(),
            );
            full.set(parsed);
        }
    });
    (models, catalog)
}

fn favorites_for(cx: &mut Context, harness: &str) -> Signal<Vec<String>> {
    FAVORITES.with(|f| {
        f.borrow_mut()
            .entry(harness.to_owned())
            .or_insert_with(|| cx.state(Vec::new()))
            .clone()
    })
}

// Pure picker state ------------------------------------------------------------------

fn str_at<'a>(config: &'a Value, key: &str) -> Option<&'a str> {
    config.get(key).and_then(Value::as_str)
}

/// The chat's model, else (none picked) the catalog's first.
fn selected_model<'a>(catalog: &'a [ModelInfo], config: &Value) -> Option<&'a ModelInfo> {
    match str_at(config, "model") {
        Some(id) => catalog.iter().find(|m| m.id == id),
        None => catalog.first(),
    }
}

fn option_value<'a>(config: &'a Value, option: &'a ModelOption) -> &'a str {
    config
        .get("modelOptions")
        .and_then(|options| options.get(&option.id))
        .and_then(Value::as_str)
        .unwrap_or(&option.default)
}

/// What the slider's stops write: a reasoning level or an option choice.
#[derive(Clone, Debug, PartialEq)]
enum Stops {
    Reasoning(Vec<String>),
    Option {
        id: String,
        choices: Vec<String>,
        default: String,
    },
}

/// `CompactEffort`: the slider's labels and the selected stop.
#[derive(Clone, Debug, PartialEq)]
struct Effort {
    labels: Vec<String>,
    selected: usize,
    stops: Stops,
}

/// `compact_effort`: the model's reasoning ladder, else an option shaped
/// like one (Cursor's `effort` / `reasoning` choices).
fn compact_effort(model: Option<&ModelInfo>, config: &Value) -> Option<Effort> {
    let model = model?;
    if !model.levels.is_empty() {
        let current = effective_effort(
            str_at(config, "reasoning").map(str::to_owned),
            &model.levels,
        );
        return Some(Effort {
            labels: model.levels.iter().map(|l| effort_label(l)).collect(),
            selected: model
                .levels
                .iter()
                .position(|l| Some(l) == current.as_ref())
                .unwrap_or(0),
            stops: Stops::Reasoning(model.levels.clone()),
        });
    }
    let option = model
        .options
        .iter()
        .find(|o| EFFORT_OPTION_IDS.contains(&o.id.as_str()) && o.choices.len() > 1)?;
    let current = option_value(config, option);
    Some(Effort {
        labels: option.choices.iter().map(|c| c.label.clone()).collect(),
        selected: option
            .choices
            .iter()
            .position(|c| c.id == current)
            .unwrap_or(0),
        stops: Stops::Option {
            id: option.id.clone(),
            choices: option.choices.iter().map(|c| c.id.clone()).collect(),
            default: option.default.clone(),
        },
    })
}

/// Fast mode's `(on, off)` choices, whatever form the harness gives it:
/// `fastMode`/`fast_mode` on/off (Claude), Cursor's `fast` true/false, or a
/// tier offering `fast` (Codex, Devin).
fn fast_mode_values(option: &ModelOption) -> Option<(&str, &str)> {
    let has = |id: &str| option.choices.iter().any(|choice| choice.id == id);
    let on = if matches!(option.id.as_str(), "fastMode" | "fast_mode") && has("on") {
        "on"
    } else if option.id == "fast" && has("true") {
        "true"
    } else if has("fast") {
        "fast"
    } else {
        return None;
    };
    let off = if option.default != on && has(&option.default) {
        option.default.as_str()
    } else {
        option
            .choices
            .iter()
            .map(|choice| choice.id.as_str())
            .find(|id| *id != on)?
    };
    Some((on, off))
}

/// `compact_fast_choice`: the fast option, the choice a toggle writes,
/// whether that is the default, and whether fast mode is on.
#[derive(Clone, Debug, PartialEq)]
struct FastChoice {
    option: String,
    next: String,
    next_default: bool,
    fast: bool,
}

fn fast_choice(model: Option<&ModelInfo>, config: &Value) -> Option<FastChoice> {
    let (option, on, off) = model?
        .options
        .iter()
        .find_map(|o| fast_mode_values(o).map(|(on, off)| (o, on, off)))?;
    let fast = option_value(config, option) == on;
    let next = if fast { off } else { on };
    Some(FastChoice {
        option: option.id.clone(),
        next: next.to_owned(),
        next_default: next == option.default,
        fast,
    })
}

/// One option row's group (`SettingGroup`).
#[derive(Clone, Debug, PartialEq)]
struct Group {
    id: String,
    label: String,
    choices: Vec<GroupChoice>,
}

#[derive(Clone, Debug, PartialEq)]
struct GroupChoice {
    id: String,
    label: String,
    selected: bool,
    default: bool,
}

/// The option rows: every option but the ones the card draws itself
/// (fast mode, an effort-shaped option).
fn option_groups(model: Option<&ModelInfo>, config: &Value) -> Vec<Group> {
    let Some(model) = model else {
        return Vec::new();
    };
    let mut hidden: Vec<String> = Vec::new();
    if let Some(fast) = fast_choice(Some(model), config) {
        hidden.push(fast.option);
    }
    if let Some(Effort {
        stops: Stops::Option { id, .. },
        ..
    }) = compact_effort(Some(model), config)
    {
        hidden.push(id);
    }
    model
        .options
        .iter()
        .filter(|o| !o.choices.is_empty() && !hidden.contains(&o.id))
        .map(|o| {
            let selected = option_value(config, o);
            Group {
                id: o.id.clone(),
                label: o.label.clone(),
                choices: o
                    .choices
                    .iter()
                    .map(|c| GroupChoice {
                        id: c.id.clone(),
                        label: c.label.clone(),
                        selected: c.id == selected,
                        default: c.id == o.default,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// `match_rank`: a label prefix ranks 0, a substring 1.
fn match_rank(query: &str, label: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(1);
    }
    let label = label.to_lowercase();
    if label.starts_with(&query) {
        Some(0)
    } else if label.contains(&query) {
        Some(1)
    } else {
        None
    }
}

/// A model list row (`ModelRowData`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ListRow {
    id: String,
    label: String,
    description: Option<String>,
    favorite: bool,
    /// The chat's model, absent from the catalog: shown, not pickable.
    selected_only: bool,
    /// Another row shares the label: show the description.
    ambiguous: bool,
}

/// `scoped_model_rows` for one provider: starred first; a search ranks
/// label prefix, label substring, then description hits.
fn list_rows(
    catalog: &[ModelInfo],
    selected: Option<&str>,
    favorites: &[String],
    query: &str,
) -> Vec<ListRow> {
    let favorite = |id: &str| favorites.iter().any(|f| f == id);
    let row = |m: &ModelInfo| ListRow {
        id: m.id.clone(),
        label: m.label.clone(),
        description: m.description.clone(),
        favorite: favorite(&m.id),
        selected_only: false,
        ambiguous: false,
    };
    let query = query.trim();
    let mut rows: Vec<ListRow> = if query.is_empty() {
        let (starred, rest): (Vec<&ModelInfo>, Vec<&ModelInfo>) =
            catalog.iter().partition(|m| favorite(&m.id));
        starred.into_iter().chain(rest).map(row).collect()
    } else {
        let mut ranked: Vec<(usize, bool, usize, ListRow)> = catalog
            .iter()
            .enumerate()
            .filter_map(|(ix, m)| {
                let by_label = match_rank(query, &m.label);
                let by_description = match_rank(
                    query,
                    &format!("{} {}", m.description.as_deref().unwrap_or(""), m.label),
                )
                .map(|rank| rank + 2);
                let rank = by_label.into_iter().chain(by_description).min()?;
                Some((rank, !favorite(&m.id), ix, row(m)))
            })
            .collect();
        ranked.sort_by_key(|(rank, unstarred, ix, _)| (*rank, *unstarred, *ix));
        ranked.into_iter().map(|(.., row)| row).collect()
    };
    if let Some(id) = selected
        && !catalog.is_empty()
        && !catalog.iter().any(|m| m.id == id)
        && (query.is_empty() || id.to_lowercase().contains(&query.to_lowercase()))
    {
        rows.insert(
            0,
            ListRow {
                id: id.to_owned(),
                label: id.to_owned(),
                description: Some(
                    "Selected in this chat; absent from the current model list".into(),
                ),
                favorite: false,
                selected_only: true,
                ambiguous: false,
            },
        );
    }
    let labels: Vec<String> = rows.iter().map(|r| r.label.clone()).collect();
    for row in &mut rows {
        row.ambiguous = labels.iter().filter(|l| **l == row.label).count() > 1;
    }
    rows
}

/// Where each group starts: the starred run, then the provider's.
fn row_groups(rows: &[ListRow]) -> Vec<(bool, usize)> {
    let mut groups: Vec<(bool, usize)> = Vec::new();
    for (ix, row) in rows.iter().enumerate() {
        if groups
            .last()
            .is_none_or(|(starred, _)| *starred != row.favorite)
        {
            groups.push((row.favorite, ix));
        }
    }
    groups
}

/// The list's scroll offset on opening at `selected`: its group at the top
/// (one row of the group above left under the fade) when it sits near the
/// group's start, else centred on it.
fn open_offset(groups: &[(bool, usize)], selected: usize, viewport: f32) -> f32 {
    let pitch = COMPACT_ROW_HEIGHT + MENU_GAP;
    let start = groups
        .iter()
        .rev()
        .find(|(_, start)| *start <= selected)
        .map_or(0, |(_, start)| *start);
    if selected - start < COMPACT_LIST_ROWS as usize - 2 {
        (start - usize::from(start > 0)) as f32 * pitch
    } else {
        (selected as f32 * pitch - (viewport - pitch) / 2.).max(0.)
    }
}

/// The list fits its rows up to seven; empty and loading keep four.
fn compact_list_height(rows: usize) -> f32 {
    let rows = if rows == 0 {
        4.
    } else {
        (rows as f32).min(COMPACT_LIST_ROWS)
    };
    rows * (COMPACT_ROW_HEIGHT + MENU_GAP) + 2. * CARD_INSET
}

/// The header inside the card inset.
fn header_block(effort: bool) -> f32 {
    2. * CARD_INSET
        + if effort {
            HEADER_HEIGHT
        } else {
            HEADER_HEIGHT_SINGLE
        }
}

/// The option rows' 4 px top inset, then 28 px rows (26 + gap).
fn options_height(count: usize) -> f32 {
    2. + count.min(OPTIONS_MAX_ROWS) as f32 * (OPTION_ROW + MENU_GAP)
}

/// The panel page's height, borders included.
fn panel_height(effort: bool, options: usize) -> f32 {
    CARD_BORDERS
        + header_block(effort)
        + if effort { EFFORT_BLOCK } else { 0. }
        + if options == 0 {
            0.
        } else {
            OPTIONS_GAP + options_height(options)
        }
}

/// The model list page's height, borders included.
fn models_height(rows: usize) -> f32 {
    LIST_HEADER + compact_list_height(rows) + CARD_BORDERS
}

/// The docked card's height budget: the room above the trigger less 14,
/// clamped to 0..640.
fn menu_budget(trigger_top: f32) -> f32 {
    (trigger_top - MENU_MARGIN).clamp(0., MENU_MAX_HEIGHT)
}

/// The card's height: its page's content, at most the budget.
fn card_height(budget: f32, content: f32) -> f32 {
    budget.min(content)
}

/// The list viewport (with its 4 px insets) within the budget.
fn list_height(budget: f32, rows: usize) -> f32 {
    compact_list_height(rows).min((budget - LIST_HEADER - CARD_BORDERS).max(0.))
}

/// Pointer x to the slider's 0..1, over the run between the thumb insets.
fn effort_fraction(x: f32, width: f32) -> f32 {
    ((x - THUMB_INSET) / (width - THUMB_INSET * 2.).max(1.)).clamp(0., 1.)
}

/// The nearest advertised stop, both ends included.
fn effort_index(x: f32, width: f32, count: usize) -> Option<usize> {
    (count > 0)
        .then(|| (effort_fraction(x, width) * count.saturating_sub(1) as f32).round() as usize)
}

/// A stop's position on the run.
fn stop_fraction(index: usize, count: usize) -> f32 {
    if count > 1 {
        index as f32 / (count - 1) as f32
    } else {
        0.
    }
}

/// The hidden native slider's value back to an x in its box: its rail
/// runs between half its 10 px thumb at either end.
fn slider_local_x(value: f32, width: f32) -> f32 {
    let thumb = width.min(10.);
    thumb / 2. + value.clamp(0., 1.) * (width - thumb).max(0.)
}

/// Thumb centre for a fraction, in the slider's box.
fn thumb_center(fraction: f32, width: f32) -> f32 {
    THUMB_INSET + fraction * (width - 2. * THUMB_INSET).max(0.)
}

/// The fill runs a rail radius past the thumb's centre.
fn fill_width(fraction: f32, width: f32) -> f32 {
    (thumb_center(fraction, width) + FILL_PAST).min(width)
}

/// Byte lengths of the shared leading and trailing runs, never overlapping.
fn shared_ends(from: &str, to: &str) -> (usize, usize) {
    let prefix: usize = from
        .chars()
        .zip(to.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    let room = (from.len() - prefix).min(to.len() - prefix);
    let suffix: usize = from[prefix..]
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

// Colour ---------------------------------------------------------------------------

/// `theme.for_popup()` in frosted dark: muted text is the text at 64%.
fn pop_muted() -> Color {
    alpha(TEXT, 0.64)
}

fn black(a: f32) -> Color {
    alpha(Color(0, 0, 0, 255), a)
}

/// `glass::lift`: toward white at the colour's own alpha.
fn lift(color: Color, amount: f32) -> Color {
    mix(color, Color(255, 255, 255, color.3), amount)
}

fn rgb_to_hsl(color: Color) -> (f32, f32, f32) {
    let (r, g, b) = (
        f32::from(color.0) / 255.,
        f32::from(color.1) / 255.,
        f32::from(color.2) / 255.,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.;
    if max == min {
        return (0., 0., l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2. - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6. } else { 0. }
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    };
    (h / 6., s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32, a: u8) -> Color {
    let channel = |t: f32| {
        let q = if l < 0.5 { l * (1. + s) } else { l + s - l * s };
        let p = 2. * l - q;
        let t = t.rem_euclid(1.);
        let v = if t < 1. / 6. {
            p + (q - p) * 6. * t
        } else if t < 0.5 {
            q
        } else if t < 2. / 3. {
            p + (q - p) * (2. / 3. - t) * 6.
        } else {
            p
        };
        (v * 255.).round().clamp(0., 255.) as u8
    };
    Color(channel(h + 1. / 3.), channel(h), channel(h - 1. / 3.), a)
}

/// The fast-mode band's colour at `x` along the fill (`effort_fill`'s
/// `flow`): two slow waves shade it between a lifted and a hue-shifted
/// accent, gathering light just before the thumb.
fn shimmer_color(x: f32, from: f32, center: f32, phase: f32, breath: f32, intensity: f32) -> Color {
    let base = ACCENT;
    let lifted = mix(base, Color(255, 255, 255, 255), 0.45);
    let (h, s, l) = rgb_to_hsl(base);
    let shifted = hsl_to_rgb((h + 0.07).fract(), s, (l + 0.14).min(0.9), 255);
    let slow = 0.5 + 0.5 * (TAU * (x / 150. - 2. * phase)).sin();
    let quick = 0.5 + 0.5 * (TAU * (x / 64. - 3. * phase) + 1.7).sin();
    let glow = (0.65 * slow + 0.35 * quick).powi(2);
    let charge = (-(center - x).max(0.) / 40.).exp() * (0.6 + 0.4 * breath);
    let lead = ((x - from) / 28.).clamp(0., 1.);
    let a = intensity * ((0.08 + 0.34 * glow) * lead + 0.22 * charge);
    alpha(mix(shifted, lifted, slow), a.min(1.))
}

fn vertical(top: Color, bottom: Color) -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.,
            color: top,
        },
        GradientStop {
            offset: 1.,
            color: bottom,
        },
    ]
}

fn drop(color: Color, y: f32, sigma: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: Transform { x: 0., y },
        blur_radius: sigma,
        spread: 0.,
    }
}

/// A rim or inset-highlight layer over a plate. zgx has no inset shadows,
/// so the 1 px rim is a bordered overlay and the 1 px inset top highlight
/// (`inset 0 1px 0`) a top-only border: the same crescent under the
/// rounded corners.
fn plate_ring(radius: f32, color: Color, top_only: bool) -> View {
    let ring = div()
        .absolute()
        .left(0.)
        .top(0.)
        .right(0.)
        .bottom(0.)
        .rounded(radius)
        .border_color(color);
    if top_only {
        ring.border_edges(Insets {
            top: 1.,
            right: 0.,
            bottom: 0.,
            left: 0.,
        })
    } else {
        ring.border(1.)
    }
}

/// `glass::light_plate` (dark): white 8% -> 5%, a 9% rim, a 7% inset top
/// highlight and a 16% drop 1 px down.
fn light_plate(view: View, radius: f32) -> View {
    view.rounded(radius)
        .bg_gradient(FRAC_PI_2, vertical(ink(0.08), ink(0.05)))
        .shadow(drop(black(0.16), 1., 2.))
        .child(plate_ring(radius, ink(0.09), false))
        .child(plate_ring(radius, ink(0.07), true))
}

/// `glass::thumb_plate` (dark): opaque white -> 96.5%, a 12% black rim, an
/// 80% inset highlight and a 22% drop.
fn thumb_plate(view: View, radius: f32) -> View {
    view.rounded(radius)
        .bg_gradient(
            FRAC_PI_2,
            vertical(Color(255, 255, 255, 255), Color(246, 246, 246, 255)),
        )
        .shadow(drop(black(0.22), 1., 2.))
        .child(plate_ring(radius, black(0.12), false))
        .child(plate_ring(radius, ink(0.8), true))
}

// Icons ------------------------------------------------------------------------------

/// Zeron icons the theme doesn't ship.
const ALT_ARROW_LEFT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M14.75 17.5 9.25 12 14.75 6.5"/></svg>"#;
const STAR_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" d="M12 3.2l2.66 5.39 5.95.86-4.3 4.2 1.01 5.92L12 16.77l-5.32 2.8 1.01-5.92-4.3-4.2 5.95-.86L12 3.2z"/></svg>"#;

fn icon_source(name: &str) -> SvgData {
    match name {
        "alt-arrow-left" => SvgData::new(ALT_ARROW_LEFT_SVG.as_bytes()).expect("icon"),
        "star" => SvgData::new(STAR_SVG.as_bytes()).expect("icon"),
        _ => theme::icon_data(name),
    }
}

fn icon(name: &'static str, size: f32, color: Color) -> View {
    svg(name, Arc::new(icon_source(name).tinted(color))).size(size, size)
}

/// An icon whose tint follows signals.
fn icon_with(name: &'static str, size: f32, color: impl Fn() -> Color + 'static) -> View {
    let source = icon_source(name);
    svg_signal(name, move || Arc::new(source.tinted(color()))).size(size, size)
}

// Rolling labels ---------------------------------------------------------------------

/// A label change in flight.
#[derive(Clone, Debug, PartialEq)]
struct Roll {
    from: String,
    to: String,
    at: f32,
}

/// `roll_text`: plain and truncating at rest; when the label changes, the
/// shared ends stay (the trailing run slides with the width) while the
/// changed glyphs roll, old ones rising out as new ones rise in from below,
/// staggered left to right over 550 ms `EASE_OUT_QUINT`, the width gliding
/// over 200 ms. The first label never rolls.
fn roll_text(
    cx: &mut Context,
    anim: &Anim,
    label: impl Fn() -> String + 'static,
    size: f32,
    weight: u16,
    line: f32,
) -> View {
    let current = cx.state(String::new());
    let roll = cx.state(None::<Roll>);
    {
        let (current, roll, anim) = (current.clone(), roll.clone(), anim.clone());
        let tasks = cx.tasks();
        let seen = Cell::new(false);
        let effect = cx.runtime().effect(move || {
            let next = label();
            let old = current.with_untracked(String::clone);
            if seen.get() && old == next {
                return;
            }
            current.set(next.clone());
            if !seen.replace(true) || old.is_empty() || next.is_empty() {
                return;
            }
            let at = anim.now();
            let span = ROLL.0 * (1. + ROLL_STAGGER);
            roll.set(Some(Roll {
                from: old,
                to: next,
                at,
            }));
            anim.keep(span + 0.05);
            let roll = roll.clone();
            tasks.spawn(async move {
                sleep(Duration::from_secs_f32(span)).await;
                if roll.with_untracked(|r| r.as_ref().is_some_and(|r| r.at == at)) {
                    roll.set(None);
                }
            });
        });
        cx.retain(effect);
    }
    let m = cx.text_measure();
    let anim = anim.clone();
    switch(
        move || roll.get(),
        move |roll, _| match roll {
            None => {
                let current = current.clone();
                text_signal(move || current.get()).min_w(0.).truncate()
            }
            Some(roll) => rolling(&m, &anim, roll, size, weight, line),
        },
    )
    .min_w(0.)
}

fn rolling(m: &TextMeasure, anim: &Anim, roll: Roll, size: f32, weight: u16, line: f32) -> View {
    let width = |s: &str| measure(m, s, size, weight, line, None).0;
    let (prefix, suffix) = shared_ends(&roll.from, &roll.to);
    let (from_len, to_len) = (roll.from.len(), roll.to.len());
    let (from_w, to_w) = (width(&roll.from), width(&roll.to));
    let travel = size * ROLL_TRAVEL_EM;
    let span = ROLL.0 * (1. + ROLL_STAGGER);
    let elapsed = {
        let (anim, at) = (anim.clone(), roll.at);
        move || anim.at(at + span) - at
    };
    let glyph = move |text_: &str| {
        text(text_)
            .text_size(size)
            .font_weight(weight)
            .line_height(line)
            .absolute()
            .top(0.)
    };
    // The changed glyphs of one side with their x, staggered by x.
    let changed = |s: &str, len: usize| -> Vec<(String, f32)> {
        s.char_indices()
            .filter(|(i, _)| *i >= prefix && *i < len - suffix)
            .map(|(i, c)| (c.to_string(), width(&s[..i])))
            .collect()
    };
    let delays = |glyphs: &[(String, f32)]| -> Vec<f32> {
        let count = glyphs.len();
        let min = glyphs.iter().map(|g| g.1).fold(f32::INFINITY, f32::min);
        let max = glyphs.iter().map(|g| g.1).fold(f32::NEG_INFINITY, f32::max);
        let spread = ROLL.0 * ROLL_STAGGER * count.saturating_sub(1) as f32 / count.max(1) as f32;
        glyphs
            .iter()
            .map(|g| {
                if max > min {
                    spread * ((g.1 - min) / (max - min)).clamp(0., 1.)
                } else {
                    0.
                }
            })
            .collect()
    };
    let mut children: Vec<View> = Vec::new();
    if prefix > 0 {
        children.push(glyph(&roll.to[..prefix]).left(0.));
    }
    if suffix > 0 {
        let from_x = width(&roll.from[..from_len - suffix]);
        let to_x = width(&roll.to[..to_len - suffix]);
        let elapsed = elapsed.clone();
        children.push(glyph(&roll.to[to_len - suffix..]).reactive_style(move || {
            let slide = RESIZE.at(elapsed());
            Styles::new().left(to_x + (from_x - to_x) * (1. - slide))
        }));
    }
    for (rising_in, glyphs) in [
        (false, changed(&roll.from, from_len)),
        (true, changed(&roll.to, to_len)),
    ] {
        for ((ch, x), delay) in glyphs.iter().zip(delays(&glyphs)) {
            let elapsed = elapsed.clone();
            children.push(glyph(ch).left(*x).reactive_style(move || {
                let t = ROLL.1.eval(((elapsed() - delay) / ROLL.0).clamp(0., 1.));
                let (dy, opacity) = if rising_in {
                    (travel * (1. - t), t)
                } else {
                    (-travel * t, 1. - t)
                };
                Styles::new().translate(0., dy).opacity(opacity)
            }));
        }
    }
    row()
        .relative()
        .h(line)
        .min_w(0.)
        .flex_shrink(1.)
        .overflow_x_hidden()
        .reactive_style(move || {
            let t = RESIZE.at(elapsed());
            Styles::new().w(lerp(from_w, to_w, t).ceil())
        })
        .children(children)
}

// The picker -------------------------------------------------------------------------

/// The model picker, rebuilt when the chat's harness changes: it decides
/// the models offered.
fn picker_for(state: &App, id: &str, chip: Chip) -> View {
    let harness = {
        let (rows, id) = (state.rows.clone(), id.to_owned());
        move || rows.with(|rows| rows.iter().find(|r| r.id == id).map(ChatRow::harness))
    };
    let (state, id) = (state.clone(), id.to_owned());
    switch(harness, move |harness, cx| {
        picker(
            cx,
            &state,
            &id,
            harness.unwrap_or_else(|| "claude-code".into()),
            chip.clone(),
        )
    })
    .min_w(0.)
}

/// The picker card's lifecycle: open (entering since), closing (since).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Closed,
    Open(f32),
    Closing(f32),
}

/// The compact card's two pages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Panel,
    Models,
}

/// A config written but not yet echoed by the chat row: the picks show at
/// once (Zeron stamps the row optimistically).
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    config: Value,
    /// The row's config when it was written; any row change retires it.
    base: Value,
}

/// The picker's shared state.
#[derive(Clone)]
struct Picker {
    state: App,
    id: String,
    harness: String,
    anim: Anim,
    rt: Runtime,
    tasks: Tasks,
    catalog: Signal<Vec<ModelInfo>>,
    favorites: Signal<Vec<String>>,
    pending: Signal<Option<Pending>>,
    page: Signal<Page>,
    /// The new page fades in over the resize.
    reveal: Signal<Tween>,
    query: Signal<String>,
    /// The highlighted model row.
    active: Signal<Option<String>>,
    offset: Signal<f32>,
    /// The chip's top in the window, read at the press that opened it.
    trigger_top: Signal<f32>,
    search_focused: Signal<bool>,
    /// The option row whose choices are open.
    submenu: Signal<Option<String>>,
}

impl Picker {
    fn chat(&self) -> Option<ChatRow> {
        self.state
            .rows
            .with(|rows| rows.iter().find(|r| r.id == self.id).cloned())
    }
    /// The effective config: the pending write, else the row's.
    fn config(&self) -> Value {
        if let Some(pending) = self.pending.get() {
            return pending.config;
        }
        self.chat().map(|c| c.config).unwrap_or(Value::Null)
    }
    fn with_model<R>(&self, f: impl FnOnce(Option<&ModelInfo>, &Value) -> R) -> R {
        let config = self.config();
        self.catalog
            .with(|c| f(selected_model(c, &config), &config))
    }
    fn effort(&self) -> Option<Effort> {
        self.with_model(compact_effort)
    }
    fn fast(&self) -> Option<FastChoice> {
        self.with_model(fast_choice)
    }
    fn fast_on(&self) -> bool {
        self.fast().is_some_and(|f| f.fast)
    }
    fn groups(&self) -> Vec<Group> {
        self.with_model(option_groups)
    }
    fn group(&self, id: &str) -> Option<Group> {
        self.groups().into_iter().find(|g| g.id == id)
    }
    fn model_label(&self) -> String {
        let config = self.config();
        let model = str_at(&config, "model").map(str::to_owned);
        self.catalog.with(|c| {
            selected_model(c, &config)
                .map(|m| m.label.clone())
                .or(model)
                .unwrap_or_else(|| title_case(&self.harness.replace('-', " ")))
        })
    }
    /// The chip's second tone: the effort's label; before the catalog
    /// loads, the chat's reasoning as is.
    fn chip_suffix(&self) -> Option<String> {
        if let Some(effort) = self.effort() {
            return effort.labels.get(effort.selected).cloned();
        }
        let config = self.config();
        let known = self.catalog.with(|c| selected_model(c, &config).is_some());
        (!known)
            .then(|| str_at(&config, "reasoning").map(effort_label))
            .flatten()
    }
    fn rows(&self) -> Vec<ListRow> {
        let config = self.config();
        let selected = str_at(&config, "model")
            .map(str::to_owned)
            .or_else(|| self.catalog.with(|c| c.first().map(|m| m.id.clone())));
        let favorites = self.favorites.get();
        let query = self.query.get();
        self.catalog
            .with(|c| list_rows(c, selected.as_deref(), &favorites, &query))
    }
    fn selected_id(&self) -> Option<String> {
        let config = self.config();
        self.catalog
            .with(|c| selected_model(c, &config).map(|m| m.id.clone()))
            .or_else(|| str_at(&config, "model").map(str::to_owned))
    }
    fn budget(&self) -> f32 {
        menu_budget(self.trigger_top.get())
    }

    /// `update_chat_config`: change the effective config, show it at once
    /// and persist it.
    fn write(&self, change: impl FnOnce(&mut Value)) {
        let Some(chat) = self.rt.untracked(|| self.chat()) else {
            return;
        };
        let mut config = self.rt.untracked(|| self.config());
        if !config.is_object() {
            config = default_config(&chat);
        }
        change(&mut config);
        self.pending.set(Some(Pending {
            config: config.clone(),
            base: chat.config.clone(),
        }));
        send_config(&self.state, &self.id, config.clone());
        // No echo (a failed write): fall back to the row.
        let pending = self.pending.clone();
        self.tasks.spawn(async move {
            sleep(Duration::from_secs(5)).await;
            if pending.with_untracked(|p| p.as_ref().is_some_and(|p| p.config == config)) {
                pending.set(None);
            }
        });
    }
    fn pick_effort(&self, index: usize) {
        let Some(effort) = self.rt.untracked(|| self.effort()) else {
            return;
        };
        if index == effort.selected {
            return;
        }
        match effort.stops {
            Stops::Reasoning(levels) => {
                if let Some(level) = levels.get(index).cloned() {
                    self.write(|config| config["reasoning"] = json!(level));
                }
            }
            Stops::Option {
                id,
                choices,
                default,
            } => {
                if let Some(choice) = choices.get(index).cloned() {
                    let is_default = choice == default;
                    self.pick_option(&id, &choice, is_default);
                }
            }
        }
    }
    fn step_effort(&self, key: &Key) -> bool {
        let Some(effort) = self.rt.untracked(|| self.effort()) else {
            return false;
        };
        let last = effort.labels.len().saturating_sub(1);
        let next = match key {
            Key::ArrowLeft => effort.selected.saturating_sub(1),
            Key::ArrowRight => (effort.selected + 1).min(last),
            Key::Home => 0,
            Key::End => last,
            _ => return false,
        };
        self.pick_effort(next);
        true
    }
    /// `pick_option`: the default choice clears the key.
    fn pick_option(&self, option: &str, choice: &str, is_default: bool) {
        let (option, choice) = (option.to_owned(), choice.to_owned());
        self.write(move |config| {
            if !config["modelOptions"].is_object() {
                config["modelOptions"] = json!({});
            }
            let options = config["modelOptions"].as_object_mut().expect("object");
            if is_default {
                options.remove(&option);
            } else {
                options.insert(option, json!(choice));
            }
        });
    }
    fn toggle_fast(&self) {
        if let Some(fast) = self.rt.untracked(|| self.fast()) {
            self.pick_option(&fast.option, &fast.next, fast.next_default);
        }
    }
    /// `pick_model`: the effort re-clamps to the new ladder and options the
    /// model doesn't offer drop; then back to the panel.
    fn pick_model(&self, model: &str) {
        let info = self
            .catalog
            .with_untracked(|c| c.iter().find(|m| m.id == model).cloned());
        let model = model.to_owned();
        self.write(move |config| {
            config["model"] = json!(model);
            if let Some(info) = info {
                if !info.levels.is_empty() {
                    let current = str_at(config, "reasoning").map(str::to_owned);
                    if let Some(level) = effective_effort(current, &info.levels) {
                        config["reasoning"] = json!(level);
                    }
                }
                if let Some(options) = config["modelOptions"].as_object_mut() {
                    options.retain(|id, choice| {
                        info.options.iter().any(|o| {
                            o.id == *id
                                && choice
                                    .as_str()
                                    .is_some_and(|c| o.choices.iter().any(|x| x.id == c))
                        })
                    });
                }
            }
        });
        self.show_panel();
    }
    fn toggle_favorite(&self, model: &str) {
        self.favorites.update(|f| {
            if let Some(at) = f.iter().position(|m| m == model) {
                f.remove(at);
            } else {
                f.push(model.to_owned());
            }
        });
    }
    fn set_page(&self, page: Page) {
        if self.page.with_untracked(|p| *p == page) {
            return;
        }
        self.page.set(page);
        self.reveal.set(Tween {
            from: 0.,
            target: 1.,
            at: self.anim.now(),
        });
        self.anim.keep(RESIZE.0 + 0.05);
    }
    /// `show_compact_models`: open on the selected model.
    fn show_models(&self) {
        self.query.set(String::new());
        self.submenu.set(None);
        let (rows, selected, budget) = self
            .rt
            .untracked(|| (self.rows(), self.selected_id(), self.budget()));
        let at = rows
            .iter()
            .position(|r| Some(&r.id) == selected.as_ref())
            .unwrap_or(0);
        self.active.set(rows.get(at).map(|r| r.id.clone()));
        let viewport = list_height(budget, rows.len()) - 2. * CARD_INSET;
        self.offset
            .set(open_offset(&row_groups(&rows), at, viewport));
        self.set_page(Page::Models);
    }
    fn show_panel(&self) {
        self.set_page(Page::Panel);
    }
    /// Up / Down over the list, wrapping, keeping the row in view.
    fn step_active(&self, delta: isize) {
        let rows = self.rt.untracked(|| self.rows());
        if rows.is_empty() {
            return;
        }
        let current = self
            .active
            .with_untracked(|a| a.clone())
            .and_then(|id| rows.iter().position(|r| r.id == id));
        let len = rows.len() as isize;
        let next = match current {
            Some(ix) => (ix as isize + delta).rem_euclid(len) as usize,
            None if delta > 0 => 0,
            None => rows.len() - 1,
        };
        self.active.set(Some(rows[next].id.clone()));
        let pitch = COMPACT_ROW_HEIGHT + MENU_GAP;
        let budget = self.rt.untracked(|| self.budget());
        let viewport = list_height(budget, rows.len()) - 2. * CARD_INSET;
        let (top, bottom) = (next as f32 * pitch, next as f32 * pitch + pitch);
        let offset = self.offset.with_untracked(|o| *o);
        if top < offset {
            self.offset.set(top);
        } else if bottom > offset + viewport {
            self.offset.set(bottom - viewport);
        }
    }
    fn activate_active(&self) {
        let rows = self.rt.untracked(|| self.rows());
        let active = self.active.with_untracked(|a| a.clone());
        if let Some(row) = rows.iter().find(|r| Some(&r.id) == active.as_ref())
            && !row.selected_only
        {
            self.pick_model(&row.id);
        }
    }
    /// The card's keys (`compact_panel_key` and the list's): effort with
    /// Left / Right / Home / End, F for fast mode, Up / Down / Enter into
    /// the list; there, Up / Down / Enter pick and typing searches.
    fn key(&self, event: &mut EventContext) {
        if event.phase != EventPhase::Capture {
            return;
        }
        let page = self.page.with_untracked(|p| *p);
        let handled = match (&event.event, page) {
            (InputEvent::KeyDown { key, modifiers, .. }, Page::Panel) => match key {
                Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End => self.step_effort(key),
                Key::Character(c)
                    if c.eq_ignore_ascii_case("f")
                        && !(modifiers.meta || modifiers.control || modifiers.alt) =>
                {
                    self.toggle_fast();
                    true
                }
                Key::ArrowUp | Key::ArrowDown => {
                    self.show_models();
                    self.step_active(if *key == Key::ArrowUp { -1 } else { 1 });
                    true
                }
                Key::Enter | Key::Space => {
                    self.show_models();
                    true
                }
                _ => false,
            },
            (InputEvent::Text(_), Page::Panel) => true,
            (InputEvent::KeyDown { key, .. }, Page::Models) => match key {
                Key::ArrowUp => {
                    self.step_active(-1);
                    true
                }
                Key::ArrowDown => {
                    self.step_active(1);
                    true
                }
                Key::Enter => {
                    self.activate_active();
                    true
                }
                Key::Backspace if !self.search_focused.with_untracked(|f| *f) => {
                    self.query.update(|q| {
                        q.pop();
                    });
                    true
                }
                _ => false,
            },
            (InputEvent::Text(text_), Page::Models)
                if !self.search_focused.with_untracked(|f| *f) =>
            {
                let text_ = text_.clone();
                self.query.update(|q| q.push_str(&text_));
                true
            }
            _ => false,
        };
        if handled {
            event.prevent_default();
            event.stop_propagation();
        }
    }
}

fn picker(cx: &mut Context, state: &App, id: &str, harness: String, chip: Chip) -> View {
    let anim = state.anim.clone();
    let (models, catalog) = models_for(state, cx, &harness);
    let p = Picker {
        state: state.clone(),
        id: id.to_owned(),
        harness: harness.clone(),
        anim: anim.clone(),
        rt: cx.runtime(),
        tasks: cx.tasks(),
        catalog,
        favorites: favorites_for(cx, &harness),
        pending: cx.state(None),
        page: cx.state(Page::Panel),
        reveal: cx.state(Tween::settled(1.)),
        query: cx.state(String::new()),
        active: cx.state(None),
        offset: cx.state(0.),
        trigger_top: cx.state(f32::MAX),
        search_focused: cx.state(false),
        submenu: cx.state(None),
    };
    // Test hook: ZERON_MODEL=<model id> picks it once the list loads.
    if let Ok(wanted) = std::env::var("ZERON_MODEL") {
        let (p2, models) = (p.clone(), models.clone());
        let done = Cell::new(false);
        let effect = cx.runtime().effect(move || {
            let known = models.with(|m| m.iter().any(|(model, _, _)| *model == wanted));
            if known
                && !done.replace(true)
                && let Some(chat) = p2.rt.untracked(|| p2.chat())
            {
                println!(
                    "ZERON_MODEL switching {} from {:?} to {wanted}",
                    chat.id,
                    chat.model()
                );
                set_config(&p2.state, &p2.id, &chat, &[("model", json!(wanted))]);
            }
        });
        cx.retain(effect);
    }
    // A pending write retires once the row changes.
    {
        let p = p.clone();
        let effect = cx.runtime().effect(move || {
            let config = p.chat().map(|c| c.config);
            if p.pending.with_untracked(|pending| {
                pending
                    .as_ref()
                    .is_some_and(|x| Some(&x.base) != config.as_ref())
            }) {
                p.pending.set(None);
            }
        });
        cx.retain(effect);
    }
    // The chip's target width, measured as it lays out.
    {
        let (p, chip) = (p.clone(), chip.clone());
        let m = cx.text_measure();
        let effect = cx.runtime().effect(move || {
            let label_w = measure(&m, &p.model_label(), 12., 500, 16., None).0;
            let suffix_w = p
                .chip_suffix()
                .map(|e| measure(&m, &e, 12., 500, 16., None).0);
            chip.retarget(chip_width(label_w, suffix_w, p.fast_on()));
        });
        cx.retain(effect);
    }

    // Open / close with motion: the card plays MENU_IN on open and MENU_OUT
    // on close, so dismissals (backdrop, Escape) are held open for the exit
    // before the popover really closes. Picks keep it open, as Zeron does.
    let shown = cx.state(false);
    let phase = cx.state(Phase::Closed);
    let begin_close: Rc<dyn Fn()> = {
        let (shown, phase, anim, submenu) = (
            shown.clone(),
            phase.clone(),
            anim.clone(),
            p.submenu.clone(),
        );
        let tasks = cx.tasks();
        Rc::new(move || {
            if !matches!(phase.with_untracked(|p| *p), Phase::Open(_)) {
                return;
            }
            submenu.set(None);
            let at = anim.now();
            phase.set(Phase::Closing(at));
            anim.keep(MENU_OUT.0 + 0.05);
            let (shown, phase) = (shown.clone(), phase.clone());
            tasks.spawn(async move {
                sleep(Duration::from_secs_f32(MENU_OUT.0)).await;
                if phase.with_untracked(|p| *p == Phase::Closing(at)) {
                    // Closed first, so the hold below lets this one through.
                    phase.set(Phase::Closed);
                    shown.set(false);
                }
            });
        })
    };
    {
        let (shown, phase, begin_close) = (shown.clone(), phase.clone(), begin_close.clone());
        // A dismissal while open or exiting is held open for the exit.
        // Created before the popover mounts, so it runs ahead of its binding.
        let effect = cx.runtime().effect(move || {
            if shown.get() {
                return;
            }
            if phase.with_untracked(|p| *p != Phase::Closed) {
                begin_close();
                shown.set(true);
            }
        });
        cx.retain(effect);
    }
    // The card's height: its page's content within the budget, gliding
    // over 200 ms; it opens at its size.
    let height = cx.state(Tween::settled(0.));
    let target: Rc<dyn Fn() -> f32> = {
        let p = p.clone();
        Rc::new(move || {
            let content = match p.page.get() {
                Page::Panel => panel_height(p.effort().is_some(), p.groups().len()),
                Page::Models => models_height(p.rows().len()),
            };
            card_height(p.budget(), content)
        })
    };
    {
        let (anim, height, target) = (anim.clone(), height.clone(), target.clone());
        let first = Cell::new(true);
        let effect = cx.runtime().effect(move || {
            let target = target();
            if first.replace(false) {
                height.set(Tween::settled(target));
            } else {
                anim.retarget(&height, target, RESIZE);
            }
        });
        cx.retain(effect);
    }
    let open = {
        let (shown, phase, anim, begin_close, p, height) = (
            shown.clone(),
            phase.clone(),
            anim.clone(),
            begin_close.clone(),
            p.clone(),
            height.clone(),
        );
        let rt = cx.runtime();
        move || match phase.with_untracked(|p| *p) {
            Phase::Open(_) => begin_close(),
            _ => {
                p.page.set(Page::Panel);
                p.reveal.set(Tween::settled(1.));
                p.query.set(String::new());
                p.submenu.set(None);
                // Open at the content's size, not gliding from the last.
                height.set(Tween::settled(rt.untracked(|| target())));
                phase.set(Phase::Open(anim.now()));
                anim.keep(MENU_IN.0 + 0.05);
                shown.set(true);
            }
        }
    };

    let trigger = chip_view(cx, &p, &harness, phase.clone(), chip.clone(), open);

    // The frosted card (`popover_card_flush` in `frosted`): above the
    // chip, end-aligned, 6 px off; its blur sits over the composer's.
    let card = {
        let (phase, anim, keys) = (phase.clone(), anim.clone(), p.clone());
        let content = {
            let (p, reveal) = (p.clone(), p.reveal.clone());
            let anim = anim.clone();
            column()
                .w_full()
                .shrink_0()
                .reactive_style(move || Styles::new().opacity(anim.tween(&reveal, RESIZE)))
                .child(
                    switch(
                        {
                            let page = p.page.clone();
                            move || page.get()
                        },
                        {
                            let p = p.clone();
                            move |page, cx| match page {
                                Page::Panel => panel_view(&p),
                                Page::Models => models_view(cx, &p),
                            }
                        },
                    )
                    .w_full(),
                )
        };
        // An invisible anchor the card's width, ending at the chip's right
        // edge, so the start-aligned placement lands end-aligned.
        let anchor = div().w(CARD_WIDTH).h(32.);
        popover("Model", shown.clone(), anchor)
            .w(CARD_WIDTH)
            .p(0.)
            .gap(0.)
            .rounded(CARD_RADIUS)
            .border(1.)
            .border_color(BORDER)
            .bg(SURFACE_TINT)
            .text_size(13.)
            .text_color(TEXT)
            .overflow_hidden()
            .focus(|s| s)
            .reactive_style(move || {
                let (opacity, travel, blur) = card_motion(phase.get(), &anim);
                // Placement leaves 4 px above the anchor; Zeron's gap is 6.
                Styles::new()
                    .opacity(opacity)
                    .translate(0., travel - (CARD_GAP - 4.))
                    .blur(blur)
                    .h(anim.tween(&height, RESIZE))
            })
            .on_event(move |event| keys.key(event))
            .child(content)
    };
    row()
        .relative()
        .min_w(0.)
        .items_center()
        .child(trigger)
        .child(
            div()
                .absolute()
                .right(0.)
                .top(0.)
                .w(CARD_WIDTH)
                .h(32.)
                .child(card),
        )
}

/// The model chip (`trigger_chip`): mark, rolling label, the effort in a
/// muted second tone, then fast mode's bolt; a 150 ms hover wash (held
/// while open) and a 200 ms width glide.
fn chip_view(
    cx: &mut Context,
    p: &Picker,
    harness: &str,
    phase: Signal<Phase>,
    chip: Chip,
    open: impl FnMut() + 'static,
) -> View {
    let anim = p.anim.clone();
    let hover = Hover::new(cx, &anim);
    let blend = hover.clone();
    let label = {
        let p = p.clone();
        roll_text(cx, &anim, move || p.model_label(), 12., 500, 16.)
    };
    let suffix = {
        let p2 = p.clone();
        let rolled = roll_text(
            cx,
            &anim,
            move || p2.chip_suffix().unwrap_or_default(),
            12.,
            500,
            16.,
        );
        let p = p.clone();
        row()
            .min_w(0.)
            .flex_shrink(1000.)
            .text_color(alpha(TEXT_MUTED, 0.7))
            .reactive_style(move || {
                if p.chip_suffix().is_some() {
                    Styles::new().flex()
                } else {
                    Styles::new().hidden()
                }
            })
            .child(rolled)
    };
    // `motion::fast_tier`: the bolt pulses once over 700 ms as it appears.
    let bolt = {
        let since = cx.state(None::<f32>);
        {
            let (p, since, anim) = (p.clone(), since.clone(), anim.clone());
            let effect = cx.runtime().effect(move || {
                let on = p.fast_on();
                if on != since.with_untracked(Option::is_some) {
                    since.set(on.then(|| anim.now()));
                    if on {
                        anim.keep(0.75);
                    }
                }
            });
            cx.retain(effect);
        }
        div()
            .shrink_0()
            .child(icon("fast-tier-bold", 13., ACCENT))
            .reactive_style(move || match since.get() {
                Some(at) => {
                    let t = ((anim.at(at + 0.7) - at) / 0.7).clamp(0., 1.);
                    Styles::new()
                        .flex()
                        .opacity(1. - 0.35 * (1. - t) * (PI * t).sin())
                }
                None => Styles::new().hidden(),
            })
    };
    // 1 px strips over the chip: the press that opens the card tells its
    // top in the window, the height budget's reference.
    let probe = {
        let top = p.trigger_top.clone();
        div()
            .absolute()
            .left(0.)
            .right(0.)
            .top(0.)
            .bottom(0.)
            .children(
                (0..32)
                    .map(|i| {
                        let top = top.clone();
                        div()
                            .absolute()
                            .left(0.)
                            .right(0.)
                            .top(i as f32)
                            .h(1.)
                            .on_event(move |e| {
                                if let InputEvent::PointerDown { y, .. } = e.event
                                    && e.phase == EventPhase::Target
                                {
                                    top.set(y - i as f32 - 0.5);
                                }
                            })
                    })
                    .collect::<Vec<_>>(),
            )
    };
    hover.track(
        button()
            .relative()
            .h(32.)
            .px(6.)
            .py(0.)
            .gap(6.)
            .rounded(8.)
            .border(0.)
            .min_w(0.)
            .max_w_percent(100.)
            .items_center()
            .text_size(12.)
            .font_weight(500)
            .cursor(Cursor::Pointer)
            .hover(|s| s)
            .focus(|s| s)
            .reactive_style(move || {
                let t = blend.get();
                let open = matches!(phase.get(), Phase::Open(_));
                Styles::new()
                    .w(chip.now())
                    .text_color(mix(alpha(TEXT, 0.9), TEXT, t))
                    .bg(if open {
                        GLASS_HOVER
                    } else {
                        alpha(GLASS_HOVER, t)
                    })
            })
            .child(theme::harness_icon(harness, 16., TEXT_MUTED).shrink_0())
            .child(label)
            .child(suffix)
            .child(bolt)
            .child(probe)
            .on_click(open),
    )
}

/// The card's opacity, travel (toward the trigger, below it) and blur:
/// MENU_IN from 0.3 and 4 px; MENU_OUT to 0, retreating half the travel,
/// the blur riding the exit down.
fn card_motion(phase: Phase, anim: &Anim) -> (f32, f32, f32) {
    let progress = |at: f32, spec: Spec| spec.at(anim.at(at + spec.0) - at);
    match phase {
        Phase::Open(at) => {
            let t = progress(at, MENU_IN);
            (lerp(0.3, 1., t), MENU_TRAVEL * (1. - t), MENU_BLUR)
        }
        Phase::Closing(at) => {
            let t = progress(at, MENU_OUT);
            (1. - t, MENU_TRAVEL * 0.5 * t, MENU_BLUR * (1. - t))
        }
        Phase::Closed => (0., MENU_TRAVEL, 0.),
    }
}

// The panel page -----------------------------------------------------------------------

/// `render_compact_model_panel`: the header (effort over the model name,
/// fast mode beside it), the effort slider, then the option rows.
fn panel_view(p: &Picker) -> View {
    let key = {
        let p = p.clone();
        move || {
            (
                p.effort().is_some(),
                p.fast().is_some(),
                p.groups().into_iter().map(|g| g.id).collect::<Vec<_>>(),
            )
        }
    };
    let p = p.clone();
    switch(key, move |(effort, fast, groups), cx| {
        let mut panel = column()
            .w_full()
            .p(CARD_INSET)
            .child(panel_header(cx, &p, effort, fast));
        if effort {
            // The rail starts on the title's and the option rows' edge.
            panel = panel.child(
                div()
                    .w_full()
                    .px(8.)
                    .pt(SLIDER_TOP)
                    .pb(SLIDER_BOTTOM)
                    .child(effort_slider(cx, &p)),
            );
        }
        if !groups.is_empty() {
            panel = panel.child(options_view(cx, &p, groups, effort));
        }
        panel
    })
    .w_full()
}

fn panel_header(cx: &mut Context, p: &Picker, effort: bool, fast: bool) -> View {
    let anim = p.anim.clone();
    let hover = Hover::new(cx, &anim);
    let title = {
        let (blend, lean_blend, name_blend) = (hover.clone(), hover.clone(), hover.clone());
        let lean = move || EASE_OUT_QUINT.eval(lean_blend.get());
        let lean2 = lean.clone();
        let chevron = div()
            .shrink_0()
            .reactive_style(move || {
                let lean = lean();
                Styles::new()
                    .translate(3. * lean, 0.)
                    .opacity(0.55 + 0.45 * lean)
            })
            .child(icon_with("alt-arrow-right", 10., move || {
                mix(pop_muted(), TEXT, lean2())
            }));
        let name = {
            let p = p.clone();
            text_signal(move || p.model_label()).min_w(0.).truncate()
        };
        let mut name_row = row()
            .max_w_percent(100.)
            .min_w(0.)
            .items_center()
            .gap(3.)
            .child(name)
            .child(chevron);
        name_row = if effort {
            name_row
                .text_size(12.)
                .line_height(15.)
                .font_weight(400)
                .reactive_style(move || {
                    Styles::new().text_color(mix(pop_muted(), TEXT, name_blend.get()))
                })
        } else {
            // The name is the whole title: no effort above it.
            name_row
                .text_size(14.)
                .line_height(17.)
                .font_weight(600)
                .text_color(TEXT)
        };
        let mut title = column()
            .grow()
            .min_w(0.)
            .h_full()
            .px(8.)
            .rounded(MENU_ITEM_RADIUS)
            .items_start()
            .justify_center()
            .cursor(Cursor::Pointer)
            .reactive_style(move || Styles::new().bg(ink(0.05 * blend.get())));
        if effort {
            // The slider retitles the panel as it drags; the level rolls.
            let p = p.clone();
            let level = roll_text(
                cx,
                &anim,
                move || {
                    p.effort()
                        .and_then(|e| e.labels.get(e.selected).cloned())
                        .unwrap_or_else(|| "Default".into())
                },
                14.,
                600,
                17.,
            );
            title = title.child(
                row()
                    .max_w_percent(100.)
                    .min_w(0.)
                    .text_size(14.)
                    .line_height(17.)
                    .font_weight(600)
                    .text_color(TEXT)
                    .child(level),
            );
        }
        let p = p.clone();
        hover.track(title.child(name_row).on_click(move || p.show_models()))
    };
    let mut header = row()
        .w_full()
        .h(if effort {
            HEADER_HEIGHT
        } else {
            HEADER_HEIGHT_SINGLE
        })
        .shrink_0()
        .gap(CARD_INSET)
        .child(title);
    if fast {
        header = header.child(fast_button(cx, p));
    }
    header
}

/// Fast mode: bare until hovered; the bolt's colour is the state, the
/// solid twin fading in over the outline.
fn fast_button(cx: &mut Context, p: &Picker) -> View {
    let anim = p.anim.clone();
    let hover = Hover::new(cx, &anim);
    let on = cx.state(Tween::settled(if p.rt.untracked(|| p.fast_on()) {
        1.
    } else {
        0.
    }));
    {
        let (p, on, anim) = (p.clone(), on.clone(), anim.clone());
        let effect = cx.runtime().effect(move || {
            let target = if p.fast_on() { 1. } else { 0. };
            anim.retarget(&on, target, RESIZE);
        });
        cx.retain(effect);
    }
    let color = {
        let (hover, on, anim) = (hover.clone(), on.clone(), anim.clone());
        move || {
            mix(
                mix(pop_muted(), TEXT, hover.get()),
                ACCENT,
                anim.tween(&on, RESIZE),
            )
        }
    };
    let solid = {
        let (color, on, anim) = (color.clone(), on.clone(), anim.clone());
        icon_with("fast-tier-bold", 15., move || {
            alpha(color(), anim.tween(&on, RESIZE))
        })
        .absolute()
        .left(0.)
        .top(0.)
    };
    let blend = hover.clone();
    let p = p.clone();
    hover.track(
        div()
            .w(FAST_BUTTON_WIDTH)
            .h_full()
            .shrink_0()
            .rounded(MENU_ITEM_RADIUS)
            .items_center()
            .justify_center()
            .cursor(Cursor::Pointer)
            .reactive_style(move || Styles::new().bg(ink(0.05 * blend.get())))
            .child(
                div()
                    .relative()
                    .size(15., 15.)
                    .child(icon_with("fast-tier", 15., color))
                    .child(solid),
            )
            .on_click(move || p.toggle_fast()),
    )
}

/// The shimmer clock, integrated per frame so a changing intensity never
/// jumps the band.
#[derive(Default)]
struct ShimmerClock {
    phase: Cell<f32>,
    last: Cell<Option<f32>>,
}

impl ShimmerClock {
    fn sample(&self, now: f32, intensity: f32) -> f32 {
        let alive = intensity > 0.001;
        let last = self.last.replace(alive.then_some(now));
        if alive && let Some(last) = last {
            let dt = (now - last).clamp(0., 0.05);
            self.phase
                .set((self.phase.get() + dt * (0.32 + 0.22 * intensity)).fract());
        }
        self.phase.get()
    }
}

/// The effort slider: a light glass rail, the accent fill to just past the
/// thumb, quiet stops, and a white pill thumb; draggable, eased between
/// stops over 200 ms. Fast mode adds a flowing band and a breathing glow.
fn effort_slider(cx: &mut Context, p: &Picker) -> View {
    let anim = p.anim.clone();
    let width = cx.state(0_f32);
    let dragging = cx.state(false);
    let initial =
        p.rt.untracked(|| p.effort())
            .map_or(0., |e| stop_fraction(e.selected, e.labels.len()));
    let fraction = cx.state(Tween::settled(initial));
    let press = cx.state(Tween::settled(0.));
    let energy = cx.state(Tween::settled(if p.rt.untracked(|| p.fast_on()) {
        1.
    } else {
        0.
    }));
    // The native slider underneath does the pointer maths (its value is
    // where along its own rail the pointer sits).
    let value = cx.state(0_f32);
    let shimmer = Rc::new(ShimmerClock::default());

    // At rest the fill eases to the selected stop; a release eases from
    // where the pointer left it.
    {
        let (p, dragging, fraction, anim) =
            (p.clone(), dragging.clone(), fraction.clone(), anim.clone());
        let effect = cx.runtime().effect(move || {
            let target = p
                .effort()
                .map_or(0., |e| stop_fraction(e.selected, e.labels.len()));
            if !dragging.get() {
                anim.retarget(&fraction, target, RESIZE);
            }
        });
        cx.retain(effect);
    }
    {
        let (dragging, press, anim) = (dragging.clone(), press.clone(), anim.clone());
        let effect = cx.runtime().effect(move || {
            anim.retarget(&press, if dragging.get() { 1. } else { 0. }, RESIZE);
        });
        cx.retain(effect);
    }
    // Fast mode powers the fill up over 200 ms and keeps frames coming.
    {
        let (p, energy, anim) = (p.clone(), energy.clone(), anim.clone());
        let tasks = cx.tasks();
        let running = Rc::new(Cell::new(false));
        let effect = cx.runtime().effect(move || {
            let on = p.fast_on();
            anim.retarget(&energy, if on { 1. } else { 0. }, RESIZE);
            if on && !running.replace(true) {
                let (p, anim, running) = (p.clone(), anim.clone(), running.clone());
                tasks.spawn(async move {
                    while p.rt.untracked(|| p.fast_on()) {
                        anim.keep(0.3);
                        sleep(Duration::from_millis(200)).await;
                    }
                    running.set(false);
                });
            }
        });
        cx.retain(effect);
    }
    // A drag follows the pointer and picks the nearest stop as it goes.
    {
        let (p, dragging, fraction, width, value) = (
            p.clone(),
            dragging.clone(),
            fraction.clone(),
            width.clone(),
            value.clone(),
        );
        let effect = cx.runtime().effect(move || {
            let v = value.get();
            if !dragging.with_untracked(|d| *d) {
                return;
            }
            let w = width.with_untracked(|w| *w);
            p.rt.untracked(|| {
                let Some(effort) = p.effort() else {
                    return;
                };
                let count = effort.labels.len();
                let x = slider_local_x(v, w);
                let at = if count > 1 { effort_fraction(x, w) } else { 0. };
                fraction.set(Tween::settled(at));
                if let Some(index) = effort_index(x, w, count) {
                    p.pick_effort(index);
                }
            });
        });
        cx.retain(effect);
    }

    let shown_fraction = {
        let (fraction, anim) = (fraction.clone(), anim.clone());
        move || anim.tween(&fraction, RESIZE)
    };
    // (intensity, phase, breath), the clock tracked while fast mode lives.
    let live = {
        let (energy, anim, shimmer) = (energy.clone(), anim.clone(), shimmer.clone());
        move || {
            let intensity = anim.tween(&energy, RESIZE);
            if intensity <= 0.001 {
                return (0., shimmer.sample(anim.now(), 0.), 0.5);
            }
            let now = anim.clock.get();
            let phase = shimmer.sample(now, intensity);
            (intensity, phase, 0.5 + 0.5 * (TAU * phase).sin())
        }
    };

    let probe = {
        let width = width.clone();
        canvas(move |(w, _)| {
            if width.with_untracked(|x| (x - w).abs() > 0.5) {
                width.set(w);
            }
            Canvas::new()
        })
        .absolute()
        .left(0.)
        .right(0.)
        .top(0.)
        .h(1.)
    };
    let rail = light_plate(
        div()
            .absolute()
            .left(0.)
            .right(0.)
            .top(RAIL_TOP)
            .h(RAIL_HEIGHT),
        RAIL_HEIGHT / 2.,
    );
    // `accent_plate(theme, 1, glow)` (dark): the accent lifted 6% at the
    // top, a soft lifted rim, a 12% inset highlight, a halo only with glow.
    let fill = {
        let (fraction, live, width) = (shown_fraction.clone(), live.clone(), width.clone());
        let radius = RAIL_HEIGHT / 2.;
        div()
            .absolute()
            .left(0.)
            .top(RAIL_TOP)
            .h(RAIL_HEIGHT)
            .rounded(radius)
            .bg_gradient(FRAC_PI_2, vertical(lift(ACCENT, 0.06), ACCENT))
            .reactive_style(move || {
                let (intensity, _, breath) = live();
                let glow = intensity * (0.2 + 0.25 * breath);
                Styles::new()
                    .w(fill_width(fraction(), width.get()))
                    .shadow(drop(
                        alpha(ACCENT, 0.3 * glow),
                        2. + 2. * glow,
                        6. + 14. * glow,
                    ))
            })
            .child(plate_ring(radius, alpha(lift(ACCENT, 0.35), 0.35), false))
            .child(plate_ring(radius, ink(0.12), true))
    };
    // Fast mode's band through the fill, then the stops over rail and fill.
    let marks = {
        let (fraction, live, p) = (shown_fraction.clone(), live.clone(), p.clone());
        canvas(move |(w, _)| {
            let mut canvas = Canvas::new();
            let fraction = fraction();
            let (intensity, phase, breath) = live();
            let count = p.effort().map_or(0, |e| e.labels.len());
            let center = thumb_center(fraction, w);
            let fill_end = fill_width(fraction, w);
            if intensity > 0.001 && fill_end > RAIL_HEIGHT {
                let inset = 1.;
                let height = RAIL_HEIGHT - 2. * inset;
                let (from, to) = (inset, fill_end - inset);
                let mut edges = vec![from, (from + height / 2.).min(to)];
                let inner_end = (to - height / 2.).max(edges[1]);
                let mut x = edges[1];
                while x + 4. < inner_end {
                    x += 4.;
                    edges.push(x);
                }
                for end in [inner_end, to] {
                    if end > *edges.last().expect("edge") {
                        edges.push(end);
                    }
                }
                let stops: Vec<GradientStop> = edges
                    .iter()
                    .map(|x| GradientStop {
                        offset: ((x - from) / (to - from)).clamp(0., 1.),
                        color: shimmer_color(*x, from, center, phase, breath, intensity),
                    })
                    .collect();
                if let (Ok(brush), Some(path)) = (
                    Brush::linear(
                        Point::new(from, RAIL_TOP + inset),
                        Point::new(to, RAIL_TOP + inset),
                        stops,
                    ),
                    rounded_rect(from, RAIL_TOP + inset, to - from, height, height / 2.),
                ) {
                    canvas.fill(path, brush);
                }
            }
            // Quiet stops: filled ones follow the animated front.
            for i in 0..count {
                let stop = stop_fraction(i, count);
                let x = thumb_center(stop, w);
                let color = if stop <= fraction {
                    ink(0.4)
                } else {
                    alpha(pop_muted(), 0.28)
                };
                if let Some(dot) = rounded_rect(x - 2., SLIDER_HEIGHT / 2. - 2., 4., 4., 2.) {
                    canvas.fill(dot, color);
                }
            }
            canvas
        })
        .absolute()
        .left(0.)
        .right(0.)
        .top(0.)
        .bottom(0.)
    };
    // A plain handle that keeps its size; a press lifts it on a softer,
    // wider drop, and fast mode breathes an accent halo under it.
    let thumb = {
        let place = {
            let (fraction, width) = (shown_fraction.clone(), width.clone());
            move || fraction() * (width.get() - 2. * THUMB_INSET).max(0.) - THUMB_WIDTH / 2.
        };
        let pill = || {
            div()
                .absolute()
                .top(0.)
                .w(THUMB_WIDTH)
                .h(THUMB_HEIGHT)
                .rounded(THUMB_HEIGHT / 2.)
        };
        let halo = {
            let (place, live) = (place.clone(), live.clone());
            pill().reactive_style(move || {
                let (intensity, _, breath) = live();
                let live = intensity * (0.55 + 0.45 * breath);
                if intensity > 0.001 {
                    Styles::new().flex().left(place()).shadow(drop(
                        alpha(ACCENT, 0.35 * live),
                        1.,
                        10.,
                    ))
                } else {
                    Styles::new().hidden()
                }
            })
        };
        let lift_ = {
            let (place, press, anim) = (place.clone(), press.clone(), anim.clone());
            pill().reactive_style(move || {
                let press = anim.tween(&press, RESIZE);
                if press > 0.001 {
                    Styles::new().flex().left(place()).shadow(drop(
                        black(0.12 * press),
                        2. * press,
                        8.,
                    ))
                } else {
                    Styles::new().hidden()
                }
            })
        };
        let handle = thumb_plate(pill(), THUMB_HEIGHT / 2.)
            .reactive_style(move || Styles::new().left(place()));
        div()
            .absolute()
            .left(THUMB_INSET)
            .right(THUMB_INSET)
            .top((SLIDER_HEIGHT - THUMB_HEIGHT) / 2.)
            .h(THUMB_HEIGHT)
            .child(halo)
            .child(lift_)
            .child(handle)
    };
    // The native slider takes the pointer (and its capture) on top, all
    // but invisible: zgx hit-tests only painted opacity.
    let input = slider("Reasoning effort", value, 0.0..=1.0)
        .absolute()
        .left(0.)
        .top(0.)
        .w_full()
        .h_full()
        .p(0.)
        .rounded(0.)
        .focus(|s| s)
        .opacity(0.001)
        .cursor(Cursor::Pointer);
    div()
        .relative()
        .w_full()
        .h(SLIDER_HEIGHT)
        .cursor(Cursor::Pointer)
        .on_event(move |e| {
            if e.phase != EventPhase::Capture {
                return;
            }
            match e.event {
                InputEvent::PointerDown {
                    button: PointerButton::Primary,
                    ..
                } => {
                    dragging.set(true);
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
                | InputEvent::PointerCancel => {
                    dragging.set(false);
                }
                _ => {}
            }
        })
        .child(probe)
        .child(rail)
        .child(fill)
        .child(marks)
        .child(thumb)
        .child(input)
}

/// A rounded rectangle path (the radius clamped to fit).
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    if w <= 0. || h <= 0. {
        return None;
    }
    let r = r.min(w / 2.).min(h / 2.).max(0.);
    let mut path = Path::builder();
    path.arc(Point::new(x + r, y + r), r, PI, FRAC_PI_2)
        .arc(Point::new(x + w - r, y + r), r, -FRAC_PI_2, FRAC_PI_2)
        .arc(Point::new(x + w - r, y + h - r), r, 0., FRAC_PI_2)
        .arc(Point::new(x + r, y + h - r), r, FRAC_PI_2, FRAC_PI_2)
        .close();
    path.build().ok()
}

/// The option rows (`render_setting_rows` in the compact tray): 26 px
/// rows, label, value, chevron; each opens its choices beside it.
fn options_view(cx: &mut Context, p: &Picker, groups: Vec<String>, effort: bool) -> View {
    let count = groups.len();
    let rows: Vec<View> = groups.into_iter().map(|id| option_row(cx, p, id)).collect();
    let chrome =
        CARD_BORDERS + header_block(effort) + if effort { EFFORT_BLOCK } else { 0. } + OPTIONS_GAP;
    let offset = cx.state(0_f32);
    let p = p.clone();
    let fade_offset = offset.clone();
    // Space, not a rule, separates the options from the effort group; the
    // list bleeds to the card edge and re-pads.
    div().w_full().mt(OPTIONS_GAP).child(
        scroll(offset)
            .mx(-CARD_INSET)
            .px(CARD_INSET)
            .reactive_style(move || {
                let max = options_height(count).min((p.budget() - chrome).max(0.));
                let content = 2. + count as f32 * (OPTION_ROW + MENU_GAP);
                let (top, bottom) = edge_fades(fade_offset.get(), content, max);
                Styles::new().h(max).fade_edges(top, bottom)
            })
            .child(
                column()
                    .w_full()
                    .pt(CARD_INSET)
                    .gap(MENU_GAP)
                    .children(rows),
            ),
    )
}

/// The 12 px overflow fades: each edge fades over what is scrolled past it.
fn edge_fades(offset: f32, content: f32, viewport: f32) -> (f32, f32) {
    let rest = (content - viewport - offset).max(0.);
    (offset.clamp(0., 12.), rest.min(12.))
}

fn option_row(cx: &mut Context, p: &Picker, id: String) -> View {
    let anim = p.anim.clone();
    let hover = Hover::new(cx, &anim);
    let open = cx.state(false);
    // One choice list at a time; a dismissed one lets go of the slot.
    {
        let (submenu, open, id) = (p.submenu.clone(), open.clone(), id.clone());
        let effect = cx.runtime().effect(move || {
            let current = submenu.get();
            if current.as_deref() != Some(id.as_str()) && open.with_untracked(|o| *o) {
                open.set(false);
            }
        });
        cx.retain(effect);
    }
    let opened_at = cx.state(0_f32);
    {
        let (submenu, open, id, opened_at, anim) = (
            p.submenu.clone(),
            open.clone(),
            id.clone(),
            opened_at.clone(),
            anim.clone(),
        );
        let effect = cx.runtime().effect(move || {
            if open.get() {
                opened_at.set(anim.now());
                anim.keep(MENU_IN.0 + 0.05);
            } else if submenu.with_untracked(|s| s.as_deref() == Some(id.as_str())) {
                submenu.set(None);
            }
        });
        cx.retain(effect);
    }
    let label = {
        let (p, id) = (p.clone(), id.clone());
        text_signal(move || p.group(&id).map(|g| g.label).unwrap_or_default())
            .grow()
            .min_w(0.)
            .truncate()
    };
    let value = {
        let (p, id) = (p.clone(), id.clone());
        text_signal(move || {
            p.group(&id)
                .and_then(|g| g.choices.into_iter().find(|c| c.selected))
                .map(|c| c.label)
                .unwrap_or_default()
        })
        .max_w(100.)
        .truncate()
        .text_color(pop_muted())
    };
    let choices = {
        let (p, id, open) = (p.clone(), id.clone(), open.clone());
        let anim = anim.clone();
        switch(
            {
                let (p, id) = (p.clone(), id.clone());
                move || p.group(&id).map(|g| g.choices).unwrap_or_default()
            },
            move |choices, cx| {
                let rows: Vec<View> = choices
                    .into_iter()
                    .map(|choice| {
                        let (p, id, open) = (p.clone(), id.clone(), open.clone());
                        let mut content = row()
                            .w_full()
                            .min_w(0.)
                            .items_center()
                            .gap(10.)
                            .child(text(choice.label.clone()).min_w(0.).truncate())
                            .child(div().grow());
                        if choice.default {
                            content = content.child(
                                text("Default")
                                    .text_size(10.)
                                    .font_weight(600)
                                    .text_color(pop_muted())
                                    .shrink_0(),
                            );
                        }
                        if choice.selected {
                            content = content.child(icon("check", 14., TEXT).shrink_0());
                        }
                        menu_row(cx, &anim, choice.selected, 30., content, move || {
                            p.pick_option(&id, &choice.id, choice.default);
                            open.set(false);
                        })
                    })
                    .collect();
                scroll(cx.state(0.))
                    .w_full()
                    .max_h(240.)
                    .child(column().w_full().gap(MENU_GAP).children(rows))
            },
        )
        .w_full()
    };
    // `nested_menu` on the left: its right edge 10 px off the row's left,
    // tops aligned (zgx places it 4 below the 1 px anchor).
    let submenu = {
        let anchor = div().w(SUBMENU_WIDTH).h(1.);
        let anim = anim.clone();
        div()
            .absolute()
            .left(-(SUBMENU_WIDTH + CARD_INSET + 6.))
            .top(-5.)
            .w(SUBMENU_WIDTH)
            .h(1.)
            .child(
                popover("Choices", open.clone(), anchor)
                    .w(SUBMENU_WIDTH)
                    .p(CARD_INSET)
                    .gap(MENU_GAP)
                    .rounded(CARD_RADIUS)
                    .border(1.)
                    .border_color(BORDER)
                    .bg(SURFACE_TINT)
                    .blur(MENU_BLUR)
                    .text_size(13.)
                    .text_color(TEXT)
                    .overflow_hidden()
                    .focus(|s| s)
                    .reactive_style(move || {
                        // `menu_in`: 0.3 -> 1 and 2 px down into place.
                        let at = opened_at.get();
                        let t = MENU_IN.at(anim.at(at + MENU_IN.0) - at);
                        Styles::new()
                            .opacity(lerp(0.3, 1., t))
                            .translate(0., -2. * (1. - t))
                    })
                    .child(choices),
            )
    };
    let blend = hover.clone();
    let shown = open.clone();
    let (submenu_slot, id_) = (p.submenu.clone(), id.clone());
    hover.track(
        row()
            .relative()
            .w_full()
            .h(OPTION_ROW)
            .px(8.)
            .py(0.)
            .gap(10.)
            .rounded(MENU_ITEM_RADIUS)
            .min_w(0.)
            .shrink_0()
            .items_center()
            .text_size(13.)
            .cursor(Cursor::Pointer)
            .reactive_style(move || {
                if shown.get() {
                    Styles::new().bg(wash(0.11)).text_color(TEXT)
                } else {
                    let t = blend.get();
                    Styles::new()
                        .bg(mix(wash(0.), wash(0.11), t))
                        .text_color(mix(alpha(TEXT, 0.9), TEXT, t))
                }
            })
            .child(label)
            .child(value)
            .child(icon("alt-arrow-right", 12., pop_muted()).shrink_0())
            .child(submenu)
            .on_click(move || {
                let next = !open.with_untracked(|o| *o);
                if next {
                    submenu_slot.set(Some(id_.clone()));
                }
                open.set(next);
            }),
    )
}

/// `popover::menu_row`: `px 8`, `py 0` at a fixed height, radius 7, gap
/// 10; the active row takes the selected wash, others fade it in.
fn menu_row(
    cx: &mut Context,
    anim: &Anim,
    active: bool,
    height: f32,
    content: View,
    action: impl FnMut() + 'static,
) -> View {
    let hover = Hover::new(cx, anim);
    let blend = hover.clone();
    hover.track(
        row()
            .w_full()
            .h(height)
            .px(8.)
            .py(0.)
            .gap(10.)
            .rounded(MENU_ITEM_RADIUS)
            .min_w(0.)
            .shrink_0()
            .items_center()
            .text_size(13.)
            .cursor(Cursor::Pointer)
            .reactive_style(move || {
                if active {
                    Styles::new().bg(wash(0.11)).text_color(TEXT)
                } else {
                    let t = blend.get();
                    Styles::new()
                        .bg(mix(wash(0.), wash(0.11), t))
                        .text_color(mix(alpha(TEXT, 0.9), TEXT, t))
                }
            })
            .child(content)
            .on_click(action),
    )
}

// The model list page --------------------------------------------------------------

/// `render_harness_model_popover` in compact form: back and the provider
/// strip, the search row, then the list.
fn models_view(cx: &mut Context, p: &Picker) -> View {
    column()
        .w_full()
        .child(back_header(p))
        .child(search_row(cx, p))
        .child(model_list(cx, p))
}

fn back_header(p: &Picker) -> View {
    let back = {
        let p = p.clone();
        div()
            .size(COMPACT_ROW_HEIGHT, COMPACT_ROW_HEIGHT)
            .shrink_0()
            .rounded(MENU_ITEM_RADIUS)
            .items_center()
            .justify_center()
            .cursor(Cursor::Pointer)
            .hover(|s| s.bg(ink(0.05)))
            .child(icon("alt-arrow-left", 14., pop_muted()))
            .on_click(move || p.show_panel())
    };
    // The group whose rows sit at the top of the scroll lights its tab.
    let current = {
        let p = p.clone();
        move || -> Option<bool> {
            let rows = p.rows();
            let top = (p.offset.get() / (COMPACT_ROW_HEIGHT + MENU_GAP))
                .round()
                .max(0.) as usize;
            row_groups(&rows)
                .iter()
                .rev()
                .find(|(_, start)| *start <= top)
                .map(|(starred, _)| *starred)
        }
    };
    let strip = {
        let (p, keys) = (p.clone(), p.clone());
        keyed(
            move || {
                if keys.query.with(|q| q.trim().is_empty()) {
                    row_groups(&keys.rows())
                } else {
                    Vec::new()
                }
            },
            move |(starred, start), cx| {
                let hovered = cx.state(false);
                let (current, hovered_, p) = (current.clone(), hovered.clone(), p.clone());
                let harness = p.harness.clone();
                let viewed = {
                    let current = current.clone();
                    move || current() == Some(starred)
                };
                let viewed2 = viewed.clone();
                let glyph = if starred {
                    icon_with(
                        "star-bold",
                        14.,
                        move || {
                            if viewed2() { TEXT } else { pop_muted() }
                        },
                    )
                } else {
                    let (name, tint) = theme::harness_mark(&harness);
                    icon_with(name, 14., move || {
                        tint.unwrap_or(if viewed2() { TEXT } else { pop_muted() })
                    })
                };
                div()
                    .size(COMPACT_ROW_HEIGHT, COMPACT_ROW_HEIGHT)
                    .shrink_0()
                    .rounded(MENU_ITEM_RADIUS)
                    .items_center()
                    .justify_center()
                    .cursor(Cursor::Pointer)
                    .reactive_style(move || {
                        if viewed() {
                            Styles::new().bg(ink(0.08)).opacity(1.)
                        } else if hovered_.get() {
                            Styles::new().bg(ink(0.05)).opacity(1.)
                        } else {
                            Styles::new().bg(ink(0.)).opacity(0.7)
                        }
                    })
                    .on_event(move |e| match e.event {
                        InputEvent::PointerEnter => {
                            hovered.set(true);
                        }
                        InputEvent::PointerLeave | InputEvent::PointerCancel => {
                            hovered.set(false);
                        }
                        _ => {}
                    })
                    .child(glyph)
                    .on_click(move || {
                        let rows = p.rt.untracked(|| p.rows());
                        p.active.set(rows.get(start).map(|r| r.id.clone()));
                        p.offset.set(start as f32 * (COMPACT_ROW_HEIGHT + MENU_GAP));
                    })
            },
        )
    };
    // The card's inset around one row-height strip.
    row()
        .w_full()
        .h(COMPACT_ROW_HEIGHT + 2. * CARD_INSET)
        .shrink_0()
        .px(CARD_INSET)
        .items_center()
        .gap(MENU_GAP)
        .child(back)
        .child(
            row()
                .grow()
                .min_w(0.)
                .h_full()
                .items_center()
                .gap(MENU_GAP)
                .overflow_hidden()
                .child(strip),
        )
}

/// The search row: a magnifier and a borderless input over a hairline.
fn search_row(cx: &mut Context, p: &Picker) -> View {
    let _ = cx;
    let placeholder = {
        let query = p.query.clone();
        text("Search models…")
            .text_size(13.)
            .text_color(TEXT_FAINT)
            .truncate()
            .reactive_style(move || {
                if query.with(String::is_empty) {
                    Styles::new().flex()
                } else {
                    Styles::new().hidden()
                }
            })
    };
    let focused = p.search_focused.clone();
    let input = text_input("Search models", p.query.clone())
        .w_full()
        .font_family(theme::SANS)
        .text_size(13.)
        .text_color(TEXT)
        .bg(alpha(TEXT, 0.))
        .border(0.)
        .rounded(0.)
        .p(0.)
        .focus(|s| s)
        .on_event(move |e| match e.event {
            InputEvent::Focus => {
                focused.set(true);
            }
            InputEvent::Blur => {
                focused.set(false);
            }
            _ => {}
        });
    row()
        .w_full()
        .h(40.)
        .shrink_0()
        .px(10.)
        .gap(8.)
        .items_center()
        .border_edges(Insets {
            top: 0.,
            right: 0.,
            bottom: 1.,
            left: 0.,
        })
        .border_color(hairline(0.08))
        .child(icon("magnifer", 14., pop_muted()).shrink_0())
        .child(
            overlay()
                .grow()
                .min_w(0.)
                .items_center()
                .child(placeholder)
                .child(input),
        )
}

/// The list: a whisper of wash under 32 px rows on a 34 px pitch, faded at
/// the edges it scrolls past; skeletons while loading.
fn model_list(cx: &mut Context, p: &Picker) -> View {
    let state_key = {
        let p = p.clone();
        move || {
            if !p.rows().is_empty() {
                0
            } else if !p.query.with(|q| q.trim().is_empty()) {
                1
            } else {
                2
            }
        }
    };
    let body = {
        let p = p.clone();
        switch(state_key, move |state, cx| match state {
            0 => {
                let (keys, p2) = (p.clone(), p.clone());
                let rows = keyed(move || keys.rows(), move |row, cx| model_row(cx, &p2, row));
                let (p3, offset) = (p.clone(), p.offset.clone());
                scroll(p.offset.clone())
                    .w_full()
                    .h_full()
                    .px(CARD_INSET)
                    .reactive_style(move || {
                        let count = p3.rows().len();
                        let viewport = list_height(p3.budget(), count) - 2. * CARD_INSET;
                        let content = count as f32 * (COMPACT_ROW_HEIGHT + MENU_GAP);
                        let (top, bottom) = edge_fades(offset.get(), content, viewport);
                        Styles::new().fade_edges(top, bottom)
                    })
                    .child(column().w_full().child(rows))
            }
            1 => text("No models found")
                .w_full()
                .px(8.)
                .py(24.)
                .text_size(12.)
                .text_color(pop_muted())
                .text_align(TextAlign::Center),
            _ => {
                // `skeleton_menu_rows`: ghost bars in the rows' places.
                let _ = &cx;
                column().w_full().px(CARD_INSET).gap(MENU_GAP).children(
                    [0.62_f32, 0.48, 0.56, 0.4, 0.52]
                        .into_iter()
                        .map(|share| {
                            row()
                                .w_full()
                                .h(COMPACT_ROW_HEIGHT)
                                .px(8.)
                                .gap(8.)
                                .items_center()
                                .child(div().size(14., 14.).rounded(4.).bg(ink(0.06)))
                                .child(
                                    div()
                                        .w_percent(share * 100.)
                                        .h(10.)
                                        .rounded(4.)
                                        .bg(ink(0.06)),
                                )
                        })
                        .collect::<Vec<_>>(),
                )
            }
        })
        .w_full()
        .h_full()
    };
    let _ = cx;
    let p = p.clone();
    div()
        .w_full()
        .shrink_0()
        .py(CARD_INSET)
        .bg(ink(0.02))
        .overflow_hidden()
        .reactive_style(move || Styles::new().h(list_height(p.budget(), p.rows().len())))
        .child(body)
}

/// `render_compact_model_row`: brand mark, name (and the description only
/// to tell identical names apart), and a star that shows on hover.
fn model_row(cx: &mut Context, p: &Picker, data: ListRow) -> View {
    let _ = cx;
    let selected = {
        let (p, id) = (p.clone(), data.id.clone());
        move || p.selected_id().as_deref() == Some(id.as_str())
    };
    let active = {
        let (active, id) = (p.active.clone(), data.id.clone());
        move || active.with(|a| a.as_deref() == Some(id.as_str()))
    };
    let (name, tint) = theme::harness_mark(&p.harness);
    let mut text_ = row()
        .grow()
        .min_w(0.)
        .items_baseline()
        .gap(6.)
        .text_size(12.)
        .child(
            text(data.label.clone())
                .font_weight(500)
                .shrink_0()
                .max_w_percent(100.)
                .truncate(),
        );
    if data.ambiguous
        && let Some(description) = data
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
    {
        text_ = text_.child(
            text(description)
                .min_w(0.)
                .truncate()
                .text_size(11.)
                .text_color(pop_muted()),
        );
    }
    let star = {
        let (p, id, active) = (p.clone(), data.id.clone(), active.clone());
        let favorite = data.favorite;
        div()
            .size(24., 24.)
            .shrink_0()
            .rounded(6.)
            .items_center()
            .justify_center()
            .cursor(Cursor::Pointer)
            .hover(|s| s.bg(ink(0.08)))
            // Quiet until the row is hovered or the model is starred.
            .reactive_style(move || {
                Styles::new().opacity(if favorite || active() { 1. } else { 0. })
            })
            .child(if favorite {
                icon("star-bold", 13., ACCENT)
            } else {
                icon("star", 13., pop_muted())
            })
            .on_click(move || p.toggle_favorite(&id))
    };
    let ring = {
        let selected = selected.clone();
        plate_ring(MENU_ITEM_RADIUS, hairline(0.09), false).reactive_style(move || {
            if selected() {
                Styles::new().flex()
            } else {
                Styles::new().hidden()
            }
        })
    };
    let (enter, pick, id) = (p.active.clone(), p.clone(), data.id.clone());
    let selected_only = data.selected_only;
    div().w_full().pb(MENU_GAP).child(
        row()
            .relative()
            .w_full()
            .h(COMPACT_ROW_HEIGHT)
            .pl(8.)
            .pr(4.)
            .rounded(MENU_ITEM_RADIUS)
            .gap(8.)
            .items_center()
            .text_color(TEXT)
            .cursor(Cursor::Pointer)
            .reactive_style(move || {
                // The selected chip's wash and inset ring; a plain wash
                // under the pointer.
                Styles::new().bg(if selected() {
                    wash(0.11)
                } else if active() {
                    ink(0.05)
                } else {
                    ink(0.)
                })
            })
            .on_event({
                let id = id.clone();
                move |e| {
                    if matches!(e.event, InputEvent::PointerEnter)
                        && enter.with_untracked(|a| a.as_deref() != Some(id.as_str()))
                    {
                        enter.set(Some(id.clone()));
                    }
                }
            })
            .child(ring)
            .child(icon(name, 14., tint.unwrap_or_else(pop_muted)).shrink_0())
            .child(text_)
            .child(star)
            .on_click(move || {
                if !selected_only {
                    pick.pick_model(&id);
                }
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_expands_past_capacity_and_collapses_with_slack() {
        let capacity = 400.;
        // Compact holds up to capacity, then expands.
        assert!(!composer_flip(false, 400., capacity, false));
        assert!(composer_flip(false, 400.5, capacity, false));
        // Expanded holds until the text is 32 px under capacity.
        assert!(composer_flip(true, 390., capacity, false));
        assert!(composer_flip(true, 368., capacity, false));
        assert!(!composer_flip(true, 367.9, capacity, false));
        // A newline or a narrow pill always expands.
        assert!(composer_flip(false, 0., capacity, true));
        assert!(composer_flip(false, 0., 199., false));
        // Unmeasured: compact.
        assert!(!composer_flip(
            false,
            10_000.,
            compact_capacity(0., 120.),
            false
        ));
    }

    #[test]
    fn compact_capacity_leaves_room_for_the_controls() {
        // 734 inside a 736 pill, a 150 px chip: 734 - 96 - 150 - 8.
        assert_eq!(compact_capacity(734., 150.), 480.);
        // The chip is capped at 45% of the outer width.
        assert_eq!(compact_capacity(734., 600.), 734. - 96. - 0.45 * 736. - 8.);
    }

    #[test]
    fn heights_follow_the_docked_clamp() {
        assert_eq!(pill_target(false, 500.), COMPACT_TOTAL_HEIGHT);
        // One line: the 60 px floor.
        assert_eq!(expanded_height(INPUT_LINE_HEIGHT), 104.);
        // Three lines: 68.25 + 20 = 88.25 box.
        assert!((expanded_height(3. * INPUT_LINE_HEIGHT) - (88.25 + 44.)).abs() < 1e-3);
        // Capped at 260 + 44.
        assert_eq!(expanded_height(1000.), 304.);
        assert_eq!(stack_height(49.), 89.);
        assert!(!input_overflows(true, 240.));
        assert!(input_overflows(true, 241.));
        assert!(!input_overflows(false, 1000.));
    }

    #[test]
    fn morph_geometry_hands_off() {
        assert_eq!(morph_text_pad(0.), 12.);
        assert_eq!(morph_text_pad(1.), 16.);
        assert_eq!(morph_cluster_inset(true, 0.), 8.);
        assert_eq!(morph_cluster_inset(true, 1.), 12.);
        assert_eq!(morph_cluster_inset(false, 0.), 12.);
        assert_eq!(morph_cluster_inset(false, 1.), 8.);
        assert_eq!(CLUSTER_Y_DELTA, 0.5);
        assert_eq!(morph_cluster_dy(1.), 0.);
        // Collapsing from 104: the text starts 51 higher, then settles.
        assert_eq!(collapse_text_glide(104., 0.), 51.);
        assert_eq!(collapse_text_glide(104., 1.), 0.);
        assert_eq!(collapse_text_glide(49., 0.), 0.);
    }

    #[test]
    fn chip_width_and_effort_labels() {
        assert_eq!(chip_width(80., None, false), 114.);
        assert_eq!(chip_width(80., Some(30.2), false), 151.);
        assert_eq!(chip_width(80., Some(30.2), true), 170.);
        assert_eq!(chip_width(400., Some(40.), false), CHIP_MAX);
        assert_eq!(effort_label("xhigh"), "X-High");
        assert_eq!(effort_label("medium"), "Medium");
        let ladder: Vec<String> = ["low", "medium", "high", "xhigh"]
            .map(String::from)
            .to_vec();
        assert_eq!(effective_effort(None, &ladder).as_deref(), Some("high"));
        assert_eq!(
            effective_effort(Some("xhigh".into()), &ladder).as_deref(),
            Some("xhigh")
        );
        assert_eq!(
            effective_effort(Some("max".into()), &ladder).as_deref(),
            Some("high")
        );
        let short: Vec<String> = ["low", "medium"].map(String::from).to_vec();
        assert_eq!(effective_effort(None, &short).as_deref(), Some("medium"));
        assert_eq!(
            effective_effort(Some("max".into()), &[]).as_deref(),
            Some("max")
        );
    }

    #[test]
    fn slider_positions_and_stops_share_the_thumb_inset() {
        let width = 228.;
        for count in 2..=8 {
            for index in 0..count {
                let fraction = stop_fraction(index, count);
                let x = THUMB_INSET + fraction * (width - THUMB_INSET * 2.);
                assert!((effort_fraction(x, width) - fraction).abs() < 1e-5);
                assert_eq!(effort_index(x, width, count), Some(index));
                assert!((thumb_center(fraction, width) - x).abs() < 1e-4);
            }
        }
        assert!((effort_fraction(68., width) - 0.25).abs() < 1e-5);
        assert_eq!(effort_fraction(-100., width), 0.);
        assert_eq!(effort_fraction(500., width), 1.);
        assert_eq!(effort_index(100., 200., 0), None);
        assert_eq!(effort_index(100., 200., 1), Some(0));
        assert_eq!(effort_index(-20., 200., 5), Some(0));
        assert_eq!(effort_index(100., 200., 5), Some(2));
        assert_eq!(effort_index(250., 200., 5), Some(4));
        assert_eq!(stop_fraction(0, 1), 0.);
        // The fill runs 14 past the thumb's centre, never past the rail.
        assert_eq!(fill_width(0., width), 36.);
        assert_eq!(fill_width(1., width), width - THUMB_INSET + FILL_PAST);
    }

    #[test]
    fn native_slider_value_maps_back_to_slider_x() {
        // zgx's rail spans half its 10 px thumb in from either end.
        assert_eq!(slider_local_x(0., 230.), 5.);
        assert_eq!(slider_local_x(1., 230.), 225.);
        assert_eq!(slider_local_x(0.5, 230.), 115.);
        // So the end zones clamp to the end stops.
        assert_eq!(effort_index(slider_local_x(0., 230.), 230., 7), Some(0));
        assert_eq!(effort_index(slider_local_x(1., 230.), 230., 7), Some(6));
    }

    #[test]
    fn card_heights_follow_the_compact_layout() {
        // Header block, effort block, then options (2 + 28 each, up to 6).
        assert_eq!(header_block(true), 56.);
        assert_eq!(header_block(false), 44.);
        assert_eq!(EFFORT_BLOCK, 42.);
        assert_eq!(panel_height(true, 0), 100.);
        assert_eq!(panel_height(true, 1), 134.);
        assert_eq!(panel_height(false, 0), 46.);
        assert_eq!(panel_height(true, 9), 100. + 4. + 2. + 6. * 28.);
        // The list: 34 px pitch, up to seven rows, four while loading.
        assert_eq!(compact_list_height(0), 4. * 34. + 8.);
        assert_eq!(compact_list_height(3), 3. * 34. + 8.);
        assert_eq!(compact_list_height(12), 7. * 34. + 8.);
        assert_eq!(models_height(12), 80. + 246. + 2.);
        // Height = min(trigger top - 14, content), the budget 0..640.
        assert_eq!(menu_budget(f32::MAX), 640.);
        assert_eq!(menu_budget(300.), 286.);
        assert_eq!(menu_budget(5.), 0.);
        assert_eq!(card_height(menu_budget(300.), models_height(12)), 286.);
        assert_eq!(card_height(menu_budget(900.), panel_height(true, 1)), 134.);
        assert_eq!(list_height(286., 12), 286. - 82.);
        assert_eq!(MENU_ITEM_RADIUS, 7.);
    }

    fn model(id: &str, levels: &[&str], options: Vec<ModelOption>) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            label: id.to_uppercase(),
            description: None,
            levels: levels.iter().map(|l| l.to_string()).collect(),
            options,
        }
    }

    fn option(id: &str, choices: &[&str], default: &str) -> ModelOption {
        ModelOption {
            id: id.into(),
            label: id.into(),
            choices: choices
                .iter()
                .map(|c| OptionChoice {
                    id: c.to_string(),
                    label: c.to_uppercase(),
                })
                .collect(),
            default: default.into(),
        }
    }

    #[test]
    fn effort_fast_and_option_rows_follow_the_model() {
        let opus = model(
            "opus",
            &["low", "medium", "high", "xhigh"],
            vec![
                option("contextWindow", &["200k", "1m"], "200k"),
                option("fastMode", &["off", "on"], "off"),
            ],
        );
        let config = json!({ "model": "opus", "reasoning": "xhigh" });
        let effort = compact_effort(Some(&opus), &config).expect("ladder");
        assert_eq!(effort.selected, 3);
        assert_eq!(effort.labels[3], "X-High");
        // Unset: High.
        assert_eq!(compact_effort(Some(&opus), &json!({})).unwrap().selected, 2);
        let fast = fast_choice(Some(&opus), &config).expect("fast");
        assert!(!fast.fast);
        assert_eq!((fast.next.as_str(), fast.next_default), ("on", false));
        let on = json!({ "modelOptions": { "fastMode": "on" } });
        let fast = fast_choice(Some(&opus), &on).unwrap();
        assert!(fast.fast);
        assert_eq!((fast.next.as_str(), fast.next_default), ("off", true));
        // Fast mode has its own button: one option row is left.
        let groups = option_groups(Some(&opus), &config);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "contextWindow");
        assert!(groups[0].choices[0].selected && groups[0].choices[0].default);
        // Cursor's effort option becomes the slider and leaves the rows.
        let cursor = model(
            "gpt",
            &[],
            vec![
                option("reasoning", &["low", "high"], "low"),
                option("fast", &["false", "true"], "false"),
            ],
        );
        let effort = compact_effort(
            Some(&cursor),
            &json!({ "modelOptions": { "reasoning": "high" } }),
        )
        .unwrap();
        assert_eq!(effort.selected, 1);
        assert!(matches!(effort.stops, Stops::Option { .. }));
        assert!(option_groups(Some(&cursor), &json!({})).is_empty());
        assert_eq!(
            fast_mode_values(&option("tier", &["standard", "fast"], "standard")),
            Some(("fast", "standard"))
        );
        assert_eq!(fast_mode_values(&option("x", &["a", "b"], "a")), None);
    }

    #[test]
    fn model_rows_star_rank_and_open_in_place() {
        let catalog = vec![
            model("a", &[], vec![]),
            model("b", &[], vec![]),
            model("c", &[], vec![]),
        ];
        let rows = list_rows(&catalog, Some("b"), &["c".into()], "");
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(row_groups(&rows), vec![(true, 0), (false, 1)]);
        // A search ranks label hits; the selection outside the catalog
        // stays as an inert first row.
        let rows = list_rows(&catalog, Some("zz"), &[], "b");
        assert_eq!(rows.len(), 1);
        let rows = list_rows(&catalog, Some("zz"), &[], "");
        assert!(rows[0].selected_only);
        assert!(list_rows(&[], Some("zz"), &[], "").is_empty());
        // Near its group's start the group tops the list (one row of the
        // group above kept); deeper, the row centres.
        assert_eq!(open_offset(&[(true, 0), (false, 2)], 3, 238.), 34.);
        assert_eq!(open_offset(&[(false, 0)], 2, 238.), 0.);
        assert_eq!(open_offset(&[(false, 0)], 10, 238.), 340. - 102.);
        assert_eq!(match_rank("op", "Opus"), Some(0));
        assert_eq!(match_rank("pu", "Opus"), Some(1));
        assert_eq!(match_rank("x", "Opus"), None);
    }

    #[test]
    fn rolls_keep_shared_ends_and_menus_retreat_half_way() {
        assert_eq!(shared_ends("Opus 5.5", "Opus 5.1"), (7, 0));
        assert_eq!(shared_ends("feature/a", "fix/a"), (1, 2));
        assert_eq!(shared_ends("aa", "aaa"), (2, 0));
        assert_eq!(shared_ends("ab", "b"), (0, 1));
        assert_eq!(edge_fades(0., 400., 200.), (0., 12.));
        assert_eq!(edge_fades(5., 400., 200.), (5., 12.));
        assert_eq!(edge_fades(200., 400., 200.), (12., 0.));
        // `lift` and the HSL round trip used by the shimmer.
        assert_eq!(lift(Color(0, 0, 0, 255), 0.5), Color(128, 128, 128, 255));
        let (h, s, l) = rgb_to_hsl(ACCENT);
        assert_eq!(hsl_to_rgb(h, s, l, 255), ACCENT);
    }
}
