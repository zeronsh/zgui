//! The GPU view comparison's zgx side (scripts/run_macos_comparison.py
//! `--scene viewport`, against wgpu_viewport): a field of lit cubes
//! (support/cubes.rs) drawn by a GPU view filling a 960 by 720 window.
//! `ZGX_MODE=animate` animates it, a frame each refresh; `idle` shows its
//! first frame and nothing after. Runs `ZGX_SECONDS`, then reports the
//! frames drawn after the first (`animation_frames=`).
use std::{cell::Cell, rc::Rc, time::Duration, time::Instant};
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::WindowOptions;
use zgx_workload::Workload;

#[path = "support/bench_font.rs"]
mod bench_font;
#[path = "support/cubes.rs"]
mod cubes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    let animate = std::env::var("ZGX_MODE").as_deref() != Ok("idle");
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
            cx.render(gpu_view("Cubes", &view).w_full().h_full());
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
