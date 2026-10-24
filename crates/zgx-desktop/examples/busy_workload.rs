//! Busy window + one spinner, shared with the GPUI adapter
//! (see zgx_workload::busy). Only the spinner changes each frame.
use std::time::{Duration, Instant};
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::{Application, WindowOptions};
use zgx_workload::{Workload, busy::*};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    Application::new()
        .window(WindowOptions {
            title: "zgx busy".into(),
            width: 960.,
            height: 720.,
            ..Default::default()
        })
        .run(move |cx| {
            let clock = cx.ui.signal(0_f32);
            let frames = cx.ui.signal(0_u64);
            let start = Instant::now();
            {
                let (clock, frames, source) = (clock.clone(), frames.clone(), cx.frames.clone());
                cx.tasks.spawn(async move {
                    loop {
                        source.next().await;
                        clock.set(start.elapsed().as_secs_f32());
                        frames.update(|n| *n += 1);
                    }
                });
            }
            if seconds > 0. {
                let (window, frames) = (cx.window.clone(), frames.clone());
                cx.tasks.spawn(async move {
                    sleep(Duration::from_secs_f64(seconds)).await;
                    let n = frames.get();
                    eprintln!(
                        "animation_frames={n} fps={:.1}",
                        n as f64 / start.elapsed().as_secs_f64()
                    );
                    window.close();
                });
            }
            let cells = (0..GRID_COLUMNS * GRID_ROWS).map(|i| {
                let (x, y) = cell_position(i);
                div()
                    .absolute()
                    .ml(x)
                    .mt(y)
                    .size(CELL.0, CELL.1 - 2.)
                    .bg(rgb(cell_color(i)))
                    .text_color(rgb(0xc9d4e3))
                    .child(text(cell_label(i)))
            });
            let lines = (0..PARAGRAPH_LINES).map(|line| {
                text(paragraph_line(line))
                    .absolute()
                    .ml(PARAGRAPH_ORIGIN.0)
                    .mt(PARAGRAPH_ORIGIN.1 + line as f32 * LINE_HEIGHT)
                    .text_size(7.)
                    .line_height(LINE_HEIGHT)
                    .text_color(rgb(0x8fa0b8))
            });
            let dots = (0..SPINNER_DOTS).map(|i| {
                let (x, y) = dot_position(i);
                let clock = clock.clone();
                div()
                    .absolute()
                    .ml(x)
                    .mt(y)
                    .size(DOT, DOT)
                    .rounded(DOT / 2.)
                    .bg(rgb(0x9ad2ff))
                    .reactive_style(move || Styles::new().opacity(dot_opacity(i, clock.get())))
            });
            cx.render(
                overlay()
                    .w_full()
                    .h_full()
                    .bg(rgb(0x10141c))
                    .text_size(11.)
                    .line_height(14.)
                    .children(cells)
                    .children(lines)
                    .children(dots),
            );
        })
}
