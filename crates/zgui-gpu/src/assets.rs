//! Bounded image decoding and SVG rasterization, independent of a GPU device.
use crate::GpuError;
use std::{io::Cursor, sync::Arc};
use zgui::image::ImageData;
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Decode PNG/JPEG into immutable, straight-alpha RGBA8. No filesystem/network access.
pub fn decode_image(bytes: &[u8]) -> Result<Arc<ImageData>, GpuError> {
    if bytes.len() > MAX_BYTES {
        return Err(GpuError("encoded image exceeds 64 MiB".into()));
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| GpuError(e.to_string()))?;
    match reader.format() {
        Some(image::ImageFormat::Png) if !cfg!(feature = "png") => {
            return Err(GpuError("PNG images need zgui-gpu's `png` feature".into()));
        }
        Some(image::ImageFormat::Jpeg) if !cfg!(feature = "jpeg") => {
            return Err(GpuError(
                "JPEG images need zgui-gpu's `jpeg` feature".into(),
            ));
        }
        _ => {}
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_BYTES as u64);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| GpuError(e.to_string()))?
        .into_rgba8();
    if decoded.as_raw().len() > MAX_BYTES {
        return Err(GpuError("decoded image exceeds 64 MiB".into()));
    }
    ImageData::new(decoded.width(), decoded.height(), decoded.into_raw())
        .map(Arc::new)
        .map_err(|e| GpuError(e.into()))
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
    // as that picture, scaled to fit.
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
            tiny_skia::Transform::from_scale(
                width as f32 / image.width() as f32,
                height as f32 / image.height() as f32,
            ),
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

/// An SVG document drawn into `pixmap`, scaled to fill it.
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
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / tree.size().width(),
        height as f32 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(())
}
#[cfg(not(feature = "svg"))]
fn render_svg(_: &[u8], _: u32, _: u32, _: &mut tiny_skia::Pixmap) -> Result<(), GpuError> {
    Err(GpuError("SVG images need zgui-gpu's `svg` feature".into()))
}

/// Decode GIF frames once, compositing offsets and Keep/Background/Previous
/// disposal into full straight-alpha frames. Background disposal is transparent,
/// matching GUI image compositing rather than filling the window with GIF palette
/// background. Absent looping extension plays once; finite repetitions add to the
/// initial play. Zero-delay GIF frames use 100 ms; other delays floor at 10 ms.
#[cfg(feature = "gif")]
pub fn decode_gif(bytes: &[u8]) -> Result<Arc<zgui::animation::Animation>, GpuError> {
    use zgui::animation::{Animation, Frame, LoopCount};
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
pub fn decode_gif(_: &[u8]) -> Result<Arc<zgui::animation::Animation>, GpuError> {
    Err(GpuError("GIF images need zgui-gpu's `gif` feature".into()))
}

#[cfg(test)]
mod tests {
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
