//! A window edged in red on every side, for `scripts/windows_resize_probe.py`:
//! while it is resized, maximized or snapped, a frame drawn for its new size
//! shows the red edge along the window's right and bottom; a stale frame
//! shows it inside them, or nothing past them. `RESIZE_PROBE_LOAD=n` fills it
//! with `n` wrapping labels, so each new size costs a real layout.
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let load: usize = std::env::var("RESIZE_PROBE_LOAD")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    Application::new()
        .window(WindowOptions {
            title: "Resize probe".into(),
            width: 600.,
            height: 400.,
            ..Default::default()
        })
        .run(move |cx| {
            let labels = (0..load).map(|n| {
                div()
                    .p(4.)
                    .rounded(4.)
                    .bg(rgb(0x3070d0))
                    .child(text(format!("label {n}")))
            });
            cx.render(
                div()
                    .w_full()
                    .h_full()
                    .bg(rgb(0x2060c0))
                    .border(8.)
                    .border_color(rgb(0xff0000))
                    .p(16.)
                    .child(
                        row()
                            .w_full()
                            .flex_wrap()
                            .gap(4.)
                            .overflow_hidden()
                            .children(labels),
                    ),
            );
        })
}
