//! Image-based light: an environment's light falling on every surface from
//! all around (`World::set_environment`), its reflections sharp on polished
//! metal and blurred on rough. Prefiltered once on the CPU: a cube of its
//! light at five roughnesses (GGX, importance sampled), and its diffuse
//! light as nine spherical-harmonic coefficients (no texture).
use crate::math::Vec3;
use std::sync::{Arc, OnceLock};

/// The cube's side at roughness 0, in texels; each level after halves it.
pub(crate) const SIDE: u32 = 128;
/// Its levels: roughness 0, ¼, ½, ¾ and 1.
pub(crate) const LEVELS: usize = 5;
/// GGX samples a texel of each blurred level.
const SAMPLES: u32 = 64;

/// The light around a scene, for `World::set_environment`: made from an
/// equirectangular image (`decode_hdr`, `from_equirect`), six cube faces
/// (`from_cube`), or built in (`studio`). Cloning shares it; `intensity`
/// and `rotated` make a brighter or turned copy of the same light.
#[derive(Clone)]
pub struct Environment {
    pub(crate) data: Arc<Prefiltered>,
    pub(crate) intensity: f32,
    pub(crate) rotation: f32,
}
impl PartialEq for Environment {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
            && self.intensity == other.intensity
            && self.rotation == other.rotation
    }
}
impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("id", &self.data.id)
            .field("intensity", &self.intensity)
            .field("rotation", &self.rotation)
            .finish()
    }
}

/// An environment ready to draw: its cube's levels as RGB9E5 texels (each
/// level's six faces one after another) and its diffuse light.
pub(crate) struct Prefiltered {
    pub(crate) id: u64,
    pub(crate) levels: Vec<Vec<u32>>,
    /// Irradiance over π (what a white diffuse surface reflects) facing
    /// `n`: `k0 + k1 y + k2 z + k3 x + k4 xy + k5 yz + k6 (3z² − 1) + k7 xz
    /// + k8 (x² − y²)`, each `k` RGB.
    pub(crate) irradiance: [[f32; 3]; 9],
}

impl Environment {
    /// A photographer's studio, built in (no file): a soft grey room, a
    /// large softbox above and in front, a strip light behind to one side
    /// and a fill panel to the other, so metals and gloss read well under
    /// any camera. Made once, shared after.
    pub fn studio() -> Self {
        static STUDIO: OnceLock<Arc<Prefiltered>> = OnceLock::new();
        let data = STUDIO.get_or_init(|| Arc::new(prefilter(&studio))).clone();
        Self::of(data)
    }

    /// From an equirectangular (latitude-longitude) image of linear RGB,
    /// `width` by `height` texels, rows top (up) to bottom: the image's
    /// centre column faces −z, its left edge +z through −x.
    pub fn from_equirect(width: u32, height: u32, rgb: &[f32]) -> Result<Self, &'static str> {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0 || rgb.len() != w * h * 3 {
            return Err("an equirectangular image's RGB texels are width × height × 3");
        }
        let texel = |x: usize, y: usize| {
            let at = (y * w + x) * 3;
            let c = [rgb[at], rgb[at + 1], rgb[at + 2]];
            c.map(|v| if v.is_finite() { v.max(0.) } else { 0. })
        };
        let source = |d: Vec3| {
            // Bilinear, wrapping around the horizon.
            let u = 0.5 + d.x.atan2(-d.z) / std::f32::consts::TAU;
            let v = d.y.clamp(-1., 1.).acos() / std::f32::consts::PI;
            let (fx, fy) = (
                u * w as f32 - 0.5,
                (v * h as f32 - 0.5).clamp(0., h as f32 - 1.),
            );
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            let wrap = |x: f32| (x as i64).rem_euclid(w as i64) as usize;
            let (xa, xb) = (wrap(x0), wrap(x0 + 1.));
            let (ya, yb) = (y0 as usize, (y0 as usize + 1).min(h - 1));
            let (a, b, c, e) = (texel(xa, ya), texel(xb, ya), texel(xa, yb), texel(xb, yb));
            std::array::from_fn(|i| {
                (a[i] * (1. - tx) + b[i] * tx) * (1. - ty) + (c[i] * (1. - tx) + e[i] * tx) * ty
            })
        };
        Ok(Self::of(Arc::new(prefilter(&source))))
    }

    /// From six square faces of linear RGB, `side` texels each, rows top to
    /// bottom, in WebGPU's order and orientation: +x, −x, +y, −y, +z, −z.
    pub fn from_cube(side: u32, faces: [&[f32]; 6]) -> Result<Self, &'static str> {
        let n = side as usize;
        if n == 0 || faces.iter().any(|f| f.len() != n * n * 3) {
            return Err("a cube face's RGB texels are side × side × 3");
        }
        let source = |d: Vec3| {
            let (face, u, v) = face_of(d);
            let x = (((u + 1.) * 0.5 * n as f32) as usize).min(n - 1);
            let y = (((v + 1.) * 0.5 * n as f32) as usize).min(n - 1);
            let at = (y * n + x) * 3;
            let f = faces[face];
            [f[at], f[at + 1], f[at + 2]].map(|v| if v.is_finite() { v.max(0.) } else { 0. })
        };
        Ok(Self::of(Arc::new(prefilter(&source))))
    }

    /// From a Radiance `.hdr` file (RGBE, run-length encoded or flat), an
    /// equirectangular image as `from_equirect` takes.
    pub fn decode_hdr(bytes: &[u8]) -> Result<Self, &'static str> {
        let (width, height, rgb) = decode_rgbe(bytes)?;
        Self::from_equirect(width, height, &rgb)
    }

    /// This light, `intensity` times as bright (1 as made).
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = if intensity.is_finite() {
            intensity.max(0.)
        } else {
            1.
        };
        self
    }

    /// This light turned `radians` about the vertical, as if the room were
    /// turned that way (counter-clockwise seen from above).
    pub fn rotated(mut self, radians: f32) -> Self {
        self.rotation = if radians.is_finite() { radians } else { 0. };
        self
    }

    fn of(data: Arc<Prefiltered>) -> Self {
        Self {
            data,
            intensity: 1.,
            rotation: 0.,
        }
    }
}

