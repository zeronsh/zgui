use gpui::{prelude::*, *};
use std::time::Instant;
use zgx_workload::{row_label, Mode, Workload, PERIOD, ROW_HEIGHT};
struct Lab {
    work: Workload,
    items: Vec<i32>,
    done: i32,
}
fn box_at(x: f32, y: f32, w: f32, h: f32) -> Div {
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}
impl Render for Lab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .size_full()
            .relative()
            .bg(rgb(0x10141c))
            .text_color(rgb(0xe5edf7))
            .text_size(px(14.))
            .line_height(px(20.))
            .font_family("DejaVu Sans")
            .child(
                box_at(20., 20., 920., 28.)
                    .text_size(px(20.))
                    .line_height(px(28.))
                    .child("zgx performance lab"),
            );
        for (id, label, x) in [
            (0, "+".to_owned(), 20.),
            (1, "=".to_owned(), 80.),
            (2, self.done.to_string(), 140.),
        ] {
            root = root.child(
                box_at(x, 55., 50., 32.)
                    .id(id)
                    .bg(rgb(0x293b50))
                    .child(box_at(12., 6., 38., 20.).child(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if id == 2 {
                            this.done = 999;
                            cx.notify();
                            return;
                        }
                        cx.spawn(async move |weak, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::ZERO)
                                .await;
                            let _ = weak.update(cx, |this, cx| {
                                this.done = if id == 0 {
                                    let mut result = 0;
                                    for value in [1, 2, 3] {
                                        if this.items.len() == 4 {
                                            result = -1;
                                            break;
                                        }
                                        this.items.push(value);
                                    }
                                    result
                                } else if let Some(item) = this.items.first_mut() {
                                    *item = 99;
                                    *item = 99;
                                    0
                                } else {
                                    -1
                                };
                                cx.notify();
                            });
                        })
                        .detach();
                    })),
            );
        }
        root = root
            .child(box_at(220., 62., 50., 24.).child(self.items.len().to_string()))
            .child(box_at(280., 62., 80., 24.).child("stable"))
            .child(box_at(370., 62., 50., 24.).child(self.items.len().to_string()))
            .child(
                box_at(20., 110., 920., 160.)
                    .bg(rgb(0x192230))
                    .overflow_hidden()
                    .child(box_at(8., 8., 904., 144.).child(self.work.visible_text())),
            );
        let mut list = box_at(20., 290., 920., 400.)
            .bg(rgb(0x192230))
            .overflow_hidden();
        for index in self.work.row_range() {
            list = list.child(
                box_at(
                    0.,
                    index as f32 * ROW_HEIGHT - self.work.scroll_offset,
                    920.,
                    ROW_HEIGHT,
                )
                .bg(rgb(if index % 2 == 0 { 0x192230 } else { 0x1d2838 }))
                .child(box_at(8., 4., 904., 20.).child(row_label(index))),
            );
        }
        root.child(
            list.on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta: f32 = event.delta.pixel_delta(px(ROW_HEIGHT)).y.into();
                this.work.scroll_offset = (this.work.scroll_offset - delta).clamp(
                    0.,
                    zgx_workload::ROW_COUNT as f32 * ROW_HEIGHT - zgx_workload::VIEWPORT_HEIGHT,
                );
                cx.notify();
            })),
        )
    }
}
fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx: &mut Context<Lab>| {
                    let work = Workload::from_env();
                    let seconds = work.seconds;
                    let idle = work.mode == Mode::Idle;
                    cx.spawn(async move |weak, cx| {
                        if idle && seconds == 0. {
                            return;
                        }
                        let start = Instant::now();
                        let mut deadline = start
                            + if idle {
                                std::time::Duration::from_secs_f64(seconds.max(0.001))
                            } else {
                                PERIOD
                            };
                        loop {
                            cx.background_executor()
                                .timer(deadline.saturating_duration_since(Instant::now()))
                                .await;
                            if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                                let _ = weak.update(cx, |this, _| {
                                    eprintln!("workload_ticks={}", this.work.frames)
                                });
                                let _ = cx.update(|cx| cx.quit());
                                break;
                            }
                            if !idle
                                && weak
                                    .update(cx, |this, cx| {
                                        this.work.tick();
                                        cx.notify();
                                    })
                                    .is_err()
                            {
                                break;
                            }
                            deadline += PERIOD;
                            if deadline <= Instant::now() {
                                deadline = Instant::now() + PERIOD;
                            }
                        }
                    })
                    .detach();
                    Lab {
                        work,
                        items: Vec::with_capacity(4),
                        done: 0,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
