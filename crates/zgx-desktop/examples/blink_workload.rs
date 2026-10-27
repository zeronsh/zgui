//! A caret blinking in a still window (`zgx_workload::blink`), as the
//! comparison adapters draw it: a bar shown and hidden every 530 ms. zgx's
//! own caret in a real field is `caret_idle`.
use std::time::Duration;
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::{Application, WindowOptions};
use zgx_workload::{Workload, blink::*};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = Workload::from_env().seconds;
    Application::new()
        .window(WindowOptions {
            title: "zgx blink".into(),
            width: 960.,
            height: 720.,
            ..Default::default()
        })
        .run(move |cx| {
            let shown = cx.ui.signal(true);
            let flip = shown.clone();
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                let start = std::time::Instant::now();
                loop {
                    sleep(Duration::from_millis(PHASE_MS)).await;
                    if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                        window.close();
                        return;
                    }
                    flip.update(|s| *s = !*s);
                }
            });
            let (x, y, w, h) = CARET;
            cx.render(
                div().w_full().h_full().bg(rgb(BACKGROUND)).child(
                    div()
                        .absolute()
                        .ml(x)
                        .mt(y)
                        .size(w, h)
                        .bg(rgb(CARET_COLOR))
                        .reactive_style(move || {
                            Styles::new().opacity(if shown.get() { 1. } else { 0. })
                        }),
                ),
            );
        })
}
