//! Model updates continue while hidden or minimized; restoration displays the latest state.
//! Run with --smoke-test for the timed native lifecycle regression.
use std::time::Duration;
use zgx::{compose::prelude::*, scene::Color, timer::sleep};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx hidden updates".into(),
            width: 480.,
            height: 240.,
            ..Default::default()
        })
        .run(move |cx| {
            let count = cx.ui.signal(0_u32);
            let color = cx.ui.signal(Color(180, 40, 40, 255));
            cx.render(column().p(20.).gap(16.).children([
                text("Updates survive hidden windows"),
                text_signal({
                    let count = count.clone();
                    move || format!("Completed updates: {}", count.get())
                }),
                div().w(420.).h(100.).reactive_style({
                    let color = color.clone();
                    move || Styles::new().bg(color.get())
                }),
            ]));
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    sleep(Duration::from_secs(1)).await;
                    window.set_visible(false);
                    println!("HIDDEN requested");
                    sleep(Duration::from_millis(300)).await;
                    for value in 1..=20 {
                        count.set(value);
                        sleep(Duration::from_millis(25)).await;
                    }
                    color.set(Color(40, 180, 80, 255));
                    assert_eq!(count.get(), 20);
                    println!("HIDDEN updated 20");
                    sleep(Duration::from_secs(1)).await;
                    window.set_visible(true);
                    window.request_focus();
                    println!("SHOWN 20");
                    sleep(Duration::from_secs(2)).await;
                    window.set_minimized(true);
                    println!("MINIMIZED requested");
                    sleep(Duration::from_millis(300)).await;
                    for value in 21..=40 {
                        count.set(value);
                        sleep(Duration::from_millis(25)).await;
                    }
                    color.set(Color(40, 100, 220, 255));
                    assert_eq!(count.get(), 40);
                    println!("MINIMIZED updated 40");
                    sleep(Duration::from_secs(1)).await;
                    window.set_minimized(false);
                    window.request_focus();
                    println!("RESTORED 40");
                    sleep(Duration::from_secs(2)).await;
                    window.close();
                    println!("hidden updates smoke passed");
                });
            }
        })?;
    Ok(())
}
