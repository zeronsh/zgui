//! Declarative progress: compositor-only value updates and reactive allocation.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx styled progress".into(),
            width: 700.,
            height: 260.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(0.0_f32);
            let width = cx.ui.signal(400.0_f32);
            let stage = Rc::new(Cell::new(0));
            let indicator = progress("Download", value.clone())
                .id("progress")
                .h(40.)
                .p(4.)
                .bg(rgb(0x203040))
                .reactive_style({
                    let width = width.clone();
                    move || Styles::new().w(width.get())
                });
            let next = button()
                .size(220., 44.)
                .child(text("Next state"))
                .on_click({
                    let value = value.clone();
                    let width = width.clone();
                    let stage = stage.clone();
                    move || {
                        stage.set((stage.get() + 1) % 7);
                        let (next_value, next_width) = match stage.get() {
                            1 => (0.25, 400.),
                            2 => (1., 400.),
                            3 => (1., 600.),
                            4 => (0.25, 600.),
                            5 => (0.25, 200.),
                            // (NaN would be indeterminate: a sweep.)
                            6 => (0., 200.),
                            _ => (0., 400.),
                        };
                        width.set(next_width);
                        value.set(next_value);
                        println!(
                            "STATE {} value={:.3} width={:.0}",
                            stage.get(),
                            value.get(),
                            width.get()
                        );
                    }
                });
            let view = cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .text_color(rgb(0x40a0e0))
                    .text_size(20.)
                    .child(text("Retained progress — Download").h(32.).font_bold())
                    .child(indicator)
                    .child(next)
                    .child(text_signal({
                        let value = value.clone();
                        move || format!("Progress: {:.0}%", value.get() * 100.)
                    })),
            );
            if smoke {
                let scene = cx.ui.scene.clone();
                let semantics = cx.ui.semantics.clone();
                cx.on_closed(move || {
                    println!(
                        "PROGRESS value={:.3} width={:.0} stage={}",
                        value.get(),
                        width.get(),
                        stage.get()
                    );
                    if let Some(node) = view.find("progress") {
                        println!("BOUNDS progress {:?}", scene.borrow().bounds(node));
                        println!("SEMANTICS {:?}", semantics.borrow().get(node));
                    }
                });
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(12)).await;
                    window.close();
                });
            }
        })
}
