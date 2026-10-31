//! zgx: DCT-scaled decoding (`JpegDecoder::set_scale`).
//!
//! At 1/1 the decoder must match the unmodified crate byte for byte; at 1/2,
//! 1/4 and 1/8 it must be the full decode shrunk by area averaging, within
//! JPEG's own error, and fail on bad input exactly where the full decode does.
//! The fixtures are made by `tests/scaled/make_fixtures.py`.
//!
//! `cargo test --test scaled -- --nocapture scaled_is` prints the error table.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scaled");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    assert!(out.len() >= 20, "fixtures missing from {dir:?}");
    out
}

fn options(colorspace: ColorSpace) -> DecoderOptions {
    DecoderOptions::default().jpeg_set_out_colorspace(colorspace)
}

/// Decode at 1/`d`, returning the pixels, the reported size and the reported
/// buffer size.
fn decode(
    bytes: &[u8],
    d: u8,
    colorspace: ColorSpace,
) -> Result<(Vec<u8>, usize, usize, usize), String> {
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options(colorspace));
    decoder.set_scale(d);
    decoder.decode_headers().map_err(|e| format!("{e:?}"))?;
    let (w, h) = decoder.dimensions().unwrap();
    let size = decoder.output_buffer_size().unwrap();
    let pixels = decoder.decode().map_err(|e| format!("{e:?}"))?;
    Ok((pixels, w, h, size))
}

fn decode_upstream(bytes: &[u8], colorspace: ColorSpace) -> Result<Vec<u8>, String> {
    let options = upstream::zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(
        match colorspace {
            ColorSpace::RGB => upstream::zune_core::colorspace::ColorSpace::RGB,
            ColorSpace::RGBA => upstream::zune_core::colorspace::ColorSpace::RGBA,
            ColorSpace::Luma => upstream::zune_core::colorspace::ColorSpace::Luma,
            _ => unreachable!(),
        },
    );
    let mut decoder = upstream::JpegDecoder::new_with_options(
        upstream::zune_core::bytestream::ZCursor::new(bytes),
        options,
    );
    decoder.decode().map_err(|e| format!("{e:?}"))
}

#[test]
fn full_size_matches_upstream_byte_for_byte() {
    for (name, bytes) in fixtures() {
        for colorspace in [ColorSpace::RGB, ColorSpace::RGBA, ColorSpace::Luma] {
            let Ok(theirs) = decode_upstream(&bytes, colorspace) else {
                // CMYK to grey, which neither decodes.
                assert!(
                    decode(&bytes, 1, colorspace).is_err(),
                    "{name} {colorspace:?}"
                );
                continue;
            };
            // The default, and 1 asked for.
            let mut decoder =
                JpegDecoder::new_with_options(ZCursor::new(&bytes), options(colorspace));
            assert_eq!(
                decoder.decode().unwrap(),
                theirs,
                "{name} {colorspace:?} default"
            );
            let (ours, ..) = decode(&bytes, 1, colorspace).unwrap();
            assert_eq!(ours, theirs, "{name} {colorspace:?} set_scale(1)");
        }
    }
}

#[test]
fn scale_rounds_down_to_a_supported_denominator() {
    let (_, bytes) = fixtures()
        .into_iter()
        .find(|(n, _)| n == "p420_1001x667")
        .unwrap();
    for (asked, d) in [
        (0, 1),
        (1, 1),
        (2, 2),
        (3, 2),
        (4, 4),
        (7, 4),
        (8, 8),
        (9, 8),
        (255, 8),
    ] {
        let (_, w, h, _) = decode(&bytes, asked, ColorSpace::RGB).unwrap();
        assert_eq!(
            (w, h),
            (1001usize.div_ceil(d), 667usize.div_ceil(d)),
            "set_scale({asked})"
        );
    }
}

/// The full decode averaged over `d` by `d` squares (at the edges, over the
/// part of the square inside the image).
fn area_average(full: &[u8], w: usize, h: usize, c: usize, d: usize) -> Vec<u8> {
    let (sw, sh) = (w.div_ceil(d), h.div_ceil(d));
    let mut out = vec![0u8; sw * sh * c];
    for y in 0..sh {
        for x in 0..sw {
            for ch in 0..c {
                let (mut sum, mut n) = (0u32, 0u32);
                for yy in y * d..((y + 1) * d).min(h) {
                    for xx in x * d..((x + 1) * d).min(w) {
                        sum += u32::from(full[(yy * w + xx) * c + ch]);
                        n += 1;
                    }
                }
                out[(y * sw + x) * c + ch] = ((sum + n / 2) / n) as u8;
            }
        }
    }
    out
}

