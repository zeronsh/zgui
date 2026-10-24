//! Retained vector paths, gradients and patterns through component children.
use zgx::{compose::prelude::*, scene::Rect};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|a| a == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx retained canvas".into(),
            width: 760.,
            height: 420.,
            ..Default::default()
        })
        .run(move |cx| {
            let shifted = cx.ui.signal(false);
            cx.render(
                column()
                    .w_full()
                    .p(24.)
                    .gap(16.)
                    .text_color(rgb(0xe0eaf5))
                    .child(text("Paths, gradients and patterns").text_size(24.))
                    .child(button().p(10.).child(text("Change curve")).on_click({
                        let shifted = shifted.clone();
                        move || {
                            shifted.set(!shifted.get());
                        }
                    }))
                    .child(
                        canvas({
                            let shifted = shifted.clone();
                            move |(w, h)| {
                                let mut drawing = Canvas::new();
                                drawing.fill(
                                    Path::rectangle(Rect::new(0., 0., w, h)),
                                    Brush::linear(
                                        Point::new(0., 0.),
                                        Point::new(w, h),
                                        vec![
                                            GradientStop {
                                                offset: 0.,
                                                color: rgb(0x23395d),
                                            },
                                            GradientStop {
                                                offset: 1.,
                                                color: rgb(0x0c1220),
                                            },
                                        ],
                                    )
                                    .unwrap(),
                                );
                                let mut path = Path::builder();
                                path.move_to(20., h * 0.5)
                                    .quadratic_to(w * 0.2, 10., w * 0.4, h * 0.5)
                                    .cubic_to(
                                        Point::new(
                                            w * 0.6,
                                            if shifted.get() { h * 0.1 } else { h * 0.9 },
                                        ),
                                        Point::new(w * 0.7, h * 0.1),
                                        Point::new(w - 20., h * 0.5),
                                    );
                                drawing.stroke(
                                    path.build().unwrap(),
                                    rgb(0x63dbb1),
                                    Stroke {
                                        width: 5.,
                                        cap: LineCap::Round,
                                        ..Default::default()
                                    },
                                );
                                drawing.fill(
                                    Path::rectangle(Rect::new(w * 0.3, h * 0.72, w * 0.4, h * 0.2)),
                                    Brush::Slash {
                                        background: rgb(0x243b53),
                                        foreground: rgb(0xf7bb60),
                                        spacing: 14.,
                                        width: 4.,
                                    },
                                );
                                drawing
                            }
                        })
                        .w_full()
                        .h(250.),
                    ),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    shifted.set(true);
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    window.close();
                });
            }
        })
}
