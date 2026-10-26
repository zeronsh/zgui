//! Shared retained overlay controls for ordinary and virtual viewports.
use crate::{
    input::{EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Color, Layout, NodeId, NodeKind, QuadStyle, Style, Transform},
    semantics::{Role, ScrollAxis, SemanticNode},
    theme::tok,
    widgets::Ui,
};
use std::{
    cell::Cell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

/// The thumb's thickness at rest and under the pointer (or dragged), and
/// how far its rounded ends sit in from the track's.
const THUMB: f32 = 6.;
const THUMB_ACTIVE: f32 = 8.;
const THUMB_INSET: f32 = 2.;

/// How long an overlay scrollbar stays once its view stops scrolling (or
/// the pointer leaves it), and how long it takes to fade out, a step a
/// refresh (`Ui::set_refresh_interval`).
const LINGER: Duration = Duration::from_millis(1000);
const FADE: Duration = Duration::from_millis(200);

/// An overlay scrollbar's showing (`Ui::set_overlay_scrollbars`).
pub(crate) struct Fade {
    /// 1 shown, 0 hidden.
    alpha: Signal<f32>,
    /// When it starts fading; `None` while it stays (held) or is hidden.
    fade_at: Cell<Option<Instant>>,
    /// Its next step: `fade_at`, then each frame of the fade.
    next: Cell<Option<Instant>>,
    /// In the document's list of fades.
    listed: Cell<bool>,
}
impl Fade {
    fn show(&self, now: Instant) {
        self.alpha.set(1.);
        self.linger(now);
    }
    /// Under the pointer or dragged: it stays.
    fn hold(&self) {
        self.alpha.set(1.);
        self.fade_at.set(None);
        self.next.set(None);
    }
    fn linger(&self, now: Instant) {
        let at = now + LINGER;
        self.fade_at.set(Some(at));
        self.next.set(Some(at));
    }
    fn hide(&self) {
        self.alpha.set(0.);
        self.fade_at.set(None);
        self.next.set(None);
    }
}

/// When the next overlay scrollbar steps toward hidden. None shown, none.
pub(crate) fn next_fade(ui: &Ui) -> Option<Instant> {
    let fades = ui.scrollbar_fades().borrow();
    if fades.is_empty() {
        return None;
    }
    fades
        .iter()
        .filter_map(|fade| fade.upgrade()?.next.get())
        .min()
}

/// Step the overlay scrollbars due by `now` toward hidden; whether any did.
pub(crate) fn advance_fades(ui: &Ui, now: Instant) -> bool {
    let fades: Vec<Rc<Fade>> = {
        let mut list = ui.scrollbar_fades().borrow_mut();
        if list.is_empty() {
            return false;
        }
        list.retain(|fade| fade.strong_count() > 0);
        list.iter().filter_map(Weak::upgrade).collect()
    };
    let mut stepped = false;
    for fade in fades {
        let (Some(next), Some(at)) = (fade.next.get(), fade.fade_at.get()) else {
            continue;
        };
        if next > now {
            continue;
        }
        let done = now.saturating_duration_since(at).as_secs_f32() / FADE.as_secs_f32();
        if done >= 1. {
            fade.hide();
        } else {
            fade.alpha.set(1. - done);
            fade.next.set(Some(now + ui.refresh_interval()));
        }
        stepped = true;
    }
    stepped
}

/// A rounded thumb of `color`, `thickness` across.
fn thumb_kind(color: Color, thickness: f32) -> NodeKind {
    NodeKind::Quad(QuadStyle {
        fill: color,
        radius: thickness / 2.,
        ..Default::default()
    })
}

pub(crate) fn mount(
    ui: &mut Ui,
    owner: NodeId,
    offset: Signal<f32>,
    size: Signal<(f32, f32)>,
    extent: Signal<f32>,
    horizontal: bool,
) {
    let overlay = ui.container(owner, Layout::Overlay, Style::default());
    // Its parts are placed by transforms from its corner, in points across
    // whichever way the view runs.
    ui.scene
        .borrow_mut()
        .set_direction(overlay, Some(crate::scene::Direction::Ltr));
    let track = ui.container(overlay, Layout::Overlay, Style::default());
    let theme = ui.theme_scope();
    let background = ui.scene.borrow_mut().append(
        track,
        NodeKind::Rect(theme.color_untracked(tok::SCROLLBAR_TRACK)),
        Style::default(),
    );
    let thumb = ui.scene.borrow_mut().append(
        track,
        thumb_kind(theme.color_untracked(tok::SCROLLBAR_THUMB), THUMB),
        Style::default(),
    );
    let main = move |size: (f32, f32)| if horizontal { size.0 } else { size.1 };
    let event_size = size.clone();
    let event_extent = extent.clone();
    let event_offset = offset.clone();
    let scene = ui.scene.clone();
    let focused = ui.signal(false);
    let write_focus = focused.clone();
    // Under the pointer, or dragged: the thumb widens and brightens.
    let active = ui.signal(false);
    let write_active = active.clone();
    let mut grab = None;
    ui.on_event(track, true, move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        if matches!(cx.event, InputEvent::Focus) {
            write_focus.set(true);
        }
        if matches!(cx.event, InputEvent::Blur) {
            write_focus.set(false);
        }
        match cx.event {
            InputEvent::PointerEnter | InputEvent::PointerMove { .. } => {
                write_active.set(true);
            }
            // A drag ended: the next move over the bar lights it again.
            InputEvent::PointerLeave if grab.is_none() => {
                write_active.set(false);
            }
            InputEvent::PointerUp { .. } | InputEvent::PointerCancel | InputEvent::Blur => {
                write_active.set(false);
            }
            _ => {}
        }
        if cx.default_prevented()
            && !matches!(
                cx.event,
                InputEvent::PointerUp { .. }
                    | InputEvent::PointerCancel
                    | InputEvent::Blur
                    | InputEvent::Focus
            )
        {
            return;
        }
        let length = main(event_size.get()) as f64;
        let total = event_extent.get() as f64;
        let limit = (total - length).max(0.);
        let thumb_length = (length * length / total.max(1.)).max(24.).min(length);
        let travel = (length - thumb_length).max(0.);
        // Right to left, a horizontal bar counts from its right end.
        let rtl = horizontal && scene.borrow().direction(owner) == crate::scene::Direction::Rtl;
        let position = match cx.event {
            InputEvent::PointerDown { x, y, .. } | InputEvent::PointerMove { x, y } => {
                let bounds = scene.borrow().bounds(track);
                Some(if rtl {
                    length - (x as f64 - bounds.x as f64)
                } else if horizontal {
                    x as f64 - bounds.x as f64
                } else {
                    y as f64 - bounds.y as f64
                })
            }
            _ => None,
        };
        if position.is_some_and(|position| !position.is_finite()) {
            return;
        }
        let before = event_offset.get() as f64;
        let next = match &cx.event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } if limit > 0. => {
                cx.focus();
                let pointer = position.unwrap();
                let thumb_start = before / limit * travel;
                if pointer >= thumb_start && pointer <= thumb_start + thumb_length {
                    grab = Some(pointer - thumb_start);
                    cx.capture_pointer();
                    None
                } else {
                    Some(
                        before
                            + if pointer < thumb_start {
                                -length
                            } else {
                                length
                            },
                    )
                }
            }
            InputEvent::PointerMove { .. } if grab.is_some() => Some(if travel > 0. {
                (position.unwrap() - grab.unwrap()) / travel * limit
            } else {
                0.
            }),
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            }
            | InputEvent::PointerCancel
            | InputEvent::Blur => {
                grab = None;
                cx.release_pointer();
                None
            }
            // The wheel over the bar scrolls what it scrolls.
            InputEvent::Scroll {
                delta_x, delta_y, ..
            } => {
                let delta = if rtl {
                    -*delta_x
                } else if horizontal {
                    *delta_x
                } else {
                    *delta_y
                };
                (delta.is_finite() && delta != 0.).then_some(before + delta as f64)
            }
            InputEvent::KeyDown { key, .. } => match key {
                Key::Home => Some(0.),
                Key::End => Some(limit),
                Key::PageUp => Some(before - length),
                Key::PageDown => Some(before + length),
                Key::ArrowLeft if rtl => Some(before + 40.),
                Key::ArrowRight if rtl => Some(before - 40.),
                Key::ArrowLeft | Key::ArrowUp => Some(before - 40.),
                Key::ArrowRight | Key::ArrowDown => Some(before + 40.),
                _ => None,
            },
            InputEvent::Increment => Some(before + 40.),
            InputEvent::Decrement => Some(before - 40.),
            InputEvent::SetNumericValue(value) if value.is_finite() => Some(*value),
            InputEvent::SetValue(value) => value.parse::<f64>().ok().filter(|v| v.is_finite()),
            _ => None,
        };
        if let Some(next) = next {
            event_offset.set(next.clamp(0., limit) as f32);
            cx.prevent_default();
            cx.stop_propagation();
        } else if matches!(
            cx.event,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            }
        ) {
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
    // As an overlay scrollbar: shown as the user scrolls, then fading.
    let overlay_mode = ui.overlay_scrollbars();
    let fade = Rc::new(Fade {
        alpha: ui.signal(1.),
        fade_at: Cell::new(None),
        next: Cell::new(None),
        listed: Cell::new(false),
    });
    let (weak, shown, mode) = (ui.downgrade(), fade.clone(), overlay_mode.clone());
    let (scrolled, held, focus) = (offset.clone(), active.clone(), focused.clone());
    let mut last = None;
    ui.bind(overlay, move || {
        // Off, this reads only the mode.
        if !mode.get() {
            if last.take().is_some() {
                shown.hold();
            }
            return;
        }
        let value = scrolled.get();
        let (pointer, focused) = (held.get(), focus.get());
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if !shown.listed.replace(true) {
            ui.scrollbar_fades()
                .borrow_mut()
                .push(Rc::downgrade(&shown));
        }
        let now = Instant::now();
        match last.replace(value) {
            // Turned on: hidden until the user scrolls.
            None => shown.hide(),
            // Focused, it shows; under the pointer or dragged, it stays
            // while it shows.
            _ if focused => shown.hold(),
            _ if pointer => {
                if shown.alpha.with_untracked(|alpha| *alpha > 0.) {
                    shown.hold();
                }
            }
            // The user scrolled it, not the app.
            Some(before) if before != value && ui.is_dispatching() => shown.show(now),
            // Let go: it stays a moment, then fades.
            _ if shown.fade_at.get().is_none() && shown.alpha.with_untracked(|a| *a > 0.) => {
                shown.linger(now)
            }
            _ => {}
        }
    });
    ui.retain(overlay, fade.clone());
    let weak = ui.downgrade();
    ui.bind(overlay, move || {
        let Some(mut ui) = weak.upgrade() else {
            return;
        };
        let overlay_mode = overlay_mode.get();
        let alpha = if overlay_mode { fade.alpha.get() } else { 1. };
        let (width, height) = size.get();
        let length = main((width, height));
        let total = extent.get();
        let limit = (total - length).max(0.);
        let visible = limit > 0. && length > 0. && width > 0. && height > 0.;
        if ui
            .input
            .options(track)
            .is_none_or(|options| options.disabled == visible)
        {
            ui.set_disabled(track, !visible);
        }
        let fixed = |width, height| Style {
            width: Some(width),
            height: Some(height),
            ..Default::default()
        };
        let mut scene = ui.scene.borrow_mut();
        // Right to left, a vertical bar is at the left (the start edge, as
        // browsers put it) and a horizontal thumb starts at the right.
        let rtl = scene.direction(owner) == crate::scene::Direction::Rtl;
        // Zero area lets underlying controls receive hits outside the track.
        scene.set_style(
            overlay,
            Style {
                margin: crate::scene::Insets {
                    right: width,
                    ..Default::default()
                },
                ..fixed(0., 0.)
            },
        );
        let thickness = if horizontal { height } else { width }.min(8.);
        // Hidden, the pointer reaches what is under it.
        let track_size = if !visible || alpha <= 0. {
            (0., 0.)
        } else if horizontal {
            (length, thickness)
        } else {
            (thickness, length)
        };
        scene.set_style(track, fixed(track_size.0, track_size.1));
        scene.set_style(background, fixed(track_size.0, track_size.1));
        scene.set_transform(
            track,
            if horizontal {
                Transform {
                    x: 0.,
                    y: height - thickness,
                }
            } else {
                Transform {
                    x: if rtl { 0. } else { width - thickness },
                    y: 0.,
                }
            },
        );
        let thumb_length = if visible && alpha > 0. {
            ((length as f64 * length as f64 / total as f64)
                .max(24.)
                .min(length as f64)) as f32
        } else {
            0.
        };
        // Drawn slimmer than the track it hits in, centred across it, its
        // rounded ends inset along it.
        let active = active.get();
        let across = (if active { THUMB_ACTIVE } else { THUMB }).min(thickness);
        let along = if thumb_length > 0. {
            (thumb_length - 2. * THUMB_INSET).max(across)
        } else {
            0.
        };
        let drawn = if along <= 0. {
            (0., 0.)
        } else if horizontal {
            (along, across)
        } else {
            (across, along)
        };
        scene.set_style(thumb, fixed(drawn.0, drawn.1));
        let faded =
            |Color(r, g, b, a): Color, alpha: f32| Color(r, g, b, (a as f32 * alpha).round() as u8);
        // An overlay scrollbar's track shows under the pointer only.
        let track_alpha = if overlay_mode && !active { 0. } else { alpha };
        scene.set_kind(
            background,
            NodeKind::Rect(faded(theme.color(tok::SCROLLBAR_TRACK), track_alpha)),
        );
        let thumb_color = theme.color(tok::SCROLLBAR_THUMB);
        let color = if focused.get() {
            theme.color(tok::SCROLLBAR_THUMB_ACTIVE)
        } else if active {
            // Brighter: the colour's alpha raised by three quarters.
            let Color(r, g, b, a) = thumb_color;
            Color(r, g, b, (a as f32 * 1.75).min(255.) as u8)
        } else {
            thumb_color
        };
        scene.set_kind(thumb, thumb_kind(faded(color, alpha), across));
        let value = offset.get().clamp(0., limit);
        let position = if limit > 0. {
            (value as f64 / limit as f64 * (length - thumb_length) as f64) as f32
        } else {
            0.
        };
        let position = if rtl && horizontal {
            length - thumb_length - position
        } else {
            position
        };
        let (start, side) = (position + THUMB_INSET, (thickness - across) / 2.);
        scene.set_transform(
            thumb,
            if horizontal {
                Transform { x: start, y: side }
            } else {
                Transform { x: side, y: start }
            },
        );
        drop(scene);
        if visible {
            let mut semantic = SemanticNode::new(
                Role::ScrollBar,
                if horizontal {
                    "Horizontal scrollbar"
                } else {
                    "Vertical scrollbar"
                },
            );
            semantic.scroll_axis = Some(if horizontal {
                ScrollAxis::Horizontal
            } else {
                ScrollAxis::Vertical
            });
            semantic.min = Some(0.);
            semantic.max = Some(limit as f64);
            semantic.numeric_value = Some(value as f64);
            ui.semantics.borrow_mut().set(track, semantic);
        } else {
            ui.semantics.borrow_mut().remove(track);
        }
    });
}
