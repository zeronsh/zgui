//! The microbenchmarks (zgx_workload::micro) in Rune's headless runtime: CPU
//! time per operation, for the operation alone ("write") and through to a
//! frame ready to present ("frame": the commit, layout and scene Rune's
//! `Runtime::frame` builds, then its renderer's prepare, draw and submit to
//! an offscreen target, as Rune's own headless runner makes a frame).
//!
//!   SCENE=state|vscroll-fixed|vscroll-variable bin/micro
//!
//! - `state`: one text bound to a counter over a static grid. A write is
//!   `Runtime::update`, which runs at once and marks the state changed.
//! - `vscroll-fixed`, `vscroll-variable`: 10,000 rows in Rune's virtualized
//!   list (`El::virtual_rows`), fixed or the vlist rows, measured as they
//!   mount. A write is a 12 pt wheel step through `Runtime::wheel_xy`, the
//!   path a wheel event takes. The runs never reach the end of the list
//!   (4,400 steps of 12 pt), so the wrap to the top never comes.
//!
//! One JSON line per figure: nanoseconds of the process's CPU time per
//! operation (median, p10, p90, min, max over batches).
use std::borrow::Cow;

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::text::TextSystem;
use rune::tok::TextStyle;
use rune::{Cx, El, Env, GrowthSpec, Runtime, ScrollSpec, col, row, text};
use zgx_compare_rune::{ESTIMATE, chat_row, rgb};
use zgx_workload::vlist;

use zgx_workload::micro::*;

const SCALE: f32 = 2.0;
const DT: f64 = 1.0 / 60.0;

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
    // CLOCK_PROCESS_CPUTIME_ID on macOS.
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    // SAFETY: a valid clock id and a timespec to write.
    unsafe { clock_gettime(12, &mut time) };
    time.seconds as u64 * 1_000_000_000 + time.nanoseconds as u64
}

struct Scene {
    kind: Kind,
    counter: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    State,
    Fixed,
    Variable,
}

fn view(s: &Scene, _: &mut Cx<Scene>) -> El {
    match s.kind {
        Kind::State => {
            let style = TextStyle::new(FONT_SIZE, 400.0);
            let mut grid = col().gap(2.0);
            for r in 0..STATIC_ROWS {
                let mut line = row().gap(2.0);
                for c in 0..STATIC_COLUMNS {
                    line = line.child(
                        text(static_label(r * STATIC_COLUMNS + c))
                            .font(style)
                            .color(rgb(vlist::TEXT))
                            .size(CELL.0, CELL.1),
                    );
                }
                grid = grid.child(line);
            }
            col()
                .w_full()
                .h_full()
                .padding(8.0)
                .gap(6.0)
                .fill(rgb(vlist::BACKGROUND))
                .child(
                    text(counter_label(s.counter))
                        .font(style)
                        .color(rgb(vlist::TEXT))
                        .h(20.0),
                )
                .child(grid)
        }
        kind => {
            let fixed = kind == Kind::Fixed;
            let estimate = if fixed { FIXED_ROW } else { ESTIMATE };
            let rows = col()
                .key("rows")
                .growth(GrowthSpec::default())
                .virtual_rows(
                    ROWS,
                    estimate,
                    |_: &Scene, i| Cow::Owned(format!("r{i}")),
                    move |_: &Scene, i| {
                        if fixed {
                            row()
                                .w_full()
                                .h(FIXED_ROW)
                                .px(12.0)
                                .items_center()
                                .fill(rgb(vlist::ROW_COLORS[i % 2]))
                                .child(
                                    text(fixed_label(i))
                                        .font(
                                            TextStyle::new(vlist::FONT_SIZE, 400.0)
                                                .lh(vlist::LINE_HEIGHT),
                                        )
                                        .color(rgb(vlist::TEXT)),
                                )
                        } else {
                            chat_row(i)
                        }
                    },
                );
            col().w_full().h_full().fill(rgb(vlist::BACKGROUND)).child(
                col()
                    .key("list")
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .w_full()
                    .h_full()
                    .child(rows),
            )
        }
    }
}

