//! Resident memory while a plain text streams as in the comparison benchmark:
//! a 7-line window sliding every tick, offscreen, 1200 ticks.
//! Run: cargo run --release -p zgx-gpu --example stream_memory
use std::fmt::Write;
use zgx::{compose::prelude::*, widgets::Ui};
use zgx_gpu::GpuRenderer;

/// The working set: Windows has no `ps`.
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
    let mut gpu = GpuRenderer::new(1920, 1440).expect("GPU");
    gpu.set_scale_factor(2.);
    let mut ui = Ui::new(960., 720.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let stream = ui.signal(String::new());
    let read = stream.clone();
    ui.mount(
        overlay().w(960.).h(720.).child(
            text_signal(move || read.get())
                .w(904.)
                .h(144.)
                .text_wrap(true),
        ),
    );
    let mut source = String::new();
    for tick in 0..1200 {
        write!(source, "{tick:06} The quick brown fox streams a token. ").unwrap();
        if source.len() > 8192 {
            source.drain(..source.len() - 8192);
        }
        let tail = &source.as_bytes()[source.len().saturating_sub(735)..];
        let visible = tail
            .chunks(105)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        stream.set(visible);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        if tick == 60 || tick == 1199 {
            gpu.readback().unwrap();
            let (shaped, compact) = gpu.text_cache().borrow().bytes();
            println!(
                "tick {tick:4}: rss {:.0} MB, prepared {:.1}+{:.1} MB",
                rss_mb(),
                shaped as f64 / 1048576.,
                compact as f64 / 1048576.
            );
        }
    }
}
