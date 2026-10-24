//! Display-rate animation workload (see zgx_workload::animation).
use gpui::{prelude::*, *};
use std::time::Instant;
use zgx_workload::{animation::*, Workload};
struct Scene {
    start: Instant,
    frames: u64,
}
impl Render for Scene {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        window.request_animation_frame();
        let t = self.start.elapsed().as_secs_f32();
        let mut root = div().size_full().relative().bg(rgb(BACKGROUND)).text_size(px(18.));
        for i in 0..COLUMNS * ROWS {
            let (x, y) = dot_position(i);
            root = root.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .size(px(DOT))
                    .rounded(px(DOT / 2.))
                    .bg(rgb(DOT_COLOR))
                    .opacity(dot_opacity(i, t)),
            );
        }
        let chars: Vec<char> = SHIMMER.chars().collect();
        let mut line = div()
            .absolute()
            .left(px(SHIMMER_ORIGIN.0))
            .top(px(SHIMMER_ORIGIN.1))
            .flex()
            .flex_row();
        for (i, ch) in chars.iter().enumerate() {
            let a = shimmer_alpha(i, chars.len(), t);
            line = line.child(
                div()
                    .text_color(rgba(0xffffff00 | a as u32))
                    .child(ch.to_string()),
            );
        }
        root.child(line)
    }
}
fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let seconds = Workload::from_env().seconds;
        let bounds = Bounds::centered(None, size(px(960.), px(720.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|_| Scene {
                        start: Instant::now(),
                        frames: 0,
                    })
                },
            )
            .unwrap();
        cx.activate(true);
        if seconds > 0. {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs_f64(seconds))
                    .await;
                let _ = window.update(cx, |scene, _, _| {
                    eprintln!(
                        "animation_frames={} fps={:.1}",
                        scene.frames,
                        scene.frames as f64 / scene.start.elapsed().as_secs_f64()
                    )
                });
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}
