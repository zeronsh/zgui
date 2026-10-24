use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use zgx::text_layout::{FontFeatures, FontStyle};

thread_local! {
    // Const, destructor-free TLS keeps allocator bookkeeping allocation-free.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static REQUESTS: Cell<usize> = const { Cell::new(0) };
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn record_request(size: usize) {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = REQUESTS.try_with(|count| count.set(count.get() + 1));
        let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(size)));
    }
}

// SAFETY: Every allocation operation delegates unchanged to System. TLS only
// records calls on the measured test thread and never allocates or owns memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        // SAFETY: The caller supplies the GlobalAlloc layout contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        // SAFETY: The caller supplies the GlobalAlloc layout contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_request(new_size);
        // SAFETY: The original allocation and new size are forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The original allocation and layout are forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocation_requests(f: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTING.with(|enabled| enabled.set(false));
        }
    }
    REQUESTS.with(|count| count.set(0));
    LARGEST.with(|largest| largest.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let reset = Reset;
    f();
    drop(reset);
    REQUESTS.with(Cell::get)
}

#[test]
fn default_and_empty_font_settings_allocate_nothing_per_node_after_shared_initialization() {
    std::hint::black_box(FontStyle::default());
    assert_eq!(
        allocation_requests(|| {
            for _ in 0..10_000 {
                std::hint::black_box(FontStyle::default());
                std::hint::black_box(FontFeatures::default());
                std::hint::black_box(FontFeatures::new([]));
            }
        }),
        0
    );
    assert_eq!(FontFeatures::new([]), FontFeatures::default());
}
