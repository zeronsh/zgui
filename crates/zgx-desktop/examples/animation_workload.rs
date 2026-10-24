//! Display-rate animation workload shared with the GPUI and QuickGUI
//! comparison adapters (see zgx_workload::animation). Retained nodes whose
//! reactive styles read a clock advanced once per display refresh.
use std::time::{Duration, Instant};
use zgx::timer::sleep;
use zgx_desktop::{Application, WindowOptions};
use zgx_workload::Workload;

#[path = "support/animation_view.rs"]
mod view;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    Application::new()
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
            cx.render(view::animation(clock));
        })
}