struct Bench {
    rt: Runtime<Scene>,
    gpu: Gpu,
    renderer: Renderer,
    target: Offscreen,
    pixels: (u32, u32),
    size: (f32, f32),
}

impl Bench {
    fn new(kind: Kind) -> Self {
        let size = if kind == Kind::State {
            (zgx_workload::WIDTH, zgx_workload::HEIGHT)
        } else {
            (vlist::WIDTH, VIEWPORT)
        };
        let text_system = TextSystem::new(
            &[zgx_workload::FONT_BYTES],
            (zgx_workload::FONT, zgx_workload::FONT),
        );
        let env = Env {
            system_dark: true,
            scale: SCALE,
        };
        let rt = Runtime::with_text(Scene { kind, counter: 0 }, view, size, env, text_system);
        let gpu = Gpu::headless();
        let renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let pixels = (
            (size.0 * SCALE).round() as u32,
            (size.1 * SCALE).round() as u32,
        );
        let target = Offscreen::with_format(&gpu.device, pixels.0, pixels.1, OFFSCREEN_FORMAT);
        let mut bench = Bench {
            rt,
            gpu,
            renderer,
            target,
            pixels,
            size,
        };
        bench.frame();
        bench.settle();
        bench
    }

    fn write(&mut self, op: usize) {
        match self.rt.state.kind {
            Kind::State => self.rt.update(move |s, _| s.counter = op as u64 + 1),
            // A wheel step down (positive dy moves the content down) at the list's middle.
            _ => self
                .rt
                .wheel_xy(self.size.0 / 2.0, self.size.1 / 2.0, 0.0, -STEP),
        }
    }

    /// The commit, layout and scene, then encode and submit: a frame ready to present.
    fn frame(&mut self) {
        let rt = &mut self.rt;
        rt.frame(DT);
        self.renderer
            .upload_images(&self.gpu.device, &self.gpu.queue, &mut rt.images);
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        self.renderer.prepare(
            &self.gpu.device,
            &self.gpu.queue,
            &mut rt.text.atlas,
            &rt.instances,
            self.pixels,
        );
        self.renderer
            .draw(&mut encoder, &self.target.view, rt.clear_color());
        self.renderer.submit(&self.gpu.queue, encoder);
    }

    /// Settles the GPU between batches (untimed): a window's presents never
    /// let submitted work pile up.
    fn settle(&mut self) {
        self.gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .ok();
    }
}

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

fn measure(bench: &mut Bench, ops: usize, batch: usize, frame: bool, next: &mut usize) -> Vec<f64> {
    let one = |bench: &mut Bench, next: &mut usize| {
        bench.write(*next);
        *next += 1;
        if frame {
            bench.frame();
        }
    };
    for _ in 0..env("WARMUP", WARMUP) {
        one(bench, next);
    }
    bench.settle();
    let mut samples = Vec::with_capacity(ops / batch);
    for _ in 0..ops / batch {
        let started = cpu_ns();
        for _ in 0..batch {
            one(bench, next);
        }
        samples.push((cpu_ns() - started) as f64 / batch as f64);
        bench.settle();
    }
    // A write alone leaves the frame to do: catch up, untimed.
    if !frame {
        bench.frame();
        bench.settle();
    }
    samples
}

fn main() {
    let scene: String = env("SCENE", "state".to_string());
    let kind = match scene.as_str() {
        "state" => Kind::State,
        "vscroll-fixed" => Kind::Fixed,
        "vscroll-variable" => Kind::Variable,
        other => panic!("SCENE {other}: state, vscroll-fixed or vscroll-variable"),
    };
    let ops: usize = env("OPS", OPS);
    let mut bench = Bench::new(kind);
    let mut next = 0;
    for (figure, frame, batch) in [
        ("write", false, env("WRITE_BATCH", 50)),
        ("frame", true, env("FRAME_BATCH", 10)),
    ] {
        let mut samples = measure(&mut bench, ops, batch, frame, &mut next);
        println!(
            "{{\"framework\":\"rune\",\"scene\":\"{scene}\",\"figure\":\"{figure}\",\"render\":true,\"clock\":\"process_cpu\",\"ops\":{ops},\"batch\":{batch},{}}}",
            summary(&mut samples)
        );
    }
}
