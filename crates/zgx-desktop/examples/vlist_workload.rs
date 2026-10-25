//! The variable-height list comparison scene (see zgx_workload::vlist),
//! shared with the GPUI Fast and MyGo adapters: 100,000 chat rows of wrapped
//! text in zgx's measured virtual list, which measures each row as it is
//! laid out and estimates the rest, moved by the scene's scroll, fling or
//! jump each tick. Run with ZGX_MODE=fling ZGX_SECONDS=10.
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::compose::prelude::*;
use zgx_desktop::WindowOptions;
use zgx_workload::{
    PERIOD,
    vlist::{self, Mode},
};

#[path = "support/bench_font.rs"]
mod bench_font;
#[path = "support/vlist_view.rs"]
mod vlist_view;
use vlist_view::chat_row;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = Mode::from_env();
    let seconds: f64 = std::env::var("ZGX_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.);
    bench_font::application()?
        .window(WindowOptions {
            title: "zgx vlist".into(),
            width: vlist::WIDTH as f64,
            height: vlist::HEIGHT as f64,
            resizable: false,
            ..Default::default()
        })
        .run(move |cx| {
            let ticks = Rc::new(Cell::new(0_u64));
            // Rows start at the estimate of a three-line row with no image.
            let heights = VariableHeights::new(&cx.ui.runtime, vlist::ROWS, vlist_view::ESTIMATE);
            let offset = cx.ui.signal(0_f32);
            let list = measured_virtual_list(
                offset.clone(),
                heights.clone(),
                2,
                |i| i,
                |_, index, _| chat_row(index),
            )
            .w_full()
            .h_full();
            cx.render(vlist_view::frame(list));
            cx.on_closed({
                let ticks = ticks.clone();
                move || eprintln!("workload_ticks={}", ticks.get())
            });
            let window = cx.window.clone();
            if mode == Mode::Probe {
                // Each probe row at the top in turn, measured after a few frames.
                cx.tasks.spawn(async move {
                    let mut measured = Vec::new();
                    for row in vlist::probe_rows() {
                        offset.set(heights.row_offset(row));
                        zgx::timer::sleep(PERIOD * 6).await;
                        // Placed from the estimates above it, measured once laid out.
                        offset.set(heights.row_offset(row));
                        zgx::timer::sleep(PERIOD * 6).await;
                        measured.push((row, heights.row_height(row).unwrap_or(0.)));
                    }
                    eprintln!("{}", vlist::probe_report(&measured, vlist::WIDTH));
                    window.close();
                });
                return;
            }
            cx.tasks.spawn(async move {
                let start = Instant::now();
                let end = (seconds > 0.).then(|| start + Duration::from_secs_f64(seconds));
                let mut next = start + PERIOD;
                loop {
                    zgx::timer::sleep_until(end.map_or(next, |end| end.min(next))).await;
                    if end.is_some_and(|end| Instant::now() >= end) {
                        window.close();
                        break;
                    }
                    let tick = ticks.get() + 1;
                    ticks.set(tick);
                    match vlist::jump(tick) {
                        Some(row) if mode == Mode::Jump => {
                            offset.set(heights.row_offset(row));
                        }
                        _ => {
                            let max = (heights.content_height() - vlist::HEIGHT).max(0.);
                            let moved = offset.get() + vlist::delta(mode, tick);
                            // At the end, back to the top (100k rows take minutes to reach).
                            offset.set(if moved > max { 0. } else { moved });
                        }
                    }
                    next += PERIOD;
                    let now = Instant::now();
                    if next <= now {
                        next = now + PERIOD;
                    }
                }
            });
        })
}
