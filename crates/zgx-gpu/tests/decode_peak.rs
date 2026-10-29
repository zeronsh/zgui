//! Decoding an image allocates about the image once (`assets::decode_image`):
//! its final RGBA buffer, decoded into and widened in place, not the
//! decoder's output, its RGBA form and `ImageData`'s copy of it at once.
//! A counting allocator of this test's own process measures it.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Counting;
static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
// SAFETY: Every call goes to the system allocator unchanged; only sizes are
// counted.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let at = unsafe { System.alloc(layout) };
        if !at.is_null() {
            let now = NOW.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        at
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let at = unsafe { System.alloc_zeroed(layout) };
        if !at.is_null() {
            let now = NOW.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
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
            let now = if size >= layout.size() {
                NOW.fetch_add(size - layout.size(), Ordering::Relaxed) + size - layout.size()
            } else {
                NOW.fetch_sub(layout.size() - size, Ordering::Relaxed) - (layout.size() - size)
            };
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        moved
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The most `decode_image(bytes)` holds at once beyond what was held
/// before, and its pixels.
fn decode(bytes: &[u8]) -> (usize, Vec<u8>) {
    let before = NOW.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let image = zgx_gpu::assets::decode_image(bytes).expect("decodes");
    let peak = PEAK.load(Ordering::Relaxed) - before;
    (peak, image.pixels().to_vec())
}

/// A `width` by `height` image of `channels` bytes a pixel, a pattern no
/// two neighbours share, encoded by `encode`.
fn pattern(width: u32, height: u32, channels: usize) -> Vec<u8> {
    (0..width as usize * height as usize * channels)
        .map(|i| (i * 7 + i / 13) as u8)
        .collect()
}

#[test]
fn a_decode_holds_about_the_image_once() {
    use image::{ExtendedColorType, ImageEncoder};
    let (width, height) = (1024_u32, 768_u32);
    let image_bytes = width as usize * height as usize * 4;
    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
    for (name, channels, colour) in [
        ("png rgba", 4, ExtendedColorType::Rgba8),
        ("png rgb", 3, ExtendedColorType::Rgb8),
        ("png grey", 1, ExtendedColorType::L8),
        ("png grey alpha", 2, ExtendedColorType::La8),
    ] {
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(&pattern(width, height, channels), width, height, colour)
            .unwrap();
        cases.push((name, encoded));
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .write_image(
            &pattern(width, height, 3),
            width,
            height,
            ExtendedColorType::Rgb8,
        )
        .unwrap();
    cases.push(("jpeg rgb", jpeg));
    for (name, encoded) in &cases {
        let (peak, pixels) = decode(encoded);
        // The same pixels `image` makes of it.
        let expected = image::load_from_memory(encoded)
            .unwrap()
            .into_rgba8()
            .into_raw();
        assert!(pixels == expected, "{name}: pixels differ from image's");
        // The image once, and the decoder's own: a PNG's rows (a tenth
        // more at most); a JPEG's colour planes, which zune-jpeg fills
        // before converting into ours (1.5 bytes a pixel at 4:2:0: under
        // two fifths more). Decoded into a copy, both held twice the image.
        let own = if name.starts_with("jpeg") {
            image_bytes * 2 / 5
        } else {
            image_bytes / 10
        };
        assert!(
            peak <= image_bytes + own,
            "{name}: a decode held {:.2} images at once",
            peak as f64 / image_bytes as f64
        );
    }
}
