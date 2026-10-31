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
/// An image's pixels, behind one `Arc` as they were (an `ImageData` drops
/// as it did: one count, the rest out of line): decoded, or its file kept
/// and decoded as it is drawn (`ImageData::kept`).
#[derive(Debug)]
struct Pixels {
    decoded: Arc<[u8]>,
    /// Dropped by `drop_kept`, which can't unwind: dropped inline, its glue
    /// (out of line) gave every drop of a node's kind a cleanup path, +0.1%
    /// on scenes that show no image.
    kept: std::mem::ManuallyDrop<Option<Kept>>,
}
impl Drop for Pixels {
    fn drop(&mut self) {
        drop_kept(&mut self.kept);
    }
}
extern "C" fn drop_kept(kept: &mut std::mem::ManuallyDrop<Option<Kept>>) {
    // SAFETY: once, from `Pixels::drop`; nothing reads it after.
    unsafe { std::mem::ManuallyDrop::drop(kept) }
}
/// When a kept file decodes as it is first drawn (HTML's `decoding`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Decoding {
    /// Up to `SYNC_PIXELS` (256 by 256), as it is drawn, in that frame: an
    /// icon or an avatar shows at once (a 256 by 256 PNG decodes in about
    /// half a millisecond). Larger, off the main thread, a frame later.
    #[default]
    Auto,
    /// As it is drawn, whatever its size: no frame goes without it.
    Sync,
    /// Off the main thread, whatever its size.
    Async,
}
/// How an image's pixels are sampled where it is drawn larger or smaller
/// than they are (CSS's `image-rendering`; `Scene::set_image_rendering`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageRendering {
    /// Smooth: each point blends the pixels around it.
    #[default]
    Auto,
    /// Each point takes the nearest pixel, its edges hard: pixel art, a
    /// magnified screenshot, a QR code.
    Pixelated,
}
/// The most pixels `Decoding::Auto` decodes on the thread drawing.
pub const SYNC_PIXELS: u64 = 256 * 256;
/// How a kept file decodes: its straight-alpha RGBA8 pixels, at its size.
pub type Decode = fn(&[u8]) -> Result<Arc<[u8]>, String>;
/// How a kept file decodes smaller (`ImageData::kept_scaled`): its
/// straight-alpha RGBA8 pixels at least `size`, at a size of its own (a
/// JPEG's 1/2, 1/4 or 1/8, decoded no larger), and that size.
pub type DecodeScaled = fn(&[u8], (u32, u32)) -> Result<(u32, u32, Arc<[u8]>), String>;
/// A kept file and what of it is decoded now.
struct Kept {
    encoded: Arc<[u8]>,
    decode: Decode,
    /// Decoded at least the size drawn, not whole (`kept_scaled`).
    scaled: Option<DecodeScaled>,
    /// Its pixels at a size it was asked for, premultiplied, until released.
    held: std::sync::Mutex<Option<Held>>,
    /// The size a decode under way makes, if one is.
    pending: std::sync::Mutex<Option<(u32, u32)>>,
    /// When it decodes (`Decoding`, as a `u8`).
    decoding: std::sync::atomic::AtomicU8,
    /// Its file didn't decode: not tried again (each try would draw again).
    failed: std::sync::atomic::AtomicBool,
    /// All of it, straight, once `ImageData::pixels` borrowed it: kept for
    /// as long as the image (that borrow is for its life).
    pinned: std::sync::OnceLock<Arc<[u8]>>,
}
impl std::fmt::Debug for Kept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kept")
            .field("encoded", &self.encoded.len())
            .finish_non_exhaustive()
    }
}
/// Pixels decoded at a size, premultiplied (`ImageData::pixels_at`).
#[derive(Clone, Debug)]
pub struct Held {
    /// Its size, at most the image's.
    pub width: u32,
    /// Its size, at most the image's.
    pub height: u32,
    /// Premultiplied RGBA8.
    pub pixels: Arc<[u8]>,
}
/// Decoded straight-alpha RGBA8 pixels shared between scene nodes, with a paint
/// transform and effect. Equal when they share pixels, transform and effect.
#[derive(Debug)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    pixels: Arc<Pixels>,
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
            pixels: Arc::new(Pixels {
                decoded: pixels,
                kept: std::mem::ManuallyDrop::new(None),
            }),
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
            pixels: Arc::new(Pixels {
                decoded: Arc::from(&[][..]),
                kept: std::mem::ManuallyDrop::new(None),
            }),
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
        match &*self.pixels.kept {
            // Borrowed for its life: decoded whole once, and kept.
            Some(kept) => kept.pinned.get_or_init(|| {
                (kept.decode)(&kept.encoded).unwrap_or_else(|_| {
                    vec![0; self.width as usize * self.height as usize * 4].into()
                })
            }),
            None => &self.pixels.decoded,
        }
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
impl ImageData {
    /// A `width` by `height` image from an encoded file (a PNG, a JPEG) that
    /// `decode` makes pixels of when they are drawn, at the size they are
    /// drawn (`pixels_at`), off the thread asking; they go again once a
    /// renderer has them (`release_decoded`). Nothing is decoded here.
    pub fn kept(
        width: u32,
        height: u32,
        encoded: Arc<[u8]>,
        decode: Decode,
    ) -> Result<Self, &'static str> {
        Self::kept_with(width, height, encoded, decode, None)
    }
    /// As `kept`, its pixels decoded by `scaled` at least as large as they
    /// are drawn rather than whole (a JPEG at 1/2, 1/4 or 1/8 of its size),
    /// so a photo shown small never decodes at its full size. `decode` is
    /// what `pixels` borrows, whole.
    pub fn kept_scaled(
        width: u32,
        height: u32,
        encoded: Arc<[u8]>,
        decode: Decode,
        scaled: DecodeScaled,
    ) -> Result<Self, &'static str> {
        Self::kept_with(width, height, encoded, decode, Some(scaled))
    }
    fn kept_with(
        width: u32,
        height: u32,
        encoded: Arc<[u8]>,
        decode: Decode,
        scaled: Option<DecodeScaled>,
    ) -> Result<Self, &'static str> {
        if width == 0 || height == 0 {
            return Err("image requires nonzero dimensions");
        }
        let pixels = Arc::new(Pixels {
            decoded: Arc::from(&[][..]),
            kept: std::mem::ManuallyDrop::new(Some(Kept {
                encoded,
                decode,
                scaled,
                held: std::sync::Mutex::new(None),
                pending: std::sync::Mutex::new(None),
                decoding: std::sync::atomic::AtomicU8::new(Decoding::Auto as u8),
                failed: std::sync::atomic::AtomicBool::new(false),
                pinned: std::sync::OnceLock::new(),
            })),
        });
        registry().push(Arc::downgrade(&pixels));
        Ok(Self {
            id: next_identity()?,
            width,
            height,
            pixels,
            transform: crate::affine::Affine::IDENTITY,
            effect: ImageEffect::Plain,
            gpu: None,
        })
    }
    /// The bytes it holds for good, without decoding: its pixels, or a kept
    /// file's bytes (what it decodes is held a while, `decoded_bytes`). The
    /// same whenever asked, for budgets that add and take it away.
    pub fn bytes(&self) -> usize {
        match &*self.pixels.kept {
            Some(kept) => kept.encoded.len(),
            None => self.pixels.decoded.len(),
        }
    }
    /// Its pixels, straight, shared (`pixels`, by count).
    pub fn pixels_arc(&self) -> Arc<[u8]> {
        match &*self.pixels.kept {
            Some(_) => {
                self.pixels();
                let kept = self.pixels.kept.as_ref().expect("kept above");
                kept.pinned.get().expect("pinned by `pixels`").clone()
            }
            None => self.pixels.decoded.clone(),
        }
    }
    /// When its kept file decodes as first drawn (`Decoding`); nothing for an
    /// image that keeps no file.
    pub fn set_decoding(&self, decoding: Decoding) {
        if let Some(kept) = &*self.pixels.kept {
            kept.decoding.store(decoding as u8, Ordering::Relaxed);
        }
    }
    /// When its kept file decodes (`set_decoding`): `Auto` for an image that
    /// keeps no file.
    pub fn decoding(&self) -> Decoding {
        match self
            .pixels
            .kept
            .as_ref()
            .map(|kept| kept.decoding.load(Ordering::Relaxed))
        {
            Some(d) if d == Decoding::Sync as u8 => Decoding::Sync,
            Some(d) if d == Decoding::Async as u8 => Decoding::Async,
            _ => Decoding::Auto,
        }
    }
    /// Whether it keeps its file (`kept`), its pixels decoded as drawn.
    pub fn keeps_file(&self) -> bool {
        self.pixels.kept.is_some()
    }
    /// Its pixels at least `size` large (a renderer's drawn size), shrunk
    /// and premultiplied, if they are decoded; else `None`, and they are
    /// decoded off this thread, the wake (`set_decoded_wake`) told once
    /// they are. An image that doesn't keep its file has all of its own
    /// (`pixels`, straight): `None` too, as it needs no wait.
    pub fn pixels_at(&self, size: (u32, u32)) -> Option<Held> {
        let kept = self.pixels.kept.as_ref()?;
        let size = (size.0.clamp(1, self.width), size.1.clamp(1, self.height));
        let held = kept.held.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(held) = &held
            && held.width >= size.0
            && held.height >= size.1
        {
            return Some(held.clone());
        }
        if kept.failed.load(Ordering::Relaxed) {
            return None;
        }
        // Small (or asked to): decoded here, in this frame.
        let small = u64::from(self.width) * u64::from(self.height) <= SYNC_PIXELS;
        let now = match kept.decoding.load(Ordering::Relaxed) {
            d if d == Decoding::Sync as u8 => true,
            d if d == Decoding::Async as u8 => false,
            _ => small,
        };
        if now {
            return decode_at(&self.pixels, (self.width, self.height), size, false);
        }
        let mut pending = kept.pending.lock().unwrap_or_else(|p| p.into_inner());
        if pending.is_none_or(|(w, h)| w < size.0 || h < size.1) {
            *pending = Some(size);
            send((Arc::clone(&self.pixels), (self.width, self.height), size));
        }
        None
    }
}
/// Gives back what kept files' decoded pixels hold (pinned ones aside):
/// a renderer has them as textures, and they decode again if drawn
/// larger. The bytes given back.
pub fn release_decoded() -> usize {
    let mut freed = 0;
    let mut list = registry();
    list.retain(|weak| match weak.upgrade() {
        Some(pixels) => {
            if let Some(kept) = &*pixels.kept {
                let mut held = kept.held.lock().unwrap_or_else(|p| p.into_inner());
                freed += held.take().map_or(0, |held| held.pixels.len());
            }
            true
        }
        None => false,
    });
    freed
}
/// What kept files' decoded pixels hold now (pinned ones too).
pub fn decoded_bytes() -> usize {
    registry()
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .filter_map(|pixels| {
            let kept = pixels.kept.as_ref()?;
            let held = kept.held.lock().unwrap_or_else(|p| p.into_inner());
            let pinned = kept.pinned.get().map_or(0, |p| p.len());
            Some(held.as_ref().map_or(0, |h| h.pixels.len()) + pinned)
        })
        .sum()
}
/// Who to tell that pixels a renderer waited for are decoded: a host draws
/// again.
pub fn set_decoded_wake(wake: impl Fn() + Send + Sync + 'static) {
    *WAKE.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(wake));
}
type Wake = Arc<dyn Fn() + Send + Sync>;
static WAKE: std::sync::Mutex<Option<Wake>> = std::sync::Mutex::new(None);
static REGISTRY: std::sync::Mutex<Vec<std::sync::Weak<Pixels>>> = std::sync::Mutex::new(Vec::new());
fn registry() -> std::sync::MutexGuard<'static, Vec<std::sync::Weak<Pixels>>> {
    REGISTRY.lock().unwrap_or_else(|p| p.into_inner())
}
/// A decode to do: whose, its full size, the size wanted.
type Job = (Arc<Pixels>, (u32, u32), (u32, u32));
/// Queue a decode for the one thread decoding kept files, a file at a time:
/// a page of photos holds one whole decode at once, not one each.
fn send(job: Job) {
    static WORKER: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<Job>>> =
        std::sync::OnceLock::new();
    let sender = WORKER.get_or_init(|| {
        let (send, receive) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("zgx image decode".into())
            .spawn(move || {
                for (pixels, full, size) in receive {
                    decode_at(&pixels, full, size, true);
                }
            })
            .expect("a decoding thread");
        std::sync::Mutex::new(send)
    });
    let _ = sender.lock().unwrap_or_else(|p| p.into_inner()).send(job);
}
/// Decode `pixels`' file whole, then premultiplied and shrunk to `size`,
/// the whole decode dropped: held at that size.
fn decode_at(
    pixels: &Pixels,
    (width, height): (u32, u32),
    size: (u32, u32),
    wake: bool,
) -> Option<Held> {
    let kept = pixels.kept.as_ref()?;
    // Decoded no larger than it need be, where it can be (`kept_scaled`).
    let decoded = match kept.scaled {
        Some(scaled) => scaled(&kept.encoded, size),
        None => (kept.decode)(&kept.encoded).map(|straight| (width, height, straight)),
    };
    let done = decoded.ok().map(|(mut w, mut h, mut straight)| {
        // Premultiplied in the decoder's own buffer: one image held, not two.
        match Arc::get_mut(&mut straight) {
            Some(own) => premultiply(own),
            None => {
                let mut copy = straight.to_vec();
                premultiply(&mut copy);
                straight = copy.into();
            }
        }
        // Halved, then shrunk to the size drawn; the whole decode let go
        // once the first half is made.
        let mut rgba: Option<Vec<u8>> = None;
        while w.div_ceil(2) >= size.0 && h.div_ceil(2) >= size.1 && (w, h) != (1, 1) {
            let half;
            (half, w, h) = resample::halve(rgba.as_deref().unwrap_or(&straight), w, h);
            rgba = Some(half);
            straight = Arc::from(&[][..]);
        }
        if (w, h) != size {
            rgba = Some(resample::shrink(
                rgba.as_deref().unwrap_or(&straight),
                w,
                h,
                size,
            ));
        }
        Held {
            width: size.0,
            height: size.1,
            pixels: rgba.map_or(straight, Arc::from),
        }
    });
    *kept.pending.lock().unwrap_or_else(|p| p.into_inner()) = None;
    let Some(done) = done else {
        kept.failed.store(true, Ordering::Relaxed);
        return None;
    };
    *kept.held.lock().unwrap_or_else(|p| p.into_inner()) = Some(done.clone());
    // Off the main thread: whoever waited draws again.
    let wake = wake
        .then(|| WAKE.lock().unwrap_or_else(|p| p.into_inner()).clone())
        .flatten();
    if let Some(wake) = wake {
        wake();
    }
    Some(done)
}
fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}
impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.transform == other.transform && self.effect == other.effect
    }
}
impl Eq for ImageData {}

