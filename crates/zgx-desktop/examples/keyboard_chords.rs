//! Button activation remains armed across unrelated keyboard input.
use std::time::Duration;
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent},
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx keyboard chords".into(),
            width: 460.,
            height: 240.,
            ..Default::default()
        })
        .run(move |cx| {
            let accepted = cx.ui.signal(0);
            let rejected = cx.ui.signal(0);
            let activate = accepted.clone();
            let reject = rejected.clone();
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .child(text("Button keyboard chords").h(24.))
                    .child(
                        button()
                            .id("button")
                            .size(420., 40.)
                            .bg(rgb(0x203040))
                            .active(|style| style.bg(rgb(0x405060)))
                            .child(text("Space or Enter, with another key held"))
                            .on_click(move || {
                                activate.update(|n| *n += 1);
                            }),
                    )
                    .child(
                        button()
                            .size(420., 40.)
                            .child(text("Activation prevented"))
                            .on_event(|event| {
                                if event.phase == EventPhase::Target
                                    && matches!(event.event, InputEvent::Activate)
                                {
                                    event.prevent_default();
                                }
                            })
                            .on_click(move || {
                                reject.update(|n| *n += 1);
                            }),
                    )
                    .child(button().size(420., 40.).child(text("Move focus here"))),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("button"));
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..100 {
                        println!(
                            "KEYCHORD accepted={} rejected={}",
                            accepted.get(),
                            rejected.get()
                        );
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}
