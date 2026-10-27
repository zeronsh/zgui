//! The comparison workload's UI (component_workload), offscreen: CPU per tick,
//! resident memory and renderer allocations for each mode, without a window.
//! Run: cargo run --release -p zgx-desktop --example workload_headless
use std::time::{Duration, Instant};
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};
use zgx_gpu::GpuRenderer;
use zgx_workload::{
    HEIGHT, Mode, ROW_COUNT, ROW_HEIGHT, VIEWPORT_HEIGHT, WIDTH, Workload, row_label,
};

const SURFACE: Color = Color(25, 34, 48, 255);

fn label(value: View, x: f32, y: f32, width: f32, height: f32) -> View {
    value.w(width).h(height).translate(x, y)
}

fn control(value: View, x: f32) -> View {
    button()
        .w(50.0)
        .h(32.0)
        .p(0.0)
        .rounded(0.0)
        .bg(Color(41, 59, 80, 255))
        .translate(x, 55.0)
        .child(label(value, 12.0, 6.0, 38.0, 20.0))
}

/// The lab's view (render_parity draws it too).
pub(crate) fn app(
    stream: zgx::reactive::Signal<String>,
    offset: zgx::reactive::Signal<f32>,
) -> View {
    let rows = virtual_list(
        offset,
        ROW_HEIGHT,
        2,
        || ROW_COUNT,
        |index| index,
        move |_, index, _| {
            overlay()
                .w(920.0)
                .h(ROW_HEIGHT)
                .bg(if index % 2 == 0 {
                    SURFACE
                } else {
                    Color(29, 40, 56, 255)
                })
                .child(label(text(row_label(index)), 8.0, 4.0, 904.0, 20.0))
        },
    )
    .w(920.0)
    .h(VIEWPORT_HEIGHT)
    .translate(20.0, 290.0);
    overlay()
        .w(WIDTH)
        .h(HEIGHT)
        .bg(Color(16, 20, 28, 255))
        .text_color(Color(229, 237, 247, 255))
        .text_size(14.0)
        .child(label(
            text("zgx performance lab").text_size(20.0),
            20.0,
            20.0,
            920.0,
            28.0,
        ))
        .child(control(text("+"), 20.0))
        .child(control(text("="), 80.0))
        .child(control(text("0"), 140.0))
        .child(label(text("4"), 220.0, 62.0, 50.0, 24.0))
        .child(label(text("stable"), 280.0, 62.0, 80.0, 24.0))
        .child(
            overlay()
                .w(920.0)
                .h(160.0)
                .translate(20.0, 110.0)
                .bg(SURFACE)
                .overflow_hidden()
                .child(label(
                    text_signal(move || stream.get()),
                    8.0,
                    8.0,
                    904.0,
                    144.0,
                )),
        )
        .child(rows)
}

#[cfg(windows)]
fn rss_mb() -> f64 {
    use windows::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: fills `counters`, whose size it is given, for this process.
    unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) }
        .map_or(0., |()| counters.WorkingSetSize as f64 / 1024. / 1024.)
}

#[cfg(not(windows))]
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.)
        / 1024.
}

fn main() {
    let mode = match zgx::env::var("ZGX_MODE").as_deref() {
        Ok("idle") => Mode::Idle,
        Ok("scroll") => Mode::Scroll,
        Ok("both") => Mode::Both,
        _ => Mode::Stream,
    };
    let ticks: usize = std::env::var("TICKS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(900);
    let scale = 1.;
    let mut ui = Ui::new(WIDTH, HEIGHT);
    // `RENDER=0`: layout and damage alone, text measured as a renderer
    // would (CI's instruction gate).
    let text = zgx_gpu::TextServices::new();
    let mut gpu = (std::env::var("RENDER").as_deref() != Ok("0")).then(|| {
        let mut gpu = GpuRenderer::new(WIDTH as u32, HEIGHT as u32).expect("GPU");
        gpu.set_scale_factor(scale);
        gpu
    });
    match &gpu {
        Some(gpu) => gpu.install_text(&mut ui.scene.borrow_mut()),
        None => text.install(&mut ui.scene.borrow_mut()),
    }
    let mut work = Workload::from_env();
    work.mode = mode;
    let stream = ui.signal(work.visible_text());
    let offset = ui.signal(work.scroll_offset);
    ui.mount(app(stream.clone(), offset.clone()));
    let (mut layout, mut render) = (Duration::ZERO, Duration::ZERO);
    let mut damaged = 0_u64;
    for tick in 0..ticks {
        let started = Instant::now();
        if tick > 0 {
            work.tick();
            if matches!(mode, Mode::Stream | Mode::Both) {
                stream.set(work.visible_text());
            }
            if matches!(mode, Mode::Scroll | Mode::Both) {
                offset.set(work.scroll_offset);
            }
        }
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        let laid = Instant::now();
        let Some(gpu) = &mut gpu else {
            zgx::profile::frame();
            if tick >= 60 {
                layout += laid - started;
            }
            continue;
        };
        let stats = gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        // Frames a window would present; readback only settles the GPU.
        if tick % 60 == 0 {
            gpu.readback().unwrap();
        }
        let done = Instant::now();
        if tick >= 60 {
            layout += laid - started;
            render += done - laid;
            damaged += stats.damaged_pixels;
        }
    }
    let measured = (ticks - 60).max(1) as f64;
    let Some(gpu) = gpu else {
        println!(
            "{mode:?} without rendering: layout {:.3} ms per tick",
            layout.as_secs_f64() * 1e3 / measured
        );
        return;
    };
    let caches = gpu.debug_cache_stats();
    let mb = |bytes: usize| bytes as f64 / 1048576.;
    println!(
        "{mode:?}: layout {:.3} ms + render {:.3} ms per tick, damaged {:.0} px/tick, rss {:.0} MB, atlas {:.1} MB, device {:.1} MB, prepared {:?}",
        layout.as_secs_f64() * 1e3 / measured,
        render.as_secs_f64() * 1e3 / measured,
        damaged as f64 / measured,
        rss_mb(),
        mb(caches.atlas_bytes),
        gpu.debug_device_allocated_bytes()
            .map_or(0., |b| b as f64 / 1048576.),
        gpu.text_cache().borrow().bytes(),
    );
}