/// The direction through face `face` at `u`, `v` (each −1 to 1, `v` down),
/// as WebGPU (and Direct3D, Metal, Vulkan) face a cube.
pub(crate) fn direction(face: usize, u: f32, v: f32) -> Vec3 {
    let d = match face {
        0 => Vec3::from([1., -v, -u]),
        1 => Vec3::from([-1., -v, u]),
        2 => Vec3::from([u, 1., v]),
        3 => Vec3::from([u, -1., -v]),
        4 => Vec3::from([u, -v, 1.]),
        _ => Vec3::from([-u, -v, -1.]),
    };
    d.normalized()
}
/// The face `d` passes through, and where on it (`direction`'s inverse).
pub(crate) fn face_of(d: Vec3) -> (usize, f32, f32) {
    let (x, y, z) = (d.x.abs(), d.y.abs(), d.z.abs());
    if x >= y && x >= z {
        if d.x > 0. {
            (0, -d.z / x, -d.y / x)
        } else {
            (1, d.z / x, -d.y / x)
        }
    } else if y >= z {
        if d.y > 0. {
            (2, d.x / y, d.z / y)
        } else {
            (3, d.x / y, -d.z / y)
        }
    } else if d.z > 0. {
        (4, d.x / z, -d.y / z)
    } else {
        (5, -d.x / z, -d.y / z)
    }
}

/// A cube of linear RGB in memory: each level's faces, each `side` square.
struct Cube {
    levels: Vec<(usize, Vec<[f32; 3]>)>,
}
impl Cube {
    /// Level `level` at `d`, bilinear within its face.
    fn sample(&self, level: usize, d: Vec3) -> [f32; 3] {
        let (n, texels) = &self.levels[level.min(self.levels.len() - 1)];
        let n = *n;
        let (face, u, v) = face_of(d);
        let fx = ((u + 1.) * 0.5 * n as f32 - 0.5).clamp(0., n as f32 - 1.);
        let fy = ((v + 1.) * 0.5 * n as f32 - 0.5).clamp(0., n as f32 - 1.);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let at = |x: usize, y: usize| texels[face * n * n + y * n + x];
        let (a, b, c, e) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        std::array::from_fn(|i| {
            (a[i] * (1. - tx) + b[i] * tx) * (1. - ty) + (c[i] * (1. - tx) + e[i] * tx) * ty
        })
    }
}

