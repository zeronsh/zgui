//! Keystroke to screen: each key pressed redraws a counter label, the same as
//! the GPUI adapter's `keys`, for `scripts/windows_latency_probe.py` (click
//! the window first: it focuses). `ZGUI_SECONDS` ends it, printing `keys=N`.
use std::time::Duration;
use zgui::{compose::prelude::*, input::InputEvent, timer::sleep};
use zgui_desktop::{Application, WindowOptions};
use zgui_workload::Workload;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    Application::new()
        .window(WindowOptions {
            title: "zgui keys".into(),
            width: 480.,
            height: 200.,
            ..Default::default()
        })
        .run(move |cx| {
            let count = cx.ui.signal(0_u64);
            if seconds > 0. {
                let (window, count) = (cx.window.clone(), count.clone());
                cx.tasks.spawn(async move {
                    sleep(Duration::from_secs_f64(seconds)).await;
                    eprintln!("keys={}", count.get());
                    window.close();
                });
            }
            let label = count.clone();
            cx.render(
                div()
                    .w_full()
                    .h_full()
                    .bg(rgb(0x10141c))
                    .text_color(rgb(0xe5edf7))
                    .text_size(20.)
                    .focusable(true)
                    .on_event(move |cx| {
                        if matches!(cx.event, InputEvent::KeyDown { .. }) {
                            count.update(|n| *n += 1);
                        }
                    })
                    .child(
                        text_signal(move || format!("keys {}", label.get()))
                            .absolute()
                            .ml(24.)
                            .mt(24.),
                    ),
            );
        })
}
