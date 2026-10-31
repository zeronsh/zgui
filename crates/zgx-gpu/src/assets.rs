//! Bounded image decoding and SVG rasterization, independent of a GPU device.
use crate::GpuError;
use std::{io::Cursor, sync::Arc};
use zgx::image::ImageData;
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// A PNG or JPEG keeping its file (`ImageData::kept`): only its header is
/// read here; its pixels decode as `decode_image` does, off the main thread,
/// when it is drawn, at the size it is drawn, and go once a renderer has
/// them. The bytes are shared, not copied.
pub fn decode_image_kept(bytes: Arc<[u8]>) -> Result<Arc<ImageData>, GpuError> {
    if bytes.len() > MAX_BYTES {
        return Err(GpuError("encoded image exceeds 64 MiB".into()));
    }
    let reader = image::ImageReader::new(Cursor::new(&bytes[..]))
        .with_guessed_format()
        .map_err(|e| GpuError(e.to_string()))?;
    let format = reader.format();
    let decodable = (cfg!(feature = "png") && format == Some(image::ImageFormat::Png))
        || (cfg!(feature = "jpeg") && format == Some(image::ImageFormat::Jpeg));
    // Others (a GIF, a format left out) decode now, as they always did.
    if !decodable {
        return decode_image(&bytes);
    }
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| GpuError(e.to_string()))?;
    if width > 8192 || height > 8192 || width as usize * height as usize * 4 > MAX_BYTES {
        return Err(GpuError("decoded image exceeds 64 MiB".into()));
    }
    ImageData::kept(width, height, bytes, redecode)
        .map(Arc::new)
        .map_err(|e| GpuError(e.into()))
}
/// A kept file's pixels (`decode_image_kept`).
fn redecode(bytes: &[u8]) -> Result<Arc<[u8]>, String> {
    decode_image(bytes)
        .map(|image| image.pixels_arc())
        .map_err(|e| e.0)
}
/// Decode PNG/JPEG into immutable, straight-alpha RGBA8. No filesystem/network access.
///
/// The image is decoded into its final buffer, allocated once: 8-bit RGB,
/// grey and grey-with-alpha images decode into its front and widen to RGBA
/// in place, so a decode's peak is about the image itself, not the three
/// copies (decoder output, its RGBA form, `ImageData`'s) it once was.
pub fn decode_image(bytes: &[u8]) -> Result<Arc<ImageData>, GpuError> {
    if bytes.len() > MAX_BYTES {
        return Err(GpuError("encoded image exceeds 64 MiB".into()));
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| GpuError(e.to_string()))?;
    match reader.format() {
        Some(image::ImageFormat::Png) if !cfg!(feature = "png") => {
            return Err(GpuError("PNG images need zgx-gpu's `png` feature".into()));
        }
        Some(image::ImageFormat::Jpeg) if !cfg!(feature = "jpeg") => {
            return Err(GpuError("JPEG images need zgx-gpu's `jpeg` feature".into()));
        }
        _ => {}
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_BYTES as u64);
    reader.limits(limits);
    let failed = |e: image::ImageError| GpuError(e.to_string());
    let decoder = reader.into_decoder().map_err(failed)?;
    use image::ImageDecoder;
    let (width, height) = decoder.dimensions();
    let pixels = width as usize * height as usize;
    if pixels * 4 > MAX_BYTES {
        return Err(GpuError("decoded image exceeds 64 MiB".into()));
    }
    let channels = match decoder.color_type() {
        image::ColorType::Rgba8 => 4,
        image::ColorType::Rgb8 => 3,
        image::ColorType::La8 => 2,
        image::ColorType::L8 => 1,
        // Deeper or wider samples: as `image` converts them.
        _ => {
            let decoded = image::DynamicImage::from_decoder(decoder)
                .map_err(failed)?
                .into_rgba8();
            return ImageData::new(width, height, decoded.into_raw())
                .map(Arc::new)
                .map_err(|e| GpuError(e.into()));
        }
    };
    // SAFETY: Zeroed bytes are initialized `u8`s.
    let mut rgba = unsafe { Arc::<[u8]>::new_zeroed_slice(pixels * 4).assume_init() };
    let buffer = Arc::get_mut(&mut rgba).expect("a buffer just made");
    decoder
        .read_image(&mut buffer[..pixels * channels])
        .map_err(failed)?;
    widen(buffer, channels, pixels);
    ImageData::new(width, height, rgba)
        .map(Arc::new)
        .map_err(|e| GpuError(e.into()))
}
/// `pixels` pixels of `channels` bytes each (RGB, grey with alpha, grey) at
/// the front of `buffer`, widened in place to RGBA, last first: a pixel's
/// four bytes never reach a narrower pixel not yet read.
fn widen(buffer: &mut [u8], channels: usize, pixels: usize) {
    if channels == 4 {
        return;
    }
    for i in (0..pixels).rev() {
        let at = i * channels;
        let rgba = match channels {
            3 => [buffer[at], buffer[at + 1], buffer[at + 2], 255],
            2 => [buffer[at], buffer[at], buffer[at], buffer[at + 1]],
            _ => [buffer[at], buffer[at], buffer[at], 255],
        };
        buffer[i * 4..i * 4 + 4].copy_from_slice(&rgba);
    }
}
/// Rasterize an SVG at the requested physical size. External resources are disabled.
/// SVG text needs conversion to outlines; the lightweight SVG dependency disables fonts.
pub fn decode_svg(bytes: &[u8], width: u32, height: u32) -> Result<Arc<ImageData>, GpuError> {
    if bytes.len() > 4 * 1024 * 1024
        || width == 0
        || height == 0
        || (u64::from(width) * u64::from(height))
            .checked_mul(4)
            .is_none_or(|bytes| bytes > MAX_BYTES as u64)
    {
        return Err(GpuError("SVG input or output exceeds bounds".into()));
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| GpuError("SVG image allocation failed".into()))?;
    // A raster where an SVG was asked for (a URL that serves either): drawn
    // as that picture, fitted as an SVG is.
    if image::guess_format(bytes).is_ok() {
        let image = decode_image(bytes)?;
        let mut source = tiny_skia::Pixmap::new(image.width(), image.height())
            .ok_or_else(|| GpuError("SVG image allocation failed".into()))?;
        for (to, from) in source
            .pixels_mut()
            .iter_mut()
            .zip(image.pixels().as_chunks::<4>().0)
        {
            *to = tiny_skia::ColorU8::from_rgba(from[0], from[1], from[2], from[3]).premultiply();
        }
        pixmap.draw_pixmap(
            0,
            0,
            source.as_ref(),
            &tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bicubic,
                ..Default::default()
            },
            contain(image.width() as f32, image.height() as f32, width, height),
            None,
        );
    } else {
        render_svg(bytes, width, height, &mut pixmap)?;
    }
    let mut pixels = pixmap.take();
    for p in pixels.as_chunks_mut::<4>().0 {
        let alpha = p[3] as u32;
        if alpha > 0 {
            for c in &mut p[..3] {
                *c = (u32::from(*c) * 255 + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
    }
    ImageData::new(width, height, pixels)
        .map(Arc::new)
        .map_err(|e| GpuError(e.into()))
}

/// A picture `w` by `h` scaled to fit `width` by `height` whole, centred,
/// keeping its proportions: an SVG's default `preserveAspectRatio`
/// (`xMidYMid meet`), so a wide mark in a square box is not squeezed.
fn contain(w: f32, h: f32, width: u32, height: u32) -> tiny_skia::Transform {
    let scale = (width as f32 / w).min(height as f32 / h);
    tiny_skia::Transform::from_row(
        scale,
        0.,
        0.,
        scale,
        (width as f32 - w * scale) / 2.,
        (height as f32 - h * scale) / 2.,
    )
}

/// An SVG document drawn into `pixmap`, fitted to it (`contain`).
#[cfg(feature = "svg")]
fn render_svg(
    bytes: &[u8],
    width: u32,
    height: u32,
    pixmap: &mut tiny_skia::Pixmap,
) -> Result<(), GpuError> {
    let mut options = resvg::usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree =
        resvg::usvg::Tree::from_data(bytes, &options).map_err(|e| GpuError(e.to_string()))?;
    let transform = contain(tree.size().width(), tree.size().height(), width, height);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(())
}
#[cfg(not(feature = "svg"))]
fn render_svg(_: &[u8], _: u32, _: u32, _: &mut tiny_skia::Pixmap) -> Result<(), GpuError> {
    Err(GpuError("SVG images need zgx-gpu's `svg` feature".into()))
}

/// Decode GIF frames once, compositing offsets and Keep/Background/Previous
/// disposal into full straight-alpha frames. Background disposal is transparent,
/// matching GUI image compositing rather than filling the window with GIF palette
/// background. Absent looping extension plays once; finite repetitions add to the
/// initial play. Zero-delay GIF frames use 100 ms; other delays floor at 10 ms.
#[cfg(feature = "gif")]
pub fn decode_gif(bytes: &[u8]) -> Result<Arc<zgx::animation::Animation>, GpuError> {
    use zgx::animation::{Animation, Frame, LoopCount};
    if bytes.len() > MAX_BYTES {
        return Err(GpuError("encoded GIF exceeds 64 MiB".into()));
    }
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.check_frame_consistency(true);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(MAX_BYTES as u64).unwrap(),
    ));
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(|e| GpuError(e.to_string()))?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let length = u64::from(width) * u64::from(height) * 4;
    if length == 0 || length > MAX_BYTES as u64 {
        return Err(GpuError("GIF canvas exceeds 64 MiB".into()));
    }
    let mut canvas = vec![0u8; length as usize];
    let mut frames = Vec::new();
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|e| GpuError(e.to_string()))?
    {
        if frames.len() >= 4096 || (frames.len() + 1) as u64 * length > MAX_BYTES as u64 {
            return Err(GpuError("decoded GIF exceeds frame/64 MiB budget".into()));
        }
        let previous = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let (fw, fh, left, top) = (
            usize::from(frame.width),
            usize::from(frame.height),
            usize::from(frame.left),
            usize::from(frame.top),
        );
        for y in 0..fh {
            for x in 0..fw {
                let source = 4 * (y * fw + x);
                let destination = 4 * ((top + y) * width as usize + left + x);
                if frame.buffer[source + 3] > 0 {
                    canvas[destination..destination + 4]
                        .copy_from_slice(&frame.buffer[source..source + 4]);
                }
            }
        }
        let image = Arc::new(
            ImageData::new(width, height, canvas.clone()).map_err(|e| GpuError(e.into()))?,
        );
        let delay = if frame.delay == 0 {
            100
        } else {
            u64::from(frame.delay) * 10
        };
        frames.push(Frame {
            image,
            duration: std::time::Duration::from_millis(delay),
        });
        match frame.dispose {
            gif::DisposalMethod::Background => {
                for y in 0..fh {
                    let start = 4 * ((top + y) * width as usize + left);
                    canvas[start..start + 4 * fw].fill(0);
                }
            }
            gif::DisposalMethod::Previous => canvas = previous.unwrap(),
            _ => {}
        }
    }
    let loops = match decoder.repeat() {
        gif::Repeat::Infinite => LoopCount::Infinite,
        gif::Repeat::Finite(repeats) => {
            LoopCount::Finite(std::num::NonZeroU32::new(u32::from(repeats) + 1).unwrap())
        }
    };
    Animation::new(frames, loops)
        .map(Arc::new)
        .map_err(|e| GpuError(e.into()))
}
/// Without the `gif` feature: an error naming it.
#[cfg(not(feature = "gif"))]
pub fn decode_gif(_: &[u8]) -> Result<Arc<zgx::animation::Animation>, GpuError> {
    Err(GpuError("GIF images need zgx-gpu's `gif` feature".into()))
}

