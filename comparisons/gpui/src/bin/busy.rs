//! Busy window + one spinner (see zgx_workload::busy). GPUI's best case:
//! the static content is a cached view; only the spinner view re-renders.
use gpui::{prelude::*, *};
use std::time::Instant;
use zgx_workload::{busy::*, Workload};
struct Content;
impl Render for Content {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().size_full().relative().text_size(px(11.)).line_height(px(14.));
        for i in 0..GRID_COLUMNS * GRID_ROWS {
            let (x, y) = cell_position(i);
            root = root.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(CELL.0))
                    .h(px(CELL.1 - 2.))
                    .bg(rgb(cell_color(i)))
                    .text_color(rgb(0xc9d4e3))
                    .child(cell_label(i)),
            );
        }
        for line in 0..PARAGRAPH_LINES {
            root = root.child(
                div()
                    .absolute()
                    .left(px(PARAGRAPH_ORIGIN.0))
                    .top(px(PARAGRAPH_ORIGIN.1 + line as f32 * LINE_HEIGHT))
                    .text_size(px(7.))
                    .line_height(px(LINE_HEIGHT))
                    .text_color(rgb(0x8fa0b8))
                    .child(paragraph_line(line)),
            );
        }
        root
    }
}
struct Spinner {
    start: Instant,
    frames: u64,
}
impl Render for Spinner {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        window.request_animation_frame();
        let t = self.start.elapsed().as_secs_f32();
        let mut root = div().absolute().top_0().left_0().size_full();
        for i in 0..SPINNER_DOTS {
            let (x, y) = dot_position(i);
            root = root.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .size(px(DOT))
                    .rounded(px(DOT / 2.))
                    .bg(rgb(0x9ad2ff))
                    .opacity(dot_opacity(i, t)),
            );
        }
        root
    }
}
struct Root {
    content: Entity<Content>,
    spinner: Entity<Spinner>,
}
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .bg(rgb(0x10141c))
            .child(
                AnyView::from(self.content.clone())
                    .cached(StyleRefinement::default().size_full()),
            )
            .child(self.spinner.clone())
    }
}
fn main() {
    Application::new().run(|cx: &mut App| {
        let seconds = Workload::from_env().seconds;
        let bounds = Bounds::centered(None, size(px(960.), px(720.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    let content = cx.new(|_| Content);
                    let spinner = cx.new(|_| Spinner {
                        start: Instant::now(),
                        frames: 0,
                    });
                    cx.new(|_| Root { content, spinner })
                },
            )
            .unwrap();
        cx.activate(true);
        if seconds > 0. {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs_f64(seconds))
                    .await;
                let _ = window.update(cx, |root, _, cx| {
                    let spinner = root.spinner.read(cx);
                    eprintln!(
                        "animation_frames={} fps={:.1}",
                        spinner.frames,
                        spinner.frames as f64 / spinner.start.elapsed().as_secs_f64()
                    )
                });
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}
