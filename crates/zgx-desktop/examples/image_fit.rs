//! Centered image fitting keeps decoration and padding outside the image viewport.
use std::{sync::Arc, time::Duration};
use zgx::{compose::prelude::*, image::ImageData};
use zgx_desktop::{Application, WindowOptions};

fn source() -> Arc<ImageData> {
    let colors = [
        [224, 48, 48, 255],
        [48, 208, 80, 255],
        [48, 96, 224, 255],
        [48, 208, 208, 255],
        [208, 48, 208, 255],
        [224, 208, 48, 255],
    ];
    let mut pixels = Vec::with_capacity(120 * 60 * 4);
    for y in 0..60 {
        for x in 0..120 {
            pixels.extend_from_slice(&colors[(y / 30) * 3 + x / 40]);
        }
    }
    Arc::new(ImageData::new(120, 60, pixels).expect("six-color source"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx image fitting".into(),
            width: 500.,
            height: 320.,
            ..Default::default()
        })
        .run(move |cx| {
            let fit = cx.ui.signal(ObjectFit::Fill);
            let mut controls = column().w(100.).shrink_0().gap(6.);
            for (label, next) in [
                ("Fill", ObjectFit::Fill),
                ("Contain", ObjectFit::Contain),
                ("Cover", ObjectFit::Cover),
                ("None", ObjectFit::None),
                ("Scale down", ObjectFit::ScaleDown),
            ] {
                let fit = fit.clone();
                controls =
                    controls.child(button().w_full().h(32.).p(4.).child(text(label)).on_click(
                        move || {
                            fit.set(next);
                            println!("FIT {next:?}");
                        },
                    ));
            }
            cx.render(
                column()
                    .w_full()
                    .p(20.)
                    .gap(12.)
                    .text_size(16.)
                    .child(text("Image fit modes").h(24.))
                    .child(
                        row().w_full().gap(20.).child(controls).child(
                            image("Six color regions", source())
                                .grow()
                                .flex_shrink(1.)
                                // Narrower than its pixels in a small window,
                                // as CSS's `min-width: 0` lets an `<img>` be.
                                .min_w(0.)
                                .h(180.)
                                .p(20.)
                                .bg(rgb(0x203040))
                                .reactive_style(move || Styles::new().object_fit(fit.get())),
                        ),
                    ),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(14)).await;
                    window.close();
                });
            }
        })
}
