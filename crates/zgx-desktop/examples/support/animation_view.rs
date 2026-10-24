//! The animation workload's view (see zgx_workload::animation), shared by
//! the windowed `animation_workload` and the offscreen `animation_headless`:
//! dots whose opacity, and characters whose colour, follow `clock`.
use zgx::{compose::prelude::*, reactive::Signal, scene::Color};
use zgx_workload::animation::*;

pub fn animation(clock: Signal<f32>) -> View {
    let dots = (0..COLUMNS * ROWS).map(|i| {
        let (x, y) = dot_position(i);
        let clock = clock.clone();
        div()
            .absolute()
            .ml(x)
            .mt(y)
            .size(DOT, DOT)
            .rounded(DOT / 2.)
            .bg(rgb(DOT_COLOR))
            .reactive_style(move || Styles::new().opacity(dot_opacity(i, clock.get())))
    });
    let chars: Vec<char> = SHIMMER.chars().collect();
    let count = chars.len();
    let line = row()
        .absolute()
        .ml(SHIMMER_ORIGIN.0)
        .mt(SHIMMER_ORIGIN.1)
        .children(chars.into_iter().enumerate().map(|(i, ch)| {
            let clock = clock.clone();
            text(ch.to_string()).reactive_style(move || {
                Styles::new().text_color(Color(255, 255, 255, shimmer_alpha(i, count, clock.get())))
            })
        }));
    overlay()
        .w_full()
        .h_full()
        .bg(rgb(BACKGROUND))
        .text_size(18.)
        .children(dots)
        .child(line)
}
