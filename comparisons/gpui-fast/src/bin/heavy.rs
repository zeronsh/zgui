//! The heavy dashboard (see zgx_workload::heavy). GPUI's best case: tiles and
//! cards, the table and the log are separately cached views, and each tick
//! notifies only the views whose data changed.
use gpui::{prelude::*, *};
use std::time::Instant;
use zgx_workload::{heavy::*, Mode};

fn at((x, y): (f32, f32), w: f32, h: f32) -> Div {
    div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
}
fn panel(view: Div, radius: f32) -> Div {
    view.rounded(px(radius))
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(PANEL))
}
fn label(view: Div, size: f32, line: f32) -> Div {
    view.text_size(px(size)).line_height(px(line))
}
fn layer() -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .font_family(FONT)
        .text_color(rgb(TEXT))
}

struct Dashboard {
    live: u64,
}
impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let live = self.live;
        let mut root = layer();
        for index in 0..TILES {
            root = root.child(
                panel(at(tile_position(index), TILE.0, TILE.1), 8.)
                    .child(
                        label(at(TILE_LABEL, 176., 14.), 11., 14.)
                            .text_color(rgb(MUTED))
                            .child(tile_label(index)),
                    )
                    .child(
                        label(at(TILE_VALUE, 176., 26.), 20., 26.).child(tile_value(index, live)),
                    ),
            );
        }
        for index in 0..CARDS {
            let mut card = panel(at(card_position(index), CARD.0, CARD.1), 6.)
                .child(
                    label(at(CARD_TITLE, 104., 14.), 11., 14.)
                        .text_color(rgb(MUTED))
                        .child(card_title(index)),
                )
                .child(label(at(CARD_VALUE, 104., 20.), 16., 20.).child(card_value(index, live)))
                .child(
                    at((PROGRESS.0, PROGRESS.1), PROGRESS.2, PROGRESS.3)
                        .rounded(px(2.))
                        .bg(rgb(TRACK)),
                )
                .child(
                    at(
                        (PROGRESS.0, PROGRESS.1),
                        card_progress(index, live),
                        PROGRESS.3,
                    )
                    .rounded(px(2.))
                    .bg(rgb(ACCENT)),
                );
            for bar in 0..BARS {
                let height = bar_height(index, bar, live);
                card = card.child(
                    at((bar_x(bar), SPARK_BASE - height), BAR_WIDTH, height).bg(rgb(ACCENT)),
                );
            }
            root = root.child(card);
        }
        root
    }
}

struct Table {
    live: u64,
    scroll: f32,
}
impl Render for Table {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut header = at((0., 0.), TABLE.0, TABLE_HEADER).bg(rgb(HEADER));
        for (x, title) in COLUMNS {
            header = header.child(
                label(at((x, 7.), 80., 14.), 11., 14.)
                    .text_color(rgb(MUTED))
                    .child(title),
            );
        }
        let mut body = at((0., TABLE_HEADER), TABLE.0, TABLE_VIEWPORT).overflow_hidden();
        for row in row_range(self.scroll) {
            let (status, color) = status(row, self.live);
            let cell = |column: usize, width: f32| {
                label(at((COLUMNS[column].0, CELL_TOP), width, 16.), 12., 16.)
            };
            body = body.child(
                at(
                    (0., row as f32 * TABLE_ROW - self.scroll),
                    TABLE.0,
                    TABLE_ROW,
                )
                .bg(rgb(row_color(row)))
                .child(cell(0, 70.).text_color(rgb(MUTED)).child(row_id(row)))
                .child(cell(1, 150.).child(row_worker(row)))
                .child(
                    at((PILL.0, PILL.1), PILL.2, PILL.3)
                        .rounded(px(8.))
                        .bg(rgb(color))
                        .child(label(at(PILL_TEXT, 56., 14.), 10., 14.).child(status)),
                )
                .child(cell(3, 70.).child(row_latency(row, self.live)))
                .child(cell(4, 80.).child(row_throughput(row, self.live))),
            );
        }
        layer().child(
            panel(at(TABLE_ORIGIN, TABLE.0, TABLE.1), 8.)
                .overflow_hidden()
                .child(header)
                .child(body),
        )
    }
}

struct Log {
    text: String,
}
impl Render for Log {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        layer().child(
            panel(at(LOG_ORIGIN, LOG.0, LOG.1), 8.).child(
                label(
                    at(LOG_TEXT, LOG.0 - 24., LOG_LINES as f32 * LOG_LINE_HEIGHT),
                    11.,
                    LOG_LINE_HEIGHT,
                )
                .text_color(rgb(MUTED))
                .child(self.text.clone()),
            ),
        )
    }
}

struct Root {
    dashboard: Entity<Dashboard>,
    table: Entity<Table>,
    log: Entity<Log>,
}
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let cached = || {
            StyleRefinement::default()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
        };
        div()
            .size_full()
            .relative()
            .bg(rgb(BACKGROUND))
            .font_family(FONT)
            .text_color(rgb(TEXT))
            .child(label(at(TITLE_ORIGIN, 600., 24.), 18., 24.).child(TITLE))
            .child(AnyView::from(self.dashboard.clone()).cached(cached()))
            .child(AnyView::from(self.table.clone()).cached(cached()))
            .child(AnyView::from(self.log.clone()).cached(cached()))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx: &mut Context<Root>| {
                    let mut work = Heavy::from_env();
                    let dashboard = cx.new(|_| Dashboard { live: 0 });
                    let table = cx.new(|_| Table {
                        live: 0,
                        scroll: 0.,
                    });
                    let log = cx.new(|_| Log {
                        text: work.visible_log(),
                    });
                    let (seconds, mode) = (work.seconds, work.mode);
                    let (d, t, l) = (dashboard.clone(), table.clone(), log.clone());
                    cx.spawn(async move |_, cx| {
                        if mode == Mode::Idle && seconds == 0. {
                            return;
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
                            cx.background_executor()
                                .timer(deadline.saturating_duration_since(Instant::now()))
                                .await;
                            if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                                eprintln!("workload_ticks={}", work.frames);
                                let _ = cx.update(|cx| cx.quit());
                                break;
                            }
                            if mode != Mode::Idle && Heavy::turn() {
                                work.tick();
                                let (live, scroll) = (work.live, work.scroll);
                                cx.update(|cx| {
                                    if matches!(mode, Mode::Stream | Mode::Both) {
                                        d.update(cx, |d, cx| {
                                            d.live = live;
                                            cx.notify();
                                        });
                                        let text = work.visible_log();
                                        l.update(cx, |l, cx| {
                                            l.text = text;
                                            cx.notify();
                                        });
                                    }
                                    t.update(cx, |t, cx| {
                                        (t.live, t.scroll) = (live, scroll);
                                        cx.notify();
                                    });
                                });
                            }
                            deadline += period;
                            if deadline <= Instant::now() {
                                deadline = Instant::now() + period;
                            }
                        }
                    })
                    .detach();
                    Root {
                        dashboard,
                        table,
                        log,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
