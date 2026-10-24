//! Explicit cursors inherit through component children, including disabled regions.
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx cursor styles".into(),
            width: 600.,
            height: 320.,
            ..Default::default()
        })
        .run(|cx| {
            let busy = cx.ui.signal(false);
            cx.render(
                column()
                    .p(24.)
                    .gap(24.)
                    .text_color(rgb(0xffffff))
                    .child(text("Inherited and reactive cursors").text_size(24.))
                    .child(
                        row()
                            .gap(24.)
                            .cursor(Cursor::Move)
                            .child(
                                div()
                                    .size(160., 100.)
                                    .p(16.)
                                    .bg(rgb(0x224460))
                                    .child(text("Move handle")),
                            )
                            .child(
                                div()
                                    .size(160., 100.)
                                    .p(16.)
                                    .bg(rgb(0x663344))
                                    .disabled(true)
                                    .cursor(Cursor::NotAllowed)
                                    .child(text("Disabled")),
                            )
                            .child(
                                div()
                                    .size(160., 100.)
                                    .p(16.)
                                    .bg(rgb(0x226644))
                                    .reactive_style({
                                        let busy = busy.clone();
                                        move || {
                                            Styles::new().cursor(if busy.get() {
                                                Cursor::Wait
                                            } else {
                                                Cursor::Crosshair
                                            })
                                        }
                                    })
                                    .child(text("Reactive")),
                            ),
                    )
                    .child(
                        button()
                            .p(12.)
                            .child(text("Toggle busy"))
                            .on_click(move || {
                                busy.set(!busy.get());
                            }),
                    ),
            );
        })
}
