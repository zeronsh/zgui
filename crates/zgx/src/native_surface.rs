//! Retained immutable CoreVideo frames. Rendering requires the native Metal backend.
use objc2_core_foundation::CFRetained;
pub use objc2_core_video::CVPixelBuffer;
use objc2_core_video::{
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidth, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
/// A [`NativeSurface`]'s pixel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceFormat {
    /// 32-bit BGRA with premultiplied alpha.
    Bgra,
    /// Bi-planar 4:2:0 YCbCr (NV12), full range, BT.601.
    Nv12FullRange,
}
/// An immutable frame; retaining it prevents pixel-buffer pools reclaiming its storage.
/// This UI-thread resource intentionally uses `Rc` in scenes rather than promising
/// cross-thread access to arbitrary external CoreVideo attachments.
#[derive(Debug)]
pub struct NativeSurface {
    id: u64,
    buffer: CFRetained<CVPixelBuffer>,
    width: u32,
    height: u32,
    format: SurfaceFormat,
}
impl PartialEq for NativeSurface {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl NativeSurface {
    /// Adopt a retained, initialized BGRA or full-range bi-planar NV12 frame.
    ///
    /// # Safety
    /// The buffer's pixels and attachments must remain immutable until every
    /// reference held by the scene and renderer is released. The producing GPU
    /// or decoder must have completed writes before this method returns. The
    /// buffer must support Metal texture creation (normally IOSurface-backed).
    /// BGRA pixels must use premultiplied alpha. NV12 uses full-range BT.601
    /// conversion, matching GPUI; other color matrices and HDR are unsupported.
    pub unsafe fn from_pixel_buffer(
        buffer: CFRetained<CVPixelBuffer>,
    ) -> Result<Self, &'static str> {
        let width =
            u32::try_from(CVPixelBufferGetWidth(&buffer)).map_err(|_| "surface width overflow")?;
        let height = u32::try_from(CVPixelBufferGetHeight(&buffer))
            .map_err(|_| "surface height overflow")?;
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err("surface dimensions must be 1..=8192");
        }
        let pixel_format = CVPixelBufferGetPixelFormatType(&buffer);
        let format = if pixel_format == kCVPixelFormatType_32BGRA {
            SurfaceFormat::Bgra
        } else if pixel_format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
            && CVPixelBufferGetPlaneCount(&buffer) == 2
        {
            SurfaceFormat::Nv12FullRange
        } else {
            return Err("surface requires BGRA or full-range NV12 (420f)");
        };
        Ok(Self {
            id: crate::image::next_identity()?,
            buffer,
            width,
            height,
            format,
        })
    }
    /// Its identity, unique in the process among images, SVGs and surfaces.
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Its width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// Its height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Its pixel format.
    pub fn format(&self) -> SurfaceFormat {
        self.format
    }
    /// The CoreVideo pixel buffer it holds.
    pub fn pixel_buffer(&self) -> &CVPixelBuffer {
        &self.buffer
    }
}
