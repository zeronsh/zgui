//! Pass a GIF path, or run the generated three-frame animation.
use std::{sync::Arc, time::Duration};
use zgx::{
    animation::{Animation, Frame, LoopCount},
    compose::prelude::*,
    image::ImageData,
};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let smoke = args.iter().any(|a| a == "--smoke-test");
    let animation = if let Some(path) = args.iter().find(|a| !a.starts_with("--")) {
        zgx_gpu::assets::decode_gif(&std::fs::read(path)?)?
    } else {
        Arc::new(Animation::new(
            [[229, 94, 83, 255], [82, 190, 145, 255], [87, 141, 222, 255]]
                .into_iter()
                .map(|color| Frame {
                    image: Arc::new(ImageData::new(120, 80, color.repeat(120 * 80)).unwrap()),
                    duration: Duration::from_millis(350),
                })
                .collect(),
            LoopCount::Infinite,
        )?)
    };
    Application::new()
        .window(WindowOptions {
            title: "zgx animated image".into(),
            width: 560.,
            height: 360.,
            ..Default::default()
        })
        .run(move |cx| {
            let playing = cx.ui.signal(true);
            cx.render(
                column()
                    .p(32.)
                    .gap(20.)
                    .text_color(rgb(0xe5edf7))
                    .child(text("Retained animation").text_size(24.))
                    .child(button().p(12.).child(text("Pause / resume")).on_click({
                        let playing = playing.clone();
                        move || {
                            playing.set(!playing.get());
                        }
                    }))
                    .child(
                        animated_image_controlled("Animated preview", animation.clone(), playing)
                            .size(240., 160.),
                    ),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_millis(1400)).await;
                    window.close();
                });
            }
        })
}
