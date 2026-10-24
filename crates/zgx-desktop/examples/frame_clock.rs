//! Display-paced animation with a window frame-rate cap and a per-request limit.
//! Run: cargo run --release -p zgx-desktop --example frame_clock
//!
//! `--smoke-test` measures delivered frame rates for a few seconds and checks
//! them against the display's refresh rate, a 30 Hz window cap and a 20 Hz
//! request limit, then that nothing is delivered while no animation waits.
use std::time::Duration;
use zgx::{compose::prelude::*, frame::FrameClock, timer::sleep};
use zgx_desktop::{Application, WindowHandle, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx frame clock".into(),
            width: 520.,
            height: 240.,
            ..Default::default()
        })
        .run(move |cx| {
            let x = cx.ui.signal(0_f32);
            let label = cx.ui.signal(String::from("measuring…"));
            let frames = cx.frames.clone();
            // In smoke mode the measurements are the only frame requests, so
            // idle behaviour is observable afterwards.
            if !smoke {
                let (frames, x) = (frames.clone(), x.clone());
                cx.tasks.spawn(async move {
                    let start = frames.next().await.time;
                    loop {
                        let frame = frames.next().await;
                        let t = (frame.time - start).as_secs_f32();
                        x.set(200. + (t * 2.).sin() * 180.);
                    }
                });
            }
            if smoke {
                let (window, label) = (cx.window.clone(), label.clone());
                cx.tasks
                    .spawn(async move { smoke_test(frames, window, label).await });
            }
            let shown = label.clone();
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(24.)
                    .gap(16.)
                    .bg(rgb(0x141a24))
                    .text_color(rgb(0xe5edf7))
                    .child(text_signal(move || shown.get()).text_size(14.))
                    .child(
                        overlay().w_full().h(60.).child(
                            div()
                                .absolute()
                                .size(60., 60.)
                                .rounded(14.)
                                .bg(rgb(0x6ea8ff))
                                .reactive_style(move || Styles::new().translate(x.get(), 0.)),
                        ),
                    ),
            );
        })
}

/// Frames per second delivered to `next` over `seconds`, and the display rate.
async fn measure(frames: &FrameClock, seconds: f64, limit: Option<f64>) -> (f64, f64) {
    let next = || match limit {
        Some(hz) => frames.next().max_rate(hz),
        None => frames.next(),
    };
    let first = next().await;
    let (mut last, mut count) = (first, 0_u32);
    while (last.time - first.time).as_secs_f64() < seconds {
        last = next().await;
        count += 1;
    }
    let elapsed = (last.time - first.time).as_secs_f64();
    (count as f64 / elapsed, last.refresh_rate())
}

async fn smoke_test(
    frames: FrameClock,
    window: WindowHandle,
    label: zgx::reactive::Signal<String>,
) {
    // Let the window appear before measuring.
    sleep(Duration::from_millis(500)).await;
    let (uncapped, display) = measure(&frames, 1.5, None).await;
    println!("FRAME_CLOCK uncapped={uncapped:.1} display={display:.1}");

    window.set_max_frame_rate(Some(30.));
    let (capped, _) = measure(&frames, 1.5, None).await;
    println!("FRAME_CLOCK window_cap=30 measured={capped:.1}");
    window.set_max_frame_rate(None);

    let (limited, _) = measure(&frames, 1.5, Some(20.)).await;
    println!("FRAME_CLOCK request_limit=20 measured={limited:.1}");

    // Nothing waits now, so the source must stop delivering.
    let idle_from = frames.last().map(|frame| frame.index);
    sleep(Duration::from_millis(400)).await;
    let idle_to = frames.last().map(|frame| frame.index);
    println!("FRAME_CLOCK idle_delivered={}", idle_from != idle_to);
    label.set(format!(
        "display {display:.0} Hz · uncapped {uncapped:.0} · capped {capped:.0} · limited {limited:.0}"
    ));

    // Rates are whole divisions of the display rate. Allow for scheduling jitter
    // on shared CI machines, but never for exceeding a cap.
    let divided = |cap: f64| display / zgx::frame::divisor(display, cap) as f64;
    let mut failures = Vec::new();
    if uncapped < display * 0.7 || uncapped > display * 1.05 {
        failures.push(format!(
            "uncapped {uncapped:.1} Hz vs display {display:.1} Hz"
        ));
    }
    if capped > divided(30.) * 1.05 || capped < divided(30.) * 0.7 {
        failures.push(format!("30 Hz cap delivered {capped:.1} Hz"));
    }
    if limited > divided(20.) * 1.05 || limited < divided(20.) * 0.7 {
        failures.push(format!("20 Hz limit delivered {limited:.1} Hz"));
    }
    if idle_from != idle_to {
        failures.push("frames were delivered with no request waiting".into());
    }
    window.close();
    if failures.is_empty() {
        println!("frame clock smoke passed");
    } else {
        for failure in &failures {
            println!("FRAME_CLOCK FAIL {failure}");
        }
        std::process::exit(1);
    }
}
