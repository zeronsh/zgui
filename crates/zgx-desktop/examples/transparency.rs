//! Native compositor alpha probe. Empty client pixels remain transparent.
use std::time::Duration;
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx transparency".into(),
            width: 320.,
            height: 240.,
            transparent: true,
            ..Default::default()
        })
        .run(move |cx| {
            let blue = cx.ui.signal(false);
            let visible = cx.ui.signal(true);
            let close = cx.window.clone();
            let root = overlay()
                .w_full()
                .h_full()
                .focusable(true)
                .on_event({
                    let blue = blue.clone();
                    let visible = visible.clone();
                    move |event| {
                        if event.phase == EventPhase::Bubble {
                            return;
                        }
                        if let InputEvent::KeyDown {
                            key, repeat: false, ..
                        } = &event.event
                        {
                            match key {
                                Key::Character(key) if key.eq_ignore_ascii_case("c") => {
                                    blue.set(!blue.get());
                                    println!("COLOR blue={}", blue.get());
                                }
                                Key::Character(key) if key.eq_ignore_ascii_case("v") => {
                                    visible.set(!visible.get());
                                    println!("VISIBLE {}", visible.get());
                                }
                                Key::Escape => close.close(),
                                _ => return,
                            }
                            event.prevent_default();
                        }
                    }
                })
                // A: [20,140) x [20,100), alpha 128. C switches red/blue;
                // V removes its pixels while retaining the same layout node.
                .child(
                    div()
                        .id("alpha")
                        .absolute()
                        .ml(20.)
                        .mt(20.)
                        .size(120., 80.)
                        .reactive_style({
                            let blue = blue.clone();
                            let visible = visible.clone();
                            move || {
                                Styles::new()
                                    .bg(if blue.get() {
                                        rgba(0x2060e080)
                                    } else {
                                        rgba(0xe0402080)
                                    })
                                    .opacity(if visible.get() { 1. } else { 0. })
                            }
                        }),
                )
                // B overlaps A only on [100,140) x [60,100), alpha64.
                .child(
                    div()
                        .id("overlap")
                        .absolute()
                        .ml(100.)
                        .mt(60.)
                        .size(120., 80.)
                        .bg(rgba(0x40e08040)),
                )
                .child(
                    button()
                        .id("opaque")
                        .absolute()
                        .ml(20.)
                        .mt(160.)
                        .size(120., 40.)
                        .bg(rgb(0xeeeeee))
                        .text_color(rgb(0x102030))
                        .child(text("Close"))
                        .on_click({
                            let window = cx.window.clone();
                            move || window.close()
                        }),
                );
            let view = cx.render(root);
            cx.ui.input.focus(&cx.ui.scene, Some(view.node()));
            cx.on_closed(move || {
                println!("TRANSPARENCY blue={} visible={}", blue.get(), visible.get())
            });
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(30)).await;
                    window.close();
                });
            }
        })
}
