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
/// How an image is drawn from its pixels (`ImageData::dithered`,
/// `ImageData::characters`), worked out per pixel on the GPU as it draws.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ImageEffect {
    /// Its pixels.
    #[default]
    Plain,
    /// Zeron's two-level dither, in dots this many points square.
    Dither(f32),
    /// Characters in a 5 by 7 pixel font, each pixel this many points
    /// square, over the image (see `ImageData::characters`).
    Characters(f32),
}
impl ImageEffect {
    /// Its size in points (a dot's, a font pixel's); 0 when plain.
    pub fn size(self) -> f32 {
        match self {
            Self::Plain => 0.,
            Self::Dither(size) | Self::Characters(size) => size,
        }
    }
}
/// Decoded straight-alpha RGBA8 pixels shared between scene nodes, with a paint
/// transform and effect. Equal when they share pixels, transform and effect.
#[derive(Debug)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    transform: crate::affine::Affine,
    effect: ImageEffect,
    /// A GPU view's frame: no pixels of its own (see `gpu_frame`).
    gpu: Option<crate::gpu_view::GpuFrame>,
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
            effect: ImageEffect::Plain,
            gpu: None,
        })
    }
    /// A frame of a GPU view, its pixels left to the renderer, which asks the
    /// view to draw them (`crate::gpu_view`).
    pub(crate) fn gpu(frame: crate::gpu_view::GpuFrame) -> Result<Self, &'static str> {
        Ok(Self {
            id: next_identity()?,
            width: frame.width.ceil() as u32,
            height: frame.height.ceil() as u32,
            pixels: Arc::from(&[][..]),
            transform: crate::affine::Affine::IDENTITY,
            effect: ImageEffect::Plain,
            gpu: Some(frame),
        })
    }
    /// Make this a GPU view's next frame in place: a new identity, so a
    /// renderer draws it afresh, and the frame's size. Only for a frame no
    /// scene holds (the view's spare one).
    pub(crate) fn restamp(&mut self, frame: crate::gpu_view::GpuFrame) -> Result<(), &'static str> {
        self.id = next_identity()?;
        self.width = frame.width.ceil() as u32;
        self.height = frame.height.ceil() as u32;
        self.gpu = Some(frame);
        Ok(())
    }
    /// The GPU view frame this is, if it is one: then it has no pixels
    /// (`pixels` is empty), and a renderer draws it from the view's texture.
    pub fn gpu_frame(&self) -> Option<&crate::gpu_view::GpuFrame> {
        self.gpu.as_ref()
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
    /// Its RGBA8 bytes, four per pixel; none for a GPU view's frame
    /// (`gpu_frame`).
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
            effect: self.effect,
            gpu: self.gpu,
        }
    }
    /// Share decoded pixels and GPU upload identity, drawn dithered as Zeron
    /// dithers its wallpapers: in dots `dot` points square, on the screen's
    /// grid, each the colour at its centre lit to that colour's full value
    /// where its brightest channel passes a 4x4 Bayer threshold, and nearly
    /// black (8%) where it doesn't. A vivid two-level dot picture keeping the
    /// image's hues. Worked out per pixel as the image is drawn (one sample,
    /// a compare): it follows the image as it moves and scales, with nothing
    /// precomputed. 0 (or less, or not finite) draws it plainly.
    pub fn dithered(&self, dot: f32) -> Self {
        self.with_effect(match dot {
            dot if dot.is_finite() && dot > 0. => ImageEffect::Dither(dot),
            _ => ImageEffect::Plain,
        })
    }
    /// Share decoded pixels and GPU upload identity, drawn as characters, as
    /// an ASCII art generator draws a photo: a grid of cells, each a glyph of
    /// a 5 by 7 pixel font (its pixels `pixel` points square, a cell 6 by 8
    /// of them) picked by the brightness at the cell's centre (` .-+xXS80#@`)
    /// and lit in that colour at full value, over the image a little dimmed.
    ///
    /// Its node's shader parameters (`Scene::set_shader_uniforms`, animated as
    /// `shaderParam0` to `shaderParam5`) reveal it from a point of the window
    /// out to a rounded box, cell by cell: `params[0]` is the front (the box's
    /// size, 0 to 1 of the window's half-extents), the band of characters
    /// ahead of the image (0: no reveal, all of it shown) and how ragged the
    /// front is; `params[1]` the centre's offset from the window's middle, in
    /// fractions of it. Ahead of the front, nothing; in its band, characters
    /// scattered alone, sparse and faint at its outer edge and denser inward,
    /// scrambling as the front moves; behind it, the image, fading in pixel
    /// by pixel (no cells popping) under its glyphs. Measured in the window,
    /// so every layer drawn so shares one front. 0 (or less, or not finite)
    /// draws it plainly.
    pub fn characters(&self, pixel: f32) -> Self {
        self.with_effect(match pixel {
            pixel if pixel.is_finite() && pixel > 0. => ImageEffect::Characters(pixel),
            _ => ImageEffect::Plain,
        })
    }
    fn with_effect(&self, effect: ImageEffect) -> Self {
        Self {
            effect,
            ..self.transformed(self.transform)
        }
    }
    /// How it is drawn (`dithered`, `characters`).
    pub fn effect(&self) -> ImageEffect {
        self.effect
    }
    /// Its dither's dot size in points (`dithered`); 0 when undithered.
    pub fn dither(&self) -> f32 {
        match self.effect {
            ImageEffect::Dither(dot) => dot,
            _ => 0.,
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
        self.id == other.id && self.transform == other.transform && self.effect == other.effect
    }
}
impl Eq for ImageData {}
