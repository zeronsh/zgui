//! SVG pixels are decoded once; changing their transform reuses the GPU texture.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let source = std::sync::Arc::new(SvgData::new(
        &br##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100" viewBox="0 0 160 100"><rect x="4" y="4" width="152" height="92" rx="16" fill="#243b53"/><path d="M20 70 L80 15 L140 70 Z" fill="#63dbb1"/><circle cx="80" cy="65" r="12" fill="#ffd166"/></svg>"##[..],
    )?);
    Application::new()
        .window(WindowOptions {
            title: "zgx SVG transformations".into(),
            width: 700.,
            height: 430.,
            ..Default::default()
        })
        .run(move |cx| {
            let angle = cx.ui.signal(0_f32);
            let scale = cx.ui.signal(1_f32);
            let preview = svg_signal("Rotating SVG", {
                let angle = angle.clone();
                let scale = scale.clone();
                let source = source.clone();
                move || {
                    std::sync::Arc::new(source.transformed(
                        Affine::scale(scale.get(), scale.get()).then(Affine::rotation(angle.get())),
                    ))
                }
            })
            .size(160., 100.);
            cx.render(
                column()
                    .p(32.)
                    .gap(24.)
                    .text_color(rgb(0xe5edf7))
                    .child(text("Retained SVG: rotate and scale").text_size(24.))
                    .child(
                        row()
                            .gap(16.)
                            .child(button().p(12.).child(text("Rotate 30°")).on_click({
                                let angle = angle.clone();
                                move || {
                                    angle.set(angle.get() + std::f32::consts::PI / 6.);
                                }
                            }))
                            .child(button().p(12.).child(text("Toggle scale")).on_click({
                                let scale = scale.clone();
                                move || {
                                    scale.set(if scale.get() == 1. { 1.4 } else { 1. });
                                }
                            })),
                    )
                    .child(row().p(64.).h(240.).child(preview)),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..12 {
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                        angle.set(angle.get() + std::f32::consts::PI / 6.);
                    }
                    window.close();
                });
            }
        })
}
