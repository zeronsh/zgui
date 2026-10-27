//! A caret blinking in a still window (see zgx_workload::blink): GPUI
//! redraws the window each phase, when the view is notified.
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};
use zgx_workload::{Workload, blink::*};
struct Scene {
    shown: bool,
    frames: u64,
}
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        let (x, y, w, h) = CARET;
        let root = div().size_full().relative().bg(rgb(BACKGROUND));
        if self.shown {
            root.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(w))
                    .h(px(h))
                    .bg(rgb(CARET_COLOR)),
            )
        } else {
            root
        }
    }
}
fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        // The comparisons' one font file (comparisons/fonts), as every framework loads it.
        if let Some(bytes) = zgx_workload::font_file() {
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Owned(bytes)])
                .unwrap();
        }
        let seconds = Workload::from_env().seconds;
        let bounds = Bounds::centered(None, size(px(960.), px(720.)), cx);
        let start = Instant::now();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|_| Scene {
                        shown: true,
                        frames: 0,
                    })
                },
            )
            .unwrap();
        cx.activate(true);
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(PHASE_MS))
                    .await;
                if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                    let _ =
                        window.update(cx, |scene, _, _| eprintln!("blink_frames={} workload_ticks=0", scene.frames));
                    let _ = cx.update(|cx| cx.quit());
                    return;
                }
                let _ = window.update(cx, |scene, _, cx| {
                    scene.shown = !scene.shown;
                    cx.notify();
                });
            }
        })
        .detach();
    });
}
