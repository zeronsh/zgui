//! An app that fails while it runs, for `scripts/failures_smoke.py`: one
//! of its tasks panics (the window says so in a banner and goes on), then
//! its GPU device is lost, as a driver reset or a removed GPU would lose
//! it (the window makes a new renderer and goes on drawing).
//! `--smoke-test` does both, then quits.
use std::time::Duration;
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|a| a == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "Failures".into(),
            width: 400.,
            height: 240.,
            ..Default::default()
        })
        .run(move |cx| {
            let ticks = cx.ui.signal(0u32);
            let shown = ticks.clone();
            cx.render(
                div()
                    .w_full()
                    .h_full()
                    .p(24.)
                    .pt(64.)
                    .bg(rgb(0x203040))
                    .text_color(rgb(0xffffff))
                    .child(text_signal(move || format!("frame {}", shown.get()))),
            );
            let window = cx.window.clone();
            let tasks = cx.tasks.clone();
            let ticker = cx.tasks.spawn_scoped(async move {
                for tick in 1.. {
                    zgui::timer::sleep(Duration::from_millis(16)).await;
                    ticks.set(tick);
                    if !smoke {
                        continue;
                    }
                    if tick == 20 {
                        eprintln!("failures: a task panics");
                        tasks.spawn(async { panic!("a task failed on purpose") });
                    }
                    if tick == 40 {
                        eprintln!("failures: losing the GPU device");
                        window.debug_lose_gpu();
                    }
                    if tick == 150 {
                        eprintln!("failures: still running at frame {tick}");
                        window.close();
                    }
                }
            });
            std::mem::forget(ticker);
        })
}
