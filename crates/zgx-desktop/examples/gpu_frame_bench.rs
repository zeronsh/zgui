//! What the heavy dashboard (see zgx_workload::heavy) costs the GPU a
//! frame, offscreen at a Retina scale: each frame waited for, its Metal GPU
//! time read back (macOS), with the frame's damaged pixels, draws and
//! instances. A hash of the frame's pixels every 100 ticks tells whether a
//! change draws exactly the same thing.
//! Run: ZGX_MODE=both cargo run --release -p zgx-desktop --example gpu_frame_bench
//! (`TICKS` sets the frames, 600 by default; `SCALE` the device scale, 2.
//! `PACE=60` draws 60 frames a second, as a window does: the GPU then
//! clocks as it would, and scripts/macsample.swift's GPU megacycles a
//! second of this process, over 60, are its cycles a frame:
//! `PACE=60 TICKS=1000 gpu_frame_bench & /tmp/zgx-macsample $! 4 10 0.05`.
//! Unpaced GPU times swing with the GPU's clock, 2x for the same build.
//! About 0.17 Mcycles of a frame is the GPU waking for it at all: an empty
//! render pass costs that, whatever the target's size.)
use std::hash::{Hash, Hasher};
use zgx::widgets::Ui;
use zgx_gpu::GpuRenderer;
use zgx_workload::{Mode, heavy::*};

#[path = "support/heavy_view.rs"]
mod view;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let ticks: usize = env("TICKS", 600);
    let pace: f64 = env("PACE", 0.);
    let started = std::time::Instant::now();
    let scale: f32 = env("SCALE", 2.);
    let mut work = Heavy::from_env();
    let mode = work.mode;
    let mut ui = Ui::new(WIDTH, HEIGHT);
    let mut gpu = GpuRenderer::new(
        (WIDTH * scale).round() as u32,
        (HEIGHT * scale).round() as u32,
    )
    .expect("GPU");
    gpu.set_scale_factor(scale);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let live = ui.signal(0_u64);
    let scroll = ui.signal(0_f32);
    let log = ui.signal(work.visible_log());
    ui.mount(view::dashboard(live.clone(), scroll.clone(), log.clone()));
    let mut times = Vec::new();
    let (mut damaged, mut draws, mut instances, mut passes) = (0_u64, 0_usize, 0_usize, 0_usize);
    // The scene's own damage: its exact union (on a grid of logical
    // points) and how many rectangles.
    let (mut raw, mut rects) = (0_u64, 0_usize);
    let mut grid = vec![false; (WIDTH * HEIGHT) as usize];
    let mut pixels = std::collections::hash_map::DefaultHasher::new();
    for tick in 0..ticks {
        if tick > 0 {
            work.tick();
            ui.runtime.batch(|| {
                if matches!(mode, Mode::Stream | Mode::Both) {
                    live.set(work.live);
                    log.set(work.visible_log());
                }
                if matches!(mode, Mode::Scroll | Mode::Both) {
                    scroll.set(work.scroll);
                }
            });
        }
        if pace > 0. {
            let due = started + std::time::Duration::from_secs_f64(tick as f64 / pace);
            std::thread::sleep(due.saturating_duration_since(std::time::Instant::now()));
        }
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        let stats = gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        // Waits for the frame, and (every 100 ticks) is what it drew. Paced,
        // only then: a readback is GPU work of its own.
        if tick % 100 == 99 || pace == 0. {
            let frame = gpu.readback().unwrap();
            if tick % 100 == 99 {
                frame.hash(&mut pixels);
            }
        }
        #[cfg(target_os = "macos")]
        let time = gpu.take_gpu_time();
        #[cfg(not(target_os = "macos"))]
        let time: Option<std::time::Duration> = None;
        if tick >= 60 {
            grid.fill(false);
            for d in &report.damage {
                let (x0, y0) = (d.x.max(0.) as usize, d.y.max(0.) as usize);
                let x1 = ((d.x + d.width).ceil() as usize).min(WIDTH as usize);
                let y1 = ((d.y + d.height).ceil() as usize).min(HEIGHT as usize);
                for y in y0..y1 {
                    grid[y * WIDTH as usize + x0..y * WIDTH as usize + x1.max(x0)].fill(true);
                }
            }
            raw += grid.iter().filter(|c| **c).count() as u64 * (scale * scale) as u64;
            rects += report.damage.len();
            times.extend(time);
            damaged += stats.damaged_pixels;
            draws += stats.draw_calls;
            instances += stats.instances;
            passes += stats.render_passes;
        }
    }
    let measured = (ticks - 60).max(1) as f64;
    times.sort();
    let at = |q: f64| {
        times
            .get(((times.len() as f64 - 1.) * q) as usize)
            .map_or(0., |t| t.as_secs_f64() * 1e3)
    };
    let mean = times.iter().map(|t| t.as_secs_f64()).sum::<f64>() / times.len().max(1) as f64 * 1e3;
    println!(
        "{mode:?}: GPU {:.3} ms p10, {:.3} median, {:.3} p90, {mean:.3} mean ({} frames timed); damaged {:.2} Mpx, \
         {:.1} draws, {:.0} instances, {:.2} passes a frame; scene damage {:.2} Mpx in {:.1} rects; pixels {:016x}",
        at(0.1),
        at(0.5),
        at(0.9),
        times.len(),
        damaged as f64 / measured / 1e6,
        draws as f64 / measured,
        instances as f64 / measured,
        passes as f64 / measured,
        raw as f64 / measured / 1e6,
        rects as f64 / measured,
        pixels.finish(),
    );
}
