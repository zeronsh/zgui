//! Retained image sources with intrinsic and allocated sizes.
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};
use zgx::{compose::prelude::*, image::ImageData};
use zgx_desktop::{Application, WindowOptions};

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Arc<ImageData> {
    Arc::new(ImageData::new(width, height, rgba.repeat((width * height) as usize)).unwrap())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx retained images".into(),
            width: 700.,
            height: 330.,
            ..Default::default()
        })
        .run(move |cx| {
            let red = solid(80, 40, [224, 64, 64, 255]);
            let blue = solid(80, 40, [64, 128, 224, 255]);
            let green = solid(120, 60, [64, 192, 96, 255]);
            let source = cx.ui.signal(red.clone());
            let width = cx.ui.signal(200.0_f32);
            let stage = Rc::new(Cell::new(0));
            let intrinsic = image_signal("Intrinsic preview", {
                let source = source.clone();
                move || source.get()
            })
            .id("intrinsic")
            .p(4.)
            .bg(rgb(0x203040));
            let allocated = image_signal("Allocated preview", {
                let source = source.clone();
                move || source.get()
            })
            .id("allocated")
            .h(80.)
            .p(4.)
            .bg(rgb(0x203040))
            .reactive_style({
                let width = width.clone();
                move || Styles::new().w(width.get())
            });
            let next = button()
                .size(220., 44.)
                .child(text("Next image state"))
                .on_click({
                    let source = source.clone();
                    let width = width.clone();
                    let stage = stage.clone();
                    move || {
                        stage.set((stage.get() + 1) % 5);
                        source.set(match stage.get() {
                            1 => blue.clone(),
                            2 | 3 => green.clone(),
                            _ => red.clone(),
                        });
                        width.set(if stage.get() == 3 { 300. } else { 200. });
                        println!(
                            "STATE {} source={}x{} allocated={:.0}",
                            stage.get(),
                            source.get().width(),
                            source.get().height(),
                            width.get()
                        );
                    }
                });
            let view = cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .text_size(20.)
                    .text_color(rgb(0xe5edf7))
                    .child(text("Retained image sources").h(32.).font_bold())
                    .child(next)
                    .child(
                        row()
                            .h(100.)
                            .gap(16.)
                            .child(column().w(180.).child(intrinsic))
                            .child(column().w(400.).child(allocated)),
                    )
                    .child(image("Static swatch", solid(24, 24, [224, 192, 64, 255])).id("static")),
            );
            if smoke {
                let scene = cx.ui.scene.clone();
                let semantics = cx.ui.semantics.clone();
                cx.on_closed(move || {
                    println!(
                        "IMAGES stage={} source={}x{} allocated={:.0}",
                        stage.get(),
                        source.get().width(),
                        source.get().height(),
                        width.get()
                    );
                    for id in ["intrinsic", "allocated", "static"] {
                        if let Some(node) = view.find(id) {
                            println!("BOUNDS {id} {:?}", scene.borrow().bounds(node));
                            println!("SEMANTICS {id} {:?}", semantics.borrow().get(node));
                        }
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
