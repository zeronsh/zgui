//! Two independent updating surfaces; a covered window must not stall the UI loop.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};
fn content(cx: &mut zgx_desktop::WindowContext, name: &'static str) {
    let tick = cx.ui.signal(0u32);
    let read = tick.clone();
    cx.render(
        column()
            .p(20.)
            .child(text_signal(move || format!("{name}: {}", read.get()))),
    );
    cx.tasks.spawn(async move {
        for i in 1..=24 {
            zgx::timer::sleep(Duration::from_millis(250)).await;
            tick.set(i);
            println!("TICK {name} {i}");
        }
    });
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx covered window".into(),
            width: 360.,
            height: 220.,
            ..Default::default()
        })
        .run(|cx| {
            content(cx, "covered");
            cx.windows.open(
                WindowOptions {
                    title: "zgx covering window".into(),
                    width: 600.,
                    height: 400.,
                    ..Default::default()
                },
                |cx| content(cx, "covering"),
            );
            let windows = cx.windows.clone();
            cx.tasks.spawn(async move {
                zgx::timer::sleep(Duration::from_secs(8)).await;
                println!("COMPLETE");
                windows.quit();
            });
            println!("READY");
        })
}
