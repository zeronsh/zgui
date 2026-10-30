//! Immutable decoded straight-alpha RGBA8 images shared between scene nodes.
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
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
    /// The file it was decoded from, kept so its pixels can go once a
    /// renderer has them and come back (`ImageData::kept`); `pixels` is
    /// then empty and the pixels are the file's.
    kept: Option<KeptFile>,
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
            kept: None,
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
            kept: None,
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
    /// (`gpu_frame`). An image keeping its file (`kept`) keeps its pixels
    /// from the first call on, decoding them again if they had gone: what
    /// this returns lives as long as the image. Renderers read
    /// `try_pixels` instead, which lets them go.
    pub fn pixels(&self) -> &[u8] {
        match &self.kept {
            Some(kept) => kept.pinned(),
            None => &self.pixels,
        }
    }
    /// Its pixels if it holds them now, shared: always for an image made
    /// from pixels; for one keeping its file (`kept`), unless they went
    /// (`uploaded`), in which case a decode starts off the main thread and
    /// the wake set with `set_decoded_wake` follows it.
    pub fn try_pixels(&self) -> Option<Arc<[u8]>> {
        match &self.kept {
            Some(kept) => kept.held_or_request(),
            None => Some(self.pixels.clone()),
        }
    }
    /// Its pixels, shared, decoded again now if they had gone; unlike
    /// `pixels`, they may go again.
    pub fn pixels_arc(&self) -> Arc<[u8]> {
        match &self.kept {
            Some(kept) => kept.held_or_decode(),
            None => self.pixels.clone(),
        }
    }
    /// Whether it keeps the file it was decoded from (`kept`).
    pub fn keeps_file(&self) -> bool {
        self.kept.is_some()
    }
    /// A renderer has its pixels (uploaded to a texture of their own) and
    /// draws it now: past the decoded-pixels budget (`set_decoded_budget`),
    /// images keeping their files that renderers have give their pixels
    /// back, least recently drawn first. Nothing for an image made from
    /// pixels.
    pub fn uploaded(&self) {
        if let Some(kept) = &self.kept {
            kept.uploaded();
        }
    }
    /// An image decoded from `encoded` (a PNG's or JPEG's bytes) into
    /// `pixels`, keeping the file: once a renderer has the pixels
    /// (`uploaded`), they may go, past the decoded-pixels budget, and
    /// `decode` brings them back. Codecs are the caller's
    /// (`zgx_gpu::assets::decode_image_kept`).
    pub fn kept(
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
        encoded: Arc<[u8]>,
        decode: Decode,
    ) -> Result<Self, &'static str> {
        let mut image = Self::new(width, height, pixels)?;
        let pixels = std::mem::replace(&mut image.pixels, Arc::from(&[][..]));
        image.kept = Some(KeptFile(std::mem::ManuallyDrop::new(Kept::new(
            pixels, encoded, decode,
        ))));
        Ok(image)
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
            kept: self.kept.clone(),
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

/// Decodes a kept file (`ImageData::kept`) to its RGBA8 pixels again.
pub type Decode = fn(&[u8]) -> Result<Arc<[u8]>, String>;

/// The decoded pixels images keeping their files may hold before those a
/// renderer has give theirs back (256 MiB unless set): below it, nothing
/// goes.
pub fn set_decoded_budget(bytes: usize) {
    ledger().budget = bytes;
}
/// What wakes the app when pixels decoded again off the main thread
/// (`ImageData::try_pixels`) are ready: its windows draw again.
pub fn set_decoded_wake(wake: impl Fn() + Send + Sync + 'static) {
    *WAKE.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(wake));
}
/// Gives back now the pixels of every image keeping its file, whatever the
/// budget: those renderers have and those none has drawn, for an app whose
/// images have settled. Pinned ones (`ImageData::pixels`) stay; one drawn
/// without a texture decodes again off the main thread. Answers the bytes
/// given back.
pub fn release_decoded() -> usize {
    let mut ledger = ledger();
    let before = decoded_bytes();
    give_back(&mut ledger, 0, None, true);
    before - decoded_bytes()
}
/// The decoded pixels images keeping their files hold now (for tests and
/// measurements).
pub fn decoded_bytes() -> usize {
    HELD.load(Ordering::Relaxed)
}

