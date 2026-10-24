//! Declarative keyboard interception before editor and native clipboard defaults.
//! Run with `--smoke-test` for the owned X11 regression script.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx event hooks".into(),
            width: 600.,
            height: 310.,
            ..Default::default()
        })
        .run(move |cx| {
            let captured = cx.ui.signal(String::new());
            let plain = cx.ui.signal("seed".to_owned());
            let counts = Rc::new([const { Cell::new(0) }; 4]);
            let hook_counts = counts.clone();
            let intercepted = text_input("Intercepted", captured.clone())
                .size(540., 40.)
                .p(8.)
                .on_event(move |cx| {
                    if cx.phase != EventPhase::Target {
                        return;
                    }
                    let InputEvent::KeyDown {
                        key: Key::Character(key),
                        modifiers,
                        ..
                    } = &cx.event
                    else {
                        return;
                    };
                    let index = if modifiers.primary_shortcut() {
                        match key.to_lowercase().as_str() {
                            "c" => Some(1),
                            "x" => Some(2),
                            "v" => Some(3),
                            _ => None,
                        }
                    } else if key.eq_ignore_ascii_case("x") {
                        Some(0)
                    } else {
                        None
                    };
                    if let Some(index) = index {
                        hook_counts[index].set(hook_counts[index].get() + 1);
                        cx.prevent_default();
                    }
                });
            let log = plain.clone();
            cx.render(component(move |context| {
                if smoke {
                    context.retain(
                        context
                            .runtime()
                            .effect(move || println!("PLAIN {:?}", log.get())),
                    );
                }
                column()
                    .p(20.)
                    .gap(10.)
                    .text_size(16.)
                    .child(text("Keyboard hooks run before editing defaults").h(24.))
                    .child(text("Blocks x and clipboard shortcuts; spaces still insert").h(20.))
                    .child(intercepted)
                    .child(text("Ordinary editor: copy, cut, paste and spaces").h(20.))
                    .child(text_input("Ordinary", plain).size(540., 40.).p(8.))
            }));
            if smoke {
                cx.on_closed(move || {
                    println!(
                        "HOOKS captured={:?} printable={} copy={} cut={} paste={}",
                        captured.get(),
                        counts[0].get(),
                        counts[1].get(),
                        counts[2].get(),
                        counts[3].get()
                    )
                });
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(8)).await;
                    window.close();
                });
            }
        })
}
