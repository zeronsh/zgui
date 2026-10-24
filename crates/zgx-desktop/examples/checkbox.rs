//! A retained checkbox with inherited typography, model binding and state styles.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{compose::prelude::*, text_layout::FontFamily};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx styled checkbox".into(),
            width: 680.,
            height: 280.,
            ..Default::default()
        })
        .run(move |cx| {
            let checked = cx.ui.signal(false);
            let disabled = cx.ui.signal(false);
            let activations = Rc::new(Cell::new(0));
            let control = checkbox("Enable notifications", checked.clone())
                .id("checkbox")
                .size(470., 44.)
                .rounded(6.)
                .bg(rgb(0x253448))
                .hover(|s| s.bg(rgb(0x354760)))
                .focus(|s| {
                    s.bg(rgb(0x314d70))
                        .border(2.)
                        .border_over(true)
                        .border_color(rgb(0x88bbff))
                })
                .disabled_style(|s| s.bg(rgb(0x30343b)).text_color(rgb(0x8893a2)))
                .disabled_when({
                    let disabled = disabled.clone();
                    move || disabled.get()
                })
                .on_click({
                    let checked = checked.clone();
                    let activations = activations.clone();
                    move || {
                        activations.set(activations.get() + 1);
                        println!(
                            "TOGGLE checked={} activations={}",
                            checked.get(),
                            activations.get()
                        );
                    }
                });
            let lock = button()
                .size(200., 44.)
                .child(text("Toggle disabled"))
                .on_click({
                    let disabled = disabled.clone();
                    move || {
                        disabled.set(!disabled.get());
                        println!("DISABLED {}", disabled.get());
                    }
                });
            let update = button()
                .size(240., 44.)
                .child(text("Set model checked"))
                .on_click({
                    let checked = checked.clone();
                    move || {
                        checked.set(true);
                        println!("MODEL checked=true");
                    }
                });
            let view = cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .font_family(FontFamily::SansSerif)
                    .text_size(22.)
                    .text_color(rgb(0xe5edf7))
                    .child(text("Retained checkbox").h(32.).font_bold())
                    .child(control)
                    .child(row().gap(12.).child(lock).child(update))
                    .child(text_signal({
                        let checked = checked.clone();
                        let disabled = disabled.clone();
                        move || {
                            format!("Checked: {}    Disabled: {}", checked.get(), disabled.get())
                        }
                    })),
            );
            if smoke {
                let scene = cx.ui.scene.clone();
                let semantics = cx.ui.semantics.clone();
                cx.on_closed(move || {
                    println!(
                        "CHECKBOX checked={} disabled={} activations={}",
                        checked.get(),
                        disabled.get(),
                        activations.get()
                    );
                    if let Some(node) = view.find("checkbox") {
                        println!("BOUNDS checkbox {:?}", scene.borrow().bounds(node));
                        println!("SEMANTICS {:?}", semantics.borrow().get(node));
                    }
                });
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(8)).await;
                    window.close();
                });
            }
        })
}
