//! The microbenchmarks (zgx_workload::micro) in GPUI Fast, through its
//! headless test platform (`HeadlessAppContext`) with the real macOS text
//! system: an entity update draws the window synchronously when it ends
//! (layout, prepaint and paint into the scene), with no renderer, so a
//! "frame" here is the scene ready to present without the GPU's encode
//! (zgx's `RENDER=0`). "write" is the change alone: the field and
//! `cx.notify()`, timed inside one update, before its draw.
//!
//!   SCENE=state|vscroll-fixed|vscroll-variable ZGX_FONT_FILE=… zgx-compare-gpui-fast-micro
use gpui::{prelude::*, *};
use std::borrow::Cow;
use zgx_workload::{micro::*, vlist};

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The process's CPU time (all its threads), in nanoseconds.
fn cpu_ns() -> u64 {
    #[repr(C)]
    struct Timespec {
        seconds: i64,
        nanoseconds: i64,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    }
    // CLOCK_PROCESS_CPUTIME_ID (macOS 12, Linux 2).
    const CLOCK: i32 = if cfg!(target_os = "macos") { 12 } else { 2 };
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    // SAFETY: a valid clock id and a timespec to write.
    unsafe { clock_gettime(CLOCK, &mut time) };
    time.seconds as u64 * 1_000_000_000 + time.nanoseconds as u64
}

struct State {
    count: u64,
}
impl Render for State {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(6.))
            .bg(rgb(vlist::BACKGROUND))
            .font_family(zgx_workload::FONT)
            .text_size(px(FONT_SIZE))
            .text_color(rgb(vlist::TEXT))
            .child(div().h(px(20.)).child(counter_label(self.count)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children((0..STATIC_ROWS).map(|r| {
                        div()
                            .flex()
                            .gap(px(2.))
                            .children((0..STATIC_COLUMNS).map(move |c| {
                                div()
                                    .w(px(CELL.0))
                                    .h(px(CELL.1))
                                    .child(static_label(r * STATIC_COLUMNS + c))
                            }))
                    })),
            )
    }
}

struct Fixed {
    scroll: UniformListScrollHandle,
}
impl Render for Fixed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(vlist::BACKGROUND))
            .font_family(zgx_workload::FONT)
            .text_size(px(vlist::FONT_SIZE))
            .text_color(rgb(vlist::TEXT))
            .child(
                uniform_list("rows", ROWS, |range, _, _| {
                    range
                        .map(|i| {
                            div()
                                .h(px(FIXED_ROW))
                                .px(px(12.))
                                .flex()
                                .items_center()
                                .bg(rgb(vlist::ROW_COLORS[i % 2]))
                                .child(fixed_label(i))
                        })
                        .collect()
                })
                .track_scroll(&self.scroll)
                .size_full(),
            )
    }
}

struct Variable {
    list: ListState,
}
impl Render for Variable {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(vlist::BACKGROUND))
            .font_family(vlist::FONT)
            .text_size(px(vlist::FONT_SIZE))
            .line_height(px(vlist::LINE_HEIGHT))
            .text_color(rgb(vlist::TEXT))
            .child(
                list(self.list.clone(), |index, _, _| {
                    chat_row(index).into_any_element()
                })
                .size_full(),
            )
    }
}

/// vlist's row, as the windowed GPUI Fast adapter draws it (src/bin/vlist.rs there).
fn chat_row(index: usize) -> Div {
    let mut body = div()
        .flex()
        .flex_col()
        .w_full()
        .px(px(vlist::PAD.0))
        .py(px(vlist::PAD.1))
        .gap(px(vlist::GAP))
        .bg(rgb(vlist::ROW_COLORS[index % 2]))
        .child(
            div()
                .flex()
                .gap(px(8.))
                .h(px(vlist::LINE_HEIGHT))
                .child(
                    div()
                        .text_color(rgb(vlist::AUTHOR))
                        .child(vlist::author(index)),
                )
                .child(
                    div()
                        .text_color(rgb(vlist::MUTED))
                        .child(vlist::time(index)),
                ),
        )
        .child(div().w(px(vlist::TEXT_WIDTH)).child(vlist::text(index)));
    if vlist::has_image(index) {
        body = body.child(
            div()
                .w(px(vlist::IMAGE.0))
                .h(px(vlist::IMAGE.1))
                .bg(rgb(vlist::IMAGE_COLOR)),
        );
    }
    body
}

