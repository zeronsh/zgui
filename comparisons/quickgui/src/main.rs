use quickgui::{
    Application, AsyncContextError, Color, Element, IntoElement, View, ViewContext, WindowOptions,
    button, div, text,
};
use std::time::Instant;
use zgx_workload::{Mode, PERIOD, ROW_HEIGHT, Workload, row_label};
struct Lab {
    work: Workload,
    items: Vec<i32>,
    done: i32,
    started: bool,
}
fn color(hex: u32) -> Color {
    Color::rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
fn box_at(x: f32, y: f32, w: f32, h: f32) -> Element {
    div().absolute().left(x).top(y).w(w).h(h)
}
impl View for Lab {
    fn render(&mut self, cx: &mut ViewContext<'_, Self>) -> impl IntoElement {
        if !self.started {
            self.started = true;
            let seconds = self.work.seconds;
            let idle = self.work.mode == Mode::Idle;
            cx.spawn(move |task| async move {
                if idle && seconds == 0. {
                    return Ok::<(), AsyncContextError>(());
                }
                let start = Instant::now();
                let mut deadline = start
                    + if idle {
                        std::time::Duration::from_secs_f64(seconds.max(0.001))
                    } else {
                        PERIOD
                    };
                loop {
                    task.sleep(deadline.saturating_duration_since(Instant::now()))
                        .await?;
                    if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                        task.update(|this, cx| {
                            eprintln!("workload_ticks={}", this.work.frames);
                            cx.close_window();
                        })
                        .await?;
                        break;
                    }
                    if !idle {
                        task.update(|this, cx| {
                            this.work.tick();
                            cx.invalidate();
                        })
                        .await?;
                    }
                    deadline += PERIOD;
                    if deadline <= Instant::now() {
                        deadline = Instant::now() + PERIOD;
                    }
                }
                Ok::<(), AsyncContextError>(())
            })
            .expect("timer task")
            .detach();
        }
        let mut root = div()
            .size_full()
            .relative()
            .bg(color(0x10141c))
            .text_color(color(0xe5edf7))
            .text_size(14.)
            .line_height(20.)
            .font_family("DejaVu Sans")
            .child(
                box_at(20., 20., 920., 28.)
                    .child(text("zgx performance lab").text_size(20.).line_height(28.)),
            );
        for (id, label, x) in [
            (0, "+".to_owned(), 20.),
            (1, "=".to_owned(), 80.),
            (2, self.done.to_string(), 140.),
        ] {
            let callback = cx.listener(format!("control-{id}"), move |this, cx| {
                if id == 2 {
                    this.done = 999;
                    cx.invalidate();
                    return;
                }
                cx.spawn(move |task: quickgui::AsyncViewContext<Lab>| async move {
                    task.sleep(std::time::Duration::ZERO).await?;
                    task.update(move |this, cx| {
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
                        cx.invalidate();
                    })
                    .await?;
                    Ok::<(), AsyncContextError>(())
                })
                .expect("button task")
                .detach();
            });
            root = root.child(
                button()
                    .absolute()
                    .left(x)
                    .top(55.)
                    .w(50.)
                    .h(32.)
                    .bg(color(0x293b50))
                    .child(box_at(12., 6., 38., 20.).child(text(label)))
                    .on_click(callback),
            );
        }
        root = root
            .child(box_at(220., 62., 50., 24.).child(text(self.items.len().to_string())))
            .child(box_at(280., 62., 80., 24.).child(text("stable")))
            .child(box_at(370., 62., 50., 24.).child(text(self.items.len().to_string())))
            .child(
                box_at(20., 110., 920., 160.)
                    .bg(color(0x192230))
                    .overflow_hidden()
                    .child(box_at(8., 8., 904., 144.).child(text(self.work.visible_text()))),
            );
        let mut list = box_at(20., 290., 920., 400.)
            .bg(color(0x192230))
            .overflow_hidden();
        for index in self.work.row_range() {
            list = list.child(
                box_at(
                    0.,
                    index as f32 * ROW_HEIGHT - self.work.scroll_offset,
                    920.,
                    ROW_HEIGHT,
                )
                .bg(color(if index % 2 == 0 { 0x192230 } else { 0x1d2838 }))
                .child(box_at(8., 4., 904., 20.).child(text(row_label(index)))),
            );
        }
        let wheel = cx.scroll_wheel_listener("list-wheel", |this, event, cx| {
            this.work.scroll_offset =
                (this.work.scroll_offset - event.delta.pixel_delta(ROW_HEIGHT).y).clamp(
                    0.,
                    zgx_workload::ROW_COUNT as f32 * ROW_HEIGHT - zgx_workload::VIEWPORT_HEIGHT,
                );
            cx.invalidate();
        });
        root.child(list.on_scroll_wheel(wheel))
    }
}
fn main() -> Result<(), quickgui::AppError> {
    Application::new().run(|cx| {
        cx.open_window(
            WindowOptions::new("zgx performance lab").size(960., 720.),
            Lab {
                work: Workload::from_env(),
                items: Vec::with_capacity(4),
                done: 0,
                started: false,
            },
        );
    })
}
