//! The heavy dashboard (see zgx_workload::heavy): one view, rendered again
//! whenever a tick changes it.
use quickgui::{
    Application, AsyncContextError, Color, Element, IntoElement, View, ViewContext, WindowOptions,
    div, text,
};
use std::time::Instant;
use zgx_workload::{Mode, heavy::*};

fn color(hex: u32) -> Color {
    Color::rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
fn at((x, y): (f32, f32), w: f32, h: f32) -> Element {
    div().absolute().left(x).top(y).w(w).h(h)
}
fn panel(view: Element, radius: f32) -> Element {
    view.rounded(radius)
        .border(1., color(BORDER))
        .bg(color(PANEL))
}
fn label(content: impl Into<std::sync::Arc<str>>, size: f32, line: f32) -> Element {
    text(content).text_size(size).line_height(line)
}

struct Dashboard {
    work: Heavy,
    started: bool,
}
impl Dashboard {
    fn start(&mut self, cx: &mut ViewContext<'_, Self>) {
        let (seconds, mode) = (self.work.seconds, self.work.mode);
        cx.spawn(move |task| async move {
            if mode == Mode::Idle && seconds == 0. {
                return Ok::<(), AsyncContextError>(());
            }
            let period = Heavy::period();
            let start = Instant::now();
            let mut deadline = start
                + if mode == Mode::Idle {
                    std::time::Duration::from_secs_f64(seconds.max(0.001))
                } else {
                    period
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
                if mode != Mode::Idle {
                    task.update(|this, cx| {
                        this.work.tick();
                        cx.invalidate();
                    })
                    .await?;
                }
                deadline += period;
                if deadline <= Instant::now() {
                    deadline = Instant::now() + period;
                }
            }
            Ok::<(), AsyncContextError>(())
        })
        .expect("timer task")
        .detach();
    }
}
impl View for Dashboard {
    fn render(&mut self, cx: &mut ViewContext<'_, Self>) -> impl IntoElement {
        if !self.started {
            self.started = true;
            self.start(cx);
        }
        let (live, scroll) = (self.work.live, self.work.scroll);
        let mut root = div()
            .size_full()
            .relative()
            .bg(color(BACKGROUND))
            .text_color(color(TEXT))
            .font_family(FONT)
            .child(at(TITLE_ORIGIN, 600., 24.).child(label(TITLE, 18., 24.)));
        for index in 0..TILES {
            root = root.child(
                panel(at(tile_position(index), TILE.0, TILE.1), 8.)
                    .child(
                        at(TILE_LABEL, 176., 14.)
                            .child(label(tile_label(index), 11., 14.).text_color(color(MUTED))),
                    )
                    .child(at(TILE_VALUE, 176., 26.).child(label(
                        tile_value(index, live),
                        20.,
                        26.,
                    ))),
            );
        }
        for index in 0..CARDS {
            let mut card = panel(at(card_position(index), CARD.0, CARD.1), 6.)
                .child(
                    at(CARD_TITLE, 104., 14.)
                        .child(label(card_title(index), 11., 14.).text_color(color(MUTED))),
                )
                .child(at(CARD_VALUE, 104., 20.).child(label(card_value(index, live), 16., 20.)))
                .child(
                    at((PROGRESS.0, PROGRESS.1), PROGRESS.2, PROGRESS.3)
                        .rounded(2.)
                        .bg(color(TRACK)),
                )
                .child(
                    at(
                        (PROGRESS.0, PROGRESS.1),
                        card_progress(index, live),
                        PROGRESS.3,
                    )
                    .rounded(2.)
                    .bg(color(ACCENT)),
                );
            for bar in 0..BARS {
                let height = bar_height(index, bar, live);
                card = card.child(
                    at((bar_x(bar), SPARK_BASE - height), BAR_WIDTH, height).bg(color(ACCENT)),
                );
            }
            root = root.child(card);
        }
        let mut header = at((0., 0.), TABLE.0, TABLE_HEADER).bg(color(HEADER));
        for (x, title) in COLUMNS {
            header = header.child(
                at((x, 7.), 80., 14.).child(label(title, 11., 14.).text_color(color(MUTED))),
            );
        }
        let mut body = at((0., TABLE_HEADER), TABLE.0, TABLE_VIEWPORT).overflow_hidden();
        for row in row_range(scroll) {
            let (status, pill) = status(row, live);
            let cell = |column: usize, width: f32| at((COLUMNS[column].0, CELL_TOP), width, 16.);
            body = body.child(
                at((0., row as f32 * TABLE_ROW - scroll), TABLE.0, TABLE_ROW)
                    .bg(color(row_color(row)))
                    .child(
                        cell(0, 70.).child(label(row_id(row), 12., 16.).text_color(color(MUTED))),
                    )
                    .child(cell(1, 150.).child(label(row_worker(row), 12., 16.)))
                    .child(
                        at((PILL.0, PILL.1), PILL.2, PILL.3)
                            .rounded(8.)
                            .bg(color(pill))
                            .child(at(PILL_TEXT, 56., 14.).child(label(status, 10., 14.))),
                    )
                    .child(cell(3, 70.).child(label(row_latency(row, live), 12., 16.)))
                    .child(cell(4, 80.).child(label(row_throughput(row, live), 12., 16.))),
            );
        }
        root.child(
            panel(at(TABLE_ORIGIN, TABLE.0, TABLE.1), 8.)
                .overflow_hidden()
                .child(header)
                .child(body),
        )
        .child(panel(at(LOG_ORIGIN, LOG.0, LOG.1), 8.).child(
            at(LOG_TEXT, LOG.0 - 24., LOG_LINES as f32 * LOG_LINE_HEIGHT).child(
                label(self.work.visible_log(), 11., LOG_LINE_HEIGHT).text_color(color(MUTED)),
            ),
        ))
    }
}
fn main() -> Result<(), quickgui::AppError> {
    Application::new().run(|cx| {
        cx.open_window(
            WindowOptions::new("quickgui heavy").size(WIDTH, HEIGHT),
            Dashboard {
                work: Heavy::from_env(),
                started: false,
            },
        );
    })
}
