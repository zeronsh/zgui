//! Native accessibility-bus probe using ordinary component controls.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx accessibility probe".into(),
            width: 600.,
            height: 440.,
            ..Default::default()
        })
        .run(move |cx| {
            let name = cx.ui.signal("Ada".to_owned());
            let count = cx.ui.signal(0_u32);
            let amount = cx.ui.signal(25_f32);
            let checked = cx.ui.signal(false);
            let disabled = cx.ui.signal(false);
            let read_count = count.clone();
            let increment = count.clone();
            let read_disabled = disabled.clone();
            let toggle = disabled.clone();
            let view = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(14.)
                    .text_size(18.)
                    .child(text("Native accessibility controls"))
                    .child(
                        text_input("AT-SPI name", name.clone())
                            .w_full()
                            .h(44.)
                            .p(8.)
                            .disabled_when(move || read_disabled.get()),
                    )
                    .child(button().h(38.).child("AT-SPI count").on_click(move || {
                        increment.update(|n| *n += 1);
                    }))
                    .child(text_signal(move || format!("Count: {}", read_count.get())))
                    .child(checkbox("AT-SPI option", checked.clone()))
                    .child(slider("AT-SPI amount", amount.clone(), 0. ..=100.).w_full())
                    .child(button().h(38.).child("Disable input").on_click(move || {
                        toggle.update(|v| *v = !*v);
                    })),
            );
            cx.ui.bind(view.node(), move || {
                println!(
                    "ATSPI name={:?} count={} amount={} checked={} disabled={}",
                    name.get(),
                    count.get(),
                    amount.get(),
                    checked.get(),
                    disabled.get()
                );
            });
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(45)).await;
                    window.close();
                });
            }
        })
}