const BUDGET: usize = 256 * 1024 * 1024;

type Wake = Arc<dyn Fn() + Send + Sync>;
static WAKE: std::sync::Mutex<Option<Wake>> = std::sync::Mutex::new(None);

/// The decoded pixels images keeping their files hold. Not in the ledger:
/// dropping an image must take no lock, or nothing holding one (a style's
/// background, on every hot path) drops without paths for unwinding.
static HELD: AtomicUsize = AtomicUsize::new(0);

struct Ledger {
    budget: usize,
    /// Images keeping their files, which may give their pixels back.
    kept: Vec<std::sync::Weak<Kept>>,
    clock: u64,
}
static LEDGER: std::sync::Mutex<Ledger> = std::sync::Mutex::new(Ledger {
    budget: BUDGET,
    kept: Vec::new(),
    clock: 0,
});
fn ledger() -> std::sync::MutexGuard<'static, Ledger> {
    LEDGER.lock().unwrap_or_else(|p| p.into_inner())
}

/// An image's share of its `Kept`. Its drop can't unwind, and says so: every
/// style holding an image (a box's background) drops one, and an `Arc<Kept>`
/// dropped directly gave each of those drops paths for unwinding, on nodes
/// with no image at all (heavy +0.08%, animate +0.14%).
#[derive(Debug)]
struct KeptFile(std::mem::ManuallyDrop<Arc<Kept>>);
impl Clone for KeptFile {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl std::ops::Deref for KeptFile {
    type Target = Arc<Kept>;
    fn deref(&self) -> &Arc<Kept> {
        &self.0
    }
}
impl Drop for KeptFile {
    #[inline]
    fn drop(&mut self) {
        // `extern "C"`: callers know nothing unwinds out of it.
        extern "C" fn release(kept: *const Kept) {
            // SAFETY: from `Arc::into_raw` below, taken once.
            drop(unsafe { Arc::from_raw(kept) });
        }
        // SAFETY: not used again: this is its drop.
        let kept = unsafe { std::mem::ManuallyDrop::take(&mut self.0) };
        release(Arc::into_raw(kept));
    }
}

/// An image's kept file and the pixels decoded from it, while held.
#[derive(Debug)]
pub(crate) struct Kept {
    encoded: Arc<[u8]>,
    decode: Decode,
    len: usize,
    held: std::sync::Mutex<Option<Arc<[u8]>>>,
    /// Pixels `pixels` handed out a borrow of: held for good.
    pinned: std::sync::OnceLock<Arc<[u8]>>,
    /// When a renderer last had it (`uploaded`), by the ledger's clock.
    drawn: AtomicU64,
    /// A decode off the main thread is under way.
    decoding: std::sync::atomic::AtomicBool,
}
impl Kept {
    fn new(pixels: Arc<[u8]>, encoded: Arc<[u8]>, decode: Decode) -> Arc<Self> {
        let len = pixels.len();
        HELD.fetch_add(len, Ordering::Relaxed);
        let kept = Arc::new(Self {
            encoded,
            decode,
            len,
            held: std::sync::Mutex::new(Some(pixels)),
            pinned: std::sync::OnceLock::new(),
            drawn: AtomicU64::new(0),
            decoding: std::sync::atomic::AtomicBool::new(false),
        });
        let mut ledger = ledger();
        if ledger.kept.len() == ledger.kept.capacity() {
            ledger.kept.retain(|kept| kept.strong_count() > 0);
        }
        ledger.kept.push(Arc::downgrade(&kept));
        drop(ledger);
        kept
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Arc<[u8]>>> {
        self.held.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn pinned(&self) -> &[u8] {
        self.pinned.get_or_init(|| self.held_or_decode())
    }
    fn held_or_decode(&self) -> Arc<[u8]> {
        if let Some(pixels) = self.pinned.get() {
            return pixels.clone();
        }
        if let Some(pixels) = self.lock().clone() {
            return pixels;
        }
        let pixels = (self.decode)(&self.encoded).unwrap_or_else(|_| Arc::from(vec![0; self.len]));
        self.hold(pixels.clone());
        pixels
    }
    fn hold(&self, pixels: Arc<[u8]>) {
        let mut held = self.lock();
        if held.is_none() {
            *held = Some(pixels);
            HELD.fetch_add(self.len, Ordering::Relaxed);
        }
    }
    fn held_or_request(self: &Arc<Self>) -> Option<Arc<[u8]>> {
        if let Some(pixels) = self.pinned.get() {
            return Some(pixels.clone());
        }
        if let Some(pixels) = self.lock().clone() {
            return Some(pixels);
        }
        if !self.decoding.swap(true, Ordering::AcqRel) {
            let kept = Arc::downgrade(self);
            decode_later(Box::new(move || {
                let Some(kept) = kept.upgrade() else { return };
                if let Ok(pixels) = (kept.decode)(&kept.encoded) {
                    kept.hold(pixels);
                }
                kept.decoding.store(false, Ordering::Release);
                let wake = WAKE.lock().unwrap_or_else(|p| p.into_inner()).clone();
                if let Some(wake) = wake {
                    wake();
                }
            }));
        }
        None
    }
    fn uploaded(self: &Arc<Self>) {
        let mut ledger = ledger();
        ledger.clock += 1;
        let now = ledger.clock;
        self.drawn.store(now, Ordering::Relaxed);
        let budget = ledger.budget;
        // Past the budget: those renderers have give their pixels back,
        // least recently drawn first, not this one.
        give_back(&mut ledger, budget, Some(self), false);
    }
}

/// Gives back pixels, least recently drawn first, until those held fit
/// `budget`: never `except`'s or pinned ones, and those no renderer has
/// drawn only with `undrawn` (past the budget, they wait for their upload).
fn give_back(ledger: &mut Ledger, budget: usize, except: Option<&Arc<Kept>>, undrawn: bool) {
    if decoded_bytes() <= budget {
        return;
    }
    ledger.kept.retain(|kept| kept.strong_count() > 0);
    let mut order: Vec<Arc<Kept>> = ledger.kept.iter().filter_map(|k| k.upgrade()).collect();
    order.sort_by_key(|kept| kept.drawn.load(Ordering::Relaxed));
    for kept in order {
        if decoded_bytes() <= budget {
            break;
        }
        if except.is_some_and(|except| Arc::ptr_eq(&kept, except))
            || kept.pinned.get().is_some()
            || (!undrawn && kept.drawn.load(Ordering::Relaxed) == 0)
        {
            continue;
        }
        if kept.lock().take().is_some() {
            HELD.fetch_sub(kept.len, Ordering::Relaxed);
        }
    }
}
impl Drop for Kept {
    fn drop(&mut self) {
        let held = self.held.get_mut().map(|h| h.is_some()).unwrap_or(false);
        if held {
            HELD.fetch_sub(self.len, Ordering::Relaxed);
        }
    }
}

/// A decode for the decoding thread (`decode_later`).
type Job = Box<dyn FnOnce() + Send>;

/// Runs `job` on the one thread decoding kept files again.
fn decode_later(job: Job) {
    static QUEUE: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<Job>>> =
        std::sync::OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (send, receive) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("zgx image decode".into())
            .spawn(move || {
                for job in receive {
                    job();
                }
            })
            .expect("image decode thread");
        std::sync::Mutex::new(send)
    });
    let _ = queue.lock().unwrap_or_else(|p| p.into_inner()).send(job);
}

#[cfg(test)]
mod kept_tests {
    use super::*;

