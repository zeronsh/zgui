//! A focused text field left alone: its caret blinks and nothing else draws,
//! for measuring what an idle window with a blinking caret costs (footprint,
//! CPU, the window server). `ZGX_FOCUS=0` leaves the field unfocused; the
//! caret's blink follows the system's (`-NSTextInsertionPointBlinkPeriodOn
//! 3600000` on the command line holds it shown on macOS).
//! Run: cargo run --release -p zgx-desktop --example caret_idle
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let focus = !zgx::env::var_os("ZGX_FOCUS").is_some_and(|v| v == "0");
    Application::new()
        .window(WindowOptions {
            title: "zgx caret idle".into(),
            width: 600.,
            height: 240.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(String::from("Reading, not typing"));
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(24.)
                    .bg(rgb(0x141a24))
                    .text_color(rgb(0xe5edf7))
                    .child(
                        text_input("Type here", value)
                            .w(520.)
                            .h(40.)
                            .on_mount(move |field| {
                                if focus {
                                    field.focus();
                                }
                            }),
                    ),
            );
        })
}
