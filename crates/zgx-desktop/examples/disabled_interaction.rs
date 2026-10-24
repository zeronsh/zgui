//! Press D during a gesture to disable its ancestor, then re-enable after one second.
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
            title: "zgx disabled interaction".into(),
            width: 460.,
            height: 190.,
            ..Default::default()
        })
        .run(move |cx| {
            let disabled = cx.ui.signal(false);
            let accepted = cx.ui.signal(0);
            let amount = cx.ui.signal(10.);
            let trigger = disabled.clone();
            let read = disabled.clone();
            let activate = accepted.clone();
            let tasks = cx.tasks.clone();
            let root_events = move |event: &mut zgx::input::EventContext| {
                if event.phase != EventPhase::Capture {
                    return;
                }
                let keyboard = matches!(
                    &event.event,
                    InputEvent::KeyDown { key: Key::Character(key), repeat: false, .. }
                        if key.eq_ignore_ascii_case("d")
                );
                if keyboard {
                    event.prevent_default();
                    trigger.set(true);
                    let reset = trigger.clone();
                    tasks.spawn(async move {
                        zgx::timer::sleep(Duration::from_secs(1)).await;
                        reset.set(false);
                    });
                }
            };
            let controls = column()
                .gap(12.)
                .disabled_when(move || read.get())
                .child(
                    button()
                        .id("button")
                        .size(420., 40.)
                        .child(text("Gesture target"))
                        .on_click(move || {
                            activate.update(|n| *n += 1);
                        }),
                )
                .child(slider("Drag target", amount.clone(), 0. ..=100.).size(420., 40.));
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .on_event(root_events)
                    .child(text("D disables controls for one second").h(24.))
                    .child(controls),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("button"));
            if smoke {
                let input = cx.ui.input.clone();
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..160 {
                        println!(
                            "DISABLED disabled={} accepted={} amount={} focus={} capture={}",
                            disabled.get(),
                            accepted.get(),
                            amount.get(),
                            input.focused().is_some(),
                            input.captured().is_some()
                        );
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}
