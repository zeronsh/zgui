//! What removing a large mounted subtree costs, headless: `N` rows of two
//! texts in a column, mounted, laid out, then unmounted in one frame, and
//! the frames after it until what it held is freed (`Ui::reclaim`). Prints
//! the removal frame's time and instructions (macOS), the slowest frame
//! after it, and how many frames the freeing took.
//! Run: cargo run --release -p zgui --example teardown_bench
//! (`N` sets the rows, 30,000 by default; several sizes: `N=2000,10000,30000`.)
use std::time::{Duration, Instant};
use zgui::{compose::prelude::*, scene::Color, widgets::Ui};

/// Instructions this thread has retired (macOS; 0 elsewhere).
fn instructions() -> u64 {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn thread_selfcounts(kind: i32, buffer: *mut u64, size: usize) -> i32;
        }
        let mut counts = [0u64; 2];
        // SAFETY: THSC_CPI (1) fills two u64s: instructions, cycles.
        unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) };
        counts[0]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

fn rows(n: usize) -> View {
    let row = |i: usize| {
        row()
            .gap(8.)
            .px(12.)
            .py(6.)
            .bg(Color(0x1b, 0x22, 0x30, 255))
            .child(text(format!("Row {i}")))
            .child(text(format!("{} MB", i * 3)).text_color(Color(0x88, 0x99, 0xaa, 255)))
    };
    column().w(1200.).children((1..=n).map(row))
}

fn run(n: usize) {
    let mut ui = Ui::new(1200., 900.);
    let frame = |ui: &Ui| {
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
    };
    let mounted = ui.mount(rows(n));
    frame(&ui);
    let nodes = ui.scene.borrow().len();
    let (start, at) = (instructions(), Instant::now());
    mounted.unmount();
    frame(&ui);
    let (removal, spent) = (at.elapsed(), instructions() - start);
    let mut after = Duration::ZERO;
    let mut frames = 0;
    // The frames after it: what deferred freeing costs each, until done.
    while frames < 600 && ui.reclaiming() {
        let at = Instant::now();
        frame(&ui);
        after = after.max(at.elapsed());
        frames += 1;
    }
    println!(
        "{n} rows ({nodes} nodes): removal frame {:.2} ms, {:.1} M instructions ({:.0} a node); after it, {frames} frames, slowest {:.2} ms",
        removal.as_secs_f64() * 1e3,
        spent as f64 / 1e6,
        spent as f64 / nodes as f64,
        after.as_secs_f64() * 1e3,
    );
}

fn main() {
    let sizes = std::env::var("N").unwrap_or_else(|_| "2000,10000,30000".into());
    for n in sizes.split(',').filter_map(|n| n.trim().parse().ok()) {
        run(n);
    }
}
