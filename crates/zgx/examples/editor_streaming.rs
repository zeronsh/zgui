//! Headless TextEditor external-value microbenchmark, not a renderer/framework ranking.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct CountedAllocator;
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
fn added(bytes: usize) {
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}
// This benchmark counts Rust allocation requests, not allocator overhead or RSS.
// Each successful System operation is forwarded unchanged and accounted by size.
unsafe impl GlobalAlloc for CountedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        if !result.is_null() {
            added(layout.size());
        }
        result
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc_zeroed(layout) };
        if !result.is_null() {
            added(layout.size());
        }
        result
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
            added(size);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: CountedAllocator = CountedAllocator;
use zgx::text_edit::TextEditor;

fn resident_kib(field: &str) -> Option<usize> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix(field)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn main() {
    const BASELINE_BYTES: usize = 256 * 1024;
    const UPDATES: usize = 1000;
    const TOKEN: &str = " 好";
    const HISTORY_ENTRIES: usize = 100;
    const HISTORY_BYTES: usize = 4 * 1024 * 1024;
    let mut model = "a".repeat(BASELINE_BYTES);
    let mut editor = TextEditor::new(model.clone());
    editor.set_history_limits(HISTORY_ENTRIES, HISTORY_BYTES);
    let rss_before = resident_kib("VmRSS:");
    let live_before = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(live_before, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..UPDATES {
        model.push_str(TOKEN);
        assert!(editor.set_text(black_box(model.clone())));
    }
    let elapsed = start.elapsed();
    let live_after = LIVE_BYTES.load(Ordering::Relaxed);
    let peak_requested = PEAK_BYTES.load(Ordering::Relaxed);
    let rss_after = resident_kib("VmRSS:");
    let peak_rss = resident_kib("VmHWM:");
    assert_eq!(editor.text(), model);
    let mut undo_steps = 0;
    while editor.undo() {
        undo_steps += 1;
        assert!(undo_steps <= HISTORY_ENTRIES);
        let expected_len = BASELINE_BYTES + (UPDATES - undo_steps) * TOKEN.len();
        assert_eq!(editor.text(), &model[..expected_len]);
        assert_eq!(editor.selection().focus, expected_len);
    }
    let recovered_bytes = editor.text().len();
    let mut redo_steps = 0;
    while editor.redo() {
        redo_steps += 1;
        let expected_len = recovered_bytes + redo_steps * TOKEN.len();
        assert_eq!(editor.text(), &model[..expected_len]);
    }
    assert_eq!(undo_steps, redo_steps);
    assert_eq!(editor.text(), model);
    let optional = |value: Option<usize>| value.map_or("null".to_owned(), |n| n.to_string());
    println!(
        "{{\"baseline_bytes\":{BASELINE_BYTES},\"updates\":{UPDATES},\"token_bytes\":{},\"history_entry_limit\":{HISTORY_ENTRIES},\"history_byte_limit\":{HISTORY_BYTES},\"requested_live_before_bytes\":{live_before},\"requested_live_after_bytes\":{live_after},\"requested_peak_bytes\":{peak_requested},\"elapsed_ms\":{:.6},\"rss_before_kib\":{},\"rss_after_kib\":{},\"peak_rss_kib\":{},\"undo_steps\":{undo_steps},\"redo_steps\":{redo_steps},\"recovered_bytes\":{recovered_bytes},\"correct\":true}}",
        TOKEN.len(),
        elapsed.as_secs_f64() * 1000.,
        optional(rss_before),
        optional(rss_after),
        optional(peak_rss)
    );
}