    // The ledger is the process's: these take turns.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn decode(bytes: &[u8]) -> Result<Arc<[u8]>, String> {
        // A "file" of one byte, its pixels 2x2 of that value.
        Ok(Arc::from(vec![bytes[0]; 16]))
    }
    fn kept(value: u8) -> ImageData {
        ImageData::kept(
            2,
            2,
            Arc::from(vec![value; 16]),
            Arc::from(vec![value]),
            decode,
        )
        .unwrap()
    }
    fn wait(image: &ImageData) -> Arc<[u8]> {
        for _ in 0..200 {
            if let Some(pixels) = image.try_pixels() {
                return pixels;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("its pixels never came back");
    }

    #[test]
    fn pixels_stay_below_the_budget_and_go_past_it_least_recently_drawn_first() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let start = decoded_bytes();
        set_decoded_budget(usize::MAX);
        let (a, b, c) = (kept(1), kept(2), kept(3));
        assert_eq!(decoded_bytes(), start + 48);
        for image in [&a, &b, &c] {
            image.uploaded();
        }
        assert!(
            a.try_pixels().is_some() && c.try_pixels().is_some(),
            "below the budget nothing goes"
        );
        // Room for two: the least recently drawn (a) goes, not the one
        // just uploaded.
        set_decoded_budget(start + 32);
        b.uploaded();
        c.uploaded();
        assert_eq!(decoded_bytes(), start + 32);
        assert!(b.try_pixels().is_some() && c.try_pixels().is_some());
        // a's come back from its file, off this thread.
        assert_eq!(&*wait(&a), &[1; 16][..]);
        set_decoded_budget(usize::MAX);
        drop((a, b, c));
        assert_eq!(decoded_bytes(), start, "dropped images hold nothing");
    }

    #[test]
    fn released_all_but_the_pinned_go_and_past_the_budget_only_those_drawn() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let start = decoded_bytes();
        set_decoded_budget(usize::MAX);
        let (drawn, pinned, unseen) = (kept(1), kept(2), kept(3));
        drawn.uploaded();
        assert_eq!(pinned.pixels(), &[2; 16][..]);
        pinned.uploaded();
        // Past the budget, one no renderer has drawn keeps its pixels for
        // its upload.
        set_decoded_budget(0);
        drawn.uploaded();
        assert!(unseen.try_pixels().is_some());
        set_decoded_budget(usize::MAX);
        // Released, it goes too, and comes back if it is ever drawn.
        assert_eq!(release_decoded(), 32);
        assert!(pinned.try_pixels().is_some());
        assert_eq!(&*wait(&drawn), &[1; 16][..]);
        assert_eq!(&*wait(&unseen), &[3; 16][..]);
        drop((drawn, pinned, unseen));
        assert_eq!(decoded_bytes(), start);
    }

    #[test]
    fn pixels_borrowed_stay_for_good() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let start = decoded_bytes();
        let image = kept(7);
        assert_eq!(image.pixels(), &[7; 16][..]);
        set_decoded_budget(0);
        image.uploaded();
        // Borrowed out: kept whatever the budget.
        assert_eq!(image.pixels(), &[7; 16][..]);
        assert!(image.try_pixels().is_some());
        set_decoded_budget(usize::MAX);
        drop(image);
        assert_eq!(decoded_bytes(), start);
    }

    #[test]
    fn transformed_copies_share_the_file_and_images_from_pixels_never_go() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let image = kept(5);
        let turned = image.transformed(crate::affine::Affine::IDENTITY);
        assert!(turned.keeps_file() && image.keeps_file());
        let plain = ImageData::new(2, 2, vec![9; 16]).unwrap();
        set_decoded_budget(0);
        plain.uploaded();
        image.uploaded();
        assert!(plain.try_pixels().is_some() && !plain.keeps_file());
        assert_eq!(&*wait(&turned), &[5; 16][..]);
        set_decoded_budget(usize::MAX);
    }
}
