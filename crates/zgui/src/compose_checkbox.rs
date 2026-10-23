//! A checkbox's indicator, drawn as the platform draws one: a rounded box,
//! outlined when off and filled with the accent when on, its checkmark
//! drawn on as it turns on; a dash for the mixed state. Colours are theme
//! tokens, so the system's accent (`Themes::set_system_tokens`) shows.
use crate::{
    canvas::{Canvas, LineCap, LineJoin, Path, Stroke},
    compose::{TaskRunner, TaskToken, View},
    reactive::Signal,
    scene::Color,
    style::{Styled, Styles},
    theme::{ThemeScope, tok},
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

/// The box's side, in points.
pub(crate) const SIZE: f32 = 16.;
/// How long the checkmark takes to draw on.
const DRAW: Duration = Duration::from_millis(150);

/// The checkmark (or the mixed state's dash) in a `size` box, `progress`
/// of it drawn (0 to 1), in `color`.
pub(crate) fn mark(size: f32, mixed: bool, progress: f32, color: Color) -> Canvas {
    let mut canvas = Canvas::new();
    if progress <= 0. {
        return canvas;
    }
    let s = size / SIZE;
    let mut path = Path::builder();
    let length = if mixed {
        path.move_to(4.5 * s, 8. * s).line_to(11.5 * s, 8. * s);
        7. * s
    } else {
        path.move_to(4.5 * s, 8.4 * s)
            .line_to(7. * s, 10.9 * s)
            .line_to(11.5 * s, 5.4 * s);
        (2.5f32.hypot(2.5) + 4.5f32.hypot(5.5)) * s
    };
    let Ok(path) = path.build() else {
        return canvas;
    };
    let drawn = length * progress.clamp(0., 1.);
    canvas.stroke(
        path,
        color,
        Stroke {
            width: 2. * s,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dash: if progress >= 1. {
                Vec::new()
            } else {
                vec![drawn.max(0.01), length * 2.]
            },
            ..Default::default()
        },
    );
    canvas
}

/// A checkbox's indicator for `checked` (and `mixed`, when given), styled
/// by `theme`. With `runner`, the checkmark draws on; without one (a
/// headless `Ui`), it shows at once.
pub(crate) fn indicator(
    checked: Signal<bool>,
    mixed: Option<Signal<bool>>,
    theme: Rc<ThemeScope>,
    runner: Option<Rc<TaskRunner>>,
) -> View {
    let on = {
        let (checked, mixed) = (checked.clone(), mixed.clone());
        move || checked.get() || mixed.as_ref().is_some_and(|m| m.get())
    };
    let animating: Rc<RefCell<Option<TaskToken>>> = Rc::default();
    let style_on = on.clone();
    let (mark_mixed, mark_on, mark_theme) = (mixed, on, theme);
    let mut last_on: Option<bool> = None;
    crate::compose::div()
        .w(SIZE)
        .h(SIZE)
        .shrink_0()
        .rounded(4.)
        .reactive_style(move || {
            if style_on() {
                Styles::new()
                    .bg(tok::ACCENT)
                    .border(1.)
                    .border_color(tok::ACCENT)
            } else {
                Styles::new()
                    .bg(tok::INPUT_BG)
                    .border(1.)
                    .border_color(tok::BORDER_STRONG)
            }
        })
        .child(
            crate::compose::component(move |cx| {
                // How much of the mark shows: 1 at rest while on, 0 off.
                let shown = cx.runtime().signal(if mark_on() { 1_f32 } else { 0. });
                let follow = {
                    let (shown, animating, runner) = (shown.clone(), animating.clone(), runner);
                    let mark_on = mark_on.clone();
                    cx.runtime().effect(move || {
                        let now_on = mark_on();
                        let was = last_on.replace(now_on);
                        animating.borrow_mut().take();
                        match (was, now_on, &runner) {
                            // Turned on with a clock: drawn on.
                            (Some(false), true, Some(runner)) => {
                                shown.set(0.);
                                let (shown, start) = (shown.clone(), Instant::now());
                                *animating.borrow_mut() =
                                    Some(runner.spawn(Box::pin(async move {
                                        loop {
                                            crate::timer::sleep(Duration::from_millis(8)).await;
                                            let t = (start.elapsed().as_secs_f32()
                                                / DRAW.as_secs_f32())
                                            .min(1.);
                                            shown.set(1. - (1. - t).powi(3));
                                            if t >= 1. {
                                                break;
                                            }
                                        }
                                    })));
                            }
                            _ => {
                                shown.set(if now_on { 1. } else { 0. });
                            }
                        }
                    })
                };
                cx.retain(follow);
                let (mixed, theme) = (mark_mixed.clone(), mark_theme.clone());
                crate::compose::canvas(move |(w, _)| {
                    let mixed = mixed.as_ref().is_some_and(|m| m.get());
                    mark(w, mixed, shown.get(), theme.color(tok::ACCENT_TEXT))
                })
                .w(SIZE)
                .h(SIZE)
            })
            .w(SIZE)
            .h(SIZE),
        )
}
