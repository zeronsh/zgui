//! Immutable decoded straight-alpha RGBA8 images shared between scene nodes.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) fn next_identity() -> Result<u64, &'static str> {
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| "image identity exhausted")
}
/// Decoded straight-alpha RGBA8 pixels shared between scene nodes, with a paint
/// transform. Equal when they share pixels and transform.
#[derive(Debug)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    transform: crate::affine::Affine,
}
impl ImageData {
    /// A `width` by `height` image from RGBA8 `pixels`; fails unless both are
    /// nonzero and `pixels` holds exactly `width * height * 4` bytes.
    pub fn new(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, &'static str> {
        let pixels = pixels.into();
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("image size overflow")?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err("image requires nonzero dimensions and width * height * 4 RGBA bytes");
        }
        let id = next_identity()?;
        Ok(Self {
            id,
            width,
            height,
            pixels,
            transform: crate::affine::Affine::IDENTITY,
        })
    }
    /// Its pixels' identity, shared by `transformed` copies; renderers key GPU
    /// uploads by it.
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
    /// Its RGBA8 bytes, four per pixel.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Share decoded pixels and GPU upload identity with a different paint
    /// transformation. The matrix acts around the allocated image's center;
    /// it does not change intrinsic size or surrounding layout. Non-finite
    /// matrices reset to identity. Singular matrices paint and hit-test empty.
    pub fn transformed(&self, transform: crate::affine::Affine) -> Self {
        Self {
            id: self.id,
            width: self.width,
            height: self.height,
            pixels: self.pixels.clone(),
            transform: if transform.is_finite() {
                transform
            } else {
                crate::affine::Affine::IDENTITY
            },
        }
    }
    /// Its paint transform (the identity unless `transformed`).
    pub fn transform(&self) -> crate::affine::Affine {
        self.transform
    }
    /// Its transform about the centre of `bounds`, the box it is laid out in.
    pub fn paint_transform(&self, bounds: crate::scene::Rect) -> crate::affine::Affine {
        if self.transform == crate::affine::Affine::IDENTITY {
            return self.transform;
        }
        self.transform.around(
            bounds.x + bounds.width * 0.5,
            bounds.y + bounds.height * 0.5,
        )
    }
    /// Where it paints when laid out in `bounds`: the box covering `bounds` under
    /// its transform; empty when the transform is singular.
    pub fn paint_bounds(&self, bounds: crate::scene::Rect) -> crate::scene::Rect {
        if self.transform == crate::affine::Affine::IDENTITY {
            return bounds;
        }
        if self.transform.inverse().is_none() {
            return crate::scene::Rect::default();
        }
        self.paint_transform(bounds).bounds(bounds)
    }
}
impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.transform == other.transform
    }
}
impl Eq for ImageData {}
