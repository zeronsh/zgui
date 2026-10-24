//! Declarative slider with native dragging, keyboard input and reactive dimensions.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{compose::prelude::*, text_layout::FontFamily};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx styled slider".into(),
            width: 700.,
            height: 340.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(25.0_f32);
            let disabled = cx.ui.signal(false);
            let wide = cx.ui.signal(false);
            let snapshots = Rc::new(Cell::new(0));
            let control = slider("Volume", value.clone(), 0.0..=100.0)
                .id("slider")
                .h(44.)
                .rounded(6.)
                .bg(rgb(0x253448))
                .reactive_style({
                    let wide = wide.clone();
                    move || Styles::new().w(if wide.get() { 580. } else { 420. })
                })
                .focus(|s| s.border(2.).border_over(true).border_color(rgb(0x88bbff)))
                .disabled_style(|s| s.opacity(0.4))
                .disabled_when({
                    let disabled = disabled.clone();
                    move || disabled.get()
                });
            let lock = button()
                .size(180., 44.)
                .child(text("Toggle disabled"))
                .on_click({
                    let disabled = disabled.clone();
                    move || {
                        disabled.set(!disabled.get());
                    }
                });
            let update = button()
                .size(180., 44.)
                .child(text("Set model 25"))
                .on_click({
                    let value = value.clone();
                    move || {
                        value.set(25.);
                    }
                });
            let resize = button()
                .size(180., 44.)
                .child(text("Toggle width"))
                .on_click({
                    let wide = wide.clone();
                    move || {
                        wide.set(!wide.get());
                    }
                });
            let snapshot = button()
                .size(180., 44.)
                .child(text("Report state"))
                .on_click({
                    let value = value.clone();
                    let disabled = disabled.clone();
                    let wide = wide.clone();
                    let snapshots = snapshots.clone();
                    move || {
                        snapshots.set(snapshots.get() + 1);
                        println!(
                            "SNAPSHOT {} value={:.3} disabled={} wide={}",
                            snapshots.get(),
                            value.get(),
                            disabled.get(),
                            wide.get()
                        );
                    }
                });
            let view = cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .font_family(FontFamily::SansSerif)
                    .text_size(18.)
                    .text_color(rgb(0xe5edf7))
                    .child(
                        text("Retained slider — Volume")
                            .h(32.)
                            .text_size(22.)
                            .font_bold(),
                    )
                    .child(control)
                    .child(row().gap(12.).child(lock).child(update).child(resize))
                    .child(snapshot)
                    .child(text_signal({
                        let value = value.clone();
                        let disabled = disabled.clone();
                        move || {
                            format!("Volume: {:.1}    Disabled: {}", value.get(), disabled.get())
                        }
                    })),
            );
            if smoke {
                let scene = cx.ui.scene.clone();
                let semantics = cx.ui.semantics.clone();
                cx.on_closed(move || {
                    println!(
                        "SLIDER value={:.3} disabled={} wide={} snapshots={}",
                        value.get(),
                        disabled.get(),
                        wide.get(),
                        snapshots.get()
                    );
                    if let Some(node) = view.find("slider") {
                        println!("BOUNDS slider {:?}", scene.borrow().bounds(node));
                        println!("SEMANTICS {:?}", semantics.borrow().get(node));
                    }
                });
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(14)).await;
                    window.close();
                });
            }
        })
}