/// Each face's texels of a `side` square by `texel(face, u, v)` at their
/// centres, the faces on threads of their own.
fn faces(side: usize, texel: &(dyn Fn(usize, f32, f32) -> [f32; 3] + Sync)) -> Vec<[f32; 3]> {
    let mut out = vec![[0.; 3]; 6 * side * side];
    std::thread::scope(|scope| {
        for (face, chunk) in out.chunks_mut(side * side).enumerate() {
            scope.spawn(move || {
                for (i, slot) in chunk.iter_mut().enumerate() {
                    let (x, y) = (i % side, i / side);
                    let u = (x as f32 + 0.5) / side as f32 * 2. - 1.;
                    let v = (y as f32 + 0.5) / side as f32 * 2. - 1.;
                    *slot = texel(face, u, v);
                }
            });
        }
    });
    out
}

/// `source` (linear RGB by direction) prefiltered: its cube at each
/// roughness, and its irradiance.
fn prefilter(source: &(dyn Fn(Vec3) -> [f32; 3] + Sync)) -> Prefiltered {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let side = SIDE as usize;
    // The base cube, each texel the mean of 2 by 2 samples of the source
    // (its mirror reflection), and its box-filtered chain to sample from.
    let base = faces(side, &|face, u, v| {
        let step = 0.5 / side as f32;
        let mut sum = [0.; 3];
        for (du, dv) in [(-step, -step), (step, -step), (-step, step), (step, step)] {
            let c = source(direction(face, u + du, v + dv));
            for (sum, c) in sum.iter_mut().zip(c) {
                *sum += c * 0.25;
            }
        }
        sum
    });
    let mut chain = Cube {
        levels: vec![(side, base)],
    };
    while chain.levels.last().unwrap().0 > 1 {
        let (n, above) = chain.levels.last().unwrap();
        let (n, half) = (*n, *n / 2);
        let mut next = vec![[0.; 3]; 6 * half * half];
        for face in 0..6 {
            for y in 0..half {
                for x in 0..half {
                    let at =
                        |dx: usize, dy: usize| above[face * n * n + (y * 2 + dy) * n + x * 2 + dx];
                    let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
                    next[face * half * half + y * half + x] =
                        std::array::from_fn(|i| (a[i] + b[i] + c[i] + d[i]) * 0.25);
                }
            }
        }
        chain.levels.push((half, next));
    }
    // Each blurred level: GGX's lobe about each texel's direction (as seen
    // straight on, the split-sum's assumption), its samples read from the
    // chain's level matching their footprint (filtered importance
    // sampling: few samples, no speckle).
    let texel_angle = 4. * std::f32::consts::PI / (6. * (side * side) as f32);
    let mut levels: Vec<Vec<u32>> = Vec::with_capacity(LEVELS);
    levels.push(chain.levels[0].1.iter().map(|&c| rgb9e5(c)).collect());
    for level in 1..LEVELS {
        let n = side >> level;
        let roughness = level as f32 / (LEVELS - 1) as f32;
        let a = roughness * roughness;
        let chain = &chain;
        let texels = faces(n, &|face, u, v| {
            let normal = direction(face, u, v);
            let (t, b) = basis(normal);
            let (mut sum, mut weight) = ([0.; 3], 0.);
            for i in 0..SAMPLES {
                let (e1, e2) = hammersley(i, SAMPLES);
                // GGX half vector about the normal, and the light it reflects.
                let cos = ((1. - e2) / (1. + (a * a - 1.) * e2)).sqrt();
                let sin = (1. - cos * cos).max(0.).sqrt();
                let phi = std::f32::consts::TAU * e1;
                let h = t * (sin * phi.cos()) + b * (sin * phi.sin()) + normal * cos;
                let l = h * (2. * normal.dot(h)) - normal;
                let n_l = normal.dot(l);
                if n_l <= 0. {
                    continue;
                }
                let d = ggx(cos, a);
                let pdf = d / 4.;
                let solid = 1. / (SAMPLES as f32 * pdf + 1e-6);
                let mip = (0.5 * (solid / texel_angle).log2() + 1.).max(0.);
                let c = chain.sample(mip.round() as usize, l);
                for (sum, c) in sum.iter_mut().zip(c) {
                    *sum += c * n_l;
                }
                weight += n_l;
            }
            sum.map(|v| v / weight.max(1e-6))
        });
        levels.push(texels.iter().map(|&c| rgb9e5(c)).collect());
    }
    Prefiltered {
        id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        levels,
        irradiance: irradiance(&chain),
    }
}

