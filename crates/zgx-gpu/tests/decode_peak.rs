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
/// Held by each test while it measures: the counts are the process's, and
/// the harness runs tests on threads of their own at once, so another
/// test's decode counted in this one's peak (0.26 images, past a bound of
/// 0.25, now and then).
static ALONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _alone = ALONE.lock().unwrap_or_else(|p| p.into_inner());
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

/// A smooth `width` by `height` RGB picture, as a photo is (gradients, a
/// disc, stripes), not noise a downsample can't keep.
fn photo(width: u32, height: u32) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (x as f32 / width as f32, y as f32 / height as f32);
            let disc = ((fx - 0.6).powi(2) + (fy - 0.4).powi(2)).sqrt() < 0.2;
            let stripe = (x / 24) % 2 == 0;
            rgb.extend([
                (fx * 200. + if disc { 50. } else { 0. }) as u8,
                (fy * 180. + if stripe { 40. } else { 0. }) as u8,
                ((1. - fx) * 150. + fy * 60.) as u8,
            ]);
        }
    }
    rgb
}

/// A JPEG drawn at a quarter of its size decodes at a quarter (the DCT
/// scaled: vendor/zune-jpeg), never whole: its decode holds a small part of
/// the image, and draws what decoding it whole and shrinking it would.
#[test]
fn a_jpeg_drawn_small_never_decodes_whole() {
    let _alone = ALONE.lock().unwrap_or_else(|p| p.into_inner());
    use image::{ExtendedColorType, ImageEncoder};
    let (width, height) = (1024_u32, 768_u32);
    let image_bytes = width as usize * height as usize * 4;
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .write_image(
            &photo(width, height),
            width,
            height,
            ExtendedColorType::Rgb8,
        )
        .unwrap();
    let size = (256, 192);
    let image = zgx_gpu::assets::decode_image_kept(jpeg.clone().into()).unwrap();
    image.set_decoding(zgx::image::Decoding::Sync);
    let before = NOW.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let scaled = image.pixels_at(size).expect("decoded").pixels.to_vec();
    let peak = PEAK.load(Ordering::Relaxed) - before;
    // Its quarter is a sixteenth of the image; the decoder's rows and
    // tables, as large as the image is wide, come to as much again or so.
    // Decoded whole, it held at least the image (`a_decode_holds_...`).
    assert!(
        peak * 4 <= image_bytes,
        "a decode drawn at a quarter held {:.2} images at once",
        peak as f64 / image_bytes as f64
    );
    // The same picture as decoding whole and shrinking, near enough.
    let image = zgx_gpu::assets::decode_image(&jpeg).unwrap();
    let mut reference = image.pixels().to_vec();
    let (mut w, mut h) = (width, height);
    while w.div_ceil(2) >= size.0 && h.div_ceil(2) >= size.1 {
        (reference, w, h) = zgx::image::resample::halve(&reference, w, h);
    }
    assert_eq!((w, h), size);
    let (sum, squares, worst) = scaled
        .iter()
        .zip(&reference)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .fold((0., 0., 0f64), |(s, q, m), d| (s + d, q + d * d, m.max(d)));
    let n = scaled.len() as f64;
    let (mae, psnr) = (sum / n, 10. * (255f64.powi(2) / (squares / n)).log10());
    assert!(
        mae <= 2. && psnr >= 35.,
        "mean error {mae:.2}, PSNR {psnr:.1} dB, worst {worst}"
    );
}