/// Shrinking premultiplied RGBA8 as mip levels and an area filter do: what
/// renderers and kept files' decodes share. Renderer plumbing.
#[doc(hidden)]
pub mod resample {
    /// Premultiplied RGBA `width` by `height` at half the size (rounding up),
    /// each texel the mean of the two by two it covers (an odd edge repeats).
    pub fn halve(pixels: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
        let (w, h) = (width.div_ceil(2), height.div_ceil(2));
        let mut out = vec![0u8; w as usize * h as usize * 4];
        let at = |x: u32, y: u32| ((y.min(height - 1) * width + x.min(width - 1)) * 4) as usize;
        for y in 0..h {
            for x in 0..w {
                let (a, b) = (at(2 * x, 2 * y), at(2 * x + 1, 2 * y));
                let (c, d) = (at(2 * x, 2 * y + 1), at(2 * x + 1, 2 * y + 1));
                let o = ((y * w + x) * 4) as usize;
                for k in 0..4 {
                    let sum = u32::from(pixels[a + k])
                        + u32::from(pixels[b + k])
                        + u32::from(pixels[c + k])
                        + u32::from(pixels[d + k]);
                    out[o + k] = ((sum + 2) / 4) as u8;
                }
            }
        }
        (out, w, h)
    }
    /// Premultiplied RGBA `width` by `height` at `to` (no larger), each texel
    /// the area-weighted mean of what it covers: from a halving at most twice
    /// its size, as sharp as a mip level, at any size.
    pub fn shrink(pixels: &[u8], width: u32, height: u32, to: (u32, u32)) -> Vec<u8> {
        let (tw, th) = to;
        // Each target texel's span along an axis: (first source texel, weights).
        let spans = |from: u32, to: u32| -> Vec<(u32, Vec<f32>)> {
            let step = from as f32 / to as f32;
            (0..to)
                .map(|t| {
                    let (start, end) = (t as f32 * step, (t + 1) as f32 * step);
                    let first = start.floor() as u32;
                    let last = (end.ceil() as u32).min(from);
                    let weights = (first..last)
                        .map(|s| ((s + 1) as f32).min(end) - (s as f32).max(start))
                        .collect();
                    (first, weights)
                })
                .collect()
        };
        let (xs, ys) = (spans(width, tw), spans(height, th));
        let mut out = vec![0u8; tw as usize * th as usize * 4];
        for (ty, (y0, wy)) in ys.iter().enumerate() {
            for (tx, (x0, wx)) in xs.iter().enumerate() {
                let mut sum = [0f32; 4];
                let mut total = 0.;
                for (dy, ky) in wy.iter().enumerate() {
                    let row = (y0 + dy as u32) * width;
                    for (dx, kx) in wx.iter().enumerate() {
                        let at = ((row + x0 + dx as u32) * 4) as usize;
                        let k = kx * ky;
                        for c in 0..4 {
                            sum[c] += pixels[at + c] as f32 * k;
                        }
                        total += k;
                    }
                }
                let o = (ty * tw as usize + tx) * 4;
                for c in 0..4 {
                    out[o + c] = (sum[c] / total).round() as u8;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod kept_tests {
    use super::*;

    /// An 8 by 4 file whose pixels are all opaque red.
    fn decode(bytes: &[u8]) -> Result<Arc<[u8]>, String> {
        assert_eq!(bytes, b"file");
        Ok([255, 0, 0, 255].repeat(8 * 4).into())
    }

    fn wait_for(image: &ImageData, size: (u32, u32)) -> Held {
        for _ in 0..500 {
            if let Some(held) = image.pixels_at(size) {
                return held;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("never decoded");
    }

    /// A 300 by 300 file (larger than `SYNC_PIXELS`), all opaque blue.
    fn decode_large(bytes: &[u8]) -> Result<Arc<[u8]>, String> {
        assert_eq!(bytes, b"large");
        Ok([0, 0, 255, 255].repeat(300 * 300).into())
    }

    fn broken(_: &[u8]) -> Result<Arc<[u8]>, String> {
        Err("truncated".into())
    }

    /// A kept file decodes nothing until drawn; then (asked to decode off
    /// the main thread) off this thread, at the size asked, premultiplied;
    /// it is given back once released, and what `pixels` borrows is decoded
    /// whole and kept.
    #[test]
    fn a_kept_file_decodes_as_drawn_and_gives_its_pixels_back() {
        let image = ImageData::kept(8, 4, Arc::from(&b"file"[..]), decode).unwrap();
        image.set_decoding(Decoding::Async);
        assert!(image.keeps_file());
        assert_eq!(image.bytes(), 4, "its file, not its pixels");
        assert!(image.pixels_at((4, 2)).is_none(), "decoding");
        let held = wait_for(&image, (4, 2));
        assert_eq!((held.width, held.height), (4, 2));
        assert_eq!(&held.pixels[..4], &[255, 0, 0, 255]);
        // Smaller: what it holds serves.
        assert_eq!(image.pixels_at((2, 1)).unwrap().width, 4);
        assert!(decoded_bytes() >= 4 * 2 * 4);
        release_decoded();
        assert!(
            image.pixels_at((4, 2)).is_none(),
            "given back: decoded again"
        );
        wait_for(&image, (4, 2));
        // Borrowed: whole, straight, and kept.
        assert_eq!(image.pixels().len(), 8 * 4 * 4);
        assert_eq!(image.pixels_arc().len(), 8 * 4 * 4);
    }

    /// A small picture decodes in the frame that draws it (no pop-in); a
    /// large one off the main thread unless it is asked to wait.
    #[test]
    fn small_files_and_sync_ones_decode_in_the_frame_drawing_them() {
        let small = ImageData::kept(8, 4, Arc::from(&b"file"[..]), decode).unwrap();
        let held = small.pixels_at((8, 4)).expect("decoded at once");
        assert_eq!((held.width, held.height), (8, 4));
        let large = || ImageData::kept(300, 300, Arc::from(&b"large"[..]), decode_large).unwrap();
        assert!(large().pixels_at((300, 300)).is_none(), "off the thread");
        let sync = large();
        sync.set_decoding(Decoding::Sync);
        let held = sync.pixels_at((150, 150)).expect("waited for");
        assert_eq!((held.width, held.height), (150, 150));
        assert_eq!(&held.pixels[..4], &[0, 0, 255, 255]);
    }

    /// An 8 by 4 file decoded at a quarter (`kept_scaled`): 2 by 1, all
    /// opaque green, whatever was asked for, so its use shows.
    fn quarter(bytes: &[u8], _: (u32, u32)) -> Result<(u32, u32, Arc<[u8]>), String> {
        assert_eq!(bytes, b"file");
        Ok((2, 1, [0, 255, 0, 255].repeat(2).into()))
    }

    /// A file that decodes smaller is drawn from that, never decoded whole;
    /// what `pixels` borrows is still all of it.
    #[test]
    fn a_scaled_file_draws_from_its_smaller_decode() {
        let image = ImageData::kept_scaled(8, 4, Arc::from(&b"file"[..]), decode, quarter).unwrap();
        let held = image.pixels_at((2, 1)).expect("small: decoded at once");
        assert_eq!((held.width, held.height), (2, 1));
        assert_eq!(&held.pixels[..4], &[0, 255, 0, 255], "the scaled decode's");
        // Smaller still: shrunk from it.
        assert_eq!(
            image.pixels_at((1, 1)).unwrap().width,
            2,
            "what it holds serves"
        );
        assert_eq!(
            &image.pixels()[..4],
            &[255, 0, 0, 255],
            "whole, from `decode`"
        );
    }

    /// A file that won't decode is tried once, not at every draw.
    #[test]
    fn a_broken_file_is_tried_once() {
        let image = ImageData::kept(8, 4, Arc::from(&b"file"[..]), broken).unwrap();
        assert!(image.pixels_at((8, 4)).is_none());
        let kept = image.pixels.kept.as_ref().unwrap();
        assert!(kept.failed.load(Ordering::Relaxed));
        assert!(image.pixels_at((8, 4)).is_none());
        assert!(kept.pending.lock().unwrap().is_none(), "not queued again");
    }

    /// An image that doesn't keep its file has its pixels already.
    #[test]
    fn a_plain_image_needs_no_decode() {
        let image = ImageData::new(1, 1, vec![1, 2, 3, 4]).unwrap();
        assert!(!image.keeps_file());
        assert!(image.pixels_at((1, 1)).is_none());
        assert_eq!(image.pixels(), [1, 2, 3, 4]);
        assert_eq!(image.bytes(), 4);
    }
}
