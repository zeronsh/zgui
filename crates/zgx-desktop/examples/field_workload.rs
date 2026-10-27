//! A focused text field left alone (the suite's field scene): zgx's text
//! input, focused as it mounts, its caret blinking as the system's. On
//! macOS Core Animation blinks it, so the window draws no frame meanwhile.
use std::time::Duration;
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::WindowOptions;
use zgx_workload::{Workload, blink::*};

#[path = "support/bench_font.rs"]
mod bench_font;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    bench_font::application()?
        .window(WindowOptions {
            title: "zgx field".into(),
            width: 960.,
            height: 720.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(String::from("Reading, not typing"));
            if seconds > 0. {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    sleep(Duration::from_secs_f64(seconds)).await;
                    eprintln!("workload_ticks=0");
                    window.close();
                });
            }
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(40.)
                    .bg(rgb(BACKGROUND))
                    .text_color(rgb(CARET_COLOR))
                    .child(text_input("Field", value).w(400.).h(32.).on_mount(|field| {
                        field.focus();
                    })),
            );
        })
}
