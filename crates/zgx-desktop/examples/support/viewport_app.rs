//! The GPU view comparison's zgx app (viewport_workload, viewport_composite):
//! a field of lit cubes (support/cubes.rs) drawn by a GPU view filling a 960
//! by 720 window. `ZGX_MODE=animate` animates it, a frame each refresh;
//! `idle` shows its first frame and nothing after; `switch` flips it between
//! its own layer and the window's frame every quarter second, over magenta
//! (scripts/viewport_probe.swift). Runs `ZGX_SECONDS`, then reports the
//! frames drawn after the first (`animation_frames=`).
use std::{cell::Cell, rc::Rc, time::Duration, time::Instant};
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::WindowOptions;
use zgx_workload::Workload;

#[path = "bench_font.rs"]
mod bench_font;
#[path = "cubes.rs"]
mod cubes;

/// The app: its view composited into the window's frame if `composite`.
pub fn run(composite: bool) -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    let mode = std::env::var("ZGX_MODE").unwrap_or_default();
    let animate = mode != "idle";
    // Switching: magenta under the view, which flips between its own layer
    // and the window's frame every quarter second (an opacity just under 1
    // composites it), for scripts/viewport_probe.swift to find any frame
    // that shows neither.
    let switching = mode == "switch";
    bench_font::application()?
        .window(WindowOptions {
            title: "zgx GPU view".into(),
            width: 960.,
            height: 720.,
            ..Default::default()
        })
        .run(move |cx| {
            let drawn = Rc::new(Cell::new(0_u64));
            let view = zgx_desktop::viewport::view(cubes::Cubes::new(drawn.clone()));
            view.animate(animate);
            // Its cubes clear to an opaque colour: on a layer of its own,
            // unless asked to composite (the comparison's third cell).
            view.set_opaque(!composite && std::env::var_os("ZGX_VIEW_COMPOSITE").is_none());
            let composited = cx.ui.signal(false);
            let fade = composited.clone();
            cx.render(
                div()
                    .w_full()
                    .h_full()
                    .bg(if switching {
                        rgb(0xff00ff)
                    } else {
                        rgb(0x000000)
                    })
                    .child(
                        gpu_view("Cubes", &view)
                            .w_full()
                            .h_full()
                            .reactive_style(move || {
                                Styles::new().opacity(if fade.get() { 0.999 } else { 1. })
                            }),
                    ),
            );
            if switching {
                let frames = cx.frames.clone();
                cx.tasks.spawn(async move {
                    loop {
                        zgx::timer::sleep(Duration::from_millis(250)).await;
                        frames.next().await;
                        composited.set(!composited.get());
                    }
                });
            }
            if seconds > 0. {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    let start = Instant::now();
                    sleep(Duration::from_secs_f64(seconds)).await;
                    let n = drawn.get().saturating_sub(1);
                    eprintln!(
                        "animation_frames={n} fps={:.1}",
                        n as f64 / start.elapsed().as_secs_f64()
                    );
                    window.close();
                });
            }
        })
}
