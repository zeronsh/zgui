//! Textures: images a mesh's colour is multiplied by across its texture
//! coordinates (`World::set_texture`).
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

/// An image (sRGB, straight alpha) a mesh's colour is multiplied by across
/// its texture coordinates, repeating past 0 to 1. Shared by clones (the
/// same texture), immutable, uploaded once per renderer with its mipmaps.
///
/// One decoded from a PNG or JPEG (`decode`) keeps that file's bytes, not
/// its pixels: those go once a renderer has uploaded it, and are decoded
/// again only if another upload needs them (a lost device, another
/// renderer's). One made from pixels (`from_rgba8`) keeps them.
#[derive(Clone)]
pub struct Texture(Arc<Data>);
struct Data {
    id: u64,
    width: u32,
    height: u32,
    /// Its pixels, while held (see `Texture`): the decoder's own, shared,
    /// not a copy.
    pixels: Mutex<Option<Arc<zgx::image::ImageData>>>,
    /// The PNG or JPEG it was decoded from, if it was.
    encoded: Option<Arc<[u8]>>,
}
impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .finish()
    }
}
impl PartialEq for Texture {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
/// What a mesh's texture gives it (`World::set_map`).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Map {
    /// Its colour, multiplied by the material's (sRGB, its alpha the
    /// opacity of a translucent mesh).
    BaseColour,
    /// How metal (blue) and how rough (green) each point is, multiplying
    /// the material's metallic and roughness (glTF's metallicRoughness).
    MetalRoughness,
    /// Which way each point faces, in the surface's own frame (red right,
    /// green up the texture, blue out; glTF's and OpenGL's convention).
    Normal,
}
impl Map {
    /// Every map, in the order a frame binds them.
    pub(crate) const ALL: [Map; 3] = [Map::BaseColour, Map::MetalRoughness, Map::Normal];
    /// Whether its texels are colours (sRGB) rather than data.
    pub(crate) fn srgb(self) -> bool {
        matches!(self, Map::BaseColour)
    }
}

/// The longest side a texture may have.
pub(crate) const MAX_SIDE: u32 = 8192;
impl Texture {
    /// `width` by `height` pixels, four bytes each (red, green, blue, alpha;
    /// sRGB, straight alpha), rows top to bottom. Fails on an empty or
    /// oversized image or a pixel count that doesn't match.
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, &'static str> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err("a texture is 1 to 8192 pixels on a side");
        }
        if pixels.len() != width as usize * height as usize * 4 {
            return Err("a texture's pixels are 4 bytes each, width by height");
        }
        let image = zgx::image::ImageData::new(width, height, pixels)?;
        Ok(Self::made(Arc::new(image), None))
    }
    /// A PNG's or JPEG's bytes, decoded (as zgx-gpu's `png` and `jpeg`
    /// features allow). It keeps `bytes` and drops its pixels once uploaded.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let image = zgx_gpu::assets::decode_image(bytes).map_err(|e| e.to_string())?;
        if image.width() > MAX_SIDE || image.height() > MAX_SIDE {
            return Err("a texture is 1 to 8192 pixels on a side".into());
        }
        Ok(Self::made(image, Some(bytes.into())))
    }
    fn made(image: Arc<zgx::image::ImageData>, encoded: Option<Arc<[u8]>>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(Arc::new(Data {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            width: image.width(),
            height: image.height(),
            pixels: Mutex::new(Some(image)),
            encoded,
        }))
    }
    /// Its width in pixels.
    pub fn width(&self) -> u32 {
        self.0.width
    }
    /// Its height in pixels.
    pub fn height(&self) -> u32 {
        self.0.height
    }
    /// Stable for its lifetime: renderers cache its upload by it.
    pub fn id(&self) -> u64 {
        self.0.id
    }
    /// Its pixels: held, or decoded again from its file. `None` only if
    /// that file no longer decodes to this size (it did when made).
    pub(crate) fn pixels(&self) -> Option<Arc<zgx::image::ImageData>> {
        if let Some(pixels) = self.held().clone() {
            return Some(pixels);
        }
        let image = zgx_gpu::assets::decode_image(self.0.encoded.as_ref()?).ok()?;
        ((image.width(), image.height()) == (self.0.width, self.0.height)).then_some(image)
    }
    /// A renderer has it: its pixels go if its file can bring them back.
    pub(crate) fn uploaded(&self) {
        if self.0.encoded.is_some() {
            *self.held() = None;
        }
    }
    /// Whether it holds its pixels now.
    #[cfg(test)]
    pub(crate) fn holds_pixels(&self) -> bool {
        self.held().is_some()
    }
    fn held(&self) -> std::sync::MutexGuard<'_, Option<Arc<zgx::image::ImageData>>> {
        self.0.pixels.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Each half-size level below `pixels` (`width` by `height`, RGBA8), down
/// to 1 by 1: each texel the mean of the up to 2 by 2 above it. `pixels`
/// itself is the first level, not copied.
pub(crate) fn mipmaps(width: u32, height: u32, pixels: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    loop {
        let (w, h, above) = match levels.last() {
            Some((w, h, above)) => (*w, *h, above.as_slice()),
            None => (width, height, pixels),
        };
        if w <= 1 && h <= 1 {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0_u8; nw as usize * nh as usize * 4];
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0_u32; 4];
                let mut count = 0;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                    let at = (sy * w + sx) as usize * 4;
                    for (total, &byte) in sum.iter_mut().zip(&above[at..at + 4]) {
                        *total += u32::from(byte);
                    }
                    count += 1;
                }
                let at = (y * nw + x) as usize * 4;
                for (out, total) in next[at..at + 4].iter_mut().zip(sum) {
                    *out = ((total + count / 2) / count) as u8;
                }
            }
        }
        levels.push((nw, nh, next));
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 by 1 PNG: red, then blue.
    const RED_BLUE: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 244, 34, 127, 138, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0,
        66, 255, 1, 15, 249, 3, 253, 133, 17, 153, 118, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
        130,
    ];

    #[test]
    fn a_decoded_texture_keeps_its_file_not_its_pixels() {
        let texture = Texture::decode(RED_BLUE).unwrap();
        assert!(texture.holds_pixels(), "until a renderer has it");
        texture.uploaded();
        assert!(!texture.holds_pixels());
        // Decoded again, the same, as often as an upload asks.
        let pixels = [255, 0, 0, 255, 0, 0, 255, 255];
        assert_eq!(texture.pixels().unwrap().pixels(), pixels);
        assert_eq!(texture.pixels().unwrap().pixels(), pixels);
        assert!(!texture.holds_pixels(), "and not kept");
        // Pixels given are all it has: they stay.
        let made = Texture::from_rgba8(2, 1, pixels.to_vec()).unwrap();
        made.uploaded();
        assert!(made.holds_pixels());
    }
}