/// Median, p10, p90, min and max of `samples` (ns per operation).
fn summary(samples: &mut [f64]) -> String {
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    format!(
        "\"median_ns\":{:.1},\"p10_ns\":{:.1},\"p90_ns\":{:.1},\"min_ns\":{:.1},\"max_ns\":{:.1},\"batches\":{}",
        at(0.5),
        at(0.1),
        at(0.9),
        samples[0],
        samples[samples.len() - 1],
        samples.len()
    )
}

fn report(scene: &str, figure: &str, ops: usize, batch: usize, mut samples: Vec<f64>) {
    println!(
        "{{\"framework\":\"gpui-fast\",\"scene\":\"{scene}\",\"figure\":\"{figure}\",\"render\":false,\"clock\":\"process_cpu\",\"ops\":{ops},\"batch\":{batch},{}}}",
        summary(&mut samples)
    );
}

/// `write` runs `batch` changes inside one update (timed before its draw);
/// `frame` runs one change per update (each drawing when it ends).
fn run<V: 'static>(
    cx: &mut HeadlessAppContext,
    view: &Entity<V>,
    scene: &str,
    step: impl Fn(&mut V, &mut Context<V>) + Copy,
) {
    let ops: usize = env("OPS", OPS);
    let warmup: usize = env("WARMUP", WARMUP);
    let (write_batch, frame_batch) = (env("WRITE_BATCH", 50), env("FRAME_BATCH", 10));
    let mut samples = Vec::new();
    for round in 0..(warmup / write_batch + ops / write_batch) {
        let spent = cx.update(|app| {
            view.update(app, |v, cx| {
                let started = cpu_ns();
                for _ in 0..write_batch {
                    step(v, cx);
                }
                cpu_ns() - started
            })
        });
        if round >= warmup / write_batch {
            samples.push(spent as f64 / write_batch as f64);
        }
    }
    report(scene, "write", ops, write_batch, samples);
    for _ in 0..warmup {
        cx.update(|app| view.update(app, |v, cx| step(v, cx)));
    }
    let mut samples = Vec::new();
    for _ in 0..ops / frame_batch {
        let started = cpu_ns();
        for _ in 0..frame_batch {
            cx.update(|app| view.update(app, |v, cx| step(v, cx)));
        }
        samples.push((cpu_ns() - started) as f64 / frame_batch as f64);
    }
    report(scene, "frame", ops, frame_batch, samples);
}

fn main() {
    let scene: String = env("SCENE", "state".to_string());
    // The real text system (font-kit), not the test platform's no-op shaping.
    let text = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::new(text);
    if let Some(bytes) = zgx_workload::font_file() {
        cx.update(|cx| cx.text_system().add_fonts(vec![Cow::Owned(bytes)]).unwrap());
    }
    match scene.as_str() {
        "state" => {
            let window = cx
                .open_window(
                    size(px(zgx_workload::WIDTH), px(zgx_workload::HEIGHT)),
                    |_, cx| cx.new(|_| State { count: 0 }),
                )
                .unwrap();
            let view = cx.update(|app| window.entity(app)).unwrap();
            run(&mut cx, &view, &scene, |v: &mut State, cx| {
                v.count += 1;
                cx.notify();
            });
        }
        "vscroll-fixed" => {
            let window = cx
                .open_window(size(px(vlist::WIDTH), px(VIEWPORT)), |_, cx| {
                    cx.new(|_| Fixed {
                        scroll: UniformListScrollHandle::new(),
                    })
                })
                .unwrap();
            let view = cx.update(|app| window.entity(app)).unwrap();
            run(&mut cx, &view, &scene, |v: &mut Fixed, cx| {
                let handle = &v.scroll.0.borrow().base_handle;
                let (offset, max) = (handle.offset(), handle.max_offset());
                // Offsets run negative as it scrolls down; past the end, back to the top.
                let next = offset.y - px(STEP);
                handle.set_offset(point(offset.x, if -next > max.y { px(0.) } else { next }));
                cx.notify();
            });
        }
        "vscroll-variable" => {
            let window = cx
                .open_window(size(px(vlist::WIDTH), px(VIEWPORT)), |_, cx| {
                    cx.new(|_| Variable {
                        list: ListState::new(ROWS, ListAlignment::Top, px(200.)),
                    })
                })
                .unwrap();
            let view = cx.update(|app| window.entity(app)).unwrap();
            run(&mut cx, &view, &scene, |v: &mut Variable, cx| {
                if v.list.logical_scroll_top().item_ix + 64 >= ROWS {
                    v.list.scroll_to(ListOffset::default());
                } else {
                    v.list.scroll_by(px(STEP));
                }
                cx.notify();
            });
        }
        other => panic!("SCENE {other}: state, vscroll-fixed or vscroll-variable"),
    }
}
