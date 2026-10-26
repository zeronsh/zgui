//! Display-rate animation workload shared with the GPUI and QuickGUI
//! comparison adapters (see zgx_workload::animation). Retained nodes whose
//! reactive styles read a clock advanced once per display refresh.
use std::time::{Duration, Instant};
use zgx::timer::sleep;
use zgx_desktop::WindowOptions;
use zgx_workload::Workload;

#[path = "support/bench_font.rs"]
mod bench_font;

#[path = "support/animation_view.rs"]
mod view;

#[path = "support/frame_stamp.rs"]
mod frame_stamp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    bench_font::application()?
        .window(WindowOptions {
            title: "zgx animation".into(),
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
            let view = view::animation(clock);
            cx.render(if frame_stamp::enabled() {
                frame_stamp::over(view, frames, 960., 720.)
            } else {
                view
            });
        })
}
