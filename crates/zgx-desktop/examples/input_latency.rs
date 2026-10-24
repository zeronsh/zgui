//! A text field to measure keystroke-to-screen latency against, optionally
//! beside an animation running at the display's rate (`ZGX_ANIMATE=1`), so
//! typed text competes with frames already queued for the display.
//! Run: cargo run --release -p zgx-desktop --example input_latency
//! Measure (Windows): python scripts/windows_latency_probe.py target/release/examples/input_latency.exe
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let animate = std::env::var_os("ZGX_ANIMATE").is_some_and(|v| v != "0");
    Application::new()
        .window(WindowOptions {
            title: "zgx input latency".into(),
            width: 600.,
            height: 240.,
            ..Default::default()
        })
        .run(move |cx| {
            let x = cx.ui.signal(0_f32);
            let value = cx.ui.signal(String::new());
            if animate {
                let (frames, x) = (cx.frames.clone(), x.clone());
                cx.tasks.spawn(async move {
                    let start = frames.next().await.time;
                    loop {
                        let frame = frames.next().await;
                        let t = (frame.time - start).as_secs_f32();
                        x.set(250. + (t * 2.).sin() * 230.);
                    }
                });
            }
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(24.)
                    .gap(16.)
                    .bg(rgb(0x141a24))
                    .text_color(rgb(0xe5edf7))
                    .child(text_input("Type here", value).w(520.).h(40.))
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
