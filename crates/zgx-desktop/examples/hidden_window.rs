//! A window made hidden, shown later whole (`WindowOptions::visible`), as an
//! app showing an intro of its own first would make its main window: its
//! content is laid out and its first frame drawn while hidden, a counter
//! keeps changing meanwhile, and `set_visible(true)` after
//! `ZGX_SHOW_AFTER_MS` (1500) shows it with the counter as it is then.
//! Prints `HIDDEN`, `SHOW` (as it asks to show) and `COMPLETE`, for
//! `scripts/macos_hidden_window_probe.py` and the Windows and X11 checks.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx::scene::Color;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let show_after = std::env::var("ZGX_SHOW_AFTER_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .unwrap_or(1500);
    Application::new()
        .window(WindowOptions {
            title: "zgx hidden window".into(),
            width: 640.,
            height: 420.,
            visible: false,
            ..Default::default()
        })
        .run(move |cx| {
            let count = cx.ui.signal(0u32);
            let read = count.clone();
            let rows = (0..12).map(|row| {
                div()
                    .h(24.)
                    .mt(4.)
                    .rounded(4.)
                    .bg(Color(40 + row * 12, 90, 200 - row * 8, 255))
                    .child(
                        text(format!("row {row} of a window drawn while hidden"))
                            .text_size(14.)
                            .ml(8.),
                    )
            });
            cx.render(
                column()
                    .p(16.)
                    .bg(Color(250, 248, 240, 255))
                    .child(
                        text_signal(move || format!("Ready {}", read.get()))
                            .text_size(28.)
                            .text_color(Color(30, 30, 40, 255)),
                    )
                    .children(rows),
            );
            let window = cx.window.clone();
            let windows = cx.windows.clone();
            cx.tasks.spawn(async move {
                // Changes while hidden: drawn only as it shows.
                for tick in 1..=show_after / 50 {
                    zgx::timer::sleep(Duration::from_millis(50)).await;
                    count.set(tick as u32);
                }
                println!("SHOW {}", count.with_untracked(|n| *n));
                window.set_visible(true);
                zgx::timer::sleep(Duration::from_secs(3)).await;
                println!("COMPLETE");
                windows.quit();
            });
            println!("HIDDEN");
        })
}
