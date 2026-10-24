//! Ordinary retained children in a clipped scroll viewport.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{compose::prelude::*, scene::Color};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx retained scroll".into(),
            width: 650.,
            height: 430.,
            ..Default::default()
        })
        .run(move |cx| {
            let offset = cx.ui.signal(0.0_f32);
            let count = cx.ui.signal(8_usize);
            let tall = cx.ui.signal(false);
            let snapshots = Rc::new(Cell::new(0));
            let viewport = scroll(offset.clone())
                .id("scroll")
                .w(300.)
                .p(8.)
                .bg(rgb(0x203040))
                .reactive_style({
                    let tall = tall.clone();
                    move || Styles::new().h(if tall.get() { 240. } else { 160. })
                })
                .child(keyed(
                    {
                        let count = count.clone();
                        move || (0..count.get()).collect::<Vec<_>>()
                    },
                    |i, _| {
                        div()
                            .w(284.)
                            .h(40.)
                            .bg(Color(224, 64 + i as u8 * 16, 64, 255))
                            .child(
                                text(format!("Row {i}"))
                                    .text_color(rgb(0x101820))
                                    .px(20.)
                                    .py(8.),
                            )
                    },
                ));
            let report = button().size(130., 44.).child(text("Report")).on_click({
                let offset = offset.clone();
                let count = count.clone();
                let tall = tall.clone();
                let snapshots = snapshots.clone();
                move || {
                    snapshots.set(snapshots.get() + 1);
                    println!(
                        "SNAPSHOT {} offset={:.3} count={} tall={}",
                        snapshots.get(),
                        offset.get(),
                        count.get(),
                        tall.get()
                    );
                }
            });
            let bottom = button()
                .size(130., 44.)
                .child(text("Set offset max"))
                .on_click({
                    let offset = offset.clone();
                    move || {
                        offset.set(10000.);
                    }
                });
            let shrink = button()
                .size(130., 44.)
                .child(text("Toggle rows"))
                .on_click({
                    let count = count.clone();
                    move || {
                        count.set(if count.get() == 8 { 3 } else { 8 });
                    }
                });
            let resize = button()
                .size(130., 44.)
                .child(text("Toggle height"))
                .on_click({
                    let tall = tall.clone();
                    move || {
                        tall.set(!tall.get());
                    }
                });
            let view = cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .text_size(14.)
                    .text_color(rgb(0xe5edf7))
                    .child(
                        text("Retained scrolling children")
                            .h(32.)
                            .text_size(22.)
                            .font_bold(),
                    )
                    .child(row().gap(12.).children([report, bottom, shrink, resize]))
                    .child(viewport),
            );
            if smoke {
                let scene = cx.ui.scene.clone();
                let semantics = cx.ui.semantics.clone();
                cx.on_closed(move || {
                    println!(
                        "SCROLL offset={:.3} count={} tall={} snapshots={}",
                        offset.get(),
                        count.get(),
                        tall.get(),
                        snapshots.get()
                    );
                    if let Some(node) = view.find("scroll") {
                        println!("BOUNDS scroll {:?}", scene.borrow().bounds(node));
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
