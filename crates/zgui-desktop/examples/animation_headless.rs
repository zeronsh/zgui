//! The animation workload (see zgui_workload::animation) offscreen: the
//! windowed `animation_workload`'s view at a Retina scale, its clock advanced
//! one 120 Hz refresh a frame, reporting CPU per frame by phase.
//! Run: FRAMES=2400 cargo run --release -p zgui-desktop --example animation_headless
use std::time::{Duration, Instant};
use zgui::widgets::Ui;
use zgui_gpu::GpuRenderer;
use zgui_workload::animation::*;

#[path = "support/animation_view.rs"]
mod view;

/// The process's CPU time so far, every thread, user and system.
#[cfg(unix)]
fn cpu_time() -> Duration {
    #[repr(C)]
    #[derive(Default)]
    struct Timeval {
        seconds: i64,
        micros: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Rusage {
        user: Timeval,
        system: Timeval,
        rest: [i64; 14],
    }
    // SAFETY: POSIX's `getrusage`; `Rusage` is `struct rusage` on 64-bit
    // Darwin and Linux (two 16-byte timevals, fourteen longs: 144 bytes).
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut usage = Rusage::default();
    // SAFETY: Fills a struct rusage for this process (RUSAGE_SELF).
    unsafe { getrusage(0, &mut usage) };
    let time = |t: &Timeval| Duration::new(t.seconds as u64, t.micros as u32 * 1000);
    time(&usage.user) + time(&usage.system)
}
/// The process's CPU time so far, every thread, user and kernel.
#[cfg(windows)]
fn cpu_time() -> Duration {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    // SAFETY: processthreadsapi.h's declarations: a HANDLE is a pointer,
    // a FILETIME two DWORDs (`FileTime`), a BOOL an i32.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let [mut creation, mut exit, mut kernel, mut user] = Default::default();
    // SAFETY: Fills four FILETIMEs for this process (a pseudo handle).
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    // FILETIME durations count 100 ns intervals.
    let time = |t: &FileTime| Duration::from_nanos(((t.high as u64) << 32 | t.low as u64) * 100);
    time(&kernel) + time(&user)
}

fn main() {
    let frames: usize = std::env::var("FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2400);
    let scale = 2.;
    let (width, height) = (960., 720.);
    let mut ui = Ui::new(width, height);
    // `RENDER=0`: the update and flush alone, text measured as a renderer
    // would (CI's instruction gate).
    let text = zgui_gpu::TextServices::new();
    let mut gpu = (std::env::var("RENDER").as_deref() != Ok("0")).then(|| {
        let mut gpu =
            GpuRenderer::new((width * scale) as u32, (height * scale) as u32).expect("GPU");
        gpu.set_scale_factor(scale);
        gpu
    });
    match &gpu {
        Some(gpu) => gpu.install_text(&mut ui.scene.borrow_mut()),
        None => text.install(&mut ui.scene.borrow_mut()),
    }
    let clock = ui.signal(0_f32);
    ui.mount(view::animation(clock.clone()));
    let (mut update, mut layout, mut render) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let warmup = 120;
    let mut cpu_start = Duration::ZERO;
    for frame in 0..frames {
        if frame == warmup {
            cpu_start = cpu_time();
        }
        let started = Instant::now();
        clock.set(frame as f32 / 120.);
        let updated = Instant::now();
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        let laid = Instant::now();
        if let Some(gpu) = &mut gpu {
            gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
            // A window presents every frame; readback only settles the GPU.
            if frame % 120 == 0 {
                gpu.readback().unwrap();
            }
        } else {
            zgui::profile::frame();
        }
        let done = Instant::now();
        if frame >= warmup {
            update += updated - started;
            layout += laid - updated;
            render += done - laid;
        }
    }
    let measured = (frames - warmup).max(1) as f64;
    let cpu = (cpu_time() - cpu_start).as_secs_f64() * 1e6 / measured;
    let us = |d: Duration| d.as_secs_f64() * 1e6 / measured;
    println!(
        "animate: update {:.1} + flush {:.1} + render {:.1} = {:.1} us/frame wall, {:.1} us/frame CPU ({} dots, {} characters)",
        us(update),
        us(layout),
        us(render),
        us(update + layout + render),
        cpu,
        COLUMNS * ROWS,
        SHIMMER.chars().count(),
    );
}
