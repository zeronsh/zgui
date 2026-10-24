//! Stationary-pointer hit testing across live X11 Xft.dpi changes.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent},
};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx live DPI pointer".into(),
            width: 500.,
            height: 200.,
            ..Default::default()
        })
        .run(move |cx| {
            let counts = Rc::new([const { Cell::new(0) }; 4]);
            let make = |label: &'static str, index: usize| {
                let click = counts.clone();
                let wheel = counts.clone();
                button()
                    .child(text(label))
                    .size(200., 80.)
                    .on_click(move || {
                        click[index].set(click[index].get() + 1);
                        println!("CLICK {label}");
                    })
                    .on_event(move |event| {
                        if event.phase != EventPhase::Capture
                            && matches!(event.event, InputEvent::Scroll { .. })
                        {
                            wheel[index + 2].set(wheel[index + 2].get() + 1);
                            println!("WHEEL {label}");
                            event.prevent_default();
                        }
                    })
            };
            let view = row()
                .p(20.)
                .gap(20.)
                .children([make("left", 0), make("right", 1)])
                .on_event(|event| {
                    if event.phase == EventPhase::Capture
                        && let InputEvent::PointerMove { x, y } = event.event
                    {
                        println!("MOVE {x} {y}");
                    }
                });
            let viewport = cx.viewport.clone();
            cx.render(component(move |context| {
                context.retain(
                    context
                        .runtime()
                        .effect(move || println!("VIEWPORT {:?}", viewport.get())),
                );
                view
            }));
            if smoke {
                cx.on_closed(move || {
                    println!(
                        "DPI_POINTER left_click={} right_click={} left_wheel={} right_wheel={}",
                        counts[0].get(),
                        counts[1].get(),
                        counts[2].get(),
                        counts[3].get()
                    )
                });
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(9)).await;
                    window.close();
                });
            }
        })
}
