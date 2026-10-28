//! The heavy dashboard (see zgx_workload::heavy) offscreen: the windowed
//! `heavy_workload`'s view at a Retina scale, ticked as fast as it renders,
//! reporting CPU and allocations per tick by phase, damage and renderer
//! memory.
//! Run: ZGX_MODE=both TICKS=900 cargo run --release -p zgx-desktop --example heavy_headless
//! (`SCALE` sets the device scale, 2 by default. `RENDER=0` lays out and
//! finds damage with no renderer: the engine's work alone, as CI's
//! instruction gate counts it.)
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::{Duration, Instant},
};
use zgx::widgets::Ui;
use zgx_gpu::{GpuRenderer, TextServices};
use zgx_workload::{Mode, heavy::*};

#[path = "support/headless_font.rs"]
mod headless_font;
#[path = "support/heavy_view.rs"]
mod view;

/// Counts the allocations made (`realloc` counts as one).
#[cfg_attr(feature = "mimalloc", allow(dead_code))]
struct Counting;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
// SAFETY: every call goes to `System` with the caller's own arguments (so
// `GlobalAlloc`'s contract passes through unchanged); counting allocates
// nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        // SAFETY: the caller's layout, as `alloc` requires.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` (every allocation here does).
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        // SAFETY: as `dealloc`, with the caller's new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
// With the `mimalloc` feature, that allocator stands instead (allocations
// then read 0).
#[cfg_attr(not(feature = "mimalloc"), global_allocator)]
#[cfg_attr(feature = "mimalloc", allow(dead_code))]
static GLOBAL: Counting = Counting;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let ticks: usize = env("TICKS", 900);
    let scale: f32 = env("SCALE", 2.);
    let mut work = Heavy::from_env();
    let mode = work.mode;
    let mut ui = Ui::new(WIDTH, HEIGHT);
    if env("RENDER", 1) == 0 {
        return without_rendering(ui, work, ticks);
    }
    let mut gpu = GpuRenderer::new(
        (WIDTH * scale).round() as u32,
        (HEIGHT * scale).round() as u32,
    )
    .expect("GPU");
    gpu.set_scale_factor(scale);
    headless_font::font().install(&mut gpu.text_system().borrow_mut());
    gpu.install_text(&mut ui.scene.borrow_mut());
    let live = ui.signal(0_u64);
    let scroll = ui.signal(0_f32);
    let log = ui.signal(work.visible_log());
    ui.mount(view::dashboard(live.clone(), scroll.clone(), log.clone()));
    let (mut update, mut layout, mut render) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut damaged, mut nodes) = (0_u64, 0_usize);
    // Allocations by phase: update, layout, render.
    let mut allocations = [0_u64; 3];
    for tick in 0..ticks {
        let started = Instant::now();
        let at_start = ALLOCATIONS.load(Relaxed);
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
        let updated = Instant::now();
        let at_update = ALLOCATIONS.load(Relaxed);
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        let laid = Instant::now();
        let at_layout = ALLOCATIONS.load(Relaxed);
        let stats = gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        // A window presents every frame; readback only settles the GPU.
        if tick % 60 == 0 {
            gpu.readback().unwrap();
        }
        let done = Instant::now();
        let at_render = ALLOCATIONS.load(Relaxed);
        if tick >= 60 {
            allocations[0] += at_update - at_start;
            allocations[1] += at_layout - at_update;
            allocations[2] += at_render - at_layout;
            update += updated - started;
            layout += laid - updated;
            render += done - laid;
            damaged += stats.damaged_pixels;
            nodes += report.layout_nodes;
        }
    }
    let measured = ticks.saturating_sub(60).max(1) as f64;
    let ms = |d: Duration| d.as_secs_f64() * 1e3 / measured;
    let mb = |bytes: usize| bytes as f64 / 1048576.;
    let caches = gpu.debug_cache_stats();
    let prepared = gpu.text_cache().borrow().bytes();
    let text_cache = gpu.text_cache();
    let (shaped, hits) = {
        let cache = text_cache.borrow();
        (cache.shaped, cache.hits)
    };
    println!(
        "{mode:?}: update {:.3} + layout {:.3} + render {:.3} = {:.3} ms/tick, \
         layout nodes {:.0}/tick, damaged {:.2} Mpx/tick, scene nodes {}, \
         atlas {:.1} MB, images {:.1} MB, shaped {:.1} MB, prepared {:.1}+{:.1} MB, device {:.1} MB, \
         texts shaped {:.1}/tick, cache hits {:.1}/tick, \
         allocations {:.0}+{:.0}+{:.0}/tick",
        ms(update),
        ms(layout),
        ms(render),
        ms(update + layout + render),
        nodes as f64 / measured,
        damaged as f64 / measured / 1e6,
        ui.scene.borrow().len(),
        mb(caches.atlas_bytes),
        mb(caches.image_bytes),
        mb(caches.shaped_bytes),
        mb(prepared.0),
        mb(prepared.1),
        gpu.debug_device_allocated_bytes()
            .map_or(0., |b| b as f64 / 1048576.),
        shaped as f64 / ticks as f64,
        hits as f64 / ticks as f64,
        allocations[0] as f64 / measured,
        allocations[1] as f64 / measured,
        allocations[2] as f64 / measured,
    );
}
/// The ticks' updates, layout and damage, with text measured as a renderer
/// would and nothing drawn.
fn without_rendering(mut ui: Ui, mut work: Heavy, ticks: usize) {
    let mode = work.mode;
    let text = TextServices::new();
    headless_font::font().install(&mut text.text_system().borrow_mut());
    text.install(&mut ui.scene.borrow_mut());
    let live = ui.signal(0_u64);
    let scroll = ui.signal(0_f32);
    let log = ui.signal(work.visible_log());
    ui.mount(view::dashboard(live.clone(), scroll.clone(), log.clone()));
    let (mut rects, mut nodes) = (0_usize, 0_usize);
    let started = Instant::now();
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
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        rects += report.damage.len();
        nodes += report.layout_nodes;
        zgx::profile::frame();
    }
    let per = ticks.max(1) as f64;
    println!(
        "{mode:?} without rendering: {:.3} ms/tick, layout nodes {:.0}/tick, damage rects {:.1}/tick",
        started.elapsed().as_secs_f64() * 1e3 / per,
        nodes as f64 / per,
        rects as f64 / per,
    );
}