struct Error {
    mae: Vec<f64>,
    psnr: f64,
    max: u8,
}

fn error(a: &[u8], b: &[u8], c: usize) -> Error {
    let mut sum = vec![0u64; c];
    let (mut sq, mut max) = (0u64, 0u8);
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let e = x.abs_diff(*y);
        sum[i % c] += u64::from(e);
        sq += u64::from(e) * u64::from(e);
        max = max.max(e);
    }
    let n = (a.len() / c) as f64;
    let mse = sq as f64 / a.len() as f64;
    Error {
        mae: sum.iter().map(|s| *s as f64 / n).collect(),
        psnr: if mse == 0.0 {
            f64::INFINITY
        } else {
            10.0 * (255.0 * 255.0 / mse).log10()
        },
        max,
    }
}

#[test]
fn scaled_is_the_full_decode_area_averaged() {
    let mut failures = vec![];
    println!(
        "{:<24} {:>2} {:>9} {:>22} {:>7} {:>4}",
        "fixture", "d", "size", "MAE per channel", "PSNR", "max"
    );
    for (name, bytes) in fixtures() {
        // RGBA, as zgx decodes, and RGB checked against it below. CMYK to RGBA
        // is broken upstream (it writes 3 bytes a pixel into a 4-byte stride,
        // at any scale), so CMYK is checked as RGB.
        let cmyk = name.starts_with("cmyk");
        let (colorspace, c) = if cmyk {
            (ColorSpace::RGB, 3)
        } else {
            (ColorSpace::RGBA, 4)
        };
        let (full, w, h, _) = decode(&bytes, 1, colorspace).unwrap();
        for d in [2u8, 4, 8] {
            let (scaled, sw, sh, size) = decode(&bytes, d, colorspace).unwrap();
            let du = usize::from(d);
            assert_eq!(
                (sw, sh),
                (w.div_ceil(du), h.div_ceil(du)),
                "{name} 1/{d} dimensions"
            );
            assert_eq!(size, sw * sh * c, "{name} 1/{d} output_buffer_size");
            assert_eq!(scaled.len(), size, "{name} 1/{d} decoded length");
            if !cmyk {
                assert!(
                    scaled.chunks_exact(4).all(|p| p[3] == 255),
                    "{name} 1/{d} alpha"
                );
                let (rgb, ..) = decode(&bytes, d, ColorSpace::RGB).unwrap();
                let rgb_of_rgba: Vec<u8> = scaled
                    .chunks_exact(4)
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect();
                assert_eq!(rgb, rgb_of_rgba, "{name} 1/{d} RGB is RGBA without alpha");
            }

            let expected = area_average(&full, w, h, c, du);
            let e = error(&scaled, &expected, c);
            let mae: Vec<String> = e.mae[..3].iter().map(|m| format!("{m:.2}")).collect();
            println!(
                "{name:<24} {d:>2} {:>9} {:>22} {:>7.2} {:>4}",
                format!("{sw}x{sh}"),
                mae.join(" "),
                e.psnr,
                e.max
            );
            // Printed, not bounded: `graphic` files, the worst case for chroma
            // rather than photos (see make_fixtures.py), and subsampled images
            // scaled to under 16 pixels. There every pixel is an edge pixel, and
            // the full decode's up-sampling weighs the last chroma sample by the
            // half of it inside the image, which a scaled decode can't.
            let subsampled = !name.contains("444") && !name.contains("grey");
            let exempt = name.starts_with("graphic") || (subsampled && sw * sh < 16);
            if !exempt && (e.mae.iter().any(|m| *m > 2.0) || e.psnr < 35.0) {
                failures.push(format!("{name} 1/{d}: MAE {mae:?} PSNR {:.2}", e.psnr));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "too far from the area-averaged full decode:\n{}",
        failures.join("\n")
    );
}

/// Grey output from colour files takes its own path (`coeff`), as does a grey
/// file to RGB; both scale too.
#[test]
fn scaled_luma_output() {
    for (name, bytes) in fixtures()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("cmyk"))
    {
        let (full, w, h, _) = decode(&bytes, 1, ColorSpace::Luma).unwrap();
        for d in [2u8, 4, 8] {
            let (scaled, sw, sh, size) = decode(&bytes, d, ColorSpace::Luma).unwrap();
            assert_eq!(scaled.len(), size);
            assert_eq!(size, sw * sh, "{name} 1/{d}");
            let e = error(&scaled, &area_average(&full, w, h, 1, usize::from(d)), 1);
            assert!(
                name.starts_with("graphic") || e.mae[0] <= 2.0 && e.psnr >= 35.0,
                "{name} 1/{d} luma: MAE {:.2} PSNR {:.2}",
                e.mae[0],
                e.psnr
            );
        }
    }
}

fn outcome(bytes: &[u8], d: u8) -> Result<Result<usize, String>, ()> {
    catch_unwind(AssertUnwindSafe(|| {
        decode(bytes, d, ColorSpace::RGBA).map(|(p, ..)| p.len())
    }))
    .map_err(|_| ())
}

/// Truncated and corrupted files: a scaled decode fails where the full one
/// fails, succeeds (degraded) where it succeeds, and never panics.
#[test]
fn bad_input_fails_as_the_full_decode_does() {
    let mut cases = 0;
    for (name, bytes) in fixtures() {
        let mut bad: Vec<(String, Vec<u8>)> = vec![];
        for cut in [3, 10, 25, 50, 75, 90, 99] {
            bad.push((
                format!("cut {cut}%"),
                bytes[..bytes.len() * cut / 100].to_vec(),
            ));
        }
        // Flip bytes in the entropy-coded data (after the first SOS), seeded.
        let sos =
            (bytes.windows(2).position(|w| w == [0xff, 0xda]).unwrap() + 20).min(bytes.len() - 3);
        let mut seed = 0x9e37_79b9_u32;
        for round in 0..12 {
            let mut b = bytes.clone();
            for _ in 0..(1 + round % 4) {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let at = sos + (seed as usize) % (b.len() - sos);
                b[at] ^= (seed >> 24) as u8 | 1;
            }
            bad.push((format!("flip {round}"), b));
        }
        for (what, b) in &bad {
            let full = outcome(b, 1);
            // Unscaled, exactly as upstream, bad input included.
            let theirs = catch_unwind(|| decode_upstream(b, ColorSpace::RGBA)).map_err(|_| ());
            let ours = catch_unwind(AssertUnwindSafe(|| {
                decode(b, 1, ColorSpace::RGBA).map(|(p, ..)| p)
            }))
            .map_err(|_| ());
            match (&theirs, &ours) {
                (Ok(Ok(t)), Ok(Ok(o))) => {
                    assert!(t == o, "{name} {what}: 1/1 differs from upstream")
                }
                _ => assert_eq!(
                    theirs.is_ok(),
                    ours.is_ok(),
                    "{name} {what}: 1/1 panics unlike upstream"
                ),
            }
            for d in [2, 4, 8] {
                let scaled = outcome(b, d);
                assert!(
                    scaled.is_ok(),
                    "{name} {what}: panicked at 1/{d} (full decode {full:?})"
                );
                if let Ok(full) = &full {
                    assert_eq!(
                        full.is_ok(),
                        scaled.as_ref().unwrap().is_ok(),
                        "{name} {what} 1/{d}: {full:?} vs {scaled:?}"
                    );
                }
                cases += 1;
            }
        }
    }
    assert!(cases > 1000);
}

/// Timing, not a check: `ZGX_JPEG_BENCH=photo.jpg cargo test --release --test
/// scaled -- --ignored --nocapture bench`. `ZGX_JPEG_BENCH_SCALE=2` times one
/// scale alone (for its peak memory, say).
#[test]
#[ignore]
fn bench() {
    let Ok(path) = std::env::var("ZGX_JPEG_BENCH") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let only: Option<u8> = std::env::var("ZGX_JPEG_BENCH_SCALE")
        .ok()
        .map(|d| d.parse().unwrap());
    for d in [1u8, 2, 4, 8]
        .into_iter()
        .filter(|d| only.map_or(true, |o| o == *d))
    {
        let mut times = vec![];
        for _ in 0..15 {
            let start = std::time::Instant::now();
            let (pixels, ..) = decode(&bytes, d, ColorSpace::RGBA).unwrap();
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(pixels);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "1/{d}: min {:.2} ms, median {:.2} ms",
            times[0],
            times[times.len() / 2]
        );
    }
}
