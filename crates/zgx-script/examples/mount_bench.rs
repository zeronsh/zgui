//! What building, mounting and tearing down an ordinary interface costs a
//! script against the same interface in Rust, headless: `N` rows of two
//! texts, padded, rounded and coloured, in a column. Counts instructions
//! retired by phase (macOS), allocations, and times them.
//! Run: cargo run --release -p zgx-script --example mount_bench
//! (`N` sets the row count, 2,000 by default; `ONLY=luau|rust` runs one.)
#[path = "support/bench_limits.rs"]
mod bench_limits;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{Duration, Instant},
};
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};
use zgx_script::ScriptHost;

/// Instructions this process has retired (macOS; 0 elsewhere).
fn instructions() -> u64 {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Usage {
            uuid: [u8; 16],
            fields: [u64; 40],
        }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut Usage) -> i32;
        }
        let mut usage = Usage {
            uuid: [0; 16],
            fields: [0; 40],
        };
        // RUSAGE_INFO_V4: `ri_instructions` is the 30th field after the uuid.
        unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut usage) };
        usage.fields[29]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

/// Counts the allocations made (`realloc` counts as one).
struct Counting;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
// SAFETY: every call goes to `System` unchanged (its contract the caller's);
// the counter is an atomic, which allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Default)]
struct Phase {
    instructions: u64,
    allocations: u64,
    time: Duration,
}
fn measure<T>(phase: &mut Phase, f: impl FnOnce() -> T) -> T {
    let (start, allocations, at) = (instructions(), ALLOCATIONS.load(Relaxed), Instant::now());
    let value = f();
    phase.time += at.elapsed();
    phase.instructions += instructions() - start;
    phase.allocations += ALLOCATIONS.load(Relaxed) - allocations;
    value
}

fn rows(n: usize) -> View {
    let row = |i: usize| {
        row()
            .gap(8.)
            .px(12.)
            .py(6.)
            .bg(Color(0x1b, 0x22, 0x30, 255))
            .rounded(6.)
            .child(text(format!("Row {i}")).text_size(13.))
            .child(
                text(format!("{} MB", i * 3))
                    .text_size(12.)
                    .text_color(Color(0x88, 0x99, 0xaa, 255)),
            )
    };
    column().w(1200.).gap(4.).children((1..=n).map(row))
}

const SCRIPT: &str = r##"
local N = ...
local rows = {}
for i = 1, N do
    rows[i] = ui.row {
        gap = 8, px = 12, py = 6, bg = "#1b2230", rounded = 6,
        ui.text { "Row " .. i, text_size = 13 },
        ui.text { tostring(i * 3) .. " MB", text_size = 12, color = "#8899aa" },
    }
end
return ui.column { w = 1200, gap = 4, rows }
"##;

/// Build, mount and lay out, tear down; each phase's cost, and the nodes.
fn run(luau: bool, n: usize) -> ([Phase; 3], usize) {
    let mut phases: [Phase; 3] = Default::default();
    let mut ui = Ui::new(1200., 900.);
    let mut host = None;
    let view = measure(&mut phases[0], || {
        if luau {
            let code = SCRIPT.replace("local N = ...", &format!("local N = {n}"));
            let script =
                ScriptHost::inline(&ui.runtime, "bench", code).limits(bench_limits::limits());
            script.reload().expect("the bench script loads");
            let view = script.view();
            host = Some(script);
            view
        } else {
            rows(n)
        }
    });
    measure(&mut phases[1], || {
        ui.mount(view);
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
    });
    let nodes = ui.scene.borrow().len();
    measure(&mut phases[2], || {
        drop(ui);
        drop(host);
    });
    (phases, nodes)
}

fn main() {
    let n: usize = std::env::var("N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let only = std::env::var("ONLY").ok();
    println!("{n} rows; instructions (M), allocations per node and time per phase:");
    for (name, luau) in [("luau", true), ("rust", false)] {
        if only.as_deref().is_some_and(|only| only != name) {
            continue;
        }
        // Warm the allocator and the font cache once.
        run(luau, n.min(100));
        let (phases, nodes) = run(luau, n);
        let cell = |p: &Phase| {
            format!(
                "{:8.1} M {:5.1} {:7.2} ms",
                p.instructions as f64 / 1e6,
                p.allocations as f64 / nodes as f64,
                p.time.as_secs_f64() * 1e3
            )
        };
        println!(
            "  {name:5} build {} | mount+layout {} | teardown {} | {nodes} nodes",
            cell(&phases[0]),
            cell(&phases[1]),
            cell(&phases[2])
        );
    }
}
