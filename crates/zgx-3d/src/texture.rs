//! Textures: images a mesh's colour is multiplied by across its texture
//! coordinates (`World::set_texture`).
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// An image (sRGB, straight alpha) a mesh's colour is multiplied by across
/// its texture coordinates, repeating past 0 to 1. Shared by clones (the
/// same texture), immutable, uploaded once per renderer with its mipmaps.
#[derive(Clone)]
pub struct Texture(Arc<Data>);
struct Data {
    id: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
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
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Ok(Self(Arc::new(Data {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            pixels,
        })))
    }
    /// A PNG's or JPEG's bytes, decoded (as zgx-gpu's `png` and `jpeg`
    /// features allow).
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let image = zgx_gpu::assets::decode_image(bytes).map_err(|e| e.to_string())?;
        Self::from_rgba8(image.width(), image.height(), image.pixels().to_vec())
            .map_err(str::to_owned)
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
    pub(crate) fn pixels(&self) -> &[u8] {
        &self.0.pixels
    }
}

/// `pixels` (`width` by `height`, RGBA8) and each half-size level below it,
/// down to 1 by 1: each texel the mean of the up to 2 by 2 above it.
pub(crate) fn mipmaps(width: u32, height: u32, pixels: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(width, height, pixels.to_vec())];
    while let Some((w, h, above)) = levels.last()
        && (*w > 1 || *h > 1)
    {
        let (w, h) = (*w, *h);
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