/// The diffuse light of the chain's 16-texel level: its radiance projected
/// onto the first nine spherical harmonics, convolved with a cosine lobe
/// (Ramamoorthi and Hanrahan), over π, with each harmonic's constant
/// folded in.
fn irradiance(chain: &Cube) -> [[f32; 3]; 9] {
    let (n, texels) = chain
        .levels
        .iter()
        .find(|(n, _)| *n <= 16)
        .unwrap_or(&chain.levels[0]);
    let n = *n;
    let mut l = [[0.; 3]; 9];
    for face in 0..6 {
        for y in 0..n {
            for x in 0..n {
                let u = (x as f32 + 0.5) / n as f32 * 2. - 1.;
                let v = (y as f32 + 0.5) / n as f32 * 2. - 1.;
                // A texel's solid angle on the unit cube's face.
                let solid = (2. / n as f32).powi(2) / (1. + u * u + v * v).powf(1.5);
                let d = direction(face, u, v);
                let basis = [
                    0.282095,
                    0.488603 * d.y,
                    0.488603 * d.z,
                    0.488603 * d.x,
                    1.092548 * d.x * d.y,
                    1.092548 * d.y * d.z,
                    0.315392 * (3. * d.z * d.z - 1.),
                    1.092548 * d.x * d.z,
                    0.546274 * (d.x * d.x - d.y * d.y),
                ];
                let c = texels[face * n * n + y * n + x];
                for (l, y) in l.iter_mut().zip(basis) {
                    for (l, c) in l.iter_mut().zip(c) {
                        *l += c * y * solid;
                    }
                }
            }
        }
    }
    // Band factors (π, 2π/3, π/4) over π, times each harmonic's constant.
    let factor = [
        0.282095,
        0.488603 * 2. / 3.,
        0.488603 * 2. / 3.,
        0.488603 * 2. / 3.,
        1.092548 / 4.,
        1.092548 / 4.,
        0.315392 / 4.,
        1.092548 / 4.,
        0.546274 / 4.,
    ];
    std::array::from_fn(|k| l[k].map(|v| v * factor[k]))
}

fn ggx(cos: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = cos * cos * (a2 - 1.) + 1.;
    a2 / (std::f32::consts::PI * d * d).max(1e-12)
}
/// Two axes square to `n` and each other.
fn basis(n: Vec3) -> (Vec3, Vec3) {
    let up = if n.y.abs() < 0.999 {
        Vec3::Y
    } else {
        Vec3::from([1., 0., 0.])
    };
    let t = up.cross(n).normalized();
    (t, n.cross(t))
}
/// The `i`th of `n` points of the Hammersley set: even over the square.
fn hammersley(i: u32, n: u32) -> (f32, f32) {
    (i as f32 / n as f32, i.reverse_bits() as f32 / 2f32.powi(32))
}

/// Linear RGB as a shared-exponent RGB9E5 texel (wgpu's `Rgb9e5Ufloat`).
pub(crate) fn rgb9e5(c: [f32; 3]) -> u32 {
    const MAX: f32 = 65408.; // (2^9 − 1) / 2^9 × 2^16
    let c = c.map(|v| if v.is_finite() { v.clamp(0., MAX) } else { 0. });
    let peak = c[0].max(c[1]).max(c[2]);
    if peak < 1e-10 {
        return 0;
    }
    let mut exponent = (peak.log2().floor() as i32).max(-16) + 1 + 15;
    let mut scale = 2f32.powi(exponent - 15 - 9);
    if (peak / scale + 0.5).floor() as u32 == 512 {
        exponent += 1;
        scale *= 2.;
    }
    let [r, g, b] = c.map(|v| ((v / scale + 0.5).floor() as u32).min(511));
    r | g << 9 | b << 18 | (exponent as u32) << 27
}

/// The studio: linear RGB seen along `d`.
fn studio(d: Vec3) -> [f32; 3] {
    // The room: a darker floor, walls brightening to the ceiling.
    let wall = if d.y < 0. {
        0.18 + 0.1 * (1. + d.y)
    } else {
        0.28 + 0.27 * d.y
    };
    let mut c = [wall, wall, wall * 1.02];
    // Softboxes: (centre, half width and height in tangent units, light).
    let lights: [([f32; 3], (f32, f32), f32); 3] = [
        ([-0.35, 0.82, 0.45], (0.55, 0.38), 9.),
        ([0.85, 0.3, -0.45], (0.12, 0.7), 5.),
        ([-0.9, 0.15, -0.35], (0.45, 0.45), 1.6),
    ];
    for (centre, (half_w, half_h), light) in lights {
        let centre = Vec3::from(centre).normalized();
        let facing = d.dot(centre);
        if facing <= 0. {
            continue;
        }
        let (t, b) = basis(centre);
        let (x, y) = (d.dot(t) / facing, d.dot(b) / facing);
        // Soft edges: full inside, fading over a fifth of each half size.
        let edge = |at: f32, half: f32| {
            let s = ((half - at.abs()) / (half * 0.2)).clamp(0., 1.);
            s * s * (3. - 2. * s)
        };
        let k = edge(x, half_w) * edge(y, half_h) * light;
        for v in &mut c {
            *v += k;
        }
    }
    c
}

