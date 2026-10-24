//! Mounting and styling built-in widgets, headless: what an app made of
//! buttons, inputs and menu items pays at mount, at rest and on hover.
//! Run: cargo run -p zgx --release --example widget_bench [rows]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
use zgx::{
    compose::{button, column, row, text, text_input},
    input::InputEvent,
    style::{Styled, rgb},
    widgets::Ui,
};

/// Counts live heap bytes, to report what mounting retains.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
// SAFETY: every call goes to `System` unchanged (its contract the caller's);
// the counters are atomics, which allocate nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Relaxed);
        ALLOCATIONS.fetch_add(1, Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, Relaxed);
        LIVE.fetch_sub(layout.size(), Relaxed);
        ALLOCATIONS.fetch_add(1, Relaxed);
        unsafe { System.realloc(ptr, layout, new) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn main() {
    let rows: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let mut ui = Ui::new(1200., 800.);
    let values: Vec<_> = (0..rows).map(|_| ui.signal(String::new())).collect();
    let before = LIVE.load(Relaxed);
    let allocations = ALLOCATIONS.load(Relaxed);
    let started = Instant::now();
    let view = column().children(values.into_iter().enumerate().map(|(i, value)| {
        row()
            .gap(8.)
            .child(button().child(text(format!("Button {i}"))))
            .child(button().bg(rgb(0x2a3a50)).child(text("Styled")))
            .child(text_input("Name", value).w(160.))
    }));
    let _handle = ui.mount(view);
    ui.scene.borrow_mut().flush();
    let mount = started.elapsed();
    let retained = LIVE.load(Relaxed) - before;
    let mount_allocations = ALLOCATIONS.load(Relaxed) - allocations;
    let started = Instant::now();
    for _ in 0..1000 {
        assert!(ui.scene.borrow_mut().flush().is_idle());
    }
    let idle = started.elapsed();
    // Hover across the first buttons: each enter and leave restyles one.
    let started = Instant::now();
    let mut painted = 0;
    for step in 0..400 {
        let y = 4. + (step % 20) as f32 * 36.;
        ui.dispatch(InputEvent::PointerMove { x: 20., y });
        painted += ui.scene.borrow_mut().flush().paint_nodes;
    }
    let hover = started.elapsed();
    println!(
        "{rows} rows ({} widgets): mount {:.2} ms, {:.1} KiB retained, {} allocations, {} effects",
        rows * 3,
        mount.as_secs_f64() * 1e3,
        retained as f64 / 1024.,
        mount_allocations,
        ui.runtime.effect_count()
    );
    println!(
        "idle flush {:.1} ns; hover step {:.2} us ({painted} paint nodes)",
        idle.as_nanos() as f64 / 1000.,
        hover.as_secs_f64() * 1e6 / 400.
    );
    // SWITCH=1: switch the theme back and forth.
    if std::env::var_os("SWITCH").is_some() {
        let themes = ui.theme().clone();
        let started = Instant::now();
        let (mut layout, mut paint) = (0, 0);
        for i in 0..20 {
            themes
                .set(if i % 2 == 0 {
                    zgx::theme::LIGHT
                } else {
                    zgx::theme::DARK
                })
                .unwrap();
            let report = ui.scene.borrow_mut().flush();
            layout += report.layout_nodes;
            paint += report.paint_nodes;
        }
        println!(
            "theme switch {:.2} ms ({} paint, {} layout nodes per switch; {} token signals)",
            started.elapsed().as_secs_f64() * 1e3 / 20.,
            paint / 20,
            layout / 20,
            themes.root().signal_count()
        );
    }
}