#[cfg(test)]
mod tests {
    /// An SVG in a box of other proportions keeps its own, centred: a 2:1
    /// mark in a square box fills its width and half its height.
    #[cfg(feature = "svg")]
    #[test]
    fn an_svg_keeps_its_proportions_in_a_box_of_others() {
        let wide = br#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12" viewBox="0 0 2 1"><rect width="2" height="1"/></svg>"#;
        let image = super::decode_svg(wide, 8, 8).unwrap();
        let alpha = |x: usize, y: usize| image.pixels()[(y * 8 + x) * 4 + 3];
        for x in 0..8 {
            assert_eq!(
                [alpha(x, 1), alpha(x, 2), alpha(x, 5), alpha(x, 6)],
                [0, 255, 255, 0]
            );
        }
        let tall = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 2"><rect width="1" height="2"/></svg>"#;
        let image = super::decode_svg(tall, 8, 8).unwrap();
        let alpha = |x: usize, y: usize| image.pixels()[(y * 8 + x) * 4 + 3];
        for y in 0..8 {
            assert_eq!(
                [alpha(1, y), alpha(2, y), alpha(5, y), alpha(6, y)],
                [0, 255, 255, 0]
            );
        }
    }
    /// A format left out fails to decode, with an error naming the feature
    /// that brings it back.
    #[cfg(not(all(feature = "svg", feature = "png", feature = "gif")))]
    #[test]
    fn a_format_left_out_names_its_feature() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>"#;
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        for (left_out, error) in [
            (!cfg!(feature = "svg"), super::decode_svg(svg, 4, 4).err()),
            (!cfg!(feature = "png"), super::decode_image(&png).err()),
            (!cfg!(feature = "gif"), super::decode_gif(b"GIF89a").err()),
        ] {
            if left_out {
                let error = error.expect("an error").0;
                assert!(error.contains("feature"), "{error}");
            }
        }
    }
}