/// A Radiance `.hdr` image's width, height and linear RGB.
fn decode_rgbe(bytes: &[u8]) -> Result<(u32, u32, Vec<f32>), &'static str> {
    const BAD: &str = "not a Radiance .hdr image";
    if !(bytes.starts_with(b"#?RADIANCE") || bytes.starts_with(b"#?RGBE")) {
        return Err(BAD);
    }
    // Header lines to an empty one, then the size line.
    fn line<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
        let start = *at;
        let end = start + bytes.get(start..)?.iter().position(|&b| b == b'\n')?;
        *at = end + 1;
        Some(&bytes[start..end])
    }
    let mut at = 0;
    loop {
        let l = line(bytes, &mut at).ok_or(BAD)?;
        if l.starts_with(b"FORMAT=") && l != b"FORMAT=32-bit_rle_rgbe" {
            return Err("a .hdr image of other than RGBE texels");
        }
        if l.is_empty() {
            break;
        }
    }
    let size = std::str::from_utf8(line(bytes, &mut at).ok_or(BAD)?).map_err(|_| BAD)?;
    let parts: Vec<&str> = size.split_whitespace().collect();
    let (height, width) = match parts.as_slice() {
        ["-Y", h, "+X", w] => (h.parse::<u32>(), w.parse::<u32>()),
        _ => return Err("a .hdr image other than rows top to bottom, left to right"),
    };
    let (width, height) = (width.map_err(|_| BAD)?, height.map_err(|_| BAD)?);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 1 << 26 {
        return Err("a .hdr image of no texels or too many");
    }
    let w = width as usize;
    let mut rgb = Vec::with_capacity(w * height as usize * 3);
    let mut row = vec![[0u8; 4]; w];
    let mut data = &bytes[at..];
    fn byte(data: &mut &[u8]) -> Result<u8, &'static str> {
        let (&b, rest) = data.split_first().ok_or("a .hdr image cut short")?;
        *data = rest;
        Ok(b)
    }
    for _ in 0..height {
        let start = [data.first(), data.get(1), data.get(2), data.get(3)];
        let rle = (8..0x8000).contains(&w)
            && start[0] == Some(&2)
            && start[1] == Some(&2)
            && start[2].is_some_and(|&b| b & 0x80 == 0)
            && start[3].is_some();
        if rle {
            let stated = usize::from(start[2].copied().unwrap_or(0)) << 8
                | usize::from(start[3].copied().unwrap_or(0));
            if stated != w {
                return Err("a .hdr row of another width");
            }
            data = &data[4..];
            // Each channel's run-length bytes in turn.
            for channel in 0..4 {
                let mut x = 0;
                while x < w {
                    let count = byte(&mut data)?;
                    if count > 128 {
                        let run = usize::from(count - 128);
                        let value = byte(&mut data)?;
                        if x + run > w {
                            return Err("a .hdr run past its row");
                        }
                        for texel in &mut row[x..x + run] {
                            texel[channel] = value;
                        }
                        x += run;
                    } else {
                        let run = usize::from(count);
                        if run == 0 || x + run > w {
                            return Err("a .hdr run past its row");
                        }
                        for texel in &mut row[x..x + run] {
                            texel[channel] = byte(&mut data)?;
                        }
                        x += run;
                    }
                }
            }
        } else {
            for texel in &mut row {
                for channel in texel.iter_mut() {
                    *channel = byte(&mut data)?;
                }
            }
        }
        for [r, g, b, e] in &row {
            if *e == 0 {
                rgb.extend([0.; 3]);
            } else {
                let scale = 2f32.powi(i32::from(*e) - 128 - 8);
                rgb.extend([*r, *g, *b].map(|v| (f32::from(v) + 0.5) * scale));
            }
        }
    }
    Ok((width, height, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_and_directions_are_inverse() {
        for face in 0..6 {
            for (u, v) in [(0., 0.), (0.5, -0.25), (-0.9, 0.7)] {
                let d = direction(face, u, v);
                let (f, fu, fv) = face_of(d);
                assert_eq!(f, face);
                assert!(
                    (fu - u).abs() < 1e-5 && (fv - v).abs() < 1e-5,
                    "{face} {u} {v}"
                );
            }
        }
        // WebGPU's faces: +y's top row looks toward −z.
        assert!(direction(2, 0., -1.).z < 0.);
        assert!(direction(4, 0., -1.).y > 0., "+z's top row looks up");
    }

    #[test]
    fn rgb9e5_keeps_three_significant_digits() {
        let unpack = |t: u32| {
            let scale = 2f32.powi((t >> 27) as i32 - 15 - 9);
            [t & 511, t >> 9 & 511, t >> 18 & 511].map(|m| m as f32 * scale)
        };
        for c in [
            [1., 0.5, 0.25],
            [9., 0.01, 3.],
            [0.003, 0.002, 0.001],
            [600., 0., 1.],
        ] {
            let back = unpack(rgb9e5(c));
            let peak = c[0].max(c[1]).max(c[2]);
            for (back_value, value) in back.iter().zip(c) {
                assert!(
                    (back_value - value).abs() <= peak / 256.,
                    "{c:?} → {back:?}"
                );
            }
        }
        assert_eq!(rgb9e5([0.; 3]), 0);
        assert_eq!(rgb9e5([f32::NAN, -1., 0.]), 0);
    }

    #[test]
    fn a_uniform_environment_lights_every_way_alike() {
        // Light 0.5 all round: a white diffuse surface reflects 0.5 facing
        // any way, and every level of the cube holds 0.5.
        let data = prefilter(&|_| [0.5; 3]);
        let e = |n: Vec3| {
            let k = &data.irradiance;
            let (x, y, z) = (n.x, n.y, n.z);
            let terms = [
                1.,
                y,
                z,
                x,
                x * y,
                y * z,
                3. * z * z - 1.,
                x * z,
                x * x - y * y,
            ];
            (0..9).map(|i| k[i][0] * terms[i]).sum::<f32>()
        };
        for n in [
            Vec3::Y,
            Vec3::from([1., 0., 0.]),
            Vec3::from([0.3, -0.8, 0.5]).normalized(),
        ] {
            assert!((e(n) - 0.5).abs() < 0.01, "{n:?}: {}", e(n));
        }
        assert_eq!(data.levels.len(), LEVELS);
        let half = rgb9e5([0.5; 3]);
        for (level, texels) in data.levels.iter().enumerate() {
            let n = (SIDE >> level) as usize;
            assert_eq!(texels.len(), 6 * n * n);
            assert!(texels.iter().all(|&t| t == half), "level {level}");
        }
    }

    #[test]
    fn a_bright_side_lights_the_surfaces_facing_it() {
        // Light from +x only: surfaces facing +x get the most, those facing
        // away almost none; a rough reflection along +x is bright, along −x
        // dark.
        let data = prefilter(&|d| if d.x > 0. { [1.; 3] } else { [0.; 3] });
        let e = |n: Vec3| {
            let k = &data.irradiance;
            k[0][0] + k[3][0] * n.x + k[1][0] * n.y + k[2][0] * n.z
        };
        assert!(
            e(Vec3::from([1., 0., 0.])) > 0.85,
            "{}",
            e(Vec3::from([1., 0., 0.]))
        );
        assert!(e(Vec3::from([-1., 0., 0.])) < 0.1);
        let level = &data.levels[2];
        let n = (SIDE >> 2) as usize;
        let middle = |face: usize| level[face * n * n + (n / 2) * n + n / 2];
        assert!(middle(0) > middle(1));
    }

    #[test]
    fn an_hdr_file_reads_flat_and_run_length_rows() {
        // 8 by 2: a flat row of mid grey, then the same run-length encoded.
        let mut file = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 2 +X 8\n".to_vec();
        for _ in 0..8 {
            file.extend([128, 128, 128, 129]);
        }
        file.extend([2, 2, 0, 8]);
        for value in [128u8, 128, 128, 129] {
            file.extend([128 + 8, value]);
        }
        let (w, h, rgb) = decode_rgbe(&file).unwrap();
        assert_eq!((w, h, rgb.len()), (8, 2, 48));
        assert!(rgb.iter().all(|&v| (v - 1.0039).abs() < 1e-3), "{rgb:?}");
        assert!(
            decode_rgbe(b"#?RADIANCE\n\n-Y 2 +X 8\n").is_err(),
            "cut short"
        );
        assert!(decode_rgbe(b"P6 not hdr").is_err());
    }
}
