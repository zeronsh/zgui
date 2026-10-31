//! Reloading a script gives back what the state it replaces held: the
//! heap stays level however many times a script reloads, as it does all day
//! under hot reload. A counting allocator of this test's own process
//! measures it.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};
use zgx_script::{ScriptHost, testing::TestApp};

struct Counting;
static NOW: AtomicUsize = AtomicUsize::new(0);
// SAFETY: Every call goes to the system allocator unchanged; only sizes are
// counted.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let at = unsafe { System.alloc(layout) };
        if !at.is_null() {
            NOW.fetch_add(layout.size(), Ordering::Relaxed);
        }
        at
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let at = unsafe { System.alloc_zeroed(layout) };
        if !at.is_null() {
            NOW.fetch_add(layout.size(), Ordering::Relaxed);
        }
        at
    }
    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        unsafe { System.dealloc(at, layout) };
        NOW.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, at: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(at, layout, size) };
        if !moved.is_null() {
            NOW.fetch_add(size, Ordering::Relaxed);
            NOW.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        moved
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Some 2 MB in the script's state, kept by an animation's closure.
const SCRIPT: &str = r##"
    local rows = {}
    for i = 1, 2000 do
        rows[i] = string.rep(tostring(i), 200)
    end
    local clip = anime.animate("#t", { x = 100, duration = 1000, loop = true,
        onUpdate = function() return rows end })
    return ui.column { on_click = function() return clip end, ui.div { id = "t", w = 50, h = 50 } }
"##;

#[test]
fn a_hundred_reloads_keep_the_heap_level() {
    let h = TestApp::new(|runtime| ScriptHost::inline(runtime, "app", SCRIPT));
    assert_eq!(h.host.error().get(), None);
    let reload = |n: usize| {
        for _ in 0..n {
            h.host.reload().unwrap();
            h.frames(2);
        }
        NOW.load(Ordering::Relaxed)
    };
    // Caches filled once; what follows should hold still.
    let settled = reload(10);
    let after = reload(90);
    assert!(
        after < settled + (4 << 20),
        "{settled} bytes grew to {after} over 90 reloads: {} a reload",
        after.saturating_sub(settled) / 90
    );
}
