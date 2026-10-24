use zgx::{
    compose::prelude::*,
    scene::{BoxShadow, Insets, Transform},
};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx detailed paint styles".into(),
            width: 780.,
            height: 400.,
            ..Default::default()
        })
        .run(move |cx| {
            let rotated = cx.ui.signal(false);
            let card = column()
                .size(330., 220.)
                .p(24.)
                .gap(12.)
                .rounded_corners(Corners {
                    top_left: 36.,
                    top_right: 6.,
                    bottom_right: 28.,
                    bottom_left: 0.,
                })
                .border_edges(Insets {
                    left: 10.,
                    right: 2.,
                    top: 4.,
                    bottom: 6.,
                })
                .border_color(rgb(0xd8ebff))
                .shadows(vec![
                    BoxShadow {
                        color: rgba(0x4e98ef70),
                        offset: Transform { x: -12., y: 8. },
                        blur_radius: 8.,
                        spread: 0.,
                    },
                    BoxShadow {
                        color: rgba(0xe373b870),
                        offset: Transform { x: 14., y: 10. },
                        blur_radius: 6.,
                        spread: 0.,
                    },
                ])
                .reactive_style({
                    let rotated = rotated.clone();
                    move || {
                        Styles::new().bg_gradient(
                            if rotated.get() { 1.2 } else { 0. },
                            vec![
                                GradientStop {
                                    offset: 0.,
                                    color: rgb(0x245690),
                                },
                                GradientStop {
                                    offset: 1.,
                                    color: rgb(0x462b63),
                                },
                            ],
                        )
                    }
                })
                .child(text("Asymmetric card").text_size(24.))
                .child(
                    text("Independent edges and corners\nTwo retained outer shadows")
                        .text_size(17.),
                )
                .child(
                    button()
                        .p(10.)
                        .bg(rgba(0xffffff20))
                        .child(text("Change gradient"))
                        .on_click({
                            let rotated = rotated.clone();
                            move || {
                                rotated.set(!rotated.get());
                            }
                        }),
                );
            cx.render(
                row()
                    .p(42.)
                    .gap(44.)
                    .text_color(rgb(0xffffff))
                    .child(card)
                    .child(
                        column()
                            .size(280., 220.)
                            .p(24.)
                            .gap(20.)
                            .bg_fill(Brush::Slash {
                                background: rgb(0x172638),
                                foreground: rgb(0x263e54),
                                spacing: 16.,
                                width: 3.,
                            })
                            .rounded(18.)
                            .border(3.)
                            .border_color(rgb(0x72d4ae))
                            .border_style(BorderStyle::Dashed {
                                length: 12.,
                                gap: 8.,
                            })
                            .child(text("Pattern and dashes").text_size(22.))
                            .child(
                                text("Paint changes retain\ncomponent children.").text_size(18.),
                            ),
                    ),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(1500)).await;
                    rotated.set(true);
                    zgx::timer::sleep(std::time::Duration::from_millis(1500)).await;
                    window.close();
                });
            }
        })
}
