//! Damage-scissored retained GPU renderer for Vulkan (Linux) and Metal (macOS).
//! The output is premultiplied BGRA8 (read back as RGBA8). Text uses cosmic-text shaping and a bounded
//! GPU glyph atlas; stable text never reshapes on paint or compositor updates.
#![warn(missing_docs)]
use cosmic_text::{Buffer, CacheKey, FontSystem, Metrics, SwashCache, SwashContent};
use rustc_hash::FxHashMap;
use std::{cell::RefCell, fmt, rc::Rc, sync::Arc};
use zgui::scene::{Color, NodeId, NodeKind, Rect, Scene};

#[cfg(all(windows, feature = "composition"))]
mod composition;
#[cfg(windows)]
mod dxgi;
mod effects;
mod font_cache;
#[cfg(target_os = "macos")]
mod layer_present;
#[cfg(target_os = "macos")]
mod metal_frame;
#[cfg(target_os = "macos")]
mod native_surface;
mod planes;
pub mod present_log;
mod surface;
mod upload;
pub use font_cache::set_desktop_sans_serif;
pub use surface::PresentationStatus;

const MAX_ATLAS: u32 = 2048;
const INITIAL_ATLAS: u32 = 512;
/// An atlas that fills again within this many frames of its last recycle
/// doubles instead (see `render_flat`).
const ATLAS_THRASH_FRAMES: u64 = 600;
/// How long after its last resize a macOS layer keeps a third drawable.
#[cfg(target_os = "macos")]
const WIDE_DRAWABLES: std::time::Duration = std::time::Duration::from_secs(2);
/// SVG rasters up to this size (physical pixels) share the glyph atlas.
const ATLAS_IMAGE_MAX: u32 = 128;
/// Write straight-alpha `pixels` premultiplied (as `upload_image` does) into
/// the interior of a bordered atlas cell `width` + 2 texels wide.
fn premultiply_into_cell(cell: &mut [u8], pixels: &[u8], width: u32) {
    let row = width as usize * 4;
    for (y, source) in pixels.chunks_exact(row).enumerate() {
        let start = ((y + 1) * (width as usize + 2) + 1) * 4;
        for (texel, pixel) in cell[start..start + row]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(source.as_chunks::<4>().0)
        {
            let alpha = u16::from(pixel[3]);
            for channel in 0..3 {
                texel[channel] = ((u16::from(pixel[channel]) * alpha + 127) / 255) as u8;
            }
            texel[3] = pixel[3];
        }
    }
}
/// The retained target's format: BGRA, as window surfaces are on macOS and
/// X11, so presenting can be a plain copy (see `copy_present`). Readback
/// converts to RGBA.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
/// The backdrop blur atlas's format (see `atlas_texture`).
const BLUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Why rendering failed, in words.
#[derive(Debug)]
pub struct GpuError(pub String);
impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for GpuError {}
/// What one frame's rendering did, for tests and diagnostics.
#[derive(Default, Debug, Clone, Copy)]
pub struct GpuStats {
    /// Draw calls encoded.
    pub draw_calls: usize,
    /// Encoded render passes, including repainted isolated layers; excludes presentation.
    pub render_passes: usize,
    /// Backdrop filter passes (three per batch of filters blurred together), included in render_passes.
    pub blur_passes: usize,
    /// Quad instances drawn.
    pub instances: usize,
    /// Text nodes shaped (not reused from the cache).
    pub shaped_nodes: usize,
    /// Glyphs uploaded to the atlas.
    pub glyph_uploads: usize,
    /// Images uploaded to textures.
    pub image_uploads: usize,
    /// Native GPU imports/conversions, never CPU image uploads.
    pub native_surface_imports: usize,
    /// Native surfaces converted to a drawable format.
    pub native_surface_conversions: usize,
    /// Canvases and decorations rasterized.
    pub canvas_rasterizations: usize,
    /// SVGs rasterized.
    pub svg_rasterizations: usize,
    /// Times the instance geometry was rebuilt.
    pub geometry_rebuilds: usize,
    /// Vertex buffers allocated or grown.
    pub vertex_buffer_allocations: usize,
    /// Physical pixels repainted.
    pub damaged_pixels: u64,
    /// Cached layers repainted.
    pub layer_repaints: usize,
    /// Cached layers composited without repainting.
    pub layer_cache_hits: usize,
    /// Layer textures allocated.
    pub layer_texture_allocations: usize,
    /// Scrolled regions shifted by copying instead of redrawn.
    pub scroll_copies: usize,
    /// Layer effect passes (content blurs and masks drawn anew; at most two
    /// a frame however many), included in render_passes.
    pub effect_passes: usize,
}
/// Retained allocation counters for diagnostics; excludes driver/font database allocations.
#[derive(Debug, Clone, Copy)]
pub struct DebugCacheStats {
    /// Text nodes whose shaping is cached.
    pub shaped_nodes: usize,
    /// Bytes of cached shaping.
    pub shaped_bytes: usize,
    /// Glyph quads cached with it.
    pub cached_quads: usize,
    /// Glyphs in the atlas.
    pub atlas_entries: usize,
    /// The atlas texture's bytes.
    pub atlas_bytes: usize,
    /// Glyph images in the rasterizer's cache.
    pub swash_images: usize,
    /// Glyph outlines in the rasterizer's cache.
    pub swash_outlines: usize,
    /// Image textures.
    pub image_textures: usize,
    /// Their bytes.
    pub image_bytes: usize,
    /// Bytes of rasterized canvases and decorations.
    pub canvas_raster_bytes: usize,
    /// Bytes of rasterized SVGs.
    pub svg_raster_bytes: usize,
    /// The vertex buffer's bytes.
    pub vertex_buffer_bytes: usize,
    /// Cached layer textures.
    pub layer_textures: usize,
    /// Their bytes.
    pub layer_bytes: usize,
}
const SHAPE_BUDGET: usize = 8 * 1024 * 1024;
/// Shaping frames a cached word survives without being used.
const SHAPE_RUN_AGES: u64 = 2;
const VERTEX_BUDGET: usize = 32 * 1024 * 1024;
/// Image textures a renderer holds at once, each with its own bind group (a
/// view in Direct3D 12's fixed-size heap; see `GpuContext::create`).
const MAX_IMAGES: usize = 16_384;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Quad {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    fade: [f32; 4],
    options: [f32; 4],
    shape: [f32; 4],
    border: [f32; 4],
    /// `x`: the index plus one of the clip region the quad lies in (its
    /// ancestors' rounded clips and edge fades, see `Region`), 0 for none.
    mask: [f32; 4],
}
const NO_MASK: [f32; 4] = [0.; 4];
/// A clip region quads share (`Region` in draw.wgsl): up to two rounded
/// boxes (x, y, width, height; a negative width for none) with their
/// corner radii, and a box (x0, y0, x1, y1) fading over bands at its edges
/// (top, right, bottom, left). In the target's logical units.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Region {
    round0: [f32; 4],
    radii0: [f32; 4],
    round1: [f32; 4],
    radii1: [f32; 4],
    fade_box: [f32; 4],
    fade: [f32; 4],
    /// Colour filters: an affine map of straight RGB, rows R, G, B, on
    /// where `tint[0]` is 1.
    color: [[f32; 4]; 3],
    tint: [f32; 4],
}
impl Region {
    /// A shader fill's uniforms as a record of the region table (`time`
    /// first, its `params` in the four lanes after; see `fill_source`).
    fn uniforms(uniforms: zgui::decoration::ShaderUniforms) -> Self {
        let [a, b, c, d] = uniforms.params;
        Self {
            round0: [uniforms.time, 0., 0., 0.],
            radii0: a,
            round1: b,
            radii1: c,
            fade_box: d,
            fade: [0.; 4],
            color: [[0.; 4]; 3],
            tint: [0.; 4],
        }
    }
    /// `item`'s, if it lies in one.
    fn of(item: &zgui::scene::PaintItem<'_>) -> Option<Self> {
        if item.mask().is_none()
            && item.rounded().iter().all(Option::is_none)
            && item.color().is_none()
        {
            return None;
        }
        let round = |clip: Option<zgui::scene::RoundRect>| match clip {
            Some(clip) => (
                [clip.rect.x, clip.rect.y, clip.rect.width, clip.rect.height],
                clip.radii,
            ),
            None => ([0., 0., -1., -1.], [0.; 4]),
        };
        let ((round0, radii0), (round1, radii1)) =
            (round(item.rounded()[0]), round(item.rounded()[1]));
        let (fade_box, fade) = item.mask().map_or(([0.; 4], [0.; 4]), |(rect, bands)| {
            (
                [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height],
                bands,
            )
        });
        Some(Self {
            round0,
            radii0,
            round1,
            radii1,
            fade_box,
            fade,
            color: item.color().unwrap_or_default(),
            tint: [if item.color().is_some() { 1. } else { 0. }, 0., 0., 0.],
        })
    }
}
/// The index (plus one) `region` has in `regions`, added if new; 0 for none.
/// Neighbouring items mostly share one (a list in a rounded card).
fn region_index(regions: &mut Vec<Region>, region: Option<Region>) -> f32 {
    let Some(region) = region else {
        return 0.;
    };
    if let Some(at) = regions.iter().rev().take(8).position(|r| *r == region) {
        return (regions.len() - at) as f32;
    }
    regions.push(region);
    regions.len() as f32
}
/// How a quad is drawn, where drawing is split (see `push_draw`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shading {
    /// Corners, borders, shadows, fades, masks or transforms: `shade`.
    Full,
    /// Colour, or colour times a texture sample: `basic`.
    Basic,
    /// An opaque flat colour: `basic` without blending, which reads every
    /// pixel it writes. Same pixels.
    Opaque,
    /// A shader fill (`FillShader`, by id): its pipeline, `shade` for
    /// whatever else the draw holds.
    Fill(u64),
}
fn shading(quad: &Quad, scale: f32) -> Shading {
    // A panel with square corners and no border only antialiases its edges:
    // on whole device pixels, coverage is exactly what rasterizing gives.
    let whole = |value: f32| {
        let device = value * scale;
        (device - device.round()).abs() < 1e-3
    };
    let [x, y, width, height] = quad.rect;
    let square = quad.shape[3] > 0.5
        && quad.shape[0] <= 0.
        && quad.shape[1] <= 0.
        && quad.options[3] < 0.5
        && quad.rect == quad.fade
        && [x, y, x + width, y + height].into_iter().all(whole);
    // The basic shader computes what the full one does in the same order,
    // bit for bit, except for the coverage of a translucent square panel (an
    // extra divide and multiply): those stay full, so either shader draws the
    // same bytes.
    let plain = quad.options[0] == 0.
        && quad.options[2] < 0.5
        && quad.shape[2] <= 0.
        && (quad.shape[3] <= 0.5 || (square && quad.color[3] >= 1.))
        && quad.mask[0] <= 0.;
    if !plain {
        Shading::Full
    } else if quad.options[1] <= 0.5 && quad.color[3] >= 1. {
        Shading::Opaque
    } else {
        Shading::Basic
    }
}
/// The quad pipelines, each compiled on first use: a software rasterizer
/// compiles a pipeline into megabytes of code, and plain interfaces never
/// need the full shader there.
struct QuadPipelines {
    device: wgpu::Device,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    full: std::cell::OnceCell<wgpu::RenderPipeline>,
    full_clear: std::cell::OnceCell<wgpu::RenderPipeline>,
    basic: std::cell::OnceCell<wgpu::RenderPipeline>,
    basic_clear: std::cell::OnceCell<wgpu::RenderPipeline>,
}
impl QuadPipelines {
    /// The pipeline drawing quads of `shading`.
    fn get(&self, shading: Shading) -> &wgpu::RenderPipeline {
        let (cell, basic, blend) = match shading {
            Shading::Full | Shading::Fill(_) => (&self.full, false, true),
            Shading::Basic => (&self.basic, true, true),
            Shading::Opaque => (&self.basic_clear, true, false),
        };
        cell.get_or_init(|| {
            quad_pipeline(&self.device, &self.shader, &self.layout, blend, None, basic)
        })
    }
    /// The pipeline clearing a region to the background quad.
    fn clear(&self, basic: bool) -> &wgpu::RenderPipeline {
        if basic {
            self.get(Shading::Opaque)
        } else {
            self.full_clear.get_or_init(|| {
                quad_pipeline(&self.device, &self.shader, &self.layout, false, None, false)
            })
        }
    }
}
/// Damage regions merge when their union adds at most half the smaller's
/// area and this many device pixels (see `merge_damage`): each region costs
/// its draws again, worth more GPU time than shading a 128×128 square.
const NEAR_PIXELS: f32 = 16_384.;
/// Damaged regions of at least this many device pixels draw opaque quads
/// without blending (see `encode_draws`).
const SPLIT_PIXELS: f32 = 32_768.;
/// Split a draw's quads into runs of one shading, in paint order, when
/// `split`. The full shader's size (the registers it holds) slows every pixel
/// it shades, on a GPU as on a software rasterizer: plain quads (backgrounds,
/// glyphs, bars) draw with the basic one.
fn push_draw(draws: &mut Vec<Draw>, quads: &[Quad], draw: Draw, split: bool, scale: f32) {
    if !split
        || draw.blur.is_some()
        || draw.layer.is_some()
        || matches!(draw.shading, Shading::Fill(_))
        || draw.end <= draw.start
    {
        draws.push(draw);
        return;
    }
    // Each run ends where the shading changes, so runs alternate shadings.
    let mut start = draw.start;
    while start < draw.end {
        let kind = shading(&quads[start as usize], scale);
        let mut end = start + 1;
        while end < draw.end && shading(&quads[end as usize], scale) == kind {
            end += 1;
        }
        // One run (the usual item): the draw's own bounds.
        let bounds = match (start, end) == (draw.start, draw.end) {
            true => draw.bounds,
            false => quad_bounds(&quads[start as usize..end as usize]),
        };
        draws.push(Draw {
            start,
            end,
            bounds,
            shading: kind,
            ..draw.clone()
        });
        start = end;
    }
}
fn texel_at(texture: &wgpu::Texture, x: u32, y: u32) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d { x, y, z: 0 },
        aspect: wgpu::TextureAspect::All,
    }
}
/// Whether moving `scroll`'s pixels reproduces the frame: its content is one
/// contiguous run of paint items free of masks, layers and filters; beneath
/// it the clip shows a single opaque rectangle (or the window background);
/// above it nothing paints in the clip outside this frame's damage.
fn copy_is_exact(
    scene: &Scene,
    scroll: &zgui::scene::ScrollMove,
    damage: &[Rect],
    scale: f32,
) -> bool {
    let clip = scroll.clip;
    let items = scene.layer_items_within(None, Some(&[clip]));
    let content = |item: &zgui::scene::PaintItem<'_>| scene.is_within(item.id, scroll.node);
    let Some(first) = items.iter().position(content) else {
        return false;
    };
    let last = items.iter().rposition(content).expect("found above");
    let paints = |item: &zgui::scene::PaintItem<'_>| -> Option<Rect> {
        let bounds = match item.kind {
            NodeKind::Container(_) if item.effects.blur_radius <= 0. => return None,
            NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => quad.paint_bounds(item.bounds),
            _ => item.bounds,
        };
        if item.effects.opacity <= 0. {
            return None;
        }
        bounds.intersection(item.clip?)?.intersection(clip)
    };
    let repainted = |rect: Rect| {
        damage.iter().any(|d| {
            d.x <= rect.x
                && d.y <= rect.y
                && d.x + d.width >= rect.x + rect.width
                && d.y + d.height >= rect.y + rect.height
        })
    };
    for item in &items[first..=last] {
        if !content(item) || item.mask().is_some() || item.isolated || item.effects.blur_radius > 0.
        {
            return false;
        }
    }
    if items[last + 1..]
        .iter()
        .filter_map(paints)
        .any(|rect| !repainted(rect))
    {
        return false;
    }
    // Content opaque across the whole clip (a list's rows) hides whatever
    // lies under it, before the move and after.
    let opaque = |item: &&zgui::scene::PaintItem<'_>| {
        matches!(item.kind, NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }
            if quad.fill.3 == 255 && quad.radius <= 0. && quad.shadow.is_none() && quad.decoration.is_none())
            && item.effects.opacity >= 1.
            && item.effects.edge_fade <= 0.
    };
    if spans_cover(
        clip,
        items[first..=last].iter().filter(opaque).filter_map(paints),
        scale,
    ) {
        return true;
    }
    for item in items[..first].iter().rev() {
        let Some(rect) = paints(item) else {
            continue;
        };
        if let NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } = item.kind
            && quad.fill.3 == 255
            && quad.radius <= 0.
            && quad.border_width <= 0.
            && quad.shadow.is_none()
            && quad.decoration.is_none()
            && item.effects.opacity >= 1.
            && item.effects.edge_fade <= 0.
            && item.mask().is_none()
            && !item.isolated
            && rect == clip
        {
            return true;
        }
        if !repainted(rect) {
            return false;
        }
    }
    true
}
/// Whether rectangles spanning `clip` side to side (or top to bottom) cover
/// it, meeting on whole device pixels: no seam lets anything beneath show.
fn spans_cover(clip: Rect, rects: impl Iterator<Item = Rect>, scale: f32) -> bool {
    let whole = |value: f32| ((value * scale) - (value * scale).round()).abs() < 1e-3;
    let (mut rows, mut columns) = (Vec::new(), Vec::new());
    for r in rects {
        let (right, bottom) = (r.x + r.width, r.y + r.height);
        if r.x <= clip.x && right >= clip.x + clip.width && whole(r.y) && whole(bottom) {
            rows.push((r.y, bottom));
        }
        if r.y <= clip.y && bottom >= clip.y + clip.height && whole(r.x) && whole(right) {
            columns.push((r.x, right));
        }
    }
    let covered = |mut spans: Vec<(f32, f32)>, start: f32, end: f32| {
        spans.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut reach = start;
        for (from, to) in spans {
            if from > reach {
                break;
            }
            reach = reach.max(to);
        }
        reach >= end
    };
    covered(rows, clip.y, clip.y + clip.height) || covered(columns, clip.x, clip.x + clip.width)
}
/// Positioned glyphs of laid-out runs; rich text takes each glyph's colour
/// from its run (it is shaped without colours, see `rich_buffer_raw`).
fn glyphs_of<'a>(
    runs: impl Iterator<Item = cosmic_text::LayoutRun<'a>>,
    starts: &[usize],
    rich: Option<&zgui::rich_text::RichText>,
    scale: f32,
) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    for run in runs {
        // Most texts are one line: one allocation at its size.
        glyphs.reserve(run.glyphs.len());
        for glyph in run.glyphs {
            let p = glyph.physical((0., 0.), scale);
            let run_index = rich.map_or(0, |rich| {
                text::run_at(rich, starts[run.line_i] + glyph.start)
            });
            glyphs.push(Glyph {
                key: p.cache_key,
                color: match rich {
                    Some(rich) => Some(rich.runs()[run_index].color),
                    None => glyph
                        .color_opt
                        .map(|c| zgui::scene::Color(c.r(), c.g(), c.b(), c.a())),
                },
                run: run_index,
                x: p.x as f32,
                // Each line's baseline on a whole device pixel too.
                y: p.y as f32 + (run.line_y * scale).round(),
            });
        }
    }
    glyphs
}
/// A pixel rectangle: x, y, width, height.
type Scissor = (u32, u32, u32, u32);
/// A scroll copy's destination and the offset of its source.
type ScrollCopy = (Scissor, (i32, i32));
/// For each damage region, the index of the topmost quad that paints every
/// pixel of the region opaque: a plain solid rectangle (no texture, radius,
/// border, shadow, fade, mask or transform) at full alpha, unclipped over the
/// region's scissor. Nothing drawn before it, including the clear, can show.
/// 0 when no quad qualifies.
fn occluded_from(
    quads: &[Quad],
    draws: &[Draw],
    regions: &[(Rect, Scissor)],
    scale: f32,
) -> Vec<u32> {
    let covers = |outer: Rect, inner: Rect| {
        outer.x <= inner.x
            && outer.y <= inner.y
            && outer.x + outer.width >= inner.x + inner.width
            && outer.y + outer.height >= inner.y + inner.height
    };
    let plain = |q: &Quad| {
        q.color[3] >= 1.
            && q.options == [0.; 4]
            && q.shape[0] <= 0.
            && q.shape[1] <= 0.
            && q.shape[2] <= 0.
            && q.mask == NO_MASK
    };
    regions
        .iter()
        .map(|(_, (x, y, w, h))| {
            // The pixels the scissor lets through, in logical units.
            let pixels = Rect::new(
                *x as f32 / scale,
                *y as f32 / scale,
                *w as f32 / scale,
                *h as f32 / scale,
            );
            for draw in draws.iter().rev() {
                if draw.image.is_some()
                    || draw.layer.is_some()
                    || draw.blur.is_some()
                    || !covers(draw.bounds, pixels)
                    || !covers(draw.clip, pixels)
                {
                    continue;
                }
                for index in (draw.start..draw.end).rev() {
                    let q = &quads[index as usize];
                    let rect = Rect::new(q.rect[0], q.rect[1], q.rect[2], q.rect[3]);
                    if plain(q) && covers(rect, pixels) {
                        return index;
                    }
                }
            }
            0
        })
        .collect()
}
#[derive(Clone)]
struct Glyph {
    key: CacheKey,
    color: Option<zgui::scene::Color>,
    /// The rich run it was shaped from, to recolour paint-only changes.
    run: usize,
    x: f32,
    y: f32,
}
struct Shaped {
    rich: Option<Arc<zgui::rich_text::RichText>>,
    text_options: zgui::text_layout::TextOptions,
    decorations: Vec<text::RichDecoration>,
    font: zgui::text_layout::FontStyle,
    text: Arc<str>,
    size: f32,
    width: f32,
    height: f32,
    /// Device pixels per logical pixel the glyphs were placed at: a layer's
    /// raster scale differs from the window's.
    scale: f32,
    glyphs: Vec<Glyph>,
    quads: Vec<Quad>,
    atlas_epoch: u64,
    last_used: u64,
    /// The prepared text these glyphs were laid out from, kept while drawn.
    prepared: Option<u64>,
}
impl Shaped {
    fn bytes(&self) -> usize {
        self.rich
            .as_ref()
            .map_or(self.text.len(), |r| r.storage_bytes())
            + self.font.storage_bytes()
            + self.decorations.capacity() * std::mem::size_of::<text::RichDecoration>()
            + self.glyphs.capacity() * std::mem::size_of::<Glyph>()
            + self.quads.capacity() * std::mem::size_of::<Quad>()
    }
}
#[derive(Clone, Copy)]
struct AtlasEntry {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    left: i32,
    top: i32,
}
struct LayerCache {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    bounds: Rect,
    revision: u64,
    root_origin: (f32, f32),
    bytes: usize,
    /// The content's extent in texture coordinates.
    uv: [f32; 2],
    /// Its raster scale over the device's, as steps of √2 (`raster_step`).
    step: i32,
    /// Promoted by a local transform rather than isolation: kept a while
    /// once back at identity (`LAYER_PARK_FRAMES`).
    transformed: bool,
    /// The frame (`GpuRenderer::layer_frame`) it was last a layer in.
    used: u64,
    /// No longer a layer, kept for its pixels.
    parked: bool,
}
/// Frames a transformed layer's texture outlives its transform, so an
/// animation passing through identity finds its pixels again instead of
/// repainting into a new allocation. The scene keeps counting the node's
/// layer revision meanwhile, so the pixels are known current.
const LAYER_PARK_FRAMES: u64 = 60;
/// Raster scales a layer renders at, in √2 steps from the device's: half
/// to four times. Beyond, a texture grows faster than detail shows.
const RASTER_STEPS: std::ops::RangeInclusive<i32> = -2..=4;
/// The raster step for content magnified `magnification` times: the
/// nearest √2 step, kept while within three quarters of a step of
/// `current`, so a scale animation re-renders a layer once per step it
/// crosses rather than every frame, and never back and forth at a boundary.
fn raster_step(magnification: f32, current: Option<i32>) -> i32 {
    if !(magnification.is_finite() && magnification > 0.) {
        return current.unwrap_or(0);
    }
    let level = 2. * magnification.log2();
    let step = match current {
        Some(step) if (level - step as f32).abs() < 0.75 => step,
        _ => level.round() as i32,
    };
    step.clamp(*RASTER_STEPS.start(), *RASTER_STEPS.end())
}
fn step_scale(step: i32) -> f32 {
    std::f32::consts::SQRT_2.powi(step)
}
#[derive(Clone)]
struct Draw {
    /// How its quads are drawn; set by `push_draw`.
    shading: Shading,
    /// A backdrop filter: its bounds, effects and corner radii.
    blur: Option<(Rect, zgui::scene::Effects, [f32; 4])>,
    start: u32,
    end: u32,
    clip: Rect,
    /// Union of this draw's quads: nothing outside it is ever touched.
    bounds: Rect,
    image: Option<u64>,
    layer: Option<NodeId>,
}
/// A frame's damage, encoded at presentation so it can be drawn into the
/// next retained target and the drawable in one pass (see `present_single_pass`).
struct Deferred {
    vertices: wgpu::Buffer,
    draws: Vec<Draw>,
    regions: Vec<(Rect, (u32, u32, u32, u32))>,
    viewport: Option<wgpu::BindGroup>,
    /// The last filter's vertical half, drawn before `draws`.
    blur: Option<VerticalBlur>,
    /// Clear each region first. False for the tail after a backdrop filter:
    /// the passes before it already cleared and drew the regions.
    clear: bool,
    /// No two regions share a pixel (`merge_damage`), so every clear can
    /// precede every draw without redrawing an overlap twice.
    disjoint: bool,
    /// Per region, the first quad to draw (see `occluded_from`).
    first: Vec<u32>,
    /// Draw into the drawable alone, leaving the retained target as it was
    /// (the next frame then repaints everything): the window changed whole
    /// this frame and the last, so the target is likely to be repainted whole
    /// again before any of it is reused. Saves writing every pixel twice.
    direct: bool,
}
/// A batch of backdrop filters' last step: each blur drawn back over the
/// original pixels, one draw per filter and damaged output (see
/// `GpuRenderer::blur`).
struct VerticalBlur {
    bind: wgpu::BindGroup,
    /// The filter (its index in the batch) and output of each draw.
    draws: Vec<(u32, Scissor)>,
    /// Drawn with `BlurPipelines::mix` (else over a copy of each area).
    mixed: bool,
}
/// The blur drawn back by dual-source blending: the blur scaled by how much
/// of it shows, and that amount for the target's share.
pub(crate) const BLUR_MIX: &str = "struct Mixed { @location(0) @blend_src(0) color:vec4<f32>, @location(0) @blend_src(1) amount:vec4<f32> }
@fragment fn fs_mix(v:Vertex)->Mixed{let b=blurs[v.index];let blend=blurred(b,v.p.xy);let a=amount(b,v.p.xy)*blend.weight;return Mixed(blend.color*a,vec4(a));}";
/// A filter's header in `blur.wgsl` (`Blur`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurRecord {
    bounds: [f32; 4],
    effect: [f32; 4],
    area: [f32; 4],
    shape: [f32; 4],
    slots: [u32; 4],
}
/// A slot of the blur atlas (`Slot` in `blur.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SlotRecord {
    atlas: [f32; 4],
    region: [f32; 4],
    owner: [u32; 4],
}
/// A batch of backdrop filters planned for the atlas (`GpuRenderer::plan_blur`).
struct BlurPlan {
    blurs: Vec<BlurRecord>,
    /// Each slot with its size in texels and whether it is blurred (else a
    /// copy of an area).
    slots: Vec<(SlotRecord, (u32, u32), bool)>,
    /// The filter and output of each draw back.
    draws: Vec<(u32, Scissor)>,
    mixed: bool,
}
/// A backdrop filter a damaged region reaches: its bounds and clip, the
/// outputs it draws, its effects and corner radii.
struct Filter {
    bounds: Rect,
    clip: Rect,
    outputs: Vec<Rect>,
    effects: zgui::scene::Effects,
    corners: [f32; 4],
}
/// Backdrop blur pipelines (blur.wgsl): average each area down into its
/// atlas slots, blur them across and down at that size, then draw them back
/// over the target, mixed by dual-source blending (`mix`) where the device
/// has it, else over a copy of the area's pixels, into the target or, at
/// presentation, the target and a drawable of `both`'s format.
struct BlurPipelines {
    shader: wgpu::ShaderModule,
    /// Shared by every pipeline so one bind group layout serves each.
    layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    reduce: wgpu::RenderPipeline,
    across: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    mix: Option<wgpu::RenderPipeline>,
    both: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}
impl BlurPipelines {
    fn new(device: &wgpu::Device) -> Self {
        // Dual-source blending needs the WGSL extension enabled for the whole
        // module, which only devices with the feature accept.
        let dual = device
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING);
        let source = if dual {
            format!(
                "enable dual_source_blending;\n{}\n{BLUR_MIX}",
                include_str!("blur.wgsl")
            )
        } else {
            include_str!("blur.wgsl").to_owned()
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("backdrop blur"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let records = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("backdrop blur"),
            entries: &[
                texture(0),
                texture(1),
                records(2),
                records(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline = |entry, format| {
            let target = Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            });
            Self::pipeline(device, &shader, Some(&layout), entry, &[target])
        };
        let (reduce, across, down, vertical) = (
            pipeline("fs_reduce", BLUR_FORMAT),
            pipeline("fs_across", BLUR_FORMAT),
            pipeline("fs_down", BLUR_FORMAT),
            pipeline("fs", FORMAT),
        );
        // Bilinear: each sample averages up to four texels (see blur.wgsl).
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("backdrop blur"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // out = blur * amount + target * (1 - amount), every channel.
        let mix = dual.then(|| {
            let component = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                operation: wgpu::BlendOperation::Add,
            };
            let target = Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: Some(wgpu::BlendState {
                    color: component,
                    alpha: component,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            });
            Self::pipeline(device, &shader, Some(&layout), "fs_mix", &[target])
        });
        Self {
            shader,
            layout,
            linear,
            reduce,
            across,
            down,
            vertical,
            mix,
            both: None,
        }
    }
    fn pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: Option<&wgpu::BindGroupLayout>,
        entry: &str,
        targets: &[Option<wgpu::ColorTargetState>],
    ) -> wgpu::RenderPipeline {
        let layout = layout.map(|layout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vertical blur"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            })
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("backdrop blur"),
            layout: layout.as_ref(),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    }
    /// The pipeline drawing `blur` back over the target.
    fn drawing(&self, blur: &VerticalBlur) -> &wgpu::RenderPipeline {
        match &self.mix {
            Some(mix) if blur.mixed => mix,
            _ => &self.vertical,
        }
    }
    /// The vertical pipeline writing the target and a `format` drawable.
    fn both(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        if self.both.as_ref().is_none_or(|(f, _)| *f != format) {
            let target = |format| {
                Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            };
            let pipeline = Self::pipeline(
                device,
                &self.shader,
                Some(&self.layout),
                "fs_both",
                &[target(FORMAT), target(format)],
            );
            self.both = Some((format, pipeline));
        }
        &self.both.as_ref().expect("created above").1
    }
}
/// Backdrop filters' scratch: two atlases at least the size a batch's slots
/// need, the blur passes rendering from one into the other.
struct BlurScratch {
    atlas: [wgpu::Texture; 2],
}
/// The spare retained target with the views and bind groups it needs.
struct Retained {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    copy_bind: Option<wgpu::BindGroup>,
    blit_bind: Option<wgpu::BindGroup>,
}
/// Pipelines writing the retained target and a drawable of `format` at once.
struct SinglePass {
    format: wgpu::TextureFormat,
    copy: wgpu::RenderPipeline,
    /// Into the next retained target and the drawable.
    both: PickedPipelines,
    /// Into the drawable alone (`Deferred::direct`).
    direct: PickedPipelines,
}
/// Presentation through surfaces of the renderer's own (`layer_present`).
#[cfg(target_os = "macos")]
#[derive(Default)]
struct LayerPresent {
    layer: Option<layer_present::Layer>,
    /// Surfaces, each with the frame it holds (None: none yet).
    buffers: Vec<(layer_present::Buffer, Option<u64>)>,
    /// The surface the layer shows: the last presented frame.
    shown: Option<usize>,
    /// The surface the next frame draws into (the retained target).
    current: Option<usize>,
    /// Frames presented so far.
    frame: u64,
    /// Pixels each recent frame changed, oldest first.
    history: std::collections::VecDeque<(u64, Vec<Scissor>)>,
    /// Pixels the frame being drawn changes so far.
    changed: Vec<Scissor>,
    /// Regions the current surface takes from the shown one before the
    /// frame draws (`begin_layer_frame`).
    catch_up: Vec<Scissor>,
    /// Frames come at the display's full rate (`GpuRenderer::set_full_rate`):
    /// a third surface is kept for them, and the layer queues them.
    full_rate: bool,
    /// Since full-rate frames stopped, until when the video layer stays
    /// (`VIDEO_LINGER`); `None` while they come, or once it went.
    video_until: Option<std::time::Instant>,
    /// When the window last changed size. Frames show on the plain layer
    /// until `RESIZE_SETTLE` after it (`Layer::show_plain`).
    resized_at: Option<std::time::Instant>,
    /// The layer to present into instead of the window's (an overlay plane).
    host: Option<objc2::rc::Retained<objc2_quartz_core::CALayer>>,
    /// Frames show on the plain layer until then (`present_plain_for`), or
    /// always (`plain_only`): they change in the caller's transaction.
    plain_until: Option<std::time::Instant>,
    plain_only: bool,
    /// Native views share the window (`present_with_native_views`): frames
    /// show on the plain layer, in the transaction that moves them.
    with_natives: bool,
}
#[cfg(target_os = "macos")]
impl LayerPresent {
    /// For `ZGUI_PRESENT_LOG`: which plane this presents.
    fn plane(&self) -> &'static str {
        if self.plain_only { "overlay" } else { "base" }
    }
    /// Per surface, whether the layer may still show it: the one on screen
    /// and any queued after it. Without word from the layer, the two latest.
    fn busy(&self) -> Vec<bool> {
        let displayed = self.layer.as_ref().and_then(|layer| layer.displayed());
        let on_screen = displayed.and_then(|id| {
            self.buffers
                .iter()
                .find(|(buffer, _)| buffer.surface.id() == id)
                .and_then(|(_, frame)| *frame)
        });
        let latest = self.frame;
        self.buffers
            .iter()
            .map(|(_, held)| match (held, on_screen) {
                (None, _) => false,
                (Some(held), Some(on_screen)) => *held >= on_screen,
                (Some(held), None) => held + 2 > latest,
            })
            .collect()
    }
}
/// Surfaces a window presents through at most: one on screen, one queued,
/// and one drawn.
#[cfg(target_os = "macos")]
const LAYER_BUFFERS: usize = 3;
/// Frames of changes kept to bring a surface up to date (older ones copy all).
#[cfg(target_os = "macos")]
const LAYER_HISTORY: usize = 8;
/// How long after a size change frames keep to the plain layer, so a live
/// resize's frames land with its window sizes before queueing resumes.
#[cfg(target_os = "macos")]
const RESIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);
/// How long the video layer outlasts full-rate frames (`LayerPresent::video_until`).
#[cfg(target_os = "macos")]
const VIDEO_LINGER: std::time::Duration = std::time::Duration::from_secs(1);
/// A pass's pipelines for each shading, and the one clearing a region.
struct PickedPipelines {
    full: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
    basic: wgpu::RenderPipeline,
    opaque: wgpu::RenderPipeline,
}
impl PickedPipelines {
    fn picks(&self) -> Picks<'_> {
        Picks {
            full: &self.full,
            clear: &self.clear,
            basic: &self.basic,
            opaque: &self.opaque,
        }
    }
}
/// Pipelines for each shading, and the one clearing a region (see
/// `GpuRenderer::encode_damage`).
#[derive(Clone, Copy)]
struct Picks<'p> {
    full: &'p wgpu::RenderPipeline,
    clear: &'p wgpu::RenderPipeline,
    basic: &'p wgpu::RenderPipeline,
    opaque: &'p wgpu::RenderPipeline,
}
impl<'p> Picks<'p> {
    fn get(&self, shading: Shading) -> &'p wgpu::RenderPipeline {
        match shading {
            Shading::Full | Shading::Fill(_) => self.full,
            Shading::Basic => self.basic,
            Shading::Opaque => self.opaque,
        }
    }
}
/// A run of draws encoded as one instanced call.
struct Batch {
    rect: Rect,
    source: (Option<u64>, Option<NodeId>),
    shading: Shading,
    start: u32,
    end: u32,
    /// Device pixels its draws cover, and of them those drawn with a
    /// costlier shader than their own (see `merged_shading`).
    pixels: f32,
    promoted: f32,
}
/// Device pixels a batch may draw with a costlier shader than they need, to
/// join neighbours of another shading into one draw call: a draw costs the
/// driver thousands of instructions, shading a small area costs little.
/// Blending opaque quads costs the GPU little more.
const PROMOTED_PIXELS: f32 = 16_384.;
/// As `PROMOTED_PIXELS`, into the full shader, whose cost per pixel is a
/// multiple of the basic one's: at 16K a batch, text promoted beside its
/// rounded panels raised the heavy dashboard's GPU cycles 5 to 8%. Icons,
/// dots and chips still join. (An early return in `shade` for plain quads,
/// computing `basic`'s colour, does not make promotion cheap: the cost is
/// the full shader's registers and flat varyings, which a branch keeps,
/// and the test slowed the quads that need it. Measured: headless GPU time
/// per frame, heavy stream +3.5% with promotion, heavy both's own +1.3%.)
const PROMOTED_FULL_PIXELS: f32 = 1024.;
/// The shading drawing both `a` and `b`, if one can: the full shader draws
/// every basic quad, and blending every opaque one, to the same bytes (see
/// `shading`). Shader fills keep their own pipeline.
fn merged_shading(a: Shading, b: Shading) -> Option<Shading> {
    let cost = |shading| match shading {
        Shading::Opaque => Some(0),
        Shading::Basic => Some(1),
        Shading::Full => Some(2),
        Shading::Fill(_) => None,
    };
    Some(if cost(a)? >= cost(b)? { a } else { b })
}
fn quad_bounds(quads: &[Quad]) -> Rect {
    quads
        .iter()
        .map(quad_rect)
        .reduce(|a, b| a.union(b))
        .unwrap_or_default()
}
/// A quad's rectangle. Bands of a split panel measure back from its far
/// edge (`push_panel`): a negative width or height.
fn quad_rect(quad: &Quad) -> Rect {
    let [x, y, w, h] = quad.rect;
    let rect = Rect::new(x.min(x + w), y.min(y + h), w.abs(), h.abs());
    if quad.options[2] < 0.5 {
        return rect;
    }
    // Transformed (an affine image or layer): where its corners land.
    let [a, c, b, d] = quad.shape;
    let [tx, ty, ..] = quad.border;
    zgui::affine::Affine { a, b, c, d, tx, ty }.bounds(rect)
}
/// Shapes using exactly the same font selection and line metrics as the renderer.
pub fn measure_text(
    fonts: &mut FontSystem,
    text: &str,
    size: f32,
    width: Option<f32>,
) -> (f32, f32) {
    text::ShapedText::new(fonts, text, size, width).size()
}
/// The GPU an interface draws on: the integrated one of a machine with two,
/// which keeps a laptop's discrete GPU asleep.
const POWER: wgpu::PowerPreference = wgpu::PowerPreference::LowPower;
/// The adapter DXGI ranks first for `POWER`, a GPU before software, with a
/// device opened on it alone (none when Direct3D 12 isn't the instance's
/// backend). wgpu's
/// `request_adapter` opens a Direct3D 12 device on every adapter to rank them,
/// and each driver keeps its memory after the device is released: 20 MiB for
/// a second adapter (an indirect display's) on an NVIDIA machine.
#[cfg(windows)]
fn preferred_dx12_adapter(instance: &wgpu::Instance) -> Option<wgpu::Adapter> {
    // SAFETY: the hal instance is only read (DXGI's adapters listed, one
    // exposed), never destroyed or changed.
    let hal = unsafe { instance.as_hal::<wgpu::hal::api::Dx12>() }?;
    let exposed = hal.expose_preferred_adapter(POWER)?;
    // SAFETY: the adapter was exposed from this instance's own hal instance.
    Some(unsafe { instance.create_adapter_from_hal(exposed) })
}
/// What a `GpuContext` holds that another thread can make (`GpuContext::prepare`).
struct Prepared {
    lost: Arc<std::sync::Mutex<Option<String>>>,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    fonts: FontSystem,
    texture_layout: wgpu::BindGroupLayout,
    viewport_layout: wgpu::BindGroupLayout,
    quad_shader: wgpu::ShaderModule,
    quad_layout: wgpu::PipelineLayout,
    mapped_uploads: bool,
}
/// A `GpuContext` being made on another thread (`GpuContext::prepare_in_background`).
pub struct PendingGpuContext(Option<std::thread::JoinHandle<Result<Prepared, GpuError>>>);
impl PendingGpuContext {
    /// The context, once made: waits for it if it is not done yet. Errors
    /// as `GpuContext::new` does (no thread to make it on: a `GpuError` too).
    pub fn join(self) -> Result<GpuContext, GpuError> {
        let handle = self
            .0
            .ok_or_else(|| GpuError("no thread to make the GPU context on".into()))?;
        let prepared = handle
            .join()
            .map_err(|_| GpuError("making the GPU context panicked".into()))??;
        Ok(GpuContext::finish(prepared))
    }
}
/// Shared native GPU device, pipelines, and font database. Clone this cheaply for
/// additional windows; render targets/atlases remain owned by each renderer.
#[derive(Clone)]
pub struct GpuContext {
    inner: Rc<GpuContextInner>,
}
struct GpuContextInner {
    /// Why the device was lost (driver reset, removal, `debug_lose_device`):
    /// set from wgpu's callback, from any thread.
    lost: Arc<std::sync::Mutex<Option<String>>>,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    fonts: Rc<RefCell<FontSystem>>,
    /// Shaped text shared by layout measurement and every renderer.
    text_cache: Rc<RefCell<prepared::TextCache>>,
    quads: QuadPipelines,
    /// Bind group layouts of the quad pipelines: texture and sampler, viewport.
    texture_layout: wgpu::BindGroupLayout,
    viewport_layout: wgpu::BindGroupLayout,
    /// Vertex and small uniform uploads are CPU-mapped instead of copied.
    mapped_uploads: bool,
    /// For dual-output (single-pass presentation) pipelines.
    quad_shader: wgpu::ShaderModule,
    quad_layout: wgpu::PipelineLayout,
}
impl GpuContext {
    /// A context on the system's GPU, found without a window.
    pub fn new() -> Result<Self, GpuError> {
        Self::create(None)
    }
    fn create(window: Option<Arc<winit::window::Window>>) -> Result<Self, GpuError> {
        Ok(Self::finish(Self::prepare(window)?))
    }
    /// Start making a context on another thread (the device, its pipelines
    /// and the system fonts take a few milliseconds), to take with
    /// `PendingGpuContext::join` once a window needs it. Found without a
    /// window: a host that knows any adapter can draw to its windows (macOS)
    /// starts this while its windows are still being made.
    pub fn prepare_in_background() -> PendingGpuContext {
        PendingGpuContext(
            std::thread::Builder::new()
                .name("zgui GPU setup".into())
                .spawn(|| Self::prepare(None))
                .ok(),
        )
    }
    /// What a context is made of that can be made on any thread.
    fn prepare(window: Option<Arc<winit::window::Window>>) -> Result<Prepared, GpuError> {
        // Avoid initializing secondary graphics stacks merely to enumerate
        // adapters. On Linux this can create an unused EGL driver and worker
        // pool even when Vulkan is selected; macOS uses Metal. Windows uses
        // Direct3D 12 alone: it presents through DXGI's flip model, which the
        // compositor schedules, and leaves no Vulkan driver loaded beside it
        // (on NVIDIA, three quarters of the idle CPU and 18 MiB of working
        // set). Vulkan, with the `vulkan` feature, stands in only where
        // Direct3D 12 has no adapter. On Linux, GL stands in where no Vulkan
        // driver is (VMs and older GPUs without one): made only then, so a
        // Vulkan system loads no EGL.
        let attempts: &[wgpu::Backends] = if cfg!(all(windows, feature = "vulkan")) {
            &[wgpu::Backends::DX12, wgpu::Backends::VULKAN]
        } else if cfg!(windows) {
            &[wgpu::Backends::DX12]
        } else if cfg!(target_os = "linux") {
            &[wgpu::Backends::PRIMARY, wgpu::Backends::GL]
        } else {
            &[wgpu::Backends::PRIMARY]
        };
        let mut found = Err(GpuError("no graphics backend".into()));
        for &backends in attempts {
            let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = backends;
            // Windows: swap chains present through a DirectComposition
            // visual, whose alpha the compositor blends (transparent and
            // glass windows), where a window's own swap chain is opaque.
            descriptor.backend_options.dx12.presentation_system =
                wgpu::Dx12SwapchainKind::DxgiFromVisual;
            // zgui draws nothing indirect; the validation would compile a compute
            // shader while creating the device, a few milliseconds of startup.
            descriptor
                .flags
                .remove(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
            // EGL draws to the window's display, which it must be given.
            if backends == wgpu::Backends::GL
                && let Some(window) = &window
            {
                descriptor = descriptor.with_display_handle(Box::new(window.clone()));
            }
            let instance = wgpu::Instance::new(descriptor.with_env());
            // No driver for these backends: the next.
            let surface = match window
                .clone()
                .map(|window| instance.create_surface(window))
                .transpose()
            {
                Ok(surface) => surface,
                Err(error) => {
                    found = Err(GpuError(error.to_string()));
                    continue;
                }
            };
            #[cfg(windows)]
            let preferred = preferred_dx12_adapter(&instance).filter(|adapter| {
                surface
                    .as_ref()
                    .is_none_or(|surface| adapter.is_surface_supported(surface))
            });
            #[cfg(not(windows))]
            let preferred = None;
            let adapter = match preferred {
                Some(adapter) => Ok(adapter),
                None => {
                    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: POWER,
                        compatible_surface: surface.as_ref(),
                        force_fallback_adapter: false,
                    }))
                }
            };
            found = adapter
                .map(|adapter| (instance, adapter))
                .map_err(|e| GpuError(e.to_string()));
            if found.is_ok() {
                break;
            }
        }
        let (instance, adapter) = found?;
        let info = adapter.get_info();
        // CPU-written vertex buffers avoid a GPU copy per frame; only worth it
        // where CPU and GPU share memory.
        let mapped_uploads = info.device_type == wgpu::DeviceType::IntegratedGpu
            && adapter
                .features()
                .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS);
        zgui::profile::mark("GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("zgui shared device"),
            required_features: if mapped_uploads {
                wgpu::Features::MAPPABLE_PRIMARY_BUFFERS
            } else {
                wgpu::Features::empty()
            } | (adapter.features() & wgpu::Features::DUAL_SOURCE_BLENDING),
            required_limits: wgpu::Limits {
                // Direct3D 12 makes its shader-visible view heap this large, in
                // write-combined memory: 30 MiB at the default million, 2 MiB
                // here. An image holds one view and a window holds at most
                // `MAX_IMAGES`: windows share the device, and four at their
                // budget still fit beside their other bind groups.
                max_non_sampler_bindings: if cfg!(windows) {
                    1 << 16
                } else {
                    wgpu::Limits::default().max_non_sampler_bindings
                },
                // GL (Linux's fallback) asks no more than older GPUs give,
                // with the textures as large as this one makes.
                ..if info.backend == wgpu::Backend::Gl {
                    wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())
                } else {
                    Default::default()
                }
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(|e| GpuError(e.to_string()))?;
        // wgpu clears a buffer of its own while creating the device and holds
        // that command buffer until the first submission. Finish it now, so
        // it is never alive beside a frame's: Metal keeps memory for as many
        // command buffers as were ever alive at once, for good.
        queue.submit([]);
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        // A lost device (a driver reset, a GPU removed) fails what follows
        // instead of panicking, as wgpu's default handler does: renders
        // then return `GpuError::is_device_lost`, and the host makes a new
        // renderer. Other errors are logged; a validation error is a bug,
        // which still panics in debug builds.
        let lost: Arc<std::sync::Mutex<Option<String>>> = Arc::default();
        {
            let lost = lost.clone();
            device.set_device_lost_callback(move |reason, message| {
                let mut slot = lost.lock().unwrap_or_else(|p| p.into_inner());
                if slot.is_none() {
                    if message.is_empty() {
                        eprintln!("zgui: the GPU device was lost ({reason:?})");
                    } else {
                        eprintln!("zgui: the GPU device was lost ({reason:?}): {message}");
                    }
                    *slot = Some(if message.is_empty() {
                        format!("{reason:?}")
                    } else {
                        message
                    });
                }
            });
        }
        {
            let lost = lost.clone();
            device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
                if lost.lock().map_or(true, |lost| lost.is_some()) {
                    return;
                }
                if cfg!(debug_assertions) && matches!(error, wgpu::Error::Validation { .. }) {
                    panic!("wgpu: {error}");
                }
                eprintln!("zgui: GPU error: {error}");
            }));
        }
        let (texture_layout, viewport_layout, quad_shader, quad_layout) =
            create_quad_pipelines(&device);
        zgui::profile::mark("GPU device and pipelines");
        let fonts = font_cache::system_fonts();
        zgui::profile::mark("system fonts");
        Ok(Prepared {
            lost,
            instance,
            adapter,
            device,
            queue,
            info,
            fonts,
            texture_layout,
            viewport_layout,
            quad_shader,
            quad_layout,
            mapped_uploads,
        })
    }
    /// The context of what `prepare` made, on the thread that uses it.
    fn finish(prepared: Prepared) -> Self {
        let Prepared {
            lost,
            instance,
            adapter,
            device,
            queue,
            info,
            fonts,
            texture_layout,
            viewport_layout,
            quad_shader,
            quad_layout,
            mapped_uploads,
        } = prepared;
        let quads = QuadPipelines {
            device: device.clone(),
            shader: quad_shader.clone(),
            layout: quad_layout.clone(),
            full: Default::default(),
            full_clear: Default::default(),
            basic: Default::default(),
            basic_clear: Default::default(),
        };
        Self {
            inner: Rc::new(GpuContextInner {
                lost,
                instance,
                adapter,
                device,
                queue,
                info,
                fonts: Rc::new(RefCell::new(fonts)),
                text_cache: Default::default(),
                quads,
                texture_layout,
                viewport_layout,
                quad_shader,
                quad_layout,
                mapped_uploads,
            }),
        }
    }
    /// Why its device was lost, once it is: every renderer on it then
    /// fails with `GpuError::is_device_lost`, and a new context is needed.
    pub fn lost(&self) -> Option<String> {
        self.inner
            .lost
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    /// Lose the device, as a driver reset would: for tests of recovery.
    pub fn debug_lose_device(&self) {
        self.inner.device.destroy();
        let _ = self.inner.device.poll(wgpu::PollType::Poll);
        let mut lost = self.inner.lost.lock().unwrap_or_else(|p| p.into_inner());
        lost.get_or_insert_with(|| "destroyed for a test".into());
    }
    /// Whether `other` is this context (a clone of it), on the same device.
    pub fn shares_device(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
    /// The font system its renderers shape and draw text with.
    pub fn text_system(&self) -> Rc<RefCell<FontSystem>> {
        self.inner.fonts.clone()
    }
    /// Prepared (shaped once) rich text, shared with the renderers.
    pub fn text_cache(&self) -> Rc<RefCell<prepared::TextCache>> {
        self.inner.text_cache.clone()
    }
}
impl GpuError {
    /// The device was lost (`GpuContext::lost`): `reason` says why.
    pub fn device_lost(reason: &str) -> Self {
        Self(format!("GPU device lost: {reason}"))
    }
    /// Whether the device was lost: drop the renderer and make another on
    /// a new context.
    pub fn is_device_lost(&self) -> bool {
        self.0.starts_with("GPU device lost")
    }
    /// Whether a window's surface can't be drawn with the context's adapter
    /// (make a context for that window instead).
    pub fn is_surface_incompatible(&self) -> bool {
        self.0 == "surface is incompatible with the shared GPU adapter"
    }
}
/// Draws a [`zgui::scene::Scene`] with wgpu (Metal, Direct3D 12 or Vulkan),
/// repainting only its damage, to an offscreen target or a window.
pub struct GpuRenderer {
    context: GpuContext,
    scene_identity: Option<u64>,
    /// Scene content revision the per-node caches were last pruned at.
    pruned_revision: Option<u64>,
    /// The last render's still-open encoder. Present records its blit into it,
    /// so a frame is one command buffer and one submission. Flushed before
    /// anything that rewrites buffers or textures the commands read.
    pending: RefCell<Option<wgpu::CommandEncoder>>,
    /// Viewport uniforms by logical size (f32 bits). Each is written once at
    /// creation; layers and the window switch between them per pass.
    viewports: FxHashMap<(u32, u32, u32), (wgpu::Buffer, wgpu::BindGroup)>,
    /// Bound as the clip regions of frames without any (`Region`).
    no_regions: wgpu::Buffer,
    /// The raster each canvas, decoration or SVG node currently draws. A
    /// resized or restyled node gets a new raster; the old texture goes at
    /// once instead of waiting for a structural prune.
    node_images: FxHashMap<NodeId, u64>,
    /// Gradient ramps (see `axis_gradient`) by their stops, and the ramp
    /// each node draws, so a ramp lives while a node shows it.
    ramps: FxHashMap<RampKey, Arc<zgui::image::ImageData>>,
    node_ramps: FxHashMap<NodeId, RampKey>,
    /// SVG sources that failed to draw, reported once each.
    failed_svgs: rustc_hash::FxHashSet<u64>,
    /// The viewport of the target currently being rendered.
    viewport_bind: Option<wgpu::BindGroup>,
    /// Reused upload memory for per-frame vertices; `write_buffer` would
    /// allocate and free a staging buffer every frame.
    belt: RefCell<wgpu::util::StagingBelt>,
    /// Replaces the belt on unified-memory adapters.
    mapped: Option<RefCell<upload::MappedRing>>,
    /// Texture writes awaiting the next render's upload passes (mapped adapters).
    texture_uploads: upload::TextureUploads,
    instance: wgpu::Instance,
    window: Option<Arc<winit::window::Window>>,
    /// A surface of the caller's, not a winit window's (`for_raw_surface`).
    raw: Option<(wgpu::rwh::RawDisplayHandle, wgpu::rwh::RawWindowHandle)>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: wgpu::AdapterInfo,
    surface: Option<wgpu::Surface<'static>>,
    /// DXGI's account of shown presents (`ZGUI_PRESENT_LOG`).
    #[cfg(windows)]
    dxgi_stats: dxgi::PresentStats,
    /// The caller's visual the surface presents into (composition.rs).
    #[cfg(all(windows, feature = "composition"))]
    composition: Option<composition::Target>,
    config: Option<wgpu::SurfaceConfiguration>,
    width: u32,
    height: u32,
    scale: f32,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    /// Single-pass presentation alternates two retained targets: each frame
    /// copies `target` and draws its damage into `spare` and the drawable in
    /// one render pass, then swaps. Two passes (damage, then a present blit)
    /// cost a second Metal command buffer per frame. Dropped by `trim`.
    spare: Option<Retained>,
    /// Bind group sampling `target` for the single-pass copy; swapped with
    /// the spare's so neither is rebuilt per frame.
    target_copy_bind: Option<wgpu::BindGroup>,
    deferred: RefCell<Option<Deferred>>,
    single_pass: Option<SinglePass>,
    /// Non-sRGB view format of the drawable. Through it, drawing directly
    /// stores the same bytes the present blit would for opaque windows (whose
    /// pixels all have alpha 1, so post-multiplied presentation is a no-op).
    single_pass_format: Option<wgpu::TextureFormat>,
    /// Rendering an isolated layer's texture, never presented directly.
    in_layer: bool,
    /// The present blit converts to straight alpha (see `unpremultiplies`).
    /// Single-pass presentation then only matches for opaque windows.
    present_unpremultiplies: bool,
    /// Present by copying the retained target into the drawable: same
    /// format, no alpha conversion. A copy is a memcpy for a software
    /// rasterizer, where a full-window shader pass costs as much as drawing.
    copy_present: bool,
    /// Frames whose damage was drawn by `present_single_pass`.
    single_pass_frames: u64,
    /// Frames presented to the drawable alone (`Deferred::direct`).
    direct_frames: u64,
    /// Frames encoded with Metal directly (`render_direct`).
    metal_frames: u64,
    /// Work encoded at presentation (single-pass frames), reported with the
    /// next `render`'s statistics.
    present_stats: GpuStats,
    /// Quads use the shader their features need (see `push_draw`).
    split_shading: bool,
    /// A text partly in its clip makes quads for its visible lines alone
    /// (`debug_cull_glyphs`).
    cull_glyphs: bool,
    bind: wgpu::BindGroup,
    blit: Option<wgpu::RenderPipeline>,
    blit_bind: Option<wgpu::BindGroup>,
    atlas: wgpu::Texture,
    atlas_size: u32,
    entries: FxHashMap<CacheKey, Option<AtlasEntry>>,
    cursor: (u32, u32, u32),
    fonts: Rc<RefCell<FontSystem>>,
    text_cache: Rc<RefCell<prepared::TextCache>>,
    swash: SwashCache,
    layers: FxHashMap<NodeId, LayerCache>,
    /// Counts rendered frames, to age parked layers (`LAYER_PARK_FRAMES`).
    layer_frame: u64,
    layer_origin: (f32, f32),
    shapes: FxHashMap<NodeId, Shaped>,
    shaped_bytes: usize,
    fresh: bool,
    /// Consecutive presented frames whose damage covered the window.
    whole_frames: u32,
    /// The scene flush the retained target was last rendered from.
    rendered_serial: Option<u64>,
    /// Scroll copies for the next root frame: destination pixel rectangle
    /// and the source offset (see `scroll_copies`).
    copies: Vec<ScrollCopy>,
    atlas_epoch: u64,
    frame: u64,
    vertices: wgpu::Buffer,
    vertex_capacity: usize,
    frame_quads: Vec<Quad>,
    /// The frame's draws, kept for their allocation.
    frame_draws: Vec<Draw>,
    /// The quads of the lines of a text partly in its clip, made each frame
    /// (see `render_flat`), kept for their allocation.
    visible_glyphs: Vec<Quad>,
    background: Color,
    /// Image textures: each bound for the quads, its size in bytes, and,
    /// where the CPU wrote its texels, the texture (see `render_direct`).
    images: FxHashMap<u64, (wgpu::BindGroup, usize, Option<wgpu::Texture>)>,
    #[cfg(target_os = "macos")]
    native_surfaces: Option<native_surface::SurfaceCache>,
    /// The next present joins the Core Animation transaction that resized the layer.
    #[cfg(target_os = "macos")]
    transaction_present: bool,
    /// Until when the layer keeps a third drawable after a resize (see
    /// `widen_drawables`).
    #[cfg(target_os = "macos")]
    wide_until: Option<std::time::Instant>,
    /// Presentation through surfaces of the renderer's own (layer_present.rs).
    #[cfg(target_os = "macos")]
    layer_present: Option<LayerPresent>,
    /// Pipelines for frames encoded with Metal directly (`render_direct`);
    /// `None` inside if they could not be built.
    #[cfg(target_os = "macos")]
    metal_frame: std::cell::OnceCell<Option<metal_frame::MetalFrame>>,
    /// A directly encoded frame, committed, until it is known complete.
    #[cfg(target_os = "macos")]
    metal_inflight: RefCell<Option<metal_frame::CommandBuffer>>,
    /// How long the GPU took to run the last direct frame (`take_gpu_time`).
    #[cfg(target_os = "macos")]
    gpu_time: std::cell::Cell<Option<std::time::Duration>>,
    /// The viewport uniform `select_viewport` chose last, for direct frames.
    #[cfg(target_os = "macos")]
    viewport_values: [f32; 4],
    /// Shifted texel copies drawn in a render pass (copy_offset.wgsl).
    #[cfg(target_os = "macos")]
    copy_offset: Option<wgpu::RenderPipeline>,
    canvases: canvas::CanvasCache,
    svgs: svg::SvgCache,
    /// Small untransformed SVG rasters packed into the glyph atlas by image
    /// id, so icons batch with text instead of binding a texture each.
    image_cells: FxHashMap<u64, (u32, u32)>,
    /// Frame of the last atlas recycle, to detect thrashing.
    atlas_recycled_at: u64,
    /// Compiled on the first backdrop filter.
    blur_pipelines: Option<BlurPipelines>,
    /// Backdrop blur scratch, shared by the window and every isolated layer.
    blur_scratch: Option<BlurScratch>,
    /// Blurred and masked layers' slots (effects.rs), made with the first.
    effects: Option<effects::EffectAtlas>,
    effect_pipelines: Option<effects::EffectPipelines>,
    /// Slots the next `render_flat` draws before compositing them.
    effect_jobs: Vec<effects::Job>,
    /// Shader fills' pipelines by shader id (`None`: it failed to compile),
    /// and the shaders, for frames drawn with Metal directly.
    fill_pipelines: FxHashMap<u64, Option<wgpu::RenderPipeline>>,
    fill_shaders: FxHashMap<u64, Arc<zgui::decoration::FillShader>>,
    /// The presentation plane drawn around native views (planes.rs).
    plane: Option<planes::PlaneState>,
}
impl GpuRenderer {
    /// An offscreen renderer `width` by `height` physical pixels, on a context
    /// of its own.
    pub fn new(width: u32, height: u32) -> Result<Self, GpuError> {
        let context = GpuContext::new()?;
        Self::create(None, width, height, &context)
    }
    /// A renderer for `window`, on a context made for it, at its size and
    /// scale.
    pub fn for_window(window: Arc<winit::window::Window>) -> Result<Self, GpuError> {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let context = GpuContext::create(Some(window.clone()))?;
        let mut renderer = Self::create(Some(window), size.width, size.height, &context)?;
        renderer.set_scale_factor(scale);
        Ok(renderer)
    }
    /// Its context, to share with other renderers.
    pub fn context(&self) -> GpuContext {
        self.context.clone()
    }
    /// An offscreen renderer `width` by `height` physical pixels on `context`.
    pub fn new_with_context(
        width: u32,
        height: u32,
        context: &GpuContext,
    ) -> Result<Self, GpuError> {
        Self::create(None, width, height, context)
    }
    /// A renderer for `window` on `context`, at its size and scale.
    pub fn for_window_with_context(
        window: Arc<winit::window::Window>,
        context: &GpuContext,
    ) -> Result<Self, GpuError> {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let mut renderer = Self::create(Some(window), size.width, size.height, context)?;
        renderer.set_scale_factor(scale);
        Ok(renderer)
    }
    /// A renderer presenting to a surface of the caller's, not a winit
    /// window's (a Wayland popup's `wl_surface`), `width` by `height`
    /// physical pixels at `scale`, on `context`.
    ///
    /// # Safety
    ///
    /// `display` and `window` are live objects a surface can be made on (a
    /// `wl_display` and a `wl_surface` of it, say), and stay so until the
    /// renderer is dropped: it makes its surface from them again whenever
    /// the surface is lost (`wgpu::SurfaceTargetUnsafe::RawHandle`'s
    /// requirements, for each surface).
    pub unsafe fn for_raw_surface(
        display: wgpu::rwh::RawDisplayHandle,
        window: wgpu::rwh::RawWindowHandle,
        width: u32,
        height: u32,
        scale: f32,
        context: &GpuContext,
    ) -> Result<Self, GpuError> {
        // SAFETY: the caller keeps the handles valid while the renderer,
        // and so the surface, lives. The instance was made without a display
        // handle, so `display` need not match one.
        let surface = unsafe {
            context
                .inner
                .instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(display),
                    raw_window_handle: window,
                })
        }
        .map_err(|e| GpuError(e.to_string()))?;
        let mut renderer = Self::create_on(None, Some(surface), width, height, context)?;
        renderer.raw = Some((display, window));
        renderer.set_scale_factor(scale);
        Ok(renderer)
    }
    fn create(
        window: Option<Arc<winit::window::Window>>,
        width: u32,
        height: u32,
        context: &GpuContext,
    ) -> Result<Self, GpuError> {
        let surface = window
            .clone()
            .map(|window| context.inner.instance.create_surface(window))
            .transpose()
            .map_err(|e| GpuError(e.to_string()))?;
        Self::create_on(window, surface, width, height, context)
    }
    /// `create` presenting to `surface` (none: offscreen).
    fn create_on(
        window: Option<Arc<winit::window::Window>>,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
        context: &GpuContext,
    ) -> Result<Self, GpuError> {
        let instance = context.inner.instance.clone();
        let device = context.inner.device.clone();
        let queue = context.inner.queue.clone();
        let adapter = &context.inner.adapter;
        let info = context.inner.info.clone();
        if surface
            .as_ref()
            .is_some_and(|surface| !adapter.is_surface_supported(surface))
        {
            return Err(GpuError(
                "surface is incompatible with the shared GPU adapter".into(),
            ));
        }
        let width = width.max(1);
        let height = height.max(1);
        let config = surface.as_ref().map(|s| {
            let mut c = s
                .get_default_config(adapter, width, height)
                .expect("supported surface");
            let caps = s.get_capabilities(adapter);
            c.alpha_mode = [
                wgpu::CompositeAlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::Inherit,
                wgpu::CompositeAlphaMode::Opaque,
            ]
            .into_iter()
            .find(|mode| caps.alpha_modes.contains(mode))
            .unwrap_or(c.alpha_mode);
            c.present_mode = wgpu::PresentMode::AutoVsync;
            // Windows: a present replaces one the compositor has not taken
            // yet (DXGI's flip model at sync interval 0 composes the newest,
            // without tearing), so input drawn between refreshes shows on
            // the next; frames are paced by the compositor's clock instead.
            if cfg!(windows) && caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
                c.present_mode = wgpu::PresentMode::Mailbox;
            }
            // Two drawables: measured no frame-rate or CPU cost against three
            // on Apple silicon, and one fewer window-sized buffer (~13 MB at
            // 5K) while animating. Resizing keeps a third (`widen_drawables`).
            // Windows keeps the default two: its presents replace one another
            // (above), and a second within a refresh must not wait for the
            // compositor to take the first.
            if cfg!(target_os = "macos") {
                c.desired_maximum_frame_latency = 1;
            }
            // The retained target already holds sRGB-encoded bytes, which the
            // compositor reads the same from either format. A non-sRGB drawable
            // takes them as they are, where an sRGB one needs a per-frame
            // aliasing view and cannot be framebuffer-only.
            let plain = c.format.remove_srgb_suffix();
            if caps.formats.contains(&plain) {
                c.format = plain;
            } else if c.format.is_srgb() {
                c.view_formats = vec![plain];
            }
            // Off macOS, where drawables must stay framebuffer-only to be
            // cheap, present by copy when the surface allows it.
            if !cfg!(target_os = "macos")
                && c.format == FORMAT
                && !unpremultiplies(&c)
                && caps.usages.contains(wgpu::TextureUsages::COPY_DST)
            {
                c.usage |= wgpu::TextureUsages::COPY_DST;
            }
            s.configure(&device, &c);
            c
        });
        let copy_present = config
            .as_ref()
            .is_some_and(|c| c.usage.contains(wgpu::TextureUsages::COPY_DST));
        // A window on macOS presents surfaces of its own (see `LayerPresent`).
        let layer_presenting =
            cfg!(target_os = "macos") && config.is_some() && info.backend == wgpu::Backend::Metal;
        // Single-pass presentation writes the drawable in a non-sRGB format,
        // storing the retained target's bytes unchanged, exactly what the
        // present blit produces for opaque pixels.
        let single_pass_format = config
            .as_ref()
            .filter(|_| !layer_presenting)
            .map(|c| c.format.remove_srgb_suffix());
        // Presenting surfaces of its own, each frame draws into one of them
        // (`begin_layer_frame`): a window-sized target would only be dropped,
        // and the driver keeps the memory it held.
        let target = if layer_presenting {
            texture(&device, 1, 1, "retained output placeholder")
        } else {
            texture(&device, width, height, "retained output")
        };
        let view = target.create_view(&Default::default());
        #[cfg(target_os = "macos")]
        if !layer_presenting {
            written(&device, &queue, &view);
        }
        let atlas = sampled_texture(
            &device,
            context.inner.mapped_uploads,
            1,
            1,
            "lazy glyph atlas placeholder",
        );
        let split_shading = true;
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas"),
            layout: &context.inner.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &atlas.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(
                        &device.create_sampler(&wgpu::SamplerDescriptor::default()),
                    ),
                },
            ],
        });
        let present_unpremultiplies = config.as_ref().is_some_and(unpremultiplies);
        // Presenting surfaces of its own, the target is never blitted (and a
        // blit bind group would keep the first one alive).
        let blit = config
            .as_ref()
            .filter(|_| !layer_presenting)
            .map(|c| blit_pipeline(&device, c.format, unpremultiplies(c)));
        let blit_bind = blit.as_ref().map(|p| blit_group(&device, p, &view));
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("retained frame vertices"),
            size: 4096,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Ok(Self {
            context: context.clone(),
            #[cfg(windows)]
            dxgi_stats: Default::default(),
            #[cfg(all(windows, feature = "composition"))]
            composition: None,
            scene_identity: None,
            pruned_revision: None,
            pending: RefCell::new(None),
            viewports: FxHashMap::default(),
            no_regions: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("no clip regions"),
                size: std::mem::size_of::<Region>() as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }),
            node_images: FxHashMap::default(),
            ramps: FxHashMap::default(),
            node_ramps: FxHashMap::default(),
            failed_svgs: Default::default(),
            viewport_bind: None,
            belt: RefCell::new(wgpu::util::StagingBelt::new(device.clone(), 256 * 1024)),
            mapped: context
                .inner
                .mapped_uploads
                .then(|| RefCell::new(upload::MappedRing::new(&device, &queue))),
            texture_uploads: Default::default(),
            instance,
            window,
            raw: None,
            device,
            queue,
            info,
            surface,
            config,
            width,
            height,
            scale: 1.,
            target,
            view,
            split_shading,
            cull_glyphs: true,
            bind,
            blit,
            blit_bind,
            spare: None,
            target_copy_bind: None,
            deferred: RefCell::new(None),
            single_pass: None,
            single_pass_format,
            #[cfg(target_os = "macos")]
            layer_present: layer_presenting.then(LayerPresent::default),
            #[cfg(target_os = "macos")]
            metal_frame: Default::default(),
            #[cfg(target_os = "macos")]
            metal_inflight: RefCell::new(None),
            #[cfg(target_os = "macos")]
            gpu_time: std::cell::Cell::new(None),
            #[cfg(target_os = "macos")]
            viewport_values: [0.; 4],
            #[cfg(target_os = "macos")]
            copy_offset: None,
            in_layer: false,
            present_unpremultiplies,
            copy_present,
            single_pass_frames: 0,
            direct_frames: 0,
            metal_frames: 0,
            present_stats: GpuStats::default(),
            atlas,
            atlas_size: 1,
            entries: FxHashMap::default(),
            cursor: (0, 0, 0),
            fonts: context.text_system(),
            text_cache: context.text_cache(),
            swash: SwashCache::new(),
            layers: FxHashMap::default(),
            layer_frame: 0,
            layer_origin: (0., 0.),
            shapes: FxHashMap::default(),
            shaped_bytes: 0,
            fresh: true,
            whole_frames: 0,
            rendered_serial: None,
            copies: Vec::new(),
            atlas_epoch: 1,
            frame: 0,
            vertices,
            vertex_capacity: 4096,
            frame_quads: Vec::new(),
            frame_draws: Vec::new(),
            visible_glyphs: Vec::new(),
            background: Color(0, 0, 0, 0),
            images: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            native_surfaces: None,
            #[cfg(target_os = "macos")]
            transaction_present: false,
            #[cfg(target_os = "macos")]
            wide_until: None,
            canvases: Default::default(),
            svgs: Default::default(),
            blur_pipelines: None,
            image_cells: FxHashMap::default(),
            // As if last recycled long ago: the first fill recycles.
            atlas_recycled_at: 0_u64.wrapping_sub(ATLAS_THRASH_FRAMES),
            blur_scratch: None,
            effects: None,
            effect_pipelines: None,
            effect_jobs: Vec::new(),
            fill_pipelines: FxHashMap::default(),
            fill_shaders: FxHashMap::default(),
            plane: None,
        })
    }
    /// Bytes the native device reports as allocated (Metal only), including
    /// driver-owned resources that the cache counters cannot see.
    pub fn debug_device_allocated_bytes(&self) -> Option<u64> {
        #[cfg(target_os = "macos")]
        {
            use objc2_metal::MTLDevice;
            // SAFETY: The raw device is only queried; nothing destroys it.
            let metal = unsafe { self.device.as_hal::<wgpu::hal::api::Metal>() }?;
            Some(metal.raw_device().currentAllocatedSize() as u64)
        }
        #[cfg(not(target_os = "macos"))]
        None
    }
    /// What its caches hold now.
    pub fn debug_cache_stats(&self) -> DebugCacheStats {
        DebugCacheStats {
            shaped_nodes: self.shapes.len(),
            shaped_bytes: self.shapes.values().map(Shaped::bytes).sum(),
            cached_quads: self.shapes.values().map(|s| s.quads.len()).sum(),
            atlas_entries: self.entries.len(),
            atlas_bytes: (self.atlas_size * self.atlas_size * 4) as usize,
            swash_images: self.swash.image_cache.len(),
            swash_outlines: self.swash.outline_command_cache.len(),
            image_textures: self.images.len(),
            image_bytes: self.images.values().map(|i| i.1).sum(),
            canvas_raster_bytes: self.canvases.bytes(),
            svg_raster_bytes: self.svgs.bytes(),
            vertex_buffer_bytes: self.vertex_capacity,
            layer_textures: self.layers.len(),
            layer_bytes: self.layers.values().map(|l| l.bytes).sum(),
        }
    }
    /// The colour the target clears to where nothing is drawn.
    pub fn set_background(&mut self, color: Color) {
        if self.background != color {
            self.background = color;
            self.fresh = true;
        }
    }
    /// The GPU adapter it renders with.
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.info.clone()
    }
    /// The font system it shapes and draws text with.
    pub fn text_system(&self) -> Rc<RefCell<FontSystem>> {
        self.fonts.clone()
    }
    /// Prepared rich text shared with layout measurement.
    pub fn text_cache(&self) -> Rc<RefCell<prepared::TextCache>> {
        self.text_cache.clone()
    }
    /// Install this renderer's text services on `scene`: native shaping,
    /// and measurement from the prepared text it draws from, so layout and
    /// drawing break lines identically. Call `Ui::refresh_text_geometry`
    /// afterwards for a scene with mounted text.
    pub fn install_text(&self, scene: &mut Scene) {
        install_text(scene, &self.text_system(), &self.text_cache());
    }
    /// Physical pixels per logical point (at least 0.1; 1 if not finite).
    pub fn set_scale_factor(&mut self, scale: f32) {
        let scale = if scale.is_finite() {
            scale.max(0.1)
        } else {
            1.
        };
        if self.scale != scale {
            self.flush_pending();
            self.release_atlas_cells();
            self.scale = scale;
            self.layers.clear();
            self.shapes.clear();
            self.entries.clear();
            self.atlas_epoch = self.atlas_epoch.wrapping_add(1);
            self.cursor = (0, 0, 0);
            self.fresh = true;
        }
    }
    /// Whether the size changed.
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) == (self.width, self.height) {
            return false;
        }
        #[cfg(target_os = "macos")]
        if let Some(present) = &mut self.layer_present {
            present.current = None;
            present.changed.clear();
            present.catch_up.clear();
            present.resized_at = Some(std::time::Instant::now());
        }
        self.flush_pending();
        // Layers stay: what they hold changes with layout (their revision),
        // not with the window's size. (Clearing them made a live resize
        // allocate and paint every layer, blurs and all, at every step.)
        // Scratch grows on demand; release it when the window shrinks a lot.
        if self.blur_scratch.as_ref().is_some_and(|scratch| {
            scratch
                .atlas
                .iter()
                .any(|texture| texture.width() > width * 2 || texture.height() > height * 2)
        }) {
            self.blur_scratch = None;
        }
        self.width = width;
        self.height = height;
        self.spare = None;
        self.target_copy_bind = None;
        // A surface of the layer's becomes the target (`begin_layer_frame`).
        #[cfg(target_os = "macos")]
        let layered = self.layer_present.is_some();
        #[cfg(not(target_os = "macos"))]
        let layered = false;
        let (target_width, target_height) = if layered { (1, 1) } else { (width, height) };
        self.target = texture(&self.device, target_width, target_height, "retained output");
        self.view = self.target.create_view(&Default::default());
        #[cfg(target_os = "macos")]
        if self.layer_present.is_none() {
            written(&self.device, &self.queue, &self.view);
        }
        if let (Some(s), Some(c)) = (&self.surface, &mut self.config) {
            c.width = width;
            c.height = height;
            s.configure(&self.device, c);
        }
        // AppKit shows the resized layer when its transaction commits. A
        // drawable presented outside it lands a frame late, so the window
        // briefly shows stretched or empty content.
        #[cfg(target_os = "macos")]
        {
            self.set_transaction_present(true);
            self.widen_drawables();
        }
        self.blit_bind = self
            .blit
            .as_ref()
            .map(|p| blit_group(&self.device, p, &self.view));
        self.fresh = true;
        true
    }
    /// Render as `render`, without counting `damage` among what the frame
    /// changed: a repaint for comparison (`ZGUI_DAMAGE_CHECK`) must leave
    /// later frames to bring their surfaces up to date as they otherwise would.
    pub fn render_uncounted(
        &mut self,
        scene: &Scene,
        damage: &[Rect],
    ) -> Result<GpuStats, GpuError> {
        #[cfg(target_os = "macos")]
        let changed = self
            .layer_present
            .as_ref()
            .map(|present| present.changed.clone());
        let stats = self.render(scene, damage);
        #[cfg(target_os = "macos")]
        if let (Some(present), Some(changed)) = (self.layer_present.as_mut(), changed) {
            present.changed = changed;
        }
        stats
    }
    /// Draw `scene`'s `damage` (logical rectangles) to the target, and report
    /// what it did.
    pub fn render(&mut self, scene: &Scene, damage: &[Rect]) -> Result<GpuStats, GpuError> {
        let presented = std::mem::take(&mut self.present_stats);
        let mut stats = {
            let _span = zgui::profile::span(zgui::profile::Phase::Paint);
            self.render_inner(scene, damage)?
        };
        if stats.shaped_nodes > 0 {
            // Words shaped by measurement or this frame are reused by the next
            // shaping frames (streaming text repeats nearly all of them, and a
            // changing value most of its words); ones unused a few shaping
            // frames go, and the table shrinks after a burst, so the cache
            // stays the size of visible text (see `ShapeRunCache::trim`).
            self.fonts.borrow_mut().shape_run_cache.trim(SHAPE_RUN_AGES);
            // Prepared text likewise: what is drawn, and a little of the rest.
            let mut cache = self.text_cache.borrow_mut();
            if cache.needs_trim() {
                cache.trim(self.shapes.values().filter_map(|shaped| shaped.prepared));
            }
        }
        stats.draw_calls += presented.draw_calls;
        stats.instances += presented.instances;
        zgui::profile::frame();
        Ok(stats)
    }
    fn render_inner(&mut self, scene: &Scene, damage: &[Rect]) -> Result<GpuStats, GpuError> {
        if let Some(reason) = self.context.lost() {
            return Err(GpuError::device_lost(&reason));
        }
        loop {
            match self.render_once(scene, damage) {
                Err(error)
                    if error.0 == "glyph atlas capacity exceeded in one frame"
                        && self.atlas_size < MAX_ATLAS =>
                {
                    self.reset_atlas((self.atlas_size * 2).min(MAX_ATLAS))
                }
                result => return result,
            }
        }
    }
    fn render_once(&mut self, scene: &Scene, damage: &[Rect]) -> Result<GpuStats, GpuError> {
        // A frame deferred but never presented draws into the retained target
        // before this one prunes images or layers it may reference.
        self.flush_pending();
        if self.scene_identity != Some(scene.identity()) {
            self.scene_identity = Some(scene.identity());
            self.layers.clear();
            self.shapes.clear();
            self.node_images.clear();
            self.shaped_bytes = 0;
            self.fresh = true;
            self.pruned_revision = None;
        }
        // Caches only go stale when nodes are added, removed or change kind.
        if self.pruned_revision != Some(scene.content_revision()) {
            self.pruned_revision = Some(scene.content_revision());
            self.prune(scene);
        }
        let frame = self.layer_frame;
        self.layers.retain(|id, cache| {
            cache.parked = scene.contains(*id) && !scene.is_layer(*id);
            scene.contains(*id)
                && (!cache.parked || cache.transformed && frame - cache.used < LAYER_PARK_FRAMES)
        });
        if let Some(atlas) = &mut self.effects {
            let layers = &self.layers;
            atlas
                .slots
                .retain(|id, _| layers.get(id).is_some_and(|cache| !cache.parked));
            // Unwanted a while, the atlas goes; the next effect makes another.
            if atlas.slots.is_empty() && frame - atlas.used >= LAYER_PARK_FRAMES {
                self.effects = None;
                self.images.remove(&effects::EFFECT_ATLAS);
            }
        }
        // Scroll moves this frame can copy; the rest repaint their clip.
        let (copies, repaint) = match self.plane_offset() {
            // An overlay plane's surface repaints what scrolls.
            Some(_) => (Vec::new(), self.plane_scrolled(scene)),
            None => self.scroll_copies(scene, damage),
        };
        self.copies = copies;
        let extended;
        let damage = if repaint.is_empty() {
            damage
        } else {
            extended = [damage, &repaint].concat();
            &extended[..]
        };
        // An overlay plane draws in its own box's space.
        let local;
        let damage = match self.plane_damage(damage) {
            Some(within) => {
                local = within;
                &local[..]
            }
            None => damage,
        };
        if damage.is_empty() && !self.fresh && self.copies.is_empty() {
            return Ok(GpuStats::default());
        }
        // Visit only subtrees that paint inside the damage the frame will
        // actually repaint. Backdrop blur samples beyond its own subtree, so
        // scenes using it keep the full walk.
        let viewport = Rect::new(
            0.,
            0.,
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        );
        let merged = (!self.fresh && !scene.has_backdrop_blur())
            .then(|| merge_damage(damage, viewport, self.scale));
        // A popup window's renderer walks its root's subtree alone.
        let start = self.plane_popup();
        if start.is_some_and(|root| !scene.contains(root)) {
            return Ok(GpuStats::default());
        }
        let items = match &merged {
            Some(regions) => scene.layer_items_within(start, Some(&self.plane_world(regions))),
            None => scene.layer_items(start),
        };
        // Around native views, only this plane's paint (planes.rs).
        let items = self.plane_items(items);
        let mut layer_stats = GpuStats::default();
        self.layer_frame += 1;
        for item in &items {
            if item.isolated && item.effects.opacity > 0. {
                self.prepare_layer(scene, item.id, 1., &mut layer_stats)?;
            }
        }
        self.prepare_effects(scene, &items)?;
        let mut stats = self.render_flat(scene, items, damage, merged)?;
        self.rendered_serial = Some(scene.flush_serial());
        stats.layer_repaints = layer_stats.layer_repaints;
        stats.layer_cache_hits = layer_stats.layer_cache_hits;
        stats.layer_texture_allocations = layer_stats.layer_texture_allocations;
        stats.damaged_pixels += layer_stats.damaged_pixels;
        stats.draw_calls += layer_stats.draw_calls;
        stats.render_passes += layer_stats.render_passes;
        stats.blur_passes += layer_stats.blur_passes;
        stats.effect_passes += layer_stats.effect_passes;
        stats.instances += layer_stats.instances;
        stats.shaped_nodes += layer_stats.shaped_nodes;
        stats.glyph_uploads += layer_stats.glyph_uploads;
        stats.image_uploads += layer_stats.image_uploads;
        stats.native_surface_imports += layer_stats.native_surface_imports;
        stats.native_surface_conversions += layer_stats.native_surface_conversions;
        stats.canvas_rasterizations += layer_stats.canvas_rasterizations;
        stats.svg_rasterizations += layer_stats.svg_rasterizations;
        stats.geometry_rebuilds += layer_stats.geometry_rebuilds;
        stats.vertex_buffer_allocations += layer_stats.vertex_buffer_allocations;
        Ok(stats)
    }
    /// Drop cached shapes, canvases, SVGs and textures of nodes that are gone.
    fn prune(&mut self, scene: &Scene) {
        self.node_images.retain(|id, _| scene.contains(*id));
        self.node_ramps.retain(|id, _| scene.contains(*id));
        let shown: std::collections::HashSet<&RampKey> = self.node_ramps.values().collect();
        self.ramps.retain(|key, _| shown.contains(key));
        self.shapes.retain(|id, _| {
            scene.contains(*id)
                && matches!(
                    scene.kind(*id),
                    NodeKind::Text { .. } | NodeKind::RichText { .. }
                )
        });
        self.shaped_bytes = self.shapes.values().map(Shaped::bytes).sum();
        self.canvases.retain(scene);
        self.svgs.retain(scene);
        if !self.images.is_empty() {
            let mut live_images: std::collections::HashSet<u64> = scene
                .paint_items()
                .filter_map(|item| match item.kind {
                    NodeKind::Image(image) => Some(image.id()),
                    #[cfg(target_os = "macos")]
                    NodeKind::NativeSurface(frame) => Some(frame.id()),
                    _ => None,
                })
                .collect();
            live_images.extend(self.canvases.image_ids());
            live_images.extend(self.ramps.values().map(|ramp| ramp.id()));
            live_images.extend(self.svgs.image_ids());
            // Boxes' background images.
            live_images.extend(scene.paint_items().filter_map(|item| match item.kind {
                NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => {
                    match quad.decoration.as_deref()?.background.as_ref()? {
                        zgui::decoration::Background::Image { image, .. } => Some(image.id()),
                        _ => None,
                    }
                }
                _ => None,
            }));
            // Effect layers' atlas and masks.
            live_images.insert(effects::EFFECT_ATLAS);
            live_images.extend(
                self.layers
                    .keys()
                    .filter(|id| scene.contains(**id))
                    .filter_map(|id| scene.mask(*id).map(|mask| mask.image.id())),
            );
            #[cfg(target_os = "macos")]
            if let Some(cache) = &mut self.native_surfaces {
                cache.retain(&live_images);
            }
            self.images.retain(|id, _| live_images.contains(id));
        }
    }
    /// Bring `id`'s layer texture up to date. `magnification`: how much its
    /// parent's content is magnified on screen (by transformed ancestors);
    /// with its own scale, it picks the raster scale (`raster_step`).
    fn prepare_layer(
        &mut self,
        scene: &Scene,
        id: NodeId,
        magnification: f32,
        stats: &mut GpuStats,
    ) -> Result<(), GpuError> {
        let revision = scene.layer_revision(id);
        let root = scene.bounds(id);
        let local = scene.local_transform(id);
        // Parked a while once it has none (`LAYER_PARK_FRAMES`).
        let transformed =
            !local.is_identity() || scene.filter_blur(id) > 0. || scene.mask(id).is_some();
        let magnification = magnification * local.magnification();
        let current = self.layers.get(&id).map(|cache| cache.step);
        let mut step = raster_step(magnification, current);
        let frame = self.layer_frame;
        if let Some(cache) = self.layers.get_mut(&id)
            && cache.revision == revision
            && cache.step == step
        {
            cache.bounds.x += root.x - cache.root_origin.0;
            cache.bounds.y += root.y - cache.root_origin.1;
            cache.root_origin = (root.x, root.y);
            cache.transformed = transformed;
            cache.used = frame;
            cache.parked = false;
            stats.layer_cache_hits += 1;
            return Ok(());
        }
        let content = scene.layer_bounds(id);
        let limit = self.device.limits().max_texture_dimension_2d;
        // Magnified past what the texture limits or the budget allow, a
        // layer renders at the finest scale that fits.
        let pixels = |step: i32| {
            let scale = self.scale * step_scale(step);
            (content.width + 2. / scale) * scale * (content.height + 2. / scale) * scale
        };
        while step > 0
            && ((content.width.max(content.height) + 2.) * self.scale * step_scale(step)
                > limit as f32
                || pixels(step) * 4. > 16. * 1024. * 1024.)
        {
            step -= 1;
        }
        let device_scale = self.scale;
        let scale = device_scale * step_scale(step);
        // A transparent texel border lets linear sampling preserve fractional
        // translation coverage (and smooth a rotated layer's edges) instead
        // of clamping opaque edge pixels.
        let bounds = content.expand(1. / scale);
        let x = bounds.x;
        let y = bounds.y;
        let width = (bounds.width * scale).ceil().max(1.) as u32;
        let height = (bounds.height * scale).ceil().max(1.) as u32;
        let bounds = Rect::new(x, y, width as f32 / scale, height as f32 / scale);
        let mut items = scene.layer_items(Some(id));
        self.plane_filter(&mut items);
        for item in &items {
            if item.isolated && item.effects.opacity > 0. {
                self.prepare_layer(scene, item.id, magnification, stats)?;
            }
        }
        self.prepare_effects(scene, &items)?;
        let bytes = width as usize * height as usize * 4;
        let mut existing = self.layers.get(&id).map_or(0, |l| l.bytes);
        let mut used: usize = self.layers.values().map(|l| l.bytes).sum();
        let over = |count: usize, used: usize, existing: usize, known: bool| {
            (!known && count >= 1024) || used - existing + bytes > 64 * 1024 * 1024
        };
        if over(
            self.layers.len(),
            used,
            existing,
            self.layers.contains_key(&id),
        ) {
            // Parked layers give way first.
            self.layers
                .retain(|other, _| *other == id || scene.is_layer(*other));
            existing = self.layers.get(&id).map_or(0, |l| l.bytes);
            used = self.layers.values().map(|l| l.bytes).sum();
        }
        if !self.layers.contains_key(&id) && self.layers.len() >= 1024 {
            return Err(GpuError("isolated layers exceed 1024 texture limit".into()));
        }
        if bytes > 64 * 1024 * 1024 || used - existing + bytes > 64 * 1024 * 1024 {
            return Err(GpuError("isolated layers exceed 64 MiB budget".into()));
        }
        if width > limit || height > limit {
            return Err(GpuError(
                "isolated layer exceeds GPU texture dimensions".into(),
            ));
        }
        // Animated layers change size every frame: round allocations up and
        // reuse any texture that fits without wasting more than half of it.
        let (alloc_w, alloc_h) = (
            width.next_multiple_of(64).min(limit),
            height.next_multiple_of(64).min(limit),
        );
        let existing = self.layers.remove(&id);
        let (texture, bind) = if let Some(cache) = existing.filter(|cache| {
            let (w, h) = (cache.texture.width(), cache.texture.height());
            w >= width
                && h >= height
                && u64::from(w) * u64::from(h) <= 2 * u64::from(alloc_w) * u64::from(alloc_h)
        }) {
            (cache.texture, cache.bind)
        } else {
            let texture = texture(&self.device, alloc_w, alloc_h, "isolated subtree");
            let bind = self.texture_bind(
                &texture.create_view(&Default::default()),
                "isolated subtree",
            );
            stats.layer_texture_allocations += 1;
            (texture, bind)
        };
        let bytes = texture.width() as usize * texture.height() as usize * 4;
        // The content occupies the top-left corner; the quad samples just that.
        let uv = [
            width as f32 / texture.width() as f32,
            height as f32 / texture.height() as f32,
        ];
        let view = texture.create_view(&Default::default());
        let target = std::mem::replace(&mut self.target, texture);
        let old_view = std::mem::replace(&mut self.view, view);
        let old_size = (self.width, self.height);
        self.width = self.target.width();
        self.height = self.target.height();
        let background = self.background;
        self.background = Color(0, 0, 0, 0);
        let fresh = self.fresh;
        self.fresh = true;
        // Everything inside draws at the layer's raster scale: glyphs,
        // canvases and nested layers rasterize as finely as it shows.
        self.scale = scale;
        let mut local = items;
        zgui::scene::translate_items(&mut local, -bounds.x, -bounds.y);
        // Nested layer images remain world-positioned in their cache; pass the offset
        // separately so the same texture can be composed into any ancestor layer.
        let old_origin = self.layer_origin;
        self.layer_origin = (bounds.x, bounds.y);
        let in_layer = std::mem::replace(&mut self.in_layer, true);
        // `fresh` repaints the whole texture, slack included, so an earlier
        // larger frame never bleeds into sampling at the content's edge.
        let result = self.render_flat(
            scene,
            local,
            &[Rect::new(0., 0., bounds.width, bounds.height)],
            None,
        );
        self.in_layer = in_layer;
        self.layer_origin = old_origin;
        let texture = std::mem::replace(&mut self.target, target);
        let _view = std::mem::replace(&mut self.view, old_view);
        self.width = old_size.0;
        self.height = old_size.1;
        self.scale = device_scale;
        self.background = background;
        self.fresh = fresh;
        let result = result?;
        stats.draw_calls += result.draw_calls;
        stats.render_passes += result.render_passes;
        stats.blur_passes += result.blur_passes;
        stats.effect_passes += result.effect_passes;
        stats.instances += result.instances;
        stats.shaped_nodes += result.shaped_nodes;
        stats.glyph_uploads += result.glyph_uploads;
        stats.image_uploads += result.image_uploads;
        stats.canvas_rasterizations += result.canvas_rasterizations;
        stats.svg_rasterizations += result.svg_rasterizations;
        stats.geometry_rebuilds += result.geometry_rebuilds;
        stats.vertex_buffer_allocations += result.vertex_buffer_allocations;
        stats.layer_repaints += 1;
        stats.damaged_pixels += result.damaged_pixels;
        self.layers.insert(
            id,
            LayerCache {
                texture,
                bind,
                bounds,
                revision,
                root_origin: (root.x, root.y),
                bytes,
                uv,
                step,
                transformed,
                used: frame,
                parked: false,
            },
        );
        Ok(())
    }
    fn select_viewport(&mut self, width: f32, height: f32) {
        let key = (width.to_bits(), height.to_bits(), self.scale.to_bits());
        if !self.viewports.contains_key(&key) {
            // Layer sizes come and go; keep the map small.
            if self.viewports.len() >= 32 {
                self.viewports.clear();
            }
            let buffer = upload::init_buffer(
                &self.device,
                self.mapped.is_some(),
                "viewport",
                // Logical size, and device pixels per logical pixel.
                bytemuck::cast_slice(&[width, height, self.scale, 0.]),
                wgpu::BufferUsages::UNIFORM,
            );
            let bind = self.viewport_group(&buffer, self.no_regions.as_entire_binding());
            self.viewports.insert(key, (buffer, bind));
        }
        self.viewport_bind = self.viewports.get(&key).map(|(_, bind)| bind.clone());
        #[cfg(target_os = "macos")]
        {
            self.viewport_values = [width, height, self.scale, 0.];
        }
    }
    /// The pipeline drawing `shading` if a shader fill's, once compiled.
    fn fill_pipeline(&self, shading: Shading) -> Option<&wgpu::RenderPipeline> {
        match shading {
            Shading::Fill(id) => self.fill_pipelines.get(&id)?.as_ref(),
            _ => None,
        }
    }
    /// Compile `shader`'s pipeline if new; whether it can draw.
    fn prepare_fill(&mut self, shader: &Arc<zgui::decoration::FillShader>) -> bool {
        if let Some(compiled) = self.fill_pipelines.get(&shader.id()) {
            return compiled.is_some();
        }
        let source = fill_source(shader.source());
        let pipeline = fill_validates(&source).then(|| {
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("shader fill"),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
            fill_pipeline(&self.device, &module, &self.context.inner.quad_layout)
        });
        self.fill_shaders.insert(shader.id(), shader.clone());
        self.fill_pipelines.insert(shader.id(), pipeline);
        self.fill_pipelines[&shader.id()].is_some()
    }
    /// Group 1 of the quad pipelines: the viewport `uniform` and `regions`.
    fn viewport_group(
        &self,
        uniform: &wgpu::Buffer,
        regions: wgpu::BindingResource<'_>,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport"),
            layout: &self.context.inner.viewport_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: regions,
                },
            ],
        })
    }
    fn texture_bind(&self, view: &wgpu::TextureView, label: &str) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.context.inner.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.device.create_sampler(
                        &wgpu::SamplerDescriptor {
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            ..Default::default()
                        },
                    )),
                },
            ],
        })
    }
    /// `merged`: `damage` merged into regions already (`merge_damage`).
    fn render_flat(
        &mut self,
        scene: &Scene,
        items: Vec<zgui::scene::PaintItem<'_>>,
        damage: &[Rect],
        merged: Option<Vec<Rect>>,
    ) -> Result<GpuStats, GpuError> {
        let mut stats = GpuStats::default();
        let copies = if self.in_layer || self.fresh {
            Vec::new()
        } else {
            std::mem::take(&mut self.copies)
        };
        // Effect slots `prepare_effects` planned for these items: drawn
        // before anything composites them.
        let effect_jobs = std::mem::take(&mut self.effect_jobs);
        if damage.is_empty() && !self.fresh && copies.is_empty() {
            self.forget_effects(&effect_jobs);
            return Ok(stats);
        }
        // Earlier commands read the vertex and uniform buffers rewritten below.
        self.flush_pending();
        let viewport = Rect::new(
            0.,
            0.,
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        );
        self.select_viewport(viewport.width, viewport.height);
        // Whether the scene's own damage covers the window (not a repaint
        // forced by a stale target): see `Deferred::direct`.
        let whole = damage.iter().any(|d| {
            d.x <= 0.
                && d.y <= 0.
                && d.x + d.width >= viewport.width
                && d.y + d.height >= viewport.height
        });
        let has_blur = items
            .iter()
            .any(|item| item.effects.opacity > 0. && item.effects.blur_radius > 0.);
        // Reconstruct every sampled backdrop pixel before replaying filters. A
        // dependency can reach another filter, so expand to a fixed point.
        let damage = if self.fresh {
            vec![viewport]
        } else if has_blur {
            let dependencies: Vec<_> = items
                .iter()
                .filter_map(|item| {
                    if item.effects.opacity <= 0. || item.effects.blur_radius <= 0. {
                        return None;
                    }
                    let output = item
                        .bounds
                        .intersection(item.clip.unwrap_or(viewport))?
                        .intersection(viewport)?;
                    Some((output, item.effects.blur_radius))
                })
                .collect();
            blur_damage(damage, &dependencies, viewport, self.scale)
        } else {
            merged.unwrap_or_else(|| merge_damage(damage, viewport, self.scale))
        };
        if damage.is_empty() && copies.is_empty() {
            self.forget_effects(&effect_jobs);
            return Ok(stats);
        }
        self.frame = self.frame.wrapping_add(1);
        if self.cursor.1 + self.cursor.2 > self.atlas_size * 3 / 4 || self.entries.len() >= 8192 {
            // Filling again soon after the last recycle means the visible
            // working set does not fit: grow rather than re-rasterize every
            // glyph and icon each time it fills.
            let thrashing = self.frame.wrapping_sub(self.atlas_recycled_at) < ATLAS_THRASH_FRAMES;
            if thrashing && self.atlas_size < MAX_ATLAS && self.entries.len() < 8192 {
                self.reset_atlas((self.atlas_size * 2).min(MAX_ATLAS));
            } else {
                self.release_atlas_cells();
                self.entries.clear();
                self.image_cells.clear();
                self.atlas_epoch = self.atlas_epoch.wrapping_add(1);
                self.cursor = (0, 0, 0);
            }
            self.atlas_recycled_at = self.frame;
        }
        let mut quads = std::mem::take(&mut self.frame_quads);
        quads.clear();
        quads.push(Quad {
            rect: [0., 0., viewport.width, viewport.height],
            uv: [0.; 4],
            color: rgba(self.background, 1.),
            fade: [0.; 4],
            options: [0.; 4],
            shape: [0.; 4],
            border: [0.; 4],
            mask: NO_MASK,
        });
        let mut draws = std::mem::take(&mut self.frame_draws);
        // The clip regions quads lie in (`Quad::mask`).
        let mut clips: Vec<Region> = Vec::new();
        draws.clear();
        for item in items {
            // Retained hidden subtrees must not allocate textures, shape text,
            // or trigger filters until their effective opacity is visible.
            if item.effects.opacity <= 0. {
                continue;
            }
            let node_clip = if !item.isolated
                && matches!(item.kind, NodeKind::Text { .. } | NodeKind::RichText { .. })
            {
                item.clip
                    .unwrap_or(viewport)
                    .intersection(item.bounds)
                    .unwrap_or_default()
            } else {
                item.clip.unwrap_or(viewport)
            };
            let Some(clip) = node_clip.intersection(viewport) else {
                continue;
            };

            // A transformed layer's matrix, from this target's origin.
            let matrix = item.matrix().map(|matrix| {
                let (x, y) = self.layer_origin;
                zgui::affine::Affine::translation(x, y)
                    .then(matrix)
                    .then(zgui::affine::Affine::translation(-x, -y))
            });
            // Blurred or masked: composited from its effect slot.
            let effect = (item.isolated
                && (item.effects.filter_blur > 0. || scene.mask(item.id).is_some()))
            .then(|| self.effect_slot(item.id))
            .flatten();
            if !damage.iter().any(|d| {
                d.intersects(clip)
                    && d.intersects(if item.isolated {
                        let b = self.layers[&item.id].bounds;
                        let b = Rect::new(
                            b.x - self.layer_origin.0,
                            b.y - self.layer_origin.1,
                            b.width,
                            b.height,
                        );
                        let b = effect.map_or(b, |slot| b.expand(slot.spread));
                        matrix.map_or(b, |matrix| matrix.bounds(b))
                    } else if let NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } =
                        item.kind
                    {
                        style.paint_bounds(item.bounds)
                    } else if let NodeKind::Svg(svg) = item.kind {
                        svg.paint_bounds(item.bounds)
                    } else if let NodeKind::Image(image) = item.kind {
                        image.paint_bounds(item.bounds)
                    } else {
                        item.bounds
                    })
            }) {
                continue;
            }
            let start = quads.len() as u32;
            // A shader fill's draw uses its pipeline.
            let mut fill_shading = None;
            if item.isolated {
                let layer = self.layers.get(&item.id).expect("prepared layer");
                let bounds = Rect::new(
                    layer.bounds.x - self.layer_origin.0,
                    layer.bounds.y - self.layer_origin.1,
                    layer.bounds.width,
                    layer.bounds.height,
                );
                let (bounds, uv) = match (effect, &self.effects) {
                    (Some(slot), Some(atlas)) => slot.composite(bounds, atlas.size()),
                    _ => (bounds, [0., 0., layer.uv[0], layer.uv[1]]),
                };
                // Transformed: the quad's corners move in the vertex shader,
                // as an affine image's; edge fades stay in its own frame.
                let m = matrix.unwrap_or_default();
                quads.push(Quad {
                    rect: [bounds.x, bounds.y, bounds.width, bounds.height],
                    uv,
                    color: [1., 1., 1., item.effects.opacity],
                    fade: [
                        item.bounds.x,
                        item.bounds.y,
                        item.bounds.width,
                        item.bounds.height,
                    ],
                    options: [
                        item.effects.edge_fade,
                        2.,
                        if matrix.is_some() { 1. } else { 0. },
                        0.,
                    ],
                    shape: match matrix {
                        Some(_) => [m.a, m.c, m.b, m.d],
                        None => [0.; 4],
                    },
                    border: match matrix {
                        Some(_) => [m.tx, m.ty, 0., 0.],
                        None => [0.; 4],
                    },
                    mask: [region_index(&mut clips, Region::of(&item)), 0., 0., 0.],
                });
                draws.push(Draw {
                    shading: Shading::Full,
                    start,
                    end: quads.len() as u32,
                    clip,
                    bounds: quad_bounds(&quads[start as usize..]),
                    // Effect layers sample the effect atlas, and batch.
                    image: effect.map(|_| effects::EFFECT_ATLAS),
                    layer: effect.is_none().then_some(item.id),
                    blur: (item.effects.blur_radius > 0.).then_some((
                        item.bounds,
                        item.effects,
                        blur_corners(item.kind, item.bounds),
                    )),
                });
                continue;
            }
            let bounds = item.bounds;
            let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
            let mut canvas_image = None;
            match item.kind {
                NodeKind::Canvas(canvas) => {
                    if bounds.width <= 0. || bounds.height <= 0. {
                        continue;
                    }
                    let before = self.canvases.rasterizations();
                    let (width, height, scale) = (bounds.width, bounds.height, self.scale);
                    let id = self.canvas_texture(item.id, &mut stats, |cache| {
                        cache.get(item.id, canvas, width, height, scale)
                    })?;
                    stats.canvas_rasterizations +=
                        (self.canvases.rasterizations() - before) as usize;
                    canvas_image = Some(id);
                    quads.push(Quad {
                        rect: fade,
                        uv: [0., 0., 1., 1.],
                        color: [1., 1., 1., item.effects.opacity],
                        fade,
                        options: [item.effects.edge_fade, 2., 0., 0.],
                        shape: [0.; 4],
                        border: [0.; 4],
                        mask: NO_MASK,
                    });
                }
                NodeKind::Rect(color) => quads.push(Quad {
                    rect: fade,
                    uv: [0.; 4],
                    color: rgba(*color, item.effects.opacity),
                    fade,
                    options: [item.effects.edge_fade, 0., 0., 0.],
                    shape: [0.; 4],
                    border: [0.; 4],
                    mask: NO_MASK,
                }),
                NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } => {
                    if style.shadows().len() > 32 {
                        return Err(GpuError("panel exceeds 32 shadows".into()));
                    }
                    // A solid border (any widths per side, any radii) is a
                    // ring the quad shader draws; dashed ones rasterize with
                    // the fill.
                    let ring = style.has_border()
                        && style.decoration.as_ref().is_none_or(|detail| {
                            detail.border_style == zgui::decoration::BorderStyle::Solid
                        });
                    let (opacity, edge_fade) = (item.effects.opacity, item.effects.edge_fade);
                    if item.border_pass {
                        // Its border alone, over what it holds, and its outline.
                        if ring && style.border_over {
                            quads.push(ring_quad(style, bounds, opacity, edge_fade));
                        }
                        if let Some(outline) = style.outline.filter(|o| o.visible()) {
                            quads.push(outline_quad(&outline, style, bounds, opacity, edge_fade));
                        }
                    } else {
                        for shadow in style.shadows() {
                            let core = bounds.expand(shadow.spread.max(0.));
                            let core = Rect::new(
                                core.x + shadow.offset.x,
                                core.y + shadow.offset.y,
                                core.width,
                                core.height,
                            );
                            // Three sigmas out (`blur_radius` is the sigma).
                            let outer = core.expand(shadow.blur_radius.max(0.) * 3.);
                            quads.push(Quad {
                                rect: [outer.x, outer.y, outer.width, outer.height],
                                uv: [0.; 4],
                                color: rgba(shadow.color, item.effects.opacity),
                                fade: [core.x, core.y, core.width, core.height],
                                options: [0., 0., 0., 1.],
                                shape: [
                                    style.radius + shadow.spread.max(0.),
                                    0.,
                                    shadow.blur_radius.max(0.001),
                                    0.,
                                ],
                                border: decoration::shadow_corners(
                                    style,
                                    core.width,
                                    core.height,
                                    shadow.spread.max(0.),
                                ),
                                mask: NO_MASK,
                            });
                        }
                        // Drawn by its border pass instead (`border_over`).
                        let later = style.border_over && ring;
                        // Per-corner radii over a flat fill need no raster: the
                        // quad shader draws per-corner rounded rects, and their
                        // solid borders as rings. Gradients, patterns and
                        // dashes still rasterize.
                        let flat_fill = style.decoration.as_deref().and_then(|detail| {
                            let fill = match &detail.background {
                                // An image or shader paints over the fill colour.
                                None
                                | Some(
                                    zgui::decoration::Background::Image { .. }
                                    | zgui::decoration::Background::Shader(_),
                                ) => style.fill,
                                Some(zgui::decoration::Background::Brush(
                                    zgui::canvas::Brush::Solid(color),
                                )) => *color,
                                Some(_) => return None,
                            };
                            (!style.has_border() || ring).then_some(fill)
                        });
                        if let Some(fill) = flat_fill {
                            if let Some(old) = self.node_images.remove(&item.id) {
                                self.images.remove(&old);
                            }
                            push_panel(
                                &mut quads,
                                Quad {
                                    rect: fade,
                                    uv: [0.; 4],
                                    color: rgba(fill, opacity),
                                    fade,
                                    options: [edge_fade, 0., 0., 1.],
                                    shape: [0., 0., 0., 1.],
                                    border: decoration::shadow_corners(
                                        style,
                                        bounds.width,
                                        bounds.height,
                                        0.,
                                    ),
                                    mask: NO_MASK,
                                },
                                self.scale,
                                self.split_shading,
                            );
                            if let Some(zgui::decoration::Background::Image { image, fit }) = style
                                .decoration
                                .as_deref()
                                .and_then(|d| d.background.as_ref())
                                && let Some(quad) =
                                    background_quad(image, *fit, style, bounds, opacity, edge_fade)
                            {
                                if !self.images.contains_key(&image.id()) {
                                    self.upload_image(image)?;
                                    stats.image_uploads += 1;
                                }
                                quads.push(quad);
                                canvas_image = Some(image.id());
                            }
                            if let Some(zgui::decoration::Background::Shader(shader)) = style
                                .decoration
                                .as_deref()
                                .and_then(|d| d.background.as_ref())
                                && self.prepare_fill(shader)
                            {
                                let uniforms = Region::uniforms(scene.shader_uniforms(item.id));
                                let record = region_index(&mut clips, Some(uniforms));
                                quads.push(Quad {
                                    rect: fade,
                                    uv: [0.; 4],
                                    color: [1., 1., 1., opacity],
                                    fade,
                                    options: [edge_fade, 0., 0., 1.],
                                    shape: [0., 0., 0., 1.],
                                    border: decoration::shadow_corners(
                                        style,
                                        bounds.width,
                                        bounds.height,
                                        0.,
                                    ),
                                    mask: [0., record, 0., 0.],
                                });
                                fill_shading = Some(Shading::Fill(shader.id()));
                            }
                            // (A shader fill shades this item's quads: no insets over it.)
                            if fill_shading.is_none() {
                                quads.extend(inset_quads(style, bounds, opacity, edge_fade));
                            }
                            if ring && !later {
                                quads.push(ring_quad(style, bounds, opacity, edge_fade));
                            }
                        } else if let Some((key, uv)) = (!style.has_border() || ring)
                            .then(|| axis_gradient(style, bounds))
                            .flatten()
                        {
                            // A gradient along one axis: a ramp texture
                            // stretched over the box (its corners cut by the
                            // quad shader), not a raster of the whole box.
                            if let Some(old) = self.node_images.remove(&item.id) {
                                self.images.remove(&old);
                            }
                            let ramp = match self.ramps.get(&key) {
                                Some(ramp) => ramp.clone(),
                                None => {
                                    let ramp = Arc::new(ramp_image(&key)?);
                                    self.ramps.insert(key.clone(), ramp.clone());
                                    ramp
                                }
                            };
                            if !self.images.contains_key(&ramp.id()) {
                                self.upload_image(&ramp)?;
                                stats.image_uploads += 1;
                            }
                            self.node_ramps.insert(item.id, key);
                            canvas_image = Some(ramp.id());
                            quads.push(Quad {
                                rect: fade,
                                uv,
                                color: [1., 1., 1., opacity],
                                fade,
                                options: [edge_fade, 2., 0., 1.],
                                shape: [0., 0., 0., 1.],
                                border: decoration::shadow_corners(
                                    style,
                                    bounds.width,
                                    bounds.height,
                                    0.,
                                ),
                                mask: NO_MASK,
                            });
                            quads.extend(inset_quads(style, bounds, opacity, edge_fade));
                            if ring && !later {
                                quads.push(ring_quad(style, bounds, opacity, edge_fade));
                            }
                        } else if style.decoration.is_some()
                            && bounds.width > 0.
                            && bounds.height > 0.
                        {
                            // Its border left to the border pass, when it has one.
                            let drawn = match later {
                                true => std::borrow::Cow::Owned(zgui::scene::QuadStyle {
                                    border_width: 0.,
                                    decoration: style.decoration.as_deref().map(|detail| {
                                        Arc::new(zgui::decoration::Decoration {
                                            border_widths: None,
                                            ..detail.clone()
                                        })
                                    }),
                                    ..style.clone()
                                }),
                                false => std::borrow::Cow::Borrowed(style),
                            };
                            let before = self.canvases.rasterizations();
                            let (width, height, scale) = (bounds.width, bounds.height, self.scale);
                            let id = self.canvas_texture(item.id, &mut stats, |cache| {
                                cache.get_decoration(item.id, &drawn, width, height, scale)
                            })?;
                            stats.canvas_rasterizations +=
                                (self.canvases.rasterizations() - before) as usize;
                            canvas_image = Some(id);
                            quads.push(Quad {
                                rect: fade,
                                uv: [0., 0., 1., 1.],
                                color: [1., 1., 1., opacity],
                                fade,
                                options: [edge_fade, 2., 0., 0.],
                                shape: [0.; 4],
                                border: [0.; 4],
                                mask: NO_MASK,
                            });
                        } else {
                            let border = match later {
                                true => 0.,
                                false => style.border_width.max(0.),
                            };
                            push_panel(
                                &mut quads,
                                Quad {
                                    rect: fade,
                                    uv: [0.; 4],
                                    color: rgba(style.fill, opacity),
                                    fade,
                                    options: [edge_fade, 0., 0., 0.],
                                    shape: [style.radius.max(0.), border, 0., 1.],
                                    border: rgba(style.border_color, opacity),
                                    mask: NO_MASK,
                                },
                                self.scale,
                                self.split_shading,
                            );
                        }
                    }
                }
                NodeKind::Svg(svg) => {
                    if svg.paint_bounds(bounds).width <= 0. || svg.paint_bounds(bounds).height <= 0.
                    {
                        continue;
                    }
                    let before = self.svgs.rasterizations();
                    // An SVG that won't draw (bad markup, too large) shows
                    // nothing; it is the content's fault, not the frame's.
                    let image =
                        match self
                            .svgs
                            .get(item.id, svg, bounds.width, bounds.height, self.scale)
                        {
                            Ok(image) => image,
                            Err(error) => {
                                if self.failed_svgs.insert(svg.source_id()) {
                                    eprintln!("zgui: an SVG doesn't draw: {}", error.0);
                                }
                                continue;
                            }
                        };
                    stats.svg_rasterizations += (self.svgs.rasterizations() - before) as usize;
                    // Nodes share SVG rasters: `prune` releases their textures
                    // once no node shows them (a new source prunes).
                    let transform = image.paint_transform(bounds);
                    let transformed = image.transform() != zgui::affine::Affine::IDENTITY;
                    // Icon-sized rasters live in the glyph atlas and batch with
                    // text; larger ones keep a texture of their own.
                    let uv =
                        if image.width() <= ATLAS_IMAGE_MAX && image.height() <= ATLAS_IMAGE_MAX {
                            let (x, y) = match self.image_cells.get(&image.id()) {
                                Some(cell) => *cell,
                                None => {
                                    let (width, height) = (image.width(), image.height());
                                    let cell = self.atlas_cell(width, height, |rgba| {
                                        premultiply_into_cell(rgba, image.pixels(), width);
                                    })?;
                                    self.image_cells.insert(image.id(), cell);
                                    stats.image_uploads += 1;
                                    cell
                                }
                            };
                            let atlas = self.atlas_size as f32;
                            [
                                x as f32 / atlas,
                                y as f32 / atlas,
                                image.width() as f32 / atlas,
                                image.height() as f32 / atlas,
                            ]
                        } else {
                            if !self.images.contains_key(&image.id()) {
                                self.upload_image(&image)?;
                                stats.image_uploads += 1;
                            }
                            canvas_image = Some(image.id());
                            [0., 0., 1., 1.]
                        };
                    quads.push(Quad {
                        rect: fade,
                        uv,
                        color: [1., 1., 1., item.effects.opacity],
                        fade,
                        options: [
                            item.effects.edge_fade,
                            2.,
                            if transformed { 1. } else { 0. },
                            0.,
                        ],
                        shape: if transformed {
                            [transform.a, transform.c, transform.b, transform.d]
                        } else {
                            [0.; 4]
                        },
                        border: if transformed {
                            [transform.tx, transform.ty, 0., 0.]
                        } else {
                            [0.; 4]
                        },
                        mask: NO_MASK,
                    });
                }
                #[cfg(target_os = "macos")]
                NodeKind::NativeSurface(frame) => {
                    if !self.images.contains_key(&frame.id()) {
                        let bytes = frame.width() as usize
                            * frame.height() as usize
                            * if frame.format() == zgui::native_surface::SurfaceFormat::Bgra {
                                4
                            } else {
                                6
                            };
                        if bytes + self.images.values().map(|e| e.1).sum::<usize>()
                            > 64 * 1024 * 1024
                        {
                            return Err(GpuError(
                                "visible image and native surface textures exceed 64 MiB".into(),
                            ));
                        }
                        if self.native_surfaces.is_none() {
                            self.native_surfaces = Some(native_surface::SurfaceCache::new(
                                &self.device,
                                &self.queue,
                            )?);
                        }
                        let cache = self.native_surfaces.as_mut().unwrap();
                        cache.import(frame.clone())?;
                        stats.native_surface_imports += 1;
                        stats.native_surface_conversions += usize::from(
                            frame.format() == zgui::native_surface::SurfaceFormat::Nv12FullRange,
                        );
                        let view = cache.texture(frame.id()).create_view(&Default::default());
                        let bytes = cache.bytes(frame.id());
                        let bind = self.texture_bind(&view, "native CoreVideo frame");
                        self.images.insert(frame.id(), (bind, bytes, None));
                    }
                    canvas_image = Some(frame.id());
                    quads.push(Quad {
                        rect: fade,
                        uv: [0., 0., 1., 1.],
                        color: [1., 1., 1., item.effects.opacity],
                        fade,
                        options: [item.effects.edge_fade, 2., 0., 0.],
                        shape: [0.; 4],
                        border: [0.; 4],
                        mask: NO_MASK,
                    });
                }
                NodeKind::Image(image) => {
                    if image.paint_bounds(bounds).width <= 0.
                        || image.paint_bounds(bounds).height <= 0.
                    {
                        continue;
                    }
                    if !self.images.contains_key(&image.id()) {
                        self.upload_image(image)?;
                        stats.image_uploads += 1;
                    }
                    let transform = image.paint_transform(bounds);
                    let transformed = image.transform() != zgui::affine::Affine::IDENTITY;
                    // Images have no procedural border/shape. Reuse those
                    // attribute lanes rather than growing every glyph instance.
                    quads.push(Quad {
                        rect: fade,
                        uv: [0., 0., 1., 1.],
                        color: [1., 1., 1., item.effects.opacity],
                        fade,
                        options: [
                            item.effects.edge_fade,
                            2.,
                            if transformed { 1. } else { 0. },
                            0.,
                        ],
                        shape: if transformed {
                            [transform.a, transform.c, transform.b, transform.d]
                        } else {
                            [0.; 4]
                        },
                        border: if transformed {
                            [transform.tx, transform.ty, 0., 0.]
                        } else {
                            [0.; 4]
                        },
                        mask: NO_MASK,
                    });
                }
                kind @ (NodeKind::Text { .. } | NodeKind::RichText { .. }) => {
                    let rich = match kind {
                        NodeKind::RichText { text } => Some(text.clone()),
                        _ => None,
                    };
                    let rich_content = rich.as_ref().map(|r| r.text_arc());
                    let (text, color, font_size) = match kind {
                        NodeKind::Text {
                            text,
                            color,
                            font_size,
                        } => (text, *color, *font_size),
                        NodeKind::RichText { .. } => (
                            rich_content.as_ref().unwrap(),
                            zgui::scene::Color(255, 255, 255, 255),
                            16.,
                        ),
                        _ => unreachable!(),
                    };
                    if !font_size.is_finite() || font_size <= 0. {
                        return Err(GpuError("font size must be finite and positive".into()));
                    }
                    // Colour-only rich changes (fades, highlights) keep glyph
                    // positions: recolour the cached shape instead of re-shaping.
                    if let (Some(new), Some(shaped)) = (&rich, self.shapes.get_mut(&item.id))
                        && let Some(old) = &shaped.rich
                        && !Arc::ptr_eq(old, new)
                        && old.same_shape(new)
                        && shaped.width == bounds.width
                        && shaped.height == bounds.height
                        && shaped.text_options == item.text_options
                        && shaped.font == *item.font
                    {
                        for glyph in &mut shaped.glyphs {
                            glyph.color = Some(new.runs()[glyph.run].color);
                        }
                        for decoration in &mut shaped.decorations {
                            decoration.color = decoration.color_in(new);
                        }
                        shaped.rich = Some(new.clone());
                        shaped.atlas_epoch = self.atlas_epoch.wrapping_sub(1);
                    }
                    let stale = self.shapes.get(&item.id).is_none_or(|s| {
                        s.rich != rich
                            || s.text_options != item.text_options
                            || s.text != *text
                            || s.font != *item.font
                            || s.size != font_size
                            || s.width != bounds.width
                            || s.height != bounds.height
                            || s.scale != self.scale
                    });
                    if stale {
                        if let Some(old) = self.shapes.remove(&item.id) {
                            self.shaped_bytes -= old.bytes();
                        }
                        let mut fonts = self.fonts.borrow_mut();
                        // Default-option rich text: the prepared lines layout
                        // measured with, laid out at this width, no reshaping.
                        let prepared = rich
                            .as_ref()
                            .filter(|rich| prepared::TextCache::handles(rich));
                        let plain = rich.is_none()
                            && item.text_options == Default::default()
                            && prepared::TextCache::handles_plain(text, font_size);
                        let b = if plain {
                            let mut cache = self.text_cache.borrow_mut();
                            let laid = cache.layout_plain(
                                &mut fonts,
                                text,
                                font_size,
                                item.font,
                                Some(bounds.width),
                            );
                            // Plain text has one run: no line starts to find it by.
                            let glyphs =
                                glyphs_of(laid.runs(Some(bounds.height)), &[], None, self.scale);
                            let underlines = item.font.underline.map_or_else(Vec::new, |line| {
                                text::plain_underlines(
                                    laid.runs(Some(bounds.height)),
                                    font_size,
                                    line,
                                )
                            });
                            Err((glyphs, underlines, laid.key()))
                        } else if let Some(rich) = prepared {
                            let mut cache = self.text_cache.borrow_mut();
                            let laid = cache.layout(&mut fonts, rich, Some(bounds.width));
                            let starts = laid.line_starts();
                            let glyphs = glyphs_of(
                                laid.runs(Some(bounds.height)),
                                &starts,
                                Some(rich),
                                self.scale,
                            );
                            let decorations = text::rich_decorations_in(
                                laid.runs(Some(bounds.height)),
                                &starts,
                                rich,
                            );
                            Err((glyphs, decorations, laid.key()))
                        } else if let Some(rich) = &rich {
                            Ok(text::rich_buffer(
                                &mut fonts,
                                rich,
                                Some(bounds.width),
                                Some(bounds.height),
                            ))
                        } else if item.text_options != Default::default() {
                            Ok(text::display_buffer(
                                &mut fonts,
                                text,
                                item.font,
                                font_size,
                                Some(bounds.width),
                                Some(bounds.height),
                                item.text_options,
                            ))
                        } else {
                            let mut b = Buffer::new_empty(Metrics::new(
                                font_size.max(1.),
                                item.font.line_height.resolve(font_size),
                            ));
                            b.set_size(text::wrap_width(Some(bounds.width)), Some(bounds.height));
                            text::set_buffer_text(
                                &mut fonts,
                                &mut b,
                                text,
                                item.font,
                                font_size.max(1.),
                            );
                            b.shape_until_scroll(&mut fonts, false);
                            Ok(b)
                        };
                        let (glyphs, decorations, prepared) = match b {
                            Err((glyphs, decorations, key)) => (glyphs, decorations, Some(key)),
                            Ok(b) => (
                                glyphs_of(
                                    b.layout_runs(),
                                    &text::line_starts(&b),
                                    rich.as_deref(),
                                    self.scale,
                                ),
                                match (&rich, item.font.underline) {
                                    (Some(rich), _) => text::rich_decorations(&b, rich),
                                    (None, Some(line)) => {
                                        text::plain_underlines(b.layout_runs(), font_size, line)
                                    }
                                    (None, None) => Vec::new(),
                                },
                                None,
                            ),
                        };
                        let shaped = Shaped {
                            decorations,
                            rich: rich.clone(),
                            text_options: item.text_options,
                            font: item.font.clone(),
                            text: text.clone(),
                            size: font_size,
                            width: bounds.width,
                            height: bounds.height,
                            scale: self.scale,
                            glyphs,
                            quads: Vec::new(),
                            atlas_epoch: 0,
                            last_used: self.frame,
                            prepared,
                        };
                        self.shaped_bytes += shaped.bytes();
                        self.shapes.insert(item.id, shaped);
                        stats.shaped_nodes += 1;
                    }
                    let mut shaped = self.shapes.remove(&item.id).unwrap();
                    self.shaped_bytes -= shaped.bytes();
                    let snap = |value: f32| (value * self.scale).round() / self.scale;
                    let (x, y) = (snap(bounds.x), snap(bounds.y));
                    // Partly outside its clip (a long document scrolled in a
                    // view): its lines that reach the clip, as quads made this
                    // frame (`visible_glyphs`), rather than every glyph's.
                    // (Its box against its clip and the viewport, not their
                    // intersection with it: an edge worked out again can be
                    // a rounding off the box's.)
                    let holds = |outer: Rect| {
                        outer.x <= bounds.x
                            && outer.y <= bounds.y
                            && outer.x + outer.width >= bounds.x + bounds.width
                            && outer.y + outer.height >= bounds.y + bounds.height
                    };
                    let partial = self.cull_glyphs
                        && !(holds(viewport) && item.clip.is_none_or(holds))
                        && self.visible_lines(
                            &shaped,
                            clip.y - y,
                            clip.height,
                            font_size,
                            &mut stats,
                        )?;
                    if !partial && shaped.atlas_epoch != self.atlas_epoch {
                        shaped.quads.clear();
                        shaped.quads.extend(
                            shaped
                                .decorations
                                .iter()
                                .filter(|d| d.background)
                                .map(rich_decoration_quad),
                        );
                        shaped.quads.reserve(shaped.glyphs.len());
                        for glyph in &shaped.glyphs {
                            if let Some(a) = self.glyph(glyph.key, &mut stats)? {
                                shaped.quads.push(Quad {
                                    rect: [
                                        (glyph.x + a.left as f32) / self.scale,
                                        (glyph.y - a.top as f32) / self.scale,
                                        a.width as f32 / self.scale,
                                        a.height as f32 / self.scale,
                                    ],
                                    uv: [
                                        a.x as f32 / self.atlas_size as f32,
                                        a.y as f32 / self.atlas_size as f32,
                                        a.width as f32 / self.atlas_size as f32,
                                        a.height as f32 / self.atlas_size as f32,
                                    ],
                                    color: glyph.color.map_or([0.; 4], |c| rgba(c, 1.)),
                                    fade: [0.; 4],
                                    options: [0., 1., 0., 0.],
                                    shape: [0.; 4],
                                    border: [0.; 4],
                                    mask: NO_MASK,
                                });
                            }
                        }
                        shaped.quads.extend(
                            shaped
                                .decorations
                                .iter()
                                .filter(|d| !d.background)
                                .map(rich_decoration_quad),
                        );
                        shaped.atlas_epoch = self.atlas_epoch;
                        stats.geometry_rebuilds += 1;
                    }
                    // A text selection, under the glyphs; rounded, its box
                    // rides in `fade`, as a rounded run background's does.
                    let highlight = scene.text_highlight(item.id);
                    if let Some(highlight) = highlight {
                        quads.extend(highlight.rects.iter().map(|r| {
                            let rect = [bounds.x + r.x, bounds.y + r.y, r.width, r.height];
                            let rounded = highlight.radius > 0.;
                            Quad {
                                rect,
                                uv: [0.; 4],
                                color: rgba(highlight.color, item.effects.opacity),
                                fade: if rounded { rect } else { fade },
                                options: [item.effects.edge_fade, 0., 0., 0.],
                                shape: if rounded {
                                    [highlight.radius, 0., 0., 1.]
                                } else {
                                    [0.; 4]
                                },
                                border: [0.; 4],
                                mask: NO_MASK,
                            }
                        }));
                    }
                    // Its selected glyphs, in the selected-text colour.
                    let selected = highlight.filter(|h| h.text_color.is_some());
                    // On whole device pixels, as a browser draws text: an
                    // atlas bitmap drawn a fraction off is sampled between
                    // its texels, and a hinted one-pixel stroke (a minus
                    // sign) turns into two rows at half strength.
                    // Plain text's one colour, worked out once for its glyphs.
                    let plain_color = rgba(color, item.effects.opacity);
                    let visible = std::mem::take(&mut self.visible_glyphs);
                    let source = if partial { &visible } else { &shaped.quads };
                    quads.reserve(source.len());
                    for cached in source {
                        let mut quad = *cached;
                        quad.rect[0] += x;
                        quad.rect[1] += y;
                        if quad.shape[3] > 0.5 {
                            quad.fade[0] += x;
                            quad.fade[1] += y;
                        }
                        let recolor = selected.filter(|_| cached.options[1] > 0.5).and_then(|h| {
                            h.glyph_color(
                                cached.rect[0] + cached.rect[2] * 0.5,
                                cached.rect[1] + cached.rect[3] * 0.5,
                            )
                        });
                        quad.color = if let Some(color) = recolor {
                            rgba(color, item.effects.opacity)
                        } else if rich.is_some() {
                            [
                                cached.color[0],
                                cached.color[1],
                                cached.color[2],
                                cached.color[3] * item.effects.opacity,
                            ]
                        } else {
                            plain_color
                        };
                        quad.fade = fade;
                        quad.options[0] = item.effects.edge_fade;
                        quads.push(quad);
                    }
                    self.visible_glyphs = visible;
                    shaped.last_used = self.frame;
                    let bytes = shaped.bytes();
                    if bytes <= SHAPE_BUDGET {
                        let mut used = self.shaped_bytes;
                        while self.shapes.len() >= 1024 || used + bytes > SHAPE_BUDGET {
                            let Some(id) = self
                                .shapes
                                .iter()
                                .min_by_key(|(_, s)| s.last_used)
                                .map(|(id, _)| *id)
                            else {
                                break;
                            };
                            used -= self.shapes.remove(&id).unwrap().bytes();
                        }
                        self.shaped_bytes = used + bytes;
                        self.shapes.insert(item.id, shaped);
                    }
                }
                NodeKind::Container(_) => {}
            }
            // Inside rounded clips or a `fade_edges` container: cut and faded
            // in the shader.
            let region = region_index(&mut clips, Region::of(&item));
            if region > 0. {
                for quad in &mut quads[start as usize..] {
                    quad.mask[0] = region;
                }
            }
            // When the clip only culls whole quads, apply it on the CPU and
            // draw under the viewport, so a change of clip never splits a
            // batch: neighbouring text, icons and rows become one draw. Quads
            // it would cut, transformed quads and filters keep the scissor.
            let mut draw_clip = clip;
            let limit = snap_out(clip, self.scale);
            // The kept quads' bounds, when the clip culls on the CPU.
            let mut culled_bounds = None;
            if item.effects.blur_radius <= 0. {
                // One pass: each quad inside the clip, outside it, or cut.
                let (mut cut, mut outside, mut kept) = (false, false, None::<Rect>);
                for quad in &quads[start as usize..] {
                    if quad.options[2] >= 0.5 {
                        cut = true;
                        break;
                    }
                    let rect = quad_rect(quad);
                    if rect_inside(rect, limit) {
                        kept = Some(kept.map_or(rect, |bounds| bounds.union(rect)));
                    } else if rect_outside(rect, limit) {
                        outside = true;
                    } else {
                        cut = true;
                        break;
                    }
                }
                if !cut {
                    if outside {
                        let mut kept = start as usize;
                        for index in start as usize..quads.len() {
                            if inside(&quads[index], limit) {
                                quads[kept] = quads[index];
                                kept += 1;
                            }
                        }
                        quads.truncate(kept);
                    }
                    draw_clip = viewport;
                    culled_bounds = Some(kept.unwrap_or_default());
                }
            }
            if quads.len() as u32 > start || item.effects.blur_radius > 0. {
                let draw = Draw {
                    shading: fill_shading.unwrap_or(Shading::Full),
                    layer: None,
                    blur: (item.effects.blur_radius > 0.).then_some((
                        bounds,
                        item.effects,
                        blur_corners(item.kind, item.bounds),
                    )),
                    start,
                    end: quads.len() as u32,
                    clip: draw_clip,
                    bounds: culled_bounds.unwrap_or_else(|| quad_bounds(&quads[start as usize..])),
                    image: if let Some(image) = canvas_image {
                        Some(image)
                    } else if let NodeKind::Image(image) = item.kind {
                        Some(image.id())
                    } else {
                        None
                    },
                };
                push_draw(&mut draws, &quads, draw, self.split_shading, self.scale);
            }
        }
        // Effect passes' quads ride in the frame's vertex buffer.
        let effect_first = quads.len() as u32;
        quads.extend(Self::effect_quads(&effect_jobs));
        // The surface this frame draws into, when presenting through the
        // layer: shown surfaces stay as they are.
        #[cfg(target_os = "macos")]
        let shown = if self.in_layer {
            None
        } else {
            self.begin_layer_frame(self.fresh || whole)?;
            self.layer_present
                .as_ref()
                .and_then(|present| present.shown)
        };
        #[cfg(not(target_os = "macos"))]
        let shown: Option<usize> = None;
        let regions: Vec<(Rect, (u32, u32, u32, u32))> = damage
            .iter()
            .filter_map(|region| {
                scissor(*region, self.scale, self.width, self.height).map(|s| (*region, s))
            })
            .collect();
        for (_, (_, _, w, h)) in &regions {
            stats.damaged_pixels += u64::from(*w) * u64::from(*h);
        }
        // What this frame changes in the retained target, for surfaces that
        // must catch up later (`begin_layer_frame`).
        #[cfg(target_os = "macos")]
        if !self.in_layer
            && let Some(present) = &mut self.layer_present
        {
            present.changed.extend(regions.iter().map(|(_, s)| *s));
            present.changed.extend(copies.iter().map(|(s, _)| *s));
        }
        // What the surface lacks: changes since it was last shown that this
        // frame doesn't repaint, and scrolled content; both drawn from the
        // shown surface at the start of the frame's first pass, as instances
        // after the frame's quads.
        #[cfg(target_os = "macos")]
        let mut catch_up = shown.and_then(|shown| {
            let (bind, draws) = self.catch_up_draws(shown, &copies, &regions)?;
            stats.scroll_copies += copies.len();
            let start = quads.len() as u32;
            quads.extend(draws.into_iter().map(|((x, y, w, h), (dx, dy))| Quad {
                rect: [x as f32, y as f32, w as f32, h as f32],
                uv: [dx as f32, dy as f32, 0., 0.],
                ..bytemuck::Zeroable::zeroed()
            }));
            Some((bind, start..quads.len() as u32))
        });
        let needed = quads.len() * std::mem::size_of::<Quad>();
        if needed > VERTEX_BUDGET {
            return Err(GpuError("frame geometry exceeds 32 MiB budget".into()));
        }
        // A clean filter elsewhere in the retained target needs no replay and
        // must not force every unrelated quad into its own render pass.
        let has_damaged_blur = has_blur
            && draws.iter().any(|draw| {
                draw.blur.is_some_and(|(bounds, _, _)| {
                    damage.iter().any(|region| {
                        region
                            .intersection(draw.clip)
                            .and_then(|rect| rect.intersection(bounds))
                            .and_then(|output| scissor(output, self.scale, self.width, self.height))
                            .is_some()
                    })
                })
            });
        // The part of a filter's output inside `region`, if it reaches it.
        let filtered = |draw: &Draw, region: &Rect| {
            let (bounds, _, _) = draw.blur?;
            region.intersection(draw.clip)?.intersection(bounds)
        };
        // A filter samples everything painted before it, so the filters a
        // damaged region reaches end passes. A filter joins the batch before
        // it when nothing drawn since that batch's first filter reaches its
        // area (its backdrop is complete there already): the batch blurs
        // together and draws back at its start, in one split. It may also
        // sit on another filter of the batch whose blur alone reaches it, at
        // most half as wide there (a frosted pill on a softly blurred edge):
        // it then blurs the backdrop under that blur, which differs from
        // blurring the blur by under an eighth of its radius. Between
        // splits, every region shares one pass: a scissor per region, draws
        // batched within it.
        let mut batches: Vec<(usize, Vec<usize>)> = Vec::new();
        if has_damaged_blur {
            let margin = 1. / self.scale;
            // What a draw paints: its quads and, for a filter, its blur.
            let reaches = |draw: &Draw, area: Rect| {
                let blur = draw
                    .blur
                    .and_then(|(bounds, _, _)| bounds.intersection(draw.clip));
                [draw.bounds.intersection(draw.clip), blur]
                    .into_iter()
                    .flatten()
                    .any(|painted| painted.intersection(area).is_some())
            };
            for i in 0..draws.len() {
                let draw = &draws[i];
                if !regions
                    .iter()
                    .any(|(region, _)| filtered(draw, region).is_some())
                {
                    continue;
                }
                let Some((bounds, effects, _)) = draw.blur else {
                    continue;
                };
                let area = bounds.intersection(draw.clip);
                let joins = area
                    .zip(batches.last())
                    .is_some_and(|(area, (start, filters))| {
                        let area = area.expand(margin);
                        (*start..i).all(|k| {
                            let before = &draws[k];
                            let quads = before.bounds.intersection(before.clip);
                            !reaches(before, area)
                                || filters.contains(&k)
                                    && quads.is_none_or(|quads| quads.intersection(area).is_none())
                                    && before.blur.is_some_and(|(under, beneath, _)| {
                                        2. * peak_blur(under, &beneath, area) <= effects.blur_radius
                                    })
                        })
                    });
                match batches.last_mut() {
                    Some((_, filters)) if joins => filters.push(i),
                    _ => batches.push((i, vec![i])),
                }
            }
        }
        // Frames of quads (images the CPU wrote, layer composites, nothing
        // else recorded, any filters mixed by dual-source blending) skip
        // wgpu: see `metal_frame`. Layers repainted this frame were submitted
        // to the queue by `flush_pending` above, so they land first: a frame
        // animating only layer transforms is a single direct pass.
        #[cfg(target_os = "macos")]
        if !self.in_layer
            && self.mapped.is_some()
            && self.texture_uploads.is_empty()
            && self.pending.get_mut().is_none()
            && (shown.is_some() || copies.is_empty())
            && (self.layer_present.is_some() || self.surface.is_none())
            && self.single_pass_format.is_none()
            && draws.iter().all(|draw| {
                draw.layer.is_none_or(|id| self.layers.contains_key(&id))
                    && draw.image.is_none_or(|id| {
                        self.images.get(&id).is_some_and(|image| image.2.is_some())
                    })
            })
            && (batches.is_empty()
                || self
                    .device
                    .features()
                    .contains(wgpu::Features::DUAL_SOURCE_BLENDING))
            // Shader fills compiled for Metal too.
            && draws.iter().all(|draw| match draw.shading {
                Shading::Fill(id) => self.fill_shaders.get(&id).is_some_and(|shader| {
                    self.metal_frame
                        .get_or_init(|| metal_frame::MetalFrame::new(&self.device, &self.queue).ok())
                        .as_ref()
                        .is_some_and(|frame| frame.fills(shader))
                }),
                _ => true,
            })
            && effect_jobs.iter().all(|job| {
                job.mask
                    .is_none_or(|id| self.images.get(&id).is_some_and(|image| image.2.is_some()))
            })
            && self
                .metal_frame
                .get_or_init(|| metal_frame::MetalFrame::new(&self.device, &self.queue).ok())
                .as_ref()
                .is_some_and(|frame| {
                    (batches.is_empty() || frame.blurs())
                        && (effect_jobs.is_empty() || frame.effects())
                })
        {
            let catch_up = catch_up.map(|(_, instances)| instances);
            let effects = (&effect_jobs[..], effect_first);
            return Ok(self.render_direct(
                quads, draws, &regions, &batches, catch_up, shown, effects, &clips, stats,
            ));
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // Glyphs rasterized while building this frame land before any pass
        // samples them. Counted as glyph uploads, not scene render passes.
        self.texture_uploads.encode(&self.device, &mut encoder);
        // Scrolled with surfaces of its own, the copies are drawn below.
        if shown.is_none() && !copies.is_empty() {
            let spare = self.spare_texture();
            for ((x, y, w, h), (sx, sy)) in &copies {
                let extent = wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                };
                let (from_x, from_y) = ((*x as i32 + sx) as u32, (*y as i32 + sy) as u32);
                encoder.copy_texture_to_texture(
                    texel_at(&self.target, from_x, from_y),
                    texel_at(&spare, *x, *y),
                    extent,
                );
                encoder.copy_texture_to_texture(
                    texel_at(&spare, *x, *y),
                    texel_at(&self.target, *x, *y),
                    extent,
                );
                stats.scroll_copies += 1;
            }
        }
        // Every batch of filters planned now: their records ride in the
        // frame's vertex buffer after the quads, and the atlas fits them all.
        let plans: Vec<Option<BlurPlan>> = batches
            .iter()
            .map(|(_, members)| self.plan_blur(&batch_filters(&draws, &regions, members)))
            .collect();
        let bytes: &[u8] = bytemuck::cast_slice(&quads);
        let align = self.device.limits().min_storage_buffer_offset_alignment as usize;
        // Each part at an aligned offset in the buffer.
        let mut records = Vec::new();
        let mut record = |part: &[u8]| {
            let at = (bytes.len() + records.len()).next_multiple_of(align);
            records.resize(at - bytes.len(), 0);
            records.extend_from_slice(part);
            (at as u64, part.len() as u64)
        };
        // The clip regions too, bound with the viewport for this frame.
        let region_range = (!clips.is_empty()).then(|| record(bytemuck::cast_slice(&clips)));
        let ranges: Vec<Option<[(u64, u64); 2]>> = plans
            .iter()
            .map(|plan| {
                let plan = plan.as_ref()?;
                let slots: Vec<SlotRecord> = plan.slots.iter().map(|slot| slot.0).collect();
                Some([
                    record(bytemuck::cast_slice(&plan.blurs)),
                    record(bytemuck::cast_slice(&slots)),
                ])
            })
            .collect();
        let needed = bytes.len() + records.len();
        let vertices = if let Some(mapped) = &mut self.mapped {
            let mapped = mapped.get_mut();
            let (vertices, allocated) = mapped.write_parts(&[bytes, &records]);
            stats.vertex_buffer_allocations += usize::from(allocated);
            self.vertex_capacity = mapped.bytes();
            vertices
        } else {
            if needed > self.vertex_capacity {
                self.vertex_capacity = needed.next_power_of_two();
                self.vertices = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("retained frame vertices"),
                    size: self.vertex_capacity as u64,
                    usage: wgpu::BufferUsages::VERTEX
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: false,
                });
                stats.vertex_buffer_allocations += 1;
            }
            let vertices = self.vertices.clone();
            let mut view = self.belt.get_mut().write_buffer(
                &mut encoder,
                &vertices,
                0,
                wgpu::BufferSize::new(needed as u64).expect("background quad"),
            );
            view.slice(..bytes.len()).copy_from_slice(bytes);
            view.slice(bytes.len()..).copy_from_slice(&records);
            drop(view);
            self.belt.get_mut().finish();
            vertices
        };
        if let Some((offset, size)) = region_range {
            let uniform = self
                .viewports
                .get(&(
                    viewport.width.to_bits(),
                    viewport.height.to_bits(),
                    self.scale.to_bits(),
                ))
                .map(|(uniform, _)| uniform.clone())
                .expect("selected above");
            let regions = wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &vertices,
                offset,
                size: wgpu::BufferSize::new(size),
            });
            self.viewport_bind = Some(self.viewport_group(&uniform, regions));
        }
        self.encode_effects(
            &mut encoder,
            &vertices,
            &effect_jobs,
            effect_first,
            &mut stats,
        );
        let splits: Vec<usize> = batches.iter().map(|(start, _)| *start).collect();
        // The window's own frame can be drawn at presentation in the same pass
        // as the drawable (`present_single_pass`). With damaged filters, only
        // the draws after the last one wait: everything before it must land
        // in the target for the filter to sample.
        // Under the topmost opaque quad covering a whole region, nothing shows:
        // skip the clear and every quad below it. Filters sample what lies
        // beneath them, so frames that replay one draw everything.
        let first = if splits.is_empty() {
            occluded_from(&quads, &draws, &regions, self.scale)
        } else {
            vec![0; regions.len()]
        };
        // Shader fills have no pipelines writing two targets at once.
        let defer = !self.in_layer
            && !draws
                .iter()
                .any(|draw| matches!(draw.shading, Shading::Fill(_)))
            && (!self.present_unpremultiplies || self.background.3 == 255)
            && self.single_pass_format.is_some()
            && !self.copy_present
            && copies.is_empty();
        if !defer && !self.in_layer {
            // Drawn into the target now: no streak of drawable-only frames.
            self.whole_frames = 0;
        }
        let mut from = 0;
        // Each batch of filters is drawn back at the start of the pass after
        // its blur passes.
        let mut vertical = None;
        for (segment, end) in splits.iter().copied().chain([draws.len()]).enumerate() {
            if defer && end == draws.len() {
                stats.render_passes += 1;
                let blur = vertical.take();
                let direct = whole && self.whole_frames > 0 && segment == 0 && blur.is_none();
                self.whole_frames = if whole {
                    self.whole_frames.saturating_add(1)
                } else {
                    0
                };
                *self.deferred.get_mut() = Some(Deferred {
                    vertices: vertices.clone(),
                    draws: draws.split_off(from),
                    regions,
                    viewport: self.viewport_bind.clone(),
                    blur,
                    clear: segment == 0,
                    disjoint: !has_blur,
                    first,
                    direct,
                });
                break;
            }
            if segment == 0 || end > from {
                stats.render_passes += 1;
                let label = ["damage 0", "damage 1", "damage 2", "damage 3"][segment.min(3)];
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                #[cfg(target_os = "macos")]
                if let Some((bind, instances)) = catch_up.take() {
                    pass.set_pipeline(self.copy_offset.as_ref().expect("made with the draws"));
                    pass.set_bind_group(0, &bind, &[]);
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.draw(0..4, instances);
                    stats.draw_calls += 1;
                }
                if let Some(blur) = vertical.take() {
                    let pipeline = self
                        .blur_pipelines
                        .as_ref()
                        .expect("filtered")
                        .drawing(&blur);
                    Self::encode_vertical(&mut pass, pipeline, &blur, &mut stats);
                }
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_bind_group(0, &self.bind, &[]);
                pass.set_bind_group(1, self.viewport_bind.as_ref(), &[]);
                let quads = &self.context.inner.quads;
                for (index, (region, (x, y, w, h))) in regions.iter().enumerate() {
                    pass.set_scissor_rect(*x, *y, *w, *h);
                    let first = first[index];
                    if segment == 0 && first == 0 {
                        pass.set_pipeline(quads.clear(self.split_shading));
                        pass.set_bind_group(0, &self.bind, &[]);
                        pass.draw(0..4, 0..1);
                        stats.draw_calls += 1;
                    }
                    self.encode_draws(
                        &mut pass,
                        &|shading| {
                            self.fill_pipeline(shading)
                                .unwrap_or_else(|| quads.get(shading))
                        },
                        *region,
                        &draws[from..end],
                        first,
                        &mut stats,
                    );
                }
            }
            if segment == batches.len() {
                break;
            }
            vertical = plans[segment]
                .as_ref()
                .zip(ranges[segment])
                .and_then(|(plan, ranges)| {
                    self.blur(&mut encoder, plan, &vertices, ranges, &mut stats)
                });
            // The first filtered draw's own quads open the next pass.
            from = end;
        }
        *self.pending.get_mut() = Some(encoder);
        self.fresh = false;
        self.frame_quads = quads;
        draws.clear();
        self.frame_draws = draws;
        Ok(stats)
    }
    /// Record this frame with Metal directly (`metal_frame`): the pass the
    /// wgpu path records for a frame of plain draws, committed at once.
    #[cfg(target_os = "macos")]
    #[allow(clippy::too_many_arguments)]
    fn render_direct(
        &mut self,
        quads: Vec<Quad>,
        mut draws: Vec<Draw>,
        regions: &[(Rect, Scissor)],
        batches: &[(usize, Vec<usize>)],
        catch_up: Option<std::ops::Range<u32>>,
        shown: Option<usize>,
        (effect_jobs, effect_first): (&[effects::Job], u32),
        clips: &[Region],
        mut stats: GpuStats,
    ) -> GpuStats {
        let _span = zgui::profile::span(zgui::profile::Phase::Encode);
        let vertices = {
            use objc2::Message;
            let mapped = self
                .mapped
                .as_mut()
                .expect("checked by the caller")
                .get_mut();
            let (_, allocated) = mapped.write(bytemuck::cast_slice(&quads));
            stats.vertex_buffer_allocations += usize::from(allocated);
            self.vertex_capacity = mapped.bytes();
            mapped.last_metal().retain()
        };
        self.whole_frames = 0;
        // As `render`'s wgpu passes: a split at each batch of filters.
        let first = if batches.is_empty() {
            occluded_from(&quads, &draws, regions, self.scale)
        } else {
            vec![0; regions.len()]
        };
        // Every batch planned first: the atlas then fits them all.
        let plans: Vec<_> = batches
            .iter()
            .map(|(_, members)| {
                let plan = self.plan_blur(&batch_filters(&draws, regions, members))?;
                let slots: Vec<SlotRecord> = plan.slots.iter().map(|slot| slot.0).collect();
                let rects: Vec<Scissor> = plan
                    .slots
                    .iter()
                    .map(|(record, (w, h), _)| {
                        (record.atlas[0] as u32, record.atlas[1] as u32, *w, *h)
                    })
                    .collect();
                Some((plan, slots, rects))
            })
            .collect();
        let committed = {
            let frame = self
                .metal_frame
                .get()
                .and_then(Option::as_ref)
                .expect("checked by the caller");
            // Metal's textures, each HAL guard dropped before the next is
            // taken (wgpu's lock is not reentrant).
            let raw = |texture: &wgpu::Texture| {
                use objc2::Message;
                // SAFETY: The raw texture is retained for a command buffer (which
                // retains it too), never destroyed; the guard is dropped at once.
                unsafe { texture.as_hal::<wgpu::hal::api::Metal>() }
                    .expect("a Metal texture")
                    .raw_handle()
                    .retain()
            };
            // The retained target: through the layer, this frame's surface.
            let (target, atlas) = (raw(&self.target), raw(&self.atlas));
            let blur_atlas = self
                .blur_scratch
                .as_ref()
                .filter(|_| plans.iter().any(Option::is_some))
                .map(|scratch| scratch.atlas.each_ref().map(raw));
            // Effect slots drawn first, each pass of them into its atlas.
            let effect_passes = self.metal_effect_passes(effect_jobs, effect_first, &raw);
            for pass in &effect_passes {
                stats.render_passes += 1;
                stats.effect_passes += 1;
                stats.draw_calls += pass.draws.len();
            }
            let mut pass = frame.begin(
                &target,
                &vertices,
                self.viewport_values,
                bytemuck::cast_slice(clips),
                &effect_passes,
            );
            let present = self.layer_present.as_ref();
            if let (Some(instances), Some(present), Some(shown)) = (catch_up, present, shown) {
                pass.texture(&present.buffers[shown].0.metal);
                pass.copies(frame, instances);
                stats.draw_calls += 1;
            }
            pass.texture(&atlas);
            let clear = if self.split_shading {
                Shading::Basic
            } else {
                Shading::Full
            };
            // The image or layer the quads sample (neither: the glyph atlas).
            let mut sampled = (None, None);
            let splits = batches.iter().map(|(start, _)| *start);
            let mut from = 0;
            // The batch drawn back at the start of the pass after its blur.
            let mut blurred = None;
            for (segment, end) in splits.chain([draws.len()]).enumerate() {
                if let Some((plan, slots, _)) = blurred.take() {
                    let plan: &BlurPlan = plan;
                    let records = metal_frame::BlurRecords {
                        blurs: bytemuck::cast_slice(&plan.blurs),
                        slots: bytemuck::cast_slice(slots),
                    };
                    let blur = blur_atlas.as_ref().expect("planned");
                    pass.mixes(frame, &blur[0], &records, &plan.draws);
                    stats.draw_calls += plan.draws.len();
                }
                for (index, (region, region_scissor)) in regions.iter().enumerate() {
                    pass.scissor(*region_scissor);
                    if segment == 0 && first[index] == 0 {
                        pass.quads(frame, clear, true, 0..1);
                        stats.draw_calls += 1;
                    }
                    self.batches(*region, &draws[from..end], first[index], |batch| {
                        let Some(rect) = scissor(batch.rect, self.scale, self.width, self.height)
                        else {
                            return;
                        };
                        if batch.source != sampled {
                            let texture = match batch.source {
                                (Some(image), _) => {
                                    self.images.get(&image).and_then(|image| image.2.as_ref())
                                }
                                (None, Some(layer)) => {
                                    self.layers.get(&layer).map(|layer| &layer.texture)
                                }
                                (None, None) => None,
                            };
                            match texture {
                                Some(texture) => pass.texture(&raw(texture)),
                                None => pass.texture(&atlas),
                            }
                            sampled = batch.source;
                        }
                        pass.scissor(rect);
                        pass.quads(frame, batch.shading, false, batch.start..batch.end);
                        stats.draw_calls += 1;
                        stats.instances += (batch.end - batch.start) as usize;
                    });
                }
                let Some(Some((plan, slots, rects))) = plans.get(segment) else {
                    from = end;
                    continue;
                };
                let records = metal_frame::BlurRecords {
                    blurs: bytemuck::cast_slice(&plan.blurs),
                    slots: bytemuck::cast_slice(slots),
                };
                let blur = blur_atlas.as_ref().expect("planned");
                pass.blur(frame, &target, [&blur[0], &blur[1]], &records, rects);
                pass.resume(&target);
                pass.texture(&atlas);
                sampled = (None, None);
                stats.draw_calls += 3 * rects.iter().filter(|r| r.2 > 0 && r.3 > 0).count();
                stats.render_passes += 4;
                stats.blur_passes += 3;
                blurred = Some((plan, slots, rects));
                from = end;
            }
            let _span = zgui::profile::span(zgui::profile::Phase::Submit);
            pass.commit()
        };
        *self.metal_inflight.get_mut() = Some(committed);
        self.metal_frames += 1;
        stats.render_passes += 1;
        self.fresh = false;
        self.frame_quads = quads;
        draws.clear();
        self.frame_draws = draws;
        stats
    }
    /// Wait for a directly encoded frame (`render_direct`) to complete.
    #[cfg(target_os = "macos")]
    fn finish_direct(&self) -> bool {
        use objc2_metal::MTLCommandBuffer;
        let Some(buffer) = self.metal_inflight.borrow_mut().take() else {
            return false;
        };
        buffer.waitUntilCompleted();
        let ran = buffer.GPUEndTime() - buffer.GPUStartTime();
        self.gpu_time
            .set((ran > 0.).then(|| std::time::Duration::from_secs_f64(ran)));
        if let Some(mapped) = &self.mapped {
            mapped.borrow_mut().completed();
        }
        true
    }
    /// The spare retained texture (single-pass presentation's other target),
    /// created at the window's size if needed.
    fn spare_texture(&mut self) -> wgpu::Texture {
        if self
            .spare
            .as_ref()
            .is_none_or(|t| t.texture.width() != self.width || t.texture.height() != self.height)
        {
            let texture = texture(&self.device, self.width, self.height, "retained output");
            let view = texture.create_view(&Default::default());
            self.spare = Some(Retained {
                texture,
                view,
                copy_bind: None,
                blit_bind: None,
            });
        }
        self.spare.as_ref().expect("created above").texture.clone()
    }
    /// Split the last flush's scroll moves into pixel copies this frame can
    /// apply and clips it must repaint instead. A copy is exact only when the
    /// retained target holds the previous flush, both the clip and the shift
    /// land on whole pixels, and within the clip nothing but the content
    /// changes position: whatever lies under it is hidden by one opaque
    /// rectangle, and anything over it is repainted anyway.
    fn scroll_copies(&self, scene: &Scene, damage: &[Rect]) -> (Vec<ScrollCopy>, Vec<Rect>) {
        let serial = scene.flush_serial();
        let mut copies = Vec::new();
        let mut repaint = Vec::new();
        // On Apple GPUs a process's first texture blit makes the driver hold
        // over 100 MB of graphics memory (see vendor/wgpu-hal/ZGUI_PATCH.md):
        // repaint there, unless presenting surfaces of its own, which copy
        // by drawing from the shown surface. That holds the previous flush
        // only once it was presented.
        #[cfg(target_os = "macos")]
        let copyable = self
            .layer_present
            .as_ref()
            .is_some_and(|present| present.current.is_none());
        #[cfg(not(target_os = "macos"))]
        let copyable = true;
        for scroll in scene.scroll_moves() {
            if scroll.serial != serial {
                continue;
            }
            let whole = |value: f32| {
                let scaled = value * self.scale;
                ((scaled - scaled.round()).abs() < 1e-3).then_some(scaled.round() as i32)
            };
            let pixels = (|| {
                let clip = scroll.clip.intersection(Rect::new(
                    0.,
                    0.,
                    self.width as f32 / self.scale,
                    self.height as f32 / self.scale,
                ))?;
                let (x, y) = (whole(clip.x)?, whole(clip.y)?);
                let (right, bottom) = (whole(clip.x + clip.width)?, whole(clip.y + clip.height)?);
                let (dx, dy) = (whole(scroll.dx)?, whole(scroll.dy)?);
                // Destination: the clip minus the exposed strip; source: the
                // same pixels before the move.
                let (x0, x1) = (x.max(x + dx), right.min(right + dx));
                let (y0, y1) = (y.max(y + dy), bottom.min(bottom + dy));
                (x0 < x1 && y0 < y1 && x0 >= 0 && y0 >= 0).then(|| {
                    (
                        (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32),
                        (-dx, -dy),
                    )
                })
            })();
            match pixels {
                Some(copy)
                    if copyable
                        && !self.fresh
                        && self.rendered_serial == Some(serial - 1)
                        && copy_is_exact(scene, scroll, damage, self.scale) =>
                {
                    copies.push(copy)
                }
                _ => repaint.push(scroll.clip),
            }
        }
        (copies, repaint)
    }
    /// Submit commands recorded by the last render, if any.
    fn flush_pending(&self) {
        #[cfg(target_os = "macos")]
        self.finish_direct();
        let Some(mut pending) = self.pending.borrow_mut().take() else {
            return;
        };
        // Not presented (skipped, hidden or read back): draw it the two-pass
        // way so the retained target is complete.
        if let Some(deferred) = self.deferred.borrow_mut().take() {
            let mut pass = pending.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("damage"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let mut stats = GpuStats::default();
            if let Some(blur) = &deferred.blur {
                let pipeline = self
                    .blur_pipelines
                    .as_ref()
                    .expect("filtered")
                    .drawing(blur);
                Self::encode_vertical(&mut pass, pipeline, blur, &mut stats);
            }
            let quads = &self.context.inner.quads;
            let picks = Picks {
                full: quads.get(Shading::Full),
                clear: quads.clear(false),
                basic: quads.get(Shading::Basic),
                opaque: quads.get(Shading::Opaque),
            };
            self.encode_damage(&mut pass, &deferred, picks, &mut stats);
        }
        self.queue.submit([pending.finish()]);
        self.belt.borrow_mut().recall();
        self.submitted();
        if let Some(mapped) = &self.mapped {
            mapped.borrow_mut().submitted();
        }
    }
    fn submitted(&self) {
        #[cfg(target_os = "macos")]
        if let Some(cache) = &self.native_surfaces {
            cache.submitted();
        }
    }
    /// Clear and redraw every damaged region of a deferred frame.
    fn encode_damage(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &Deferred,
        picks: Picks<'_>,
        stats: &mut GpuStats,
    ) {
        pass.set_vertex_buffer(0, frame.vertices.slice(..));
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_bind_group(1, frame.viewport.as_ref(), &[]);
        let (clear, quads) = (picks.clear, picks.full);
        let pick = |shading| picks.get(shading);
        let first = |index: usize| frame.first.get(index).copied().unwrap_or(0);
        if !frame.clear {
            pass.set_pipeline(quads);
            for (index, (region, (x, y, w, h))) in frame.regions.iter().enumerate() {
                pass.set_scissor_rect(*x, *y, *w, *h);
                self.encode_draws(pass, &pick, *region, &frame.draws, first(index), stats);
            }
            return;
        }
        if frame.disjoint {
            // One pipeline switch per phase instead of two per region; many
            // small animations each damage a region of their own.
            pass.set_pipeline(clear);
            for (index, (_, (x, y, w, h))) in frame.regions.iter().enumerate() {
                if first(index) == 0 {
                    pass.set_scissor_rect(*x, *y, *w, *h);
                    pass.draw(0..4, 0..1);
                    stats.draw_calls += 1;
                }
            }
            pass.set_pipeline(quads);
            for (index, (region, _)) in frame.regions.iter().enumerate() {
                self.encode_draws(pass, &pick, *region, &frame.draws, first(index), stats);
            }
            return;
        }
        for (index, (region, (x, y, w, h))) in frame.regions.iter().enumerate() {
            pass.set_scissor_rect(*x, *y, *w, *h);
            if first(index) == 0 {
                pass.set_pipeline(clear);
                pass.set_bind_group(0, &self.bind, &[]);
                pass.draw(0..4, 0..1);
                stats.draw_calls += 1;
            }
            pass.set_pipeline(quads);
            self.encode_draws(pass, &pick, *region, &frame.draws, first(index), stats);
        }
    }
    /// One render pass per frame: copy the retained image into the spare
    /// target and `output`, then redraw the damage into both, and swap. Every
    /// pixel matches the two-pass route because `output`'s format needs no
    /// conversion (see `single_pass_format`).
    fn present_single_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Deferred,
        output: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        if self.single_pass.as_ref().is_none_or(|p| p.format != format) {
            let inner = &self.context.inner;
            let shader = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("single-pass copy"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("copy_both.wgsl").into()),
                });
            let copy = self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("single-pass copy"),
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[Some(FORMAT.into()), Some(format.into())],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
            self.target_copy_bind = None;
            if let Some(spare) = &mut self.spare {
                spare.copy_bind = None;
            }
            let both = |blend, basic| {
                quad_pipeline(
                    &self.device,
                    &inner.quad_shader,
                    &inner.quad_layout,
                    blend,
                    Some(format),
                    basic,
                )
            };
            let direct = |blend, basic| {
                drawable_pipeline(
                    &self.device,
                    &inner.quad_shader,
                    &inner.quad_layout,
                    blend,
                    format,
                    basic,
                )
            };
            self.single_pass = Some(SinglePass {
                format,
                copy,
                both: PickedPipelines {
                    full: both(true, false),
                    clear: both(false, false),
                    basic: both(true, true),
                    opaque: both(false, true),
                },
                direct: PickedPipelines {
                    full: direct(true, false),
                    clear: direct(false, false),
                    basic: direct(true, true),
                    opaque: direct(false, true),
                },
            });
        }
        if !frame.regions.is_empty() {
            self.single_pass_frames += 1;
        }
        let clear = wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        };
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: clear,
            })
        };
        // A frame that clears and redraws the whole target keeps none of the
        // last one: nothing is copied, so it is drawn into the target itself
        // (or the drawable alone, `direct`) and needs no spare.
        let redrawn = frame.clear
            && frame.regions.iter().any(|(_, (x, y, w, h))| {
                (*x, *y) == (0, 0) && *w >= self.width && *h >= self.height
            });
        if frame.direct || redrawn {
            let pipelines = self.single_pass.as_ref().expect("created above");
            let mut stats = GpuStats::default();
            {
                let both = [attachment(&self.view), attachment(output)];
                let alone = [attachment(output)];
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("frame"),
                    color_attachments: if frame.direct { &alone } else { &both },
                    ..Default::default()
                });
                let picks = if frame.direct {
                    pipelines.direct.picks()
                } else {
                    pipelines.both.picks()
                };
                self.encode_damage(&mut pass, frame, picks, &mut stats);
            }
            self.present_stats.draw_calls += stats.draw_calls;
            self.present_stats.instances += stats.instances;
            if frame.direct {
                // The target still holds an earlier frame: the next render
                // repaints everything.
                self.fresh = true;
                self.direct_frames += 1;
            }
            return;
        }
        if self
            .spare
            .as_ref()
            .is_none_or(|t| t.texture.width() != self.width || t.texture.height() != self.height)
        {
            let texture = texture(&self.device, self.width, self.height, "retained output");
            let view = texture.create_view(&Default::default());
            self.spare = Some(Retained {
                texture,
                view,
                copy_bind: None,
                blit_bind: None,
            });
        }
        if frame.blur.is_some() {
            let device = &self.device;
            let blur = self.blur_pipelines.as_mut().expect("filtered");
            blur.both(device, format);
        }
        let pipelines = self.single_pass.as_ref().expect("created above");
        let previous = self
            .target_copy_bind
            .get_or_insert_with(|| blit_group(&self.device, &pipelines.copy, &self.view))
            .clone();
        let spare = self.spare.take().expect("created above");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[attachment(&spare.view), attachment(output)],
                ..Default::default()
            });
            pass.set_pipeline(&pipelines.copy);
            pass.set_bind_group(0, &previous, &[]);
            pass.draw(0..3, 0..1);
            let mut stats = GpuStats::default();
            if let Some(blur) = &frame.blur {
                let both = self.blur_pipelines.as_ref().and_then(|b| b.both.as_ref());
                let (_, pipeline) = both.expect("created above");
                Self::encode_vertical(&mut pass, pipeline, blur, &mut stats);
            }
            self.encode_damage(&mut pass, frame, pipelines.both.picks(), &mut stats);
            self.present_stats.draw_calls += stats.draw_calls + 1;
            self.present_stats.instances += stats.instances;
        }
        // The spare now holds this frame; the old target becomes the spare,
        // keeping its view and bind groups for when it is drawn into again.
        let Retained {
            texture,
            view,
            copy_bind,
            blit_bind,
        } = spare;
        let old = Retained {
            texture: std::mem::replace(&mut self.target, texture),
            view: std::mem::replace(&mut self.view, view),
            copy_bind: std::mem::replace(&mut self.target_copy_bind, copy_bind),
            blit_bind: std::mem::replace(&mut self.blit_bind, blit_bind),
        };
        self.spare = Some(old);
        if self.blit_bind.is_none() {
            self.blit_bind = self
                .blit
                .as_ref()
                .map(|p| blit_group(&self.device, p, &self.view));
        }
    }
    /// Encode `draws` clipped to `region`. Consecutive draws sharing a
    /// texture and clip become one instanced call; their quads are contiguous,
    /// so paint order holds. A draw entirely outside the region may be folded
    /// into a batch: the scissor discards its quads.
    fn encode_draws<'p>(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &dyn Fn(Shading) -> &'p wgpu::RenderPipeline,
        region: Rect,
        draws: &[Draw],
        first: u32,
        stats: &mut GpuStats,
    ) {
        // The pipeline set on `pass`: the caller's, until a batch sets one.
        let mut current = None;
        self.batches(region, draws, first, |batch| {
            self.encode_batch(pass, pipelines, &mut current, batch, stats)
        });
    }
    /// The draws within `region` from instance `first` on, as batches:
    /// consecutive draws sharing a clip, texture source and shading join.
    fn batches(&self, region: Rect, draws: &[Draw], first: u32, mut emit: impl FnMut(Batch)) {
        let mut batch: Option<Batch> = None;
        // Skipping blending pays for its extra draws only over many pixels:
        // a small region (a scrolled-in strip) blends plain quads in one draw.
        let pixels = region.width * region.height * self.scale * self.scale;
        let unblended = pixels >= SPLIT_PIXELS;
        for draw in draws {
            let shading = match draw.shading {
                Shading::Opaque if !unblended => Shading::Basic,
                shading => shading,
            };
            if !region.intersects(draw.bounds) || draw.end <= draw.start.max(first) {
                continue;
            }
            let start = draw.start.max(first);
            let Some(rect) = region.intersection(draw.clip) else {
                if let Some(done) = batch.take() {
                    emit(done);
                }
                continue;
            };
            let source = (draw.image, draw.layer);
            let pixels = draw
                .bounds
                .intersection(rect)
                .map_or(0., |covered| covered.width * covered.height)
                * self.scale
                * self.scale;
            if let Some(open) = &mut batch
                && open.rect == rect
                && open.source == source
            {
                // Another shading joins when what it promotes (quads drawn
                // with a costlier shader than theirs) stays small.
                let merged = match merged_shading(open.shading, shading) {
                    _ if open.shading == shading => Some((shading, open.promoted)),
                    Some(merged) => {
                        let promoted = if merged == open.shading {
                            open.promoted
                        } else {
                            open.pixels
                        } + if merged == shading { 0. } else { pixels };
                        let budget = match merged {
                            Shading::Full => PROMOTED_FULL_PIXELS,
                            _ => PROMOTED_PIXELS,
                        };
                        (promoted <= budget).then_some((merged, promoted))
                    }
                    None => None,
                };
                if let Some((merged, promoted)) = merged {
                    open.shading = merged;
                    open.promoted = promoted;
                    open.pixels += pixels;
                    open.end = draw.end;
                    continue;
                }
            }
            if let Some(done) = batch.take() {
                emit(done);
            }
            batch = Some(Batch {
                rect,
                source,
                shading,
                start,
                end: draw.end,
                pixels,
                promoted: 0.,
            });
        }
        if let Some(done) = batch.take() {
            emit(done);
        }
    }
    fn encode_batch<'p>(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &dyn Fn(Shading) -> &'p wgpu::RenderPipeline,
        current: &mut Option<Shading>,
        batch: Batch,
        stats: &mut GpuStats,
    ) {
        let Some((x, y, w, h)) = scissor(batch.rect, self.scale, self.width, self.height) else {
            return;
        };
        if *current != Some(batch.shading) {
            pass.set_pipeline(pipelines(batch.shading));
            *current = Some(batch.shading);
        }
        pass.set_scissor_rect(x, y, w, h);
        let (image, layer) = batch.source;
        pass.set_bind_group(
            0,
            image.and_then(|id| self.images.get(&id)).map_or_else(
                || {
                    layer
                        .and_then(|id| self.layers.get(&id))
                        .map_or(&self.bind, |l| &l.bind)
                },
                |entry| &entry.0,
            ),
            &[],
        );
        pass.draw(0..4, batch.start..batch.end);
        stats.draw_calls += 1;
        stats.instances += (batch.end - batch.start) as usize;
    }
    /// Plan a batch of backdrop filters (see blur.wgsl): each one's levels
    /// as slots of the atlas, packed, the atlas grown to hold them.
    fn plan_blur(&mut self, filters: &[Filter]) -> Option<BlurPlan> {
        // Mixed over the target by dual-source blending, unless drawn at
        // presentation into two targets at once (which it cannot be): then a
        // copy of each area's pixels to mix with.
        let mixed = self
            .device
            .features()
            .contains(wgpu::Features::DUAL_SOURCE_BLENDING)
            && self.single_pass_format.is_none();
        let scale = self.scale;
        let mut blurs = Vec::new();
        // Each slot with its size in texels and whether it is blurred (else
        // a copy of an area).
        let mut slots: Vec<(SlotRecord, (u32, u32), bool)> = Vec::new();
        let mut draws = Vec::new();
        for filter in filters {
            let Some((ax, ay, aw, ah)) = filter
                .bounds
                .intersection(filter.clip)
                .and_then(|area| scissor(area, scale, self.width, self.height))
            else {
                continue;
            };
            let index = blurs.len() as u32;
            let before = draws.len();
            draws.extend(
                filter
                    .outputs
                    .iter()
                    .filter_map(|output| scissor(*output, scale, self.width, self.height))
                    .map(|output| (index, output)),
            );
            if draws.len() == before {
                continue;
            }
            let sigma = (filter.effects.blur_radius * scale).clamp(0.1, 64.);
            // Finite and non-negative (`Scene::set_effects`).
            let fade = (filter.effects.edge_fade * scale).clamp(0., 1e6);
            // Sigma, and under a fade, each halving of it down to 2 px (below
            // which the backdrop shows through, the blur all but invisible).
            let levels: Vec<f32> = if fade > 0. {
                std::iter::successors(Some(sigma), |s| (s / 2. >= 2.).then_some(s / 2.)).collect()
            } else {
                vec![sigma]
            };
            let area = [ax as f32, ay as f32, (ax + aw) as f32, (ay + ah) as f32];
            let bounds = [
                filter.bounds.x * scale,
                filter.bounds.y * scale,
                filter.bounds.width * scale,
                filter.bounds.height * scale,
            ];
            let edges = [
                bounds[0],
                bounds[1],
                bounds[0] + bounds[2],
                bounds[1] + bounds[3],
            ];
            let first = slots.len() as u32;
            for (k, &level) in levels.iter().enumerate() {
                // Blurred where sigma spans 1.5 to 3 texels: a 16 px blur on a
                // 2x display (sigma 32) runs at an eighth of the size (4
                // texels), sigma 4 at half. Below sigma 3, the full size.
                let factor = (level / 1.5).log2().floor().clamp(0., 3.).exp2();
                // The pixels whose sigma lies within an octave of this
                // level's (see `blurred`), and as far again as its taps reach.
                let region = if levels.len() == 1 {
                    Some(area)
                } else {
                    // Sigma at distance d in: sigma * (d / fade)^2.
                    let at = |octaves: i32| fade * 2_f32.powi(-octaves).sqrt();
                    let near = if k + 1 == levels.len() {
                        0.
                    } else {
                        at(k as i32 + 1)
                    };
                    let far = if k == 0 {
                        f32::INFINITY
                    } else {
                        at(k as i32 - 1)
                    };
                    band(area, edges, near, far)
                };
                let reach = ((3. * level / factor).ceil() + 1.) * factor;
                let [x0, y0, x1, y1] = region.map_or([area[0], area[1], area[0], area[1]], |r| {
                    [
                        (r[0] - reach).max(area[0]).floor(),
                        (r[1] - reach).max(area[1]).floor(),
                        (r[2] + reach).min(area[2]).ceil(),
                        (r[3] + reach).min(area[3]).ceil(),
                    ]
                });
                let size = (
                    ((x1 - x0) / factor).ceil() as u32,
                    ((y1 - y0) / factor).ceil() as u32,
                );
                slots.push((
                    SlotRecord {
                        atlas: [0., 0., size.0 as f32, size.1 as f32],
                        region: [x0, y0, factor, level / factor],
                        owner: [index, 0, 0, 0],
                    },
                    size,
                    true,
                ));
            }
            let original = if mixed {
                u32::MAX
            } else {
                slots.push((
                    SlotRecord {
                        atlas: [0., 0., aw as f32, ah as f32],
                        region: [area[0], area[1], 1., 0.],
                        owner: [index, 0, 0, 0],
                    },
                    (aw, ah),
                    false,
                ));
                slots.len() as u32 - 1
            };
            blurs.push(BlurRecord {
                bounds,
                effect: [
                    filter.effects.opacity,
                    fade,
                    sigma,
                    *levels.last().expect("one level"),
                ],
                area,
                shape: filter.corners.map(|radius| radius * scale),
                slots: [first, levels.len() as u32, original, 0],
            });
        }
        if blurs.is_empty() {
            return None;
        }
        // Shelves as wide as the target, tallest slots first.
        let mut order: Vec<usize> = (0..slots.len()).collect();
        order.sort_by_key(|&k| std::cmp::Reverse(slots[k].1.1));
        let shelf_width = slots.iter().map(|slot| slot.1.0).fold(self.width, u32::max);
        let (mut x, mut y, mut shelf, mut width) = (0, 0, 0, 1);
        for k in order {
            let (w, h) = slots[k].1;
            if x + w > shelf_width {
                (x, y, shelf) = (0, y + shelf, 0);
            }
            slots[k].0.atlas[0] = x as f32;
            slots[k].0.atlas[1] = y as f32;
            x += w;
            shelf = shelf.max(h);
            width = width.max(x);
        }
        let height = (y + shelf).max(1);
        // Grown in steps so a slowly growing panel does not reallocate
        // every frame.
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return None;
        }
        let atlas = self.blur_scratch.take().map(|scratch| scratch.atlas);
        let atlas = [0, 1].map(|i| match &atlas {
            Some(atlas) if atlas[i].width() >= width && atlas[i].height() >= height => {
                atlas[i].clone()
            }
            current => {
                let (current_w, current_h) = current
                    .as_ref()
                    .map_or((0, 0), |a| (a[i].width(), a[i].height()));
                let grow = |want: u32, have: u32| want.max(have).next_multiple_of(64).min(limit);
                atlas_texture(
                    &self.device,
                    grow(width, current_w),
                    grow(height, current_h),
                )
            }
        });
        self.blur_scratch = Some(BlurScratch { atlas });
        Some(BlurPlan {
            blurs,
            slots,
            draws,
            mixed,
        })
    }
    /// Blur a batch of backdrop filters (see `plan_blur`): the reduce,
    /// across and down passes, every slot of the batch drawn in each. Its
    /// records lie in `buffer` (offset and size of the filters', then the
    /// slots').
    fn blur(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &BlurPlan,
        buffer: &wgpu::Buffer,
        ranges: [(u64, u64); 2],
        stats: &mut GpuStats,
    ) -> Option<VerticalBlur> {
        let BlurPlan {
            slots,
            draws,
            mixed,
            ..
        } = plan;
        let mixed = *mixed;
        if self.blur_pipelines.is_none() {
            self.blur_pipelines = Some(BlurPipelines::new(&self.device));
        }
        let atlas = &self.blur_scratch.as_ref().expect("planned").atlas;
        let views = atlas
            .each_ref()
            .map(|texture| texture.create_view(&Default::default()));
        let records = |(offset, size): (u64, u64)| {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset,
                size: wgpu::BufferSize::new(size),
            })
        };
        let pipelines = self.blur_pipelines.as_ref().expect("created above");
        // Binding 1 goes unread except by the copy mix, but may never be the
        // pass's own target.
        let bind = |source: &wgpu::TextureView, other: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("backdrop blur"),
                layout: &pipelines.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(other),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: records(ranges[0]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: records(ranges[1]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&pipelines.linear),
                    },
                ],
            })
        };
        // Whole-area passes: what any output draws depends on the area alone,
        // so a partly damaged filter matches a fresh frame. Copies land in
        // the first atlas with the averages and stay there.
        let copies = slots.iter().any(|slot| !slot.2);
        let passes = [
            (
                "blur reduce",
                &pipelines.reduce,
                bind(&self.view, &views[1]),
                &views[0],
                false,
                false,
            ),
            (
                "blur across",
                &pipelines.across,
                bind(&views[0], &views[0]),
                &views[1],
                true,
                false,
            ),
            (
                "blur down",
                &pipelines.down,
                bind(&views[1], &views[1]),
                &views[0],
                true,
                copies,
            ),
        ];
        for (label, pipeline, group, target, blurred_only, keep) in &passes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if *keep {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            for (k, (record, (w, h), blurred)) in slots.iter().enumerate() {
                if *w == 0 || *h == 0 || (*blurred_only && !blurred) {
                    continue;
                }
                pass.set_scissor_rect(record.atlas[0] as u32, record.atlas[1] as u32, *w, *h);
                pass.draw(0..3, k as u32..k as u32 + 1);
                stats.draw_calls += 1;
            }
        }
        stats.render_passes += passes.len();
        stats.blur_passes += passes.len();
        Some(VerticalBlur {
            bind: bind(&views[0], &views[0]),
            draws: draws.clone(),
            mixed,
        })
    }
    /// Draw a batch of filters back over the target, at the start of the
    /// pass that follows their blur passes.
    fn encode_vertical(
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        blur: &VerticalBlur,
        stats: &mut GpuStats,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &blur.bind, &[]);
        for &(index, (x, y, width, height)) in &blur.draws {
            pass.set_scissor_rect(x, y, width, height);
            pass.draw(0..3, index..index + 1);
            stats.draw_calls += 1;
        }
    }
    /// Record `id` as `node`'s raster, dropping the texture it replaces.
    fn retire_node_image(&mut self, node: NodeId, id: u64) {
        if let Some(old) = self.node_images.insert(node, id)
            && old != id
        {
            self.images.remove(&old);
        }
    }
    /// The texture of `node`'s canvas raster, uploaded if it is new; the
    /// cache then keeps no pixels. A lost texture rasterizes again.
    fn canvas_texture(
        &mut self,
        node: NodeId,
        stats: &mut GpuStats,
        mut raster: impl FnMut(&mut canvas::CanvasCache) -> Result<canvas::Raster, GpuError>,
    ) -> Result<u64, GpuError> {
        let mut found = raster(&mut self.canvases)?;
        self.retire_node_image(node, found.id);
        if !self.images.contains_key(&found.id) {
            if found.pixels.is_none() {
                self.canvases.forget(node);
                found = raster(&mut self.canvases)?;
            }
            let pixels = found.pixels.expect("fresh raster");
            self.upload_image(&pixels)?;
            stats.image_uploads += 1;
        }
        self.canvases.uploaded(node);
        Ok(found.id)
    }
    fn upload_image(&mut self, image: &zgui::image::ImageData) -> Result<(), GpuError> {
        let bytes = image.pixels().len();
        if bytes + self.images.values().map(|entry| entry.1).sum::<usize>() > 64 * 1024 * 1024 {
            return Err(GpuError(
                "visible image textures exceed the 64 MiB budget".into(),
            ));
        }
        if self.images.len() >= MAX_IMAGES {
            return Err(GpuError(format!(
                "visible images exceed the budget of {MAX_IMAGES}"
            )));
        }
        if image.width() > self.device.limits().max_texture_dimension_2d
            || image.height() > self.device.limits().max_texture_dimension_2d
        {
            return Err(GpuError("image exceeds GPU texture dimensions".into()));
        }
        let texture = sampled_texture(
            &self.device,
            self.mapped.is_some(),
            image.width(),
            image.height(),
            "image",
        );
        let mut pixels = image.pixels().to_vec();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
        self.write_texels(&texture, (0, 0, image.width(), image.height()), &mut pixels);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image"),
            layout: &self.context.inner.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.device.create_sampler(
                        &wgpu::SamplerDescriptor {
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            ..Default::default()
                        },
                    )),
                },
            ],
        });
        self.images.insert(image.id(), (bind, bytes, Some(texture)));
        Ok(())
    }
    /// Write premultiplied RGBA texels to `(x, y, width, height)` of a
    /// `sampled_texture`, which no submitted frame samples there.
    fn write_texels(
        &mut self,
        target: &wgpu::Texture,
        (x, y, width, height): (u32, u32, u32, u32),
        rgba: &mut [u8],
    ) {
        #[cfg(target_os = "macos")]
        if self.mapped.is_some() {
            upload::write_texels(target, (x, y, width, height), rgba);
            return;
        }
        if self.mapped.is_some() {
            self.texture_uploads
                .push(target, (x, y, width, height), rgba);
        } else {
            // A copy keeps bytes as they are: in `FORMAT`'s order, BGRA (an
            // upload pass writes colours, which the target stores in its own).
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    /// Before reusing atlas cells: the CPU writes them in place (macOS), so
    /// every frame that sampled the old ones must have finished.
    fn release_atlas_cells(&self) {
        if cfg!(target_os = "macos") && self.mapped.is_some() {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }
    }
    fn reset_atlas(&mut self, size: u32) {
        self.atlas = sampled_texture(
            &self.device,
            self.mapped.is_some(),
            size,
            size,
            "glyph atlas",
        );
        self.atlas_size = size;
        self.bind = self.texture_bind(&self.atlas.create_view(&Default::default()), "glyph atlas");
        self.entries.clear();
        self.image_cells.clear();
        self.cursor = (0, 0, 0);
        self.atlas_epoch = self.atlas_epoch.wrapping_add(1);
    }
    /// Make `visible_glyphs` the quads of `shaped`'s lines that can reach
    /// the band `top` to `top + height` (from its origin, logical points),
    /// if that leaves out most of its glyphs; whether it did. Out of line:
    /// what the per-glyph loops of `render_flat` hold stays small.
    #[inline(never)]
    fn visible_lines(
        &mut self,
        shaped: &Shaped,
        top: f32,
        height: f32,
        font_size: f32,
        stats: &mut GpuStats,
    ) -> Result<bool, GpuError> {
        // Lines are laid out downward: glyph baselines (device pixels from
        // its top) never decrease. Glyphs reach past their line by less
        // than `reach`.
        let largest = shaped.rich.as_ref().map_or(font_size, |rich| {
            rich.runs()
                .iter()
                .map(|run| run.font_size)
                .fold(font_size, f32::max)
        });
        let reach = 3. * largest;
        let (top, bottom) = (top - reach, top + height + reach);
        let glyphs = &shaped.glyphs;
        let first = glyphs.partition_point(|g| g.y < top * self.scale);
        let end = glyphs
            .partition_point(|g| g.y <= bottom * self.scale)
            .max(first);
        // Worth it when it leaves out most of its glyphs: a label cut by an
        // edge copies its kept quads, as before.
        if (end - first) * 2 >= glyphs.len() {
            return Ok(false);
        }
        let mut visible = std::mem::take(&mut self.visible_glyphs);
        visible.clear();
        let lines = |quad: &Quad| quad.rect[1] + quad.rect[3] >= top && quad.rect[1] <= bottom;
        let decorations = |background: bool| {
            shaped
                .decorations
                .iter()
                .filter(move |d| d.background == background)
                .map(rich_decoration_quad)
                .filter(lines)
        };
        visible.extend(decorations(true));
        for glyph in &glyphs[first..end] {
            // As `glyph`, through another lookup: `glyph` keeps its one caller.
            let size = f32::from_bits(glyph.key.font_size_bits);
            if !size.is_finite() || size > MAX_ATLAS as f32 {
                return Err(GpuError(
                    "glyph font size exceeds atlas raster limit".into(),
                ));
            }
            let entry = match self.entries.get_mut(&glyph.key) {
                Some(entry) => *entry,
                None => self.raster_glyph(glyph.key, stats)?,
            };
            if let Some(entry) = entry {
                visible.push(glyph_quad(glyph, entry, self.scale, self.atlas_size));
            }
        }
        visible.extend(decorations(false));
        self.visible_glyphs = visible;
        Ok(true)
    }
    fn glyph(
        &mut self,
        key: CacheKey,
        stats: &mut GpuStats,
    ) -> Result<Option<AtlasEntry>, GpuError> {
        let size = f32::from_bits(key.font_size_bits);
        if !size.is_finite() || size > MAX_ATLAS as f32 {
            return Err(GpuError(
                "glyph font size exceeds atlas raster limit".into(),
            ));
        }
        if let Some(entry) = self.entries.get(&key) {
            return Ok(*entry);
        }
        self.raster_glyph(key, stats)
    }
    /// `glyph`'s miss: rasterize `key` into the atlas. Out of line, and
    /// `glyph`, the per-glyph lookup, has one caller: inlined there.
    #[inline(never)]
    fn raster_glyph(
        &mut self,
        key: CacheKey,
        stats: &mut GpuStats,
    ) -> Result<Option<AtlasEntry>, GpuError> {
        if self.entries.len() >= 8192 {
            return Err(GpuError(
                "glyph metadata exceeds 8192 entries in one frame".into(),
            ));
        }
        let Some(image) = self
            .swash
            .get_image_uncached(&mut self.fonts.borrow_mut(), key)
        else {
            self.entries.insert(key, None);
            return Ok(None);
        };
        let p = image.placement;
        if p.width == 0 || p.height == 0 {
            self.entries.insert(key, None);
            return Ok(None);
        }
        let cell_width = p.width + 2;
        let (x, y) = self.atlas_cell(p.width, p.height, |rgba| {
            let row_start = |row: u32| ((row + 1) * cell_width + 1) as usize * 4;
            match image.content {
                SwashContent::Mask => {
                    for (row, alphas) in image.data.chunks_exact(p.width as usize).enumerate() {
                        let start = row_start(row as u32);
                        for (texel, alpha) in
                            rgba[start..].as_chunks_mut::<4>().0.iter_mut().zip(alphas)
                        {
                            texel.copy_from_slice(&[255, 255, 255, *alpha]);
                        }
                    }
                }
                SwashContent::Color => {
                    let bytes = p.width as usize * 4;
                    for (row, texels) in image.data.chunks_exact(bytes).enumerate() {
                        let start = row_start(row as u32);
                        rgba[start..start + bytes].copy_from_slice(texels);
                    }
                }
                SwashContent::SubpixelMask => {
                    let bytes = p.width as usize * 4;
                    for (row, pixels) in image.data.chunks_exact(bytes).enumerate() {
                        let start = row_start(row as u32);
                        for (texel, pixel) in rgba[start..]
                            .as_chunks_mut::<4>()
                            .0
                            .iter_mut()
                            .zip(pixels.as_chunks::<4>().0)
                        {
                            texel.copy_from_slice(&[
                                255,
                                255,
                                255,
                                pixel[0].max(pixel[1]).max(pixel[2]),
                            ]);
                        }
                    }
                }
            }
        })?;
        let entry = AtlasEntry {
            x,
            y,
            width: p.width,
            height: p.height,
            left: p.left,
            top: p.top,
        };
        self.entries.insert(key, Some(entry));
        stats.glyph_uploads += 1;
        Ok(Some(entry))
    }
    /// Reserve a `width`×`height` atlas cell with a transparent 1-texel border
    /// and upload it; `fill` writes the bordered RGBA cell. Returns the
    /// interior's origin. Linear sampling at fractional positions reads one
    /// texel past the content, and a recycled atlas is not cleared, so shared
    /// gutters could otherwise hold stale texels.
    fn atlas_cell(
        &mut self,
        width: u32,
        height: u32,
        fill: impl FnOnce(&mut [u8]),
    ) -> Result<(u32, u32), GpuError> {
        if self.atlas_size == 1 {
            self.reset_atlas(INITIAL_ATLAS);
        }
        let (cell_width, cell_height) = (width + 2, height + 2);
        if self.cursor.0 + cell_width > self.atlas_size {
            self.cursor.0 = 0;
            self.cursor.1 += self.cursor.2;
            self.cursor.2 = 0;
        }
        if self.cursor.1 + cell_height > self.atlas_size || cell_width > self.atlas_size {
            return Err(GpuError(
                "glyph atlas capacity exceeded in one frame".into(),
            ));
        }
        let mut rgba = vec![0; (cell_width * cell_height * 4) as usize];
        fill(&mut rgba);
        let atlas = self.atlas.clone();
        self.write_texels(
            &atlas,
            (self.cursor.0, self.cursor.1, cell_width, cell_height),
            &mut rgba,
        );
        let origin = (self.cursor.0 + 1, self.cursor.1 + 1);
        self.cursor.0 += cell_width;
        self.cursor.2 = self.cursor.2.max(cell_height);
        Ok(origin)
    }
    /// Present the retained target. Use `present_with_status` when scheduling retries.
    pub fn present(&mut self) -> Result<(), GpuError> {
        self.present_with_status().map(|_| ())
    }
    /// Present without discarding whether native acquisition skipped this frame.
    pub fn present_with_status(&mut self) -> Result<PresentationStatus, GpuError> {
        self.present_with_notify(|| {})
    }
    /// The copies bringing the current surface up to date from the one shown
    /// (`buffers[shown]`): the changes since it was last shown, less what `regions`
    /// repaint and `copies` cover this frame, then the scroll `copies`.
    #[cfg(target_os = "macos")]
    fn catch_up_draws(
        &mut self,
        shown: usize,
        copies: &[ScrollCopy],
        regions: &[(Rect, Scissor)],
    ) -> Option<(wgpu::BindGroup, Vec<ScrollCopy>)> {
        let mut missing = self
            .layer_present
            .as_mut()
            .map(|present| std::mem::take(&mut present.catch_up))
            .unwrap_or_default();
        let mut rest = Vec::new();
        // Whatever this frame repaints or copies over needs nothing first.
        let cuts = regions
            .iter()
            .map(|(_, s)| *s)
            .chain(copies.iter().map(|(s, _)| *s));
        for cut in cuts {
            for rect in missing.drain(..) {
                scissor_minus(rect, cut, &mut rest);
            }
            std::mem::swap(&mut missing, &mut rest);
        }
        // Frames often change the same places: draw each pixel once where
        // one rectangle holds another.
        missing
            .sort_unstable_by_key(|&(_, _, w, h)| std::cmp::Reverse(u64::from(w) * u64::from(h)));
        let mut kept: Vec<Scissor> = Vec::with_capacity(missing.len());
        for rect in missing {
            if !kept.iter().any(|&outer| scissor_holds(outer, rect)) {
                kept.push(rect);
            }
        }
        if kept.is_empty() && copies.is_empty() {
            return None;
        }
        let pipeline = self.copy_offset.get_or_insert_with(|| {
            let shader = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("shifted copy"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("copy_offset.wgsl").into()),
                });
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("shifted copy"),
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        // A quad's rect (device pixels) and uv (the shift).
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Quad>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                        }],
                    },
                    primitive: QUAD_STRIP,
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[Some(FORMAT.into())],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
        });
        // Made once per surface: it is the source every other frame.
        let source = &self.layer_present.as_ref()?.buffers[shown].0;
        let bind = source
            .copy_source
            .get_or_init(|| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shifted copy source"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source.view),
                    }],
                })
            })
            .clone();
        let draws = kept
            .into_iter()
            .map(|rect| (rect, (0, 0)))
            .chain(copies.iter().copied())
            .collect();
        Some((bind, draws))
    }
    /// Make a surface the retained target for the frame about to draw,
    /// unless one is: one the window server is done with, with the regions it
    /// lacks of the last frame queued to copy from the shown surface (none
    /// when `whole`: the frame repaints everything).
    #[cfg(target_os = "macos")]
    fn begin_layer_frame(&mut self, whole: bool) -> Result<(), GpuError> {
        let (width, height) = (self.width, self.height);
        let Some(present) = self.layer_present.as_mut() else {
            return Ok(());
        };
        if present.current.is_some() {
            return Ok(());
        }
        if present
            .buffers
            .iter()
            .any(|(b, _)| (b.texture.width(), b.texture.height()) != (width, height))
        {
            // Resized: the layer keeps showing the old surface until the next.
            present.buffers.clear();
            present.shown = None;
            present.history.clear();
        }
        // Certainly free: not the latest, and no use beyond the layer's own
        // record of it (one on screen or queued has more). The most recent of
        // them holds the frame nearest this one.
        let idle = |present: &LayerPresent| -> Vec<usize> {
            let mut idle: Vec<usize> = (0..present.buffers.len())
                .filter(|&i| {
                    Some(i) != present.shown && present.buffers[i].0.surface.use_count() <= 1
                })
                .collect();
            idle.sort_by_key(|&i| std::cmp::Reverse(present.buffers[i].1));
            idle
        };
        let budget = if present.full_rate { LAYER_BUFFERS } else { 2 };
        let mut free = idle(present);
        // Slower again: the third surface goes once free.
        if present.buffers.len() > budget
            && let Some(&spare) = free.last()
        {
            present.buffers.remove(spare);
            if let Some(shown) = &mut present.shown
                && *shown > spare
            {
                *shown -= 1;
            }
            free = idle(present);
        }
        // None: ask the layer which it no longer shows nor queues (it holds a
        // surface a while after showing the next; that one is free).
        let pick = |present: &LayerPresent| {
            let busy = present.busy();
            (0..present.buffers.len())
                .filter(|&i| Some(i) != present.shown && !busy[i])
                .max_by_key(|&i| present.buffers[i].1)
        };
        let mut index = free.first().copied().or_else(|| pick(present));
        if index.is_none() && present.buffers.len() < budget {
            present.buffers.push((
                layer_present::Buffer::new(&self.device, width, height)?,
                None,
            ));
            index = Some(present.buffers.len() - 1);
            present_log::log(|| {
                format!(
                    "new-surface count={} budget={budget}",
                    present.buffers.len()
                )
            });
        }
        // Still none: the layer moves on by its next refresh.
        let started = std::time::Instant::now();
        let mut asked = started;
        while index.is_none() && started.elapsed() < std::time::Duration::from_millis(20) {
            std::thread::sleep(std::time::Duration::from_micros(100));
            index = idle(present).first().copied();
            if index.is_none() && asked.elapsed() >= std::time::Duration::from_millis(2) {
                asked = std::time::Instant::now();
                index = pick(present);
            }
        }
        let waited = started.elapsed();
        let forced = index.is_none();
        let index = index.unwrap_or_else(|| {
            (0..present.buffers.len())
                .find(|&i| Some(i) != present.shown)
                .expect("more than one surface")
        });
        if forced || waited.as_micros() > 500 {
            present_log::log(|| {
                format!(
                    "{} acquire index={index} waited_us={} forced_busy={forced} buffers={}",
                    present.plane(),
                    waited.as_micros(),
                    present.buffers.len()
                )
            });
        }
        let held = present.buffers[index].1;
        let latest = present.shown.and_then(|shown| present.buffers[shown].1);
        present_log::log(|| {
            format!(
                "{} draw whole={whole} held={held:?} latest={latest:?}",
                present.plane()
            )
        });
        present.catch_up = match (whole, latest) {
            (true, _) | (false, None) => Vec::new(),
            (false, Some(latest)) if held == Some(latest) => Vec::new(),
            (false, Some(_)) => match held {
                // The changes of every frame since the one it holds.
                Some(held) if present.history.front().is_some_and(|(f, _)| *f <= held + 1) => {
                    present
                        .history
                        .iter()
                        .filter(|(frame, _)| *frame > held)
                        .flat_map(|(_, changed)| changed.iter().copied())
                        .collect()
                }
                _ => vec![(0, 0, width, height)],
            },
        };
        let buffer = &present.buffers[index].0;
        self.target = buffer.texture.clone();
        self.view = buffer.view.clone();
        present.current = Some(index);
        Ok(())
    }
    /// Show the retained frame (the current surface) through the layer.
    #[cfg(target_os = "macos")]
    fn present_layer(
        &mut self,
        before_present: impl FnOnce(),
    ) -> Result<PresentationStatus, GpuError> {
        let mut present = self.layer_present.take().expect("layer presenting");
        if present.layer.is_none()
            && let Some(host) = &present.host
        {
            present.layer = Some(layer_present::Layer::attach(host));
        }
        if present.layer.is_none() {
            let surface = self.surface.as_ref().expect("a window surface");
            // SAFETY: Only the surface's layer is used; nothing destroys it.
            let hal = unsafe { surface.as_hal::<wgpu::hal::api::Metal>() }
                .ok_or_else(|| GpuError("not a Metal surface".into()))?;
            present.layer = Some(layer_present::Layer::attach(&hal.render_layer().lock()));
        }
        let Some(index) = present.current.take() else {
            // Nothing drawn since: the shown frame stands.
            self.layer_present = Some(present);
            return Ok(PresentationStatus::Presented);
        };
        // The window server reads the surface from its next composite: the
        // frame must be complete by then.
        if self.finish_direct() {
            // wgpu still frees what was dropped once its own work is done.
            if present.frame.is_multiple_of(64) {
                let _ = self.device.poll(wgpu::PollType::Poll);
            }
        } else {
            let encoder = self
                .pending
                .get_mut()
                .take()
                .unwrap_or_else(|| self.device.create_command_encoder(&Default::default()));
            let submission = self.queue.submit([encoder.finish()]);
            self.belt.get_mut().recall();
            self.submitted();
            if let Some(mapped) = self.mapped.as_mut() {
                mapped.get_mut().submitted();
            }
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            });
        }
        before_present();
        let plane = present.plane();
        let opaque = self.background.3 == 255;
        let plain = present
            .resized_at
            .is_some_and(|at| at.elapsed() < RESIZE_SETTLE)
            || present.plain_only
            // Slower frames than the display's (a 30 fps shimmer, a clock)
            // are a plain layer's contents: the video layer queues frames
            // on threads of its own, a cost worth paying only for a full
            // rate. It stays a second after one, so a pause in a scroll
            // does not drop and rebuild it.
            || (!present.full_rate
                && present
                    .video_until
                    .is_none_or(|until| std::time::Instant::now() >= until))
            || present.with_natives
            || present
                .plain_until
                .is_some_and(|until| std::time::Instant::now() < until);
        present_log::log(|| {
            format!(
                "{} present frame={} index={index} held={:?} path={} full_rate={} opaque={opaque} catch_up={}",
                plane,
                present.frame + 1,
                present.buffers[index].1,
                if plain { "plain" } else { "show" },
                present.full_rate,
                present.catch_up.len()
            )
        });
        let layer = present.layer.as_mut().expect("attached above");
        let buffer = &present.buffers[index].0;
        if plain {
            layer.show_plain(buffer, opaque);
        } else {
            layer.show(buffer, opaque, present.full_rate);
        }
        present.frame += 1;
        let frame = present.frame;
        present.buffers[index].1 = Some(frame);
        present.shown = Some(index);
        let changed = std::mem::take(&mut present.changed);
        present.history.push_back((frame, changed));
        if present.history.len() > LAYER_HISTORY {
            present.history.pop_front();
        }
        self.layer_present = Some(present);
        Ok(PresentationStatus::Presented)
    }
    /// Whether a frame drawn into the renderer's own surfaces waits to be
    /// presented (`present` then shows it; without one it does nothing).
    #[cfg(target_os = "macos")]
    pub fn frame_pending(&self) -> bool {
        self.layer_present
            .as_ref()
            .is_some_and(|present| present.current.is_some())
    }
    /// The frame its layer shows (a popup window's), for an image of it (a
    /// drag image): width and height in pixels, then premultiplied BGRA rows.
    /// Waits for the GPU to finish it.
    #[cfg(target_os = "macos")]
    pub fn shown_pixels(&self) -> Option<(u32, u32, Vec<u8>)> {
        use std::ffi::c_void;
        // SAFETY: IOSurface's declarations (IOSurface.framework, linked by
        // objc2-io-surface): `IOSurfaceLockOptions` and the seed are `u32`,
        // `kern_return_t` is `i32`.
        unsafe extern "C" {
            fn IOSurfaceLock(surface: *const c_void, options: u32, seed: *mut u32) -> i32;
            fn IOSurfaceUnlock(surface: *const c_void, options: u32, seed: *mut u32) -> i32;
            fn IOSurfaceGetBaseAddress(surface: *const c_void) -> *const u8;
            fn IOSurfaceGetBytesPerRow(surface: *const c_void) -> usize;
            fn IOSurfaceGetWidth(surface: *const c_void) -> usize;
            fn IOSurfaceGetHeight(surface: *const c_void) -> usize;
        }
        let present = self.layer_present.as_ref()?;
        let (buffer, _) = present.buffers.get(present.shown?)?;
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let surface = objc2_core_foundation::CFRetained::as_ptr(&buffer.surface)
            .as_ptr()
            .cast::<c_void>()
            .cast_const();
        // SAFETY: a live surface of the renderer's, locked read-only (1)
        // while its rows are copied: `height` rows `stride` bytes apart, each
        // at least `width * 4` bytes (a BGRA surface). The GPU finished it
        // (direct frames are waited for before they are shown; wgpu's above).
        unsafe {
            if IOSurfaceLock(surface, 1, std::ptr::null_mut()) != 0 {
                return None;
            }
            let (width, height) = (IOSurfaceGetWidth(surface), IOSurfaceGetHeight(surface));
            let (base, stride) = (
                IOSurfaceGetBaseAddress(surface),
                IOSurfaceGetBytesPerRow(surface),
            );
            let mut pixels = Vec::with_capacity(width * height * 4);
            for row in 0..height {
                pixels.extend_from_slice(std::slice::from_raw_parts(
                    base.add(row * stride),
                    width * 4,
                ));
            }
            IOSurfaceUnlock(surface, 1, std::ptr::null_mut());
            Some((width as u32, height as u32, pixels))
        }
    }
    /// How long the GPU took to run the frame last presented, when it was
    /// encoded with Metal directly: the frame's own work, plus however long
    /// other work on the GPU (the window server's compositing, other apps)
    /// held it up. Taken: `None` until another such frame presents.
    #[cfg(target_os = "macos")]
    pub fn take_gpu_time(&mut self) -> Option<std::time::Duration> {
        self.gpu_time.take()
    }
    /// Whether frames come at the display's full rate (animating or
    /// scrolling). On macOS those are queued by a video layer, which still
    /// holds the surface shown before the shown one when the next frame
    /// starts, so they keep a third surface; slower frames are a plain
    /// layer's contents, each replacing the last. Elsewhere ignored.
    pub fn set_full_rate(&mut self, #[allow(unused)] full: bool) {
        #[cfg(target_os = "macos")]
        if let Some(present) = &mut self.layer_present {
            if present.full_rate && !full {
                present.video_until = Some(std::time::Instant::now() + VIDEO_LINGER);
            } else if full {
                present.video_until = None;
            }
            present.full_rate = full;
        }
    }
    /// Encode plain frames with Metal directly (the default on macOS) or
    /// always through wgpu: for comparing the two. Elsewhere ignored.
    #[doc(hidden)]
    pub fn set_direct_metal(&mut self, #[allow(unused)] enabled: bool) {
        #[cfg(target_os = "macos")]
        {
            self.flush_pending();
            self.metal_frame = if enabled {
                Default::default()
            } else {
                std::cell::OnceCell::from(None)
            };
        }
    }
    /// Notify the native host immediately before committing an acquired frame.
    /// Skipped acquisition never invokes the callback, so Wayland hosts do not
    /// wait for a frame callback on a surface commit that never happened.
    pub fn present_with_notify(
        &mut self,
        before_present: impl FnOnce(),
    ) -> Result<PresentationStatus, GpuError> {
        let _span = zgui::profile::span(zgui::profile::Phase::Present);
        if let Some(reason) = self.context.lost() {
            return Err(GpuError::device_lost(&reason));
        }
        #[cfg(target_os = "macos")]
        if self.layer_present.is_some() {
            return self.present_layer(before_present);
        }
        if self.surface.is_none() {
            return Ok(PresentationStatus::Offscreen);
        }
        let frame = match surface::acquire(self)? {
            surface::Acquired::Frame(frame) => frame,
            surface::Acquired::Skipped(status) => {
                // The retained target must still be complete for the retry.
                self.flush_pending();
                return Ok(status);
            }
        };
        let mut encoder = self
            .pending
            .get_mut()
            .take()
            .unwrap_or_else(|| self.device.create_command_encoder(&Default::default()));
        if let Some(deferred) = self.deferred.get_mut().take() {
            let format = self
                .single_pass_format
                .expect("deferred only with a view format");
            let raw = frame.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                ..Default::default()
            });
            self.present_single_pass(&mut encoder, &deferred, &raw, format);
        } else if self.copy_present {
            encoder.copy_texture_to_texture(
                self.target.as_image_copy(),
                frame.texture.as_image_copy(),
                self.target.size(),
            );
        } else {
            let view = frame.texture.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(self.blit.as_ref().unwrap());
            pass.set_bind_group(0, self.blit_bind.as_ref().unwrap(), &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.belt.get_mut().recall();
        self.submitted();
        if let Some(mapped) = self.mapped.as_mut() {
            mapped.get_mut().submitted();
        }
        before_present();
        frame.present();
        #[cfg(windows)]
        if let Some(surface) = &self.surface {
            self.dxgi_stats.presented(surface);
        }
        // Transactional presents block on scheduling; keep them to resizes.
        #[cfg(target_os = "macos")]
        self.set_transaction_present(false);
        #[cfg(target_os = "macos")]
        if self
            .wide_until
            .is_some_and(|until| std::time::Instant::now() >= until)
        {
            self.narrow_drawables();
            // Free the third drawable now rather than when the pool turns over.
            self.release_drawables();
        }
        Ok(PresentationStatus::Presented)
    }
    /// Release memory that only animation needs: filter scratch textures,
    /// idle vertex buffers and, on macOS, Core Animation's pool of window-sized
    /// drawables (two of three, ~13 MB each at 5K). The window keeps showing
    /// its last frame; the next frame recreates whatever it uses. Call when a
    /// window has stopped presenting for a while.
    pub fn trim(&mut self) {
        present_log::log(|| {
            #[cfg(target_os = "macos")]
            let plane = self.layer_present.as_ref().map_or("none", |p| p.plane());
            #[cfg(not(target_os = "macos"))]
            let plane = "none";
            format!("{plane} trim")
        });
        self.flush_pending();
        self.blur_scratch = None;
        // Effect slots draw again from their layers when next damaged.
        self.effects = None;
        self.images.remove(&effects::EFFECT_ATLAS);
        self.spare = None;
        self.layers.retain(|_, cache| !cache.parked);
        // Idle: only the shown surface is needed; the next frame makes another.
        #[cfg(target_os = "macos")]
        if let Some(present) = &mut self.layer_present
            && present.current.is_none()
            && let Some(shown) = present.shown
        {
            let kept = present.buffers.swap_remove(shown);
            present.buffers = vec![kept];
            present.shown = Some(0);
            if let Some(layer) = &mut present.layer {
                layer.rest(&present.buffers[0].0);
            }
        }
        if let Some(mapped) = &mut self.mapped {
            mapped.get_mut().trim();
        }
        // Word shapes kept for the next frame, and the table they peaked in
        // (streaming fills it): the next frame shapes what it needs again.
        self.fonts.borrow_mut().shape_run_cache = Default::default();
        #[cfg(target_os = "macos")]
        {
            self.narrow_drawables();
            self.release_drawables();
        }
    }
    #[cfg(target_os = "macos")]
    fn release_drawables(&mut self) {
        let Some(surface) = &self.surface else {
            return;
        };
        // SAFETY: Only the surface's layer is used; nothing destroys it.
        let Some(hal) = (unsafe { surface.as_hal::<wgpu::hal::api::Metal>() }) else {
            return;
        };
        let layer = hal.render_layer().lock();
        // Changing drawableSize discards pooled drawables; restoring it at once
        // leaves the configuration and the displayed contents untouched.
        // SAFETY: `drawableSize` is a CGSize property of CAMetalLayer.
        unsafe {
            let size: objc2_core_foundation::CGSize = objc2::msg_send![&**layer, drawableSize];
            let nudged = objc2_core_foundation::CGSize::new(size.width, size.height + 1.);
            let _: () = objc2::msg_send![&**layer, setDrawableSize: nudged];
            let _: () = objc2::msg_send![&**layer, setDrawableSize: size];
        }
    }
    /// A resized layer holds its transactional drawable until the window
    /// server shows the new frame size, up to a second later. With two
    /// drawables the next frame waits that long for one (every step of a
    /// drag resize would stall): keep a third until resizing settles.
    #[cfg(target_os = "macos")]
    fn widen_drawables(&mut self) {
        // Each resize configures the surface, which restores two.
        self.set_drawable_count(3);
        self.wide_until = Some(std::time::Instant::now() + WIDE_DRAWABLES);
    }
    #[cfg(target_os = "macos")]
    fn narrow_drawables(&mut self) {
        if self.wide_until.take().is_some() {
            self.set_drawable_count(2);
        }
    }
    #[cfg(target_os = "macos")]
    fn set_drawable_count(&self, count: usize) {
        let Some(surface) = &self.surface else {
            return;
        };
        // SAFETY: Only the surface's layer is used; nothing destroys it.
        let Some(hal) = (unsafe { surface.as_hal::<wgpu::hal::api::Metal>() }) else {
            return;
        };
        let layer = hal.render_layer().lock();
        // SAFETY: `maximumDrawableCount` is an NSUInteger property of CAMetalLayer.
        let _: () = unsafe { objc2::msg_send![&**layer, setMaximumDrawableCount: count] };
    }
    #[cfg(target_os = "macos")]
    fn set_transaction_present(&mut self, enabled: bool) {
        if self.transaction_present == enabled {
            return;
        }
        let Some(surface) = &self.surface else {
            return;
        };
        // SAFETY: Only the surface's layer is used; nothing destroys it.
        let Some(hal) = (unsafe { surface.as_hal::<wgpu::hal::api::Metal>() }) else {
            return;
        };
        let layer = hal.render_layer().lock();
        // SAFETY: `presentsWithTransaction` is a BOOL property of CAMetalLayer.
        let _: () = unsafe { objc2::msg_send![&**layer, setPresentsWithTransaction: enabled] };
        self.transaction_present = enabled;
    }
    /// Frames whose damage was drawn in the same pass as presentation.
    /// Frames presented to the drawable alone, the retained target untouched.
    pub fn debug_direct_frames(&self) -> u64 {
        self.direct_frames
    }
    /// Frames drawn in one pass with their presentation.
    pub fn debug_single_pass_frames(&self) -> u64 {
        self.single_pass_frames
    }
    /// Frames encoded with Metal directly rather than through wgpu (macOS).
    pub fn debug_metal_frames(&self) -> u64 {
        self.metal_frames
    }
    /// Test hook: draw plain quads with the basic shader, or every quad with
    /// the full shader.
    pub fn debug_split_shading(&mut self, split: bool) {
        self.split_shading = split;
    }
    /// Test hook: make quads for the lines of a text partly in its clip
    /// alone (the default), or for all of them.
    pub fn debug_cull_glyphs(&mut self, cull: bool) {
        self.cull_glyphs = cull;
    }
    /// Test hook: treat an offscreen `Bgra8Unorm` texture as the drawable, so
    /// single-pass presentation runs without a window.
    pub fn debug_enable_single_pass(&mut self) {
        self.single_pass_format = Some(wgpu::TextureFormat::Bgra8Unorm);
        self.present_unpremultiplies = !cfg!(target_os = "macos");
    }
    /// Test hook: present into a fresh offscreen drawable configured like a
    /// window (`Bgra8UnormSrgb`, non-opaque) and return its
    /// bytes in RGBA order. Single-pass renderers draw the pending frame
    /// through a non-sRGB view; others use the present blit, as on screen.
    pub fn debug_present_offscreen(&mut self) -> Result<Vec<u8>, GpuError> {
        let srgb = wgpu::TextureFormat::Bgra8UnormSrgb;
        let drawable = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen drawable"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: srgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Bgra8Unorm],
        });
        let mut encoder = self
            .pending
            .get_mut()
            .take()
            .unwrap_or_else(|| self.device.create_command_encoder(&Default::default()));
        if let Some(frame) = self.deferred.get_mut().take() {
            let raw = drawable.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Bgra8Unorm),
                ..Default::default()
            });
            self.present_single_pass(&mut encoder, &frame, &raw, wgpu::TextureFormat::Bgra8Unorm);
        } else {
            let pipeline = blit_pipeline(&self.device, srgb, !cfg!(target_os = "macos"));
            let bind = blit_group(&self.device, &pipeline, &self.view);
            let view = drawable.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.submitted();
        if let Some(mapped) = self.mapped.as_mut() {
            mapped.get_mut().submitted();
        }
        let mut pixels = self.read_texture(&drawable)?;
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Ok(pixels)
    }
    /// The target's pixels, RGBA, a row at a time.
    pub fn readback(&self) -> Result<Vec<u8>, GpuError> {
        if let Some(reason) = self.context.lost() {
            return Err(GpuError::device_lost(&reason));
        }
        self.flush_pending();
        self.read_texture(&self.target)
    }
    fn read_texture(&self, texture: &wgpu::Texture) -> Result<Vec<u8>, GpuError> {
        let stride = (self.width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(stride) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| GpuError(e.to_string()))?;
        rx.recv()
            .map_err(|e| GpuError(e.to_string()))?
            .map_err(|e| GpuError(e.to_string()))?;
        let mapped = buffer.get_mapped_range(..);
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in mapped.chunks(stride as usize) {
            pixels.extend_from_slice(&row[..self.width as usize * 4]);
        }
        // Retained textures are BGRA (see `FORMAT`); callers read RGBA.
        if texture.format() == wgpu::TextureFormat::Bgra8Unorm {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }
}
/// `rect` less `cut`, as up to four rectangles pushed to `out`.
/// The widest a filter of `bounds` and `effects` blurs anywhere in `area`:
/// its radius, less under an edge fade (as blur.wgsl's `sigma`).
fn peak_blur(bounds: Rect, effects: &zgui::scene::Effects, area: Rect) -> f32 {
    if effects.edge_fade <= 0. {
        return effects.blur_radius;
    }
    // How far in from either edge of `lo..hi` the span `from..to` reaches.
    let deepest = |lo: f32, hi: f32, from: f32, to: f32| {
        let at = ((lo + hi) / 2.).clamp(from, to);
        (at - lo).min(hi - at)
    };
    let depth = deepest(
        bounds.x,
        bounds.x + bounds.width,
        area.x,
        area.x + area.width,
    )
    .min(deepest(
        bounds.y,
        bounds.y + bounds.height,
        area.y,
        area.y + area.height,
    ));
    let t = (depth / effects.edge_fade).clamp(0., 1.);
    effects.blur_radius * t * t
}
/// A batch's filters (draws `members`), each with the parts of its output
/// the damaged `regions` reach.
fn batch_filters(draws: &[Draw], regions: &[(Rect, Scissor)], members: &[usize]) -> Vec<Filter> {
    members
        .iter()
        .map(|&i| {
            let draw = &draws[i];
            let (bounds, effects, corners) = draw.blur.expect("batched filter");
            Filter {
                bounds,
                clip: draw.clip,
                outputs: regions
                    .iter()
                    .filter_map(|(region, _)| region.intersection(draw.clip)?.intersection(bounds))
                    .collect(),
                effects,
                corners,
            }
        })
        .collect()
}
/// The bounding box of the part of `area` lying `near` to `far` in from the
/// nearest edge of `edges` (each x0, y0, x1, y1), if any.
fn band(area: [f32; 4], edges: [f32; 4], near: f32, far: f32) -> Option<[f32; 4]> {
    let inset = |by: f32| [edges[0] + by, edges[1] + by, edges[2] - by, edges[3] - by];
    let meet = |a: [f32; 4], b: [f32; 4]| {
        let r = [
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ];
        (r[0] < r[2] && r[1] < r[3]).then_some(r)
    };
    let within = meet(area, inset(near))?;
    let Some(hole) = far.is_finite().then(|| meet(within, inset(far))).flatten() else {
        return Some(within);
    };
    // `within` less `hole`: the strips above, below and beside it.
    [
        [within[0], within[1], within[2], hole[1]],
        [within[0], hole[3], within[2], within[3]],
        [within[0], hole[1], hole[0], hole[3]],
        [hole[2], hole[1], within[2], hole[3]],
    ]
    .into_iter()
    .filter(|s| s[0] < s[2] && s[1] < s[3])
    .reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })
}
// Used by macOS's plane split only.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn scissor_minus(rect: Scissor, cut: Scissor, out: &mut Vec<Scissor>) {
    let (x, y, right, bottom) = (rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3);
    let (cx, cy, cr, cb) = (cut.0, cut.1, cut.0 + cut.2, cut.1 + cut.3);
    if cx >= right || cr <= x || cy >= bottom || cb <= y {
        out.push(rect);
        return;
    }
    // Full-width bands above and below the cut, then its sides between.
    if cy > y {
        out.push((x, y, rect.2, cy - y));
    }
    if cb < bottom {
        out.push((x, cb, rect.2, bottom - cb));
    }
    let (top, low) = (cy.max(y), cb.min(bottom));
    if cx > x {
        out.push((x, top, cx - x, low - top));
    }
    if cr < right {
        out.push((cr, top, right - cr, low - top));
    }
}
/// Whether `outer` contains `inner`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn scissor_holds(outer: Scissor, inner: Scissor) -> bool {
    outer.0 <= inner.0
        && outer.1 <= inner.1
        && outer.0 + outer.2 >= inner.0 + inner.2
        && outer.1 + outer.3 >= inner.1 + inner.3
}
/// A texture draws sample and the CPU writes (glyph cells, images): with
/// `cpu_writes` on macOS, in shared memory written in place
/// (`upload::shared_texture`).
fn sampled_texture(
    device: &wgpu::Device,
    #[allow(unused)] cpu_writes: bool,
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    #[cfg(target_os = "macos")]
    if cpu_writes {
        return upload::shared_texture(device, width, height, label);
    }
    texture(device, width, height, label)
}
/// Clear a new target through wgpu, so wgpu knows it is written: frames drawn
/// with Metal directly (`render_direct`) write it behind wgpu's back, and wgpu
/// zeroes a texture it has never written on that texture's first use.
#[cfg(target_os = "macos")]
fn written(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView) {
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("new target"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    queue.submit([encoder.finish()]);
}
/// A backdrop blur atlas: half floats, so averaging down and blurring in
/// steps adds no 8-bit banding to smooth frost.
fn atlas_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("backdrop blur"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLUR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
fn texture(device: &wgpu::Device, width: u32, height: u32, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
/// A glyph's quad from its atlas entry, from its text's origin. Inlined
/// into the loops building a text's quads: called, its return copied each
/// glyph's quad again.
#[inline(always)]
fn glyph_quad(glyph: &Glyph, a: AtlasEntry, scale: f32, atlas_size: u32) -> Quad {
    Quad {
        rect: [
            (glyph.x + a.left as f32) / scale,
            (glyph.y - a.top as f32) / scale,
            a.width as f32 / scale,
            a.height as f32 / scale,
        ],
        uv: [
            a.x as f32 / atlas_size as f32,
            a.y as f32 / atlas_size as f32,
            a.width as f32 / atlas_size as f32,
            a.height as f32 / atlas_size as f32,
        ],
        color: glyph.color.map_or([0.; 4], |c| rgba(c, 1.)),
        fade: [0.; 4],
        options: [0., 1., 0., 0.],
        shape: [0.; 4],
        border: [0.; 4],
        mask: NO_MASK,
    }
}
fn rich_decoration_quad(d: &text::RichDecoration) -> Quad {
    let rect = [d.bounds.x, d.bounds.y, d.bounds.width, d.bounds.height];
    Quad {
        rect,
        color: rgba(d.color, 1.),
        uv: [0.; 4],
        // A rounded background (an inline code chip) is a rounded rect over
        // its own box; `fade` carries that box and moves with the text.
        fade: if d.radius > 0. { rect } else { [0.; 4] },
        options: [0.; 4],
        shape: if d.radius > 0. {
            [d.radius, 0., 0., 1.]
        } else {
            [0.; 4]
        },
        border: [0.; 4],
        mask: NO_MASK,
    }
}
/// A shader fill's module: the quad shader (draw.wgsl), the `FillInput` its
/// `fill` takes, the user's source and the fragment entry `zgui_fill`, which
/// shades a quad carrying uniforms (`mask.y`: their record in the region
/// table, plus one) with `fill`'s colour and the rest as `shade` does.
pub(crate) fn fill_source(user: &str) -> String {
    format!(
        "{}\nstruct FillInput {{ p:vec2<f32>, size:vec2<f32>, time:f32, scale:f32, params:array<vec4<f32>,4> }}\n{user}\n{}",
        include_str!("draw.wgsl"),
        "@fragment fn zgui_fill(input:Out)->@location(0) vec4<f32> {
 if input.mask.y<.5 {return shade(input);}
 let u=regions[u32(input.mask.y)-1u]; var v=input;
 let point=select(v.position.xy/viewport.z,v.point,v.options.z>0.5);
 let c=fill(FillInput(point-v.fade.xy,v.fade.zw,u.round0.x,viewport.z,array<vec4<f32>,4>(u.radii0,u.round1,u.radii1,u.fade_box)));
 v.color=vec4(c.rgb,c.a*v.color.a);
 return shade(v);
}"
    )
}
/// Checks a shader fill's source (the user's part, as `FillShader` holds
/// it) as the renderer will compile it: `Err` reads like a compiler
/// message, with lines counted in `user`, for an app or script to show.
pub fn check_fill_shader(user: &str) -> Result<(), String> {
    let source = fill_source(user);
    // Lines before the user's code in the module `fill_source` builds.
    let template = fill_source("\u{1}");
    let before = template[..template.find('\u{1}').unwrap_or(0)]
        .matches('\n')
        .count();
    let place = |location: Option<wgpu::naga::SourceLocation>| match location {
        Some(at) if at.line_number as usize > before => {
            format!("line {}: ", at.line_number as usize - before)
        }
        Some(_) => "in zgui's part of the module (a reserved name?): ".to_owned(),
        None => String::new(),
    };
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .map_err(|e| format!("{}{}", place(e.location(&source)), e.message()))?;
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map(|_| ())
    .map_err(|e| format!("{}{}", place(e.location(&source)), e.as_inner()))
}
/// Whether `source` (a shader fill's whole module) is valid WGSL.
pub(crate) fn fill_validates(source: &str) -> bool {
    let Ok(module) = wgpu::naga::front::wgsl::parse_str(source) else {
        return false;
    };
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .is_ok()
}
/// A background `image` placed in `bounds` as `fit` has it (centred), the
/// part inside them, cut to `style`'s rounded corners (the quad shader's
/// per-corner shape over the box, whatever part of it the image covers).
/// A gradient ramp: its stops (offsets and colours, as bits) and whether
/// it runs down (a column of texels) or across (a row).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RampKey {
    vertical: bool,
    stops: Vec<(u32, [u8; 4])>,
}
/// Texels in a ramp: smooth to the eye at any size, as the texture is
/// sampled linearly between them.
const RAMP: u32 = 256;
/// `style`'s linear gradient, when it runs straight down or across `bounds`
/// (any other angle rasterizes): its ramp and the texture coordinates that
/// stretch the ramp's ends to the gradient's (padded past them by the
/// sampler's clamping).
fn axis_gradient(style: &zgui::scene::QuadStyle, bounds: Rect) -> Option<(RampKey, [f32; 4])> {
    use zgui::{canvas::Point, decoration::Background};
    let (w, h) = (bounds.width, bounds.height);
    if !(w > 0. && h > 0.) {
        return None;
    }
    let (start, end, stops) = match style.decoration.as_deref()?.background.as_ref()? {
        Background::Linear { angle, stops } => {
            // As `decoration::canvas` places them.
            let (sin, cos) = angle.sin_cos();
            let half = (w * cos.abs() + h * sin.abs()) * 0.5;
            (
                Point::new(w * 0.5 - cos * half, h * 0.5 - sin * half),
                Point::new(w * 0.5 + cos * half, h * 0.5 + sin * half),
                stops,
            )
        }
        Background::Brush(zgui::canvas::Brush::Linear { start, end, stops }) => {
            (*start, *end, stops)
        }
        _ => return None,
    };
    if stops.len() < 2
        || stops.len() > 64
        || stops
            .iter()
            .any(|s| !s.offset.is_finite() || !(0. ..=1.).contains(&s.offset))
        || stops.windows(2).any(|p| p[1].offset < p[0].offset)
    {
        return None;
    }
    let vertical = (start.x - end.x).abs() <= 1e-3;
    let horizontal = (start.y - end.y).abs() <= 1e-3;
    let (from, to, extent) = match (vertical, horizontal) {
        (true, false) => (start.y, end.y, h),
        (false, true) => (start.x, end.x, w),
        _ => return None,
    };
    let span = to - from;
    if !span.is_finite() || span.abs() < 1e-3 {
        return None;
    }
    // Offset 0 at the first texel's centre, 1 at the last's.
    let texel = |t: f32| (0.5 + t * (RAMP - 1) as f32) / RAMP as f32;
    let (a, b) = (texel(-from / span), texel((extent - from) / span));
    let uv = if vertical {
        [0., a, 1., b - a]
    } else {
        [a, 0., b - a, 1.]
    };
    let stops = stops
        .iter()
        .map(|s| {
            (
                s.offset.to_bits(),
                [s.color.0, s.color.1, s.color.2, s.color.3],
            )
        })
        .collect();
    Some((RampKey { vertical, stops }, uv))
}
/// A ramp's texels: the stops interpolated with premultiplied alpha (as
/// CSS gradients are, and rasters, see `canvas::premultiplied_stops`),
/// padded with the end colours.
fn ramp_image(key: &RampKey) -> Result<zgui::image::ImageData, GpuError> {
    let stops: Vec<(f32, [f32; 4])> = key
        .stops
        .iter()
        .map(|&(offset, [r, g, b, a])| {
            let alpha = a as f32 / 255.;
            (
                f32::from_bits(offset),
                [
                    r as f32 * alpha,
                    g as f32 * alpha,
                    b as f32 * alpha,
                    a as f32,
                ],
            )
        })
        .collect();
    let mut pixels = Vec::with_capacity(RAMP as usize * 4);
    for i in 0..RAMP {
        let t = i as f32 / (RAMP - 1) as f32;
        let next = stops.iter().position(|s| s.0 > t).unwrap_or(stops.len());
        let c = match next {
            0 => stops[0].1,
            n if n == stops.len() => stops[n - 1].1,
            n => {
                let ((t0, c0), (t1, c1)) = (stops[n - 1], stops[n]);
                let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1. };
                std::array::from_fn(|k| c0[k] + (c1[k] - c0[k]) * f)
            }
        };
        // Images hold straight colour (their upload premultiplies).
        let alpha = c[3] / 255.;
        let straight = |v: f32| if alpha > 0. { v / alpha } else { 0. };
        pixels.extend(
            [straight(c[0]), straight(c[1]), straight(c[2]), c[3]]
                .map(|v| v.round().clamp(0., 255.) as u8),
        );
    }
    let (width, height) = if key.vertical { (1, RAMP) } else { (RAMP, 1) };
    zgui::image::ImageData::new(width, height, pixels).map_err(|e| GpuError(e.into()))
}
fn background_quad(
    image: &zgui::image::ImageData,
    fit: zgui::style::ObjectFit,
    style: &zgui::scene::QuadStyle,
    bounds: Rect,
    opacity: f32,
    edge_fade: f32,
) -> Option<Quad> {
    use zgui::style::ObjectFit;
    let (source_w, source_h) = (image.width() as f32, image.height() as f32);
    if bounds.width <= 0. || bounds.height <= 0. {
        return None;
    }
    let (horizontal, vertical) = (bounds.width / source_w, bounds.height / source_h);
    let (width, height) = match fit {
        ObjectFit::Fill => (bounds.width, bounds.height),
        ObjectFit::None => (source_w, source_h),
        fit => {
            let scale = match fit {
                ObjectFit::Cover => horizontal.max(vertical),
                ObjectFit::ScaleDown => horizontal.min(vertical).min(1.),
                _ => horizontal.min(vertical),
            };
            (source_w * scale, source_h * scale)
        }
    };
    let placed = Rect::new(
        bounds.x + (bounds.width - width) / 2.,
        bounds.y + (bounds.height - height) / 2.,
        width,
        height,
    );
    let shown = placed.intersection(bounds)?;
    let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
    Some(Quad {
        rect: [shown.x, shown.y, shown.width, shown.height],
        uv: [
            (shown.x - placed.x) / width,
            (shown.y - placed.y) / height,
            shown.width / width,
            shown.height / height,
        ],
        color: [1., 1., 1., opacity],
        fade,
        options: [edge_fade, 2., 0., 1.],
        shape: [0., 0., 0., 1.],
        border: decoration::shadow_corners(style, bounds.width, bounds.height, 0.),
        mask: NO_MASK,
    })
}
/// `outline` around a box of `style` at `bounds`: a ring of its width
/// between the box grown by its offset and by its offset and width, each
/// rounded corner grown alike (square ones stay square).
fn outline_quad(
    outline: &zgui::scene::Outline,
    style: &zgui::scene::QuadStyle,
    bounds: Rect,
    opacity: f32,
    edge_fade: f32,
) -> Quad {
    let outer = outline.outer(bounds);
    let grown = outline.offset + outline.width;
    let corners =
        decoration::shadow_corners(style, bounds.width, bounds.height, 0.).map(|radius| {
            if radius > 0. {
                (radius + grown).max(0.)
            } else {
                0.
            }
        });
    let width = (outline.width.max(0.) * 16.).round().min(4095.);
    let fade = [outer.x, outer.y, outer.width, outer.height];
    Quad {
        rect: fade,
        uv: [0.; 4],
        color: rgba(outline.color, opacity),
        fade,
        options: [edge_fade, 0., 0., 1.],
        shape: [width + 4096. * width, -1., width + 4096. * width, 1.],
        border: corners,
        mask: NO_MASK,
    }
}
/// `style`'s border alone at `bounds`: a ring of its side widths (top,
/// right, bottom, left) inside its rounded box (`draw.wgsl`'s ring mode,
/// two widths a lane in 16ths of a pixel, up to 255 px).
fn ring_quad(style: &zgui::scene::QuadStyle, bounds: Rect, opacity: f32, edge_fade: f32) -> Quad {
    let sides = style.border_sides();
    let width = |value: f32| match value.is_finite() {
        true => (value.max(0.) * 16.).round().min(4095.),
        false => 0.,
    };
    let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
    Quad {
        rect: fade,
        uv: [0.; 4],
        color: rgba(style.border_color, opacity),
        fade,
        options: [edge_fade, 0., 0., 1.],
        shape: [
            width(sides.top) + 4096. * width(sides.right),
            -1.,
            width(sides.bottom) + 4096. * width(sides.left),
            1.,
        ],
        border: decoration::shadow_corners(style, bounds.width, bounds.height, 0.),
        mask: NO_MASK,
    }
}
/// A box's inset shadows (`Decoration::inset_shadows`), each a ring inside
/// it as wide on each side as the shadow's offset pushes it (and its spread
/// all round), in the shadow's colour, cut to the box's corners.
fn inset_quads(
    style: &zgui::scene::QuadStyle,
    bounds: Rect,
    opacity: f32,
    edge_fade: f32,
) -> impl Iterator<Item = Quad> + '_ {
    let shadows = style
        .decoration
        .as_deref()
        .and_then(|d| d.inset_shadows.as_deref())
        .unwrap_or(&[]);
    shadows.iter().filter(|s| s.color.3 > 0).map(move |s| {
        let spread = s.spread.max(0.);
        let width = |value: f32| match value.is_finite() {
            true => ((value.max(0.) + spread) * 16.).round().min(4095.),
            false => 0.,
        };
        let (x, y) = (s.offset.x, s.offset.y);
        let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
        Quad {
            rect: fade,
            uv: [0.; 4],
            color: rgba(s.color, opacity),
            fade,
            options: [edge_fade, 0., 0., 1.],
            shape: [
                width(y) + 4096. * width(-x),
                -1.,
                width(-y) + 4096. * width(x),
                1.,
            ],
            border: decoration::shadow_corners(style, bounds.width, bounds.height, 0.),
            mask: NO_MASK,
        }
    })
}
fn rgba(c: Color, opacity: f32) -> [f32; 4] {
    [
        c.0 as f32 / 255.,
        c.1 as f32 / 255.,
        c.2 as f32 / 255.,
        c.3 as f32 / 255. * opacity,
    ]
}
/// Quads into the retained target, or with `drawable` also into a second,
/// presentation target in the same pass (`fs_both`).
/// The full quad shader drawing into a drawable of `format` alone
/// (`Deferred::direct`).
fn drawable_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    blend: bool,
    format: wgpu::TextureFormat,
    basic: bool,
) -> wgpu::RenderPipeline {
    let all = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
    let basic_attributes = [all[0], all[1], all[2], all[4]];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("quads to the drawable"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(if basic { "vs_basic" } else { "vs" }),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 128,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: if basic { &basic_attributes } else { &all },
            }],
        },
        primitive: QUAD_STRIP,
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if basic { "fs_basic" } else { "fs" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
fn quad_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    blend: bool,
    drawable: Option<wgpu::TextureFormat>,
    basic: bool,
) -> wgpu::RenderPipeline {
    let blend = blend.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
    let all = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
    // `basic` reads rect, uv, colour and options only.
    let basic_attributes = [all[0], all[1], all[2], all[4]];
    let target = |format| {
        Some(wgpu::ColorTargetState {
            format,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        })
    };
    let targets = [target(FORMAT), drawable.and_then(target)];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("quads"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(if basic { "vs_basic" } else { "vs" }),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 128,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: if basic { &basic_attributes } else { &all },
            }],
        },
        primitive: QUAD_STRIP,
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            // Single-pass presentation draws with the full shader alone.
            entry_point: Some(match (basic, drawable.is_some()) {
                (true, true) => "fs_basic_both",
                (true, false) => "fs_basic",
                (false, true) => "fs_both",
                (false, false) => "fs",
            }),
            compilation_options: Default::default(),
            targets: if drawable.is_some() {
                &targets
            } else {
                &targets[..1]
            },
        }),
        multiview_mask: None,
        cache: None,
    })
}
/// A shader fill's pipeline: quads blended over the target, `module`'s
/// `zgui_fill` shading them (see `fill_source`).
fn fill_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::RenderPipeline {
    let all = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shader fill"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 128,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &all,
            }],
        },
        primitive: QUAD_STRIP,
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("zgui_fill"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
/// Whether presentation must convert premultiplied pixels to straight alpha.
/// wgpu reports Metal layers as "post-multiplied" merely because they are not
/// opaque, but Core Animation composites `CAMetalLayer` contents as
/// premultiplied (measured: straight 50% red over black shows twice as bright).
fn unpremultiplies(config: &wgpu::SurfaceConfiguration) -> bool {
    config.alpha_mode == wgpu::CompositeAlphaMode::PostMultiplied && !cfg!(target_os = "macos")
}
fn blit_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    post_alpha: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("present"),
        source: wgpu::ShaderSource::Wgsl(
            (format!(
                "const OUTPUT_SRGB:bool={};\nconst POST_ALPHA:bool={};\n{}",
                format.is_srgb(),
                post_alpha,
                include_str!("blit.wgsl")
            ))
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("present"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
fn blit_group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("present"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}
/// `r` grown to whole physical pixels, as `scissor` rounds it.
fn snap_out(r: Rect, scale: f32) -> Rect {
    let x = (r.x * scale).floor() / scale;
    let y = (r.y * scale).floor() / scale;
    let right = ((r.x + r.width) * scale).ceil() / scale;
    let bottom = ((r.y + r.height) * scale).ceil() / scale;
    Rect::new(x, y, right - x, bottom - y)
}
/// Whether `quad` lies entirely within `clip`.
fn inside(quad: &Quad, clip: Rect) -> bool {
    rect_inside(quad_rect(quad), clip)
}
/// Whether a quad covering `rect` lies entirely within `clip`.
fn rect_inside(rect: Rect, clip: Rect) -> bool {
    let Rect {
        x,
        y,
        width: w,
        height: h,
    } = rect;
    x >= clip.x && y >= clip.y && x + w <= clip.x + clip.width && y + h <= clip.y + clip.height
}
/// Whether a quad covering `rect` lies entirely outside `clip`.
fn rect_outside(rect: Rect, clip: Rect) -> bool {
    let Rect {
        x,
        y,
        width: w,
        height: h,
    } = rect;
    x + w <= clip.x || y + h <= clip.y || x >= clip.x + clip.width || y >= clip.y + clip.height
}
/// A backdrop filter's corner radii (top-left, top-right, bottom-right,
/// bottom-left): a rounded panel blurs only inside its own curve.
fn blur_corners(kind: &NodeKind, bounds: Rect) -> [f32; 4] {
    match kind {
        NodeKind::Panel { quad, .. } | NodeKind::Quad(quad) => {
            decoration::shadow_corners(quad, bounds.width, bounds.height, 0.)
        }
        _ => [0.; 4],
    }
}
fn scissor(r: Rect, scale: f32, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let x = (r.x * scale).floor().max(0.) as u32;
    let y = (r.y * scale).floor().max(0.) as u32;
    let right = ((r.x + r.width) * scale).ceil().min(width as f32) as u32;
    let bottom = ((r.y + r.height) * scale).ceil().min(height as f32) as u32;
    (right > x && bottom > y).then(|| (x, y, right - x, bottom - y))
}
// Each activated filter contributes its complete clipped output and sampling
// halo. Bounding merges may touch previously unrelated filters; rescan until
// all such dependencies are included. Each filter activates at most once.
fn blur_damage(damage: &[Rect], filters: &[(Rect, f32)], viewport: Rect, scale: f32) -> Vec<Rect> {
    let mut regions = merge_damage(damage, viewport, scale);
    let mut dependencies: Vec<_> = filters
        .iter()
        .filter_map(|(output, _radius)| {
            let width = (viewport.width * scale).round() as u32;
            let height = (viewport.height * scale).round() as u32;
            let output = output.intersection(viewport)?;
            // A filter samples only its own visible pixels, so damage
            // anywhere in them re-blurs all of them, and nothing beyond.
            let (x, y, w, h) = scissor(output, scale, width, height)?;
            let (left, top, right, bottom) = (x, y, x + w, y + h);
            let dependency = Rect::new(
                left as f32 / scale,
                top as f32 / scale,
                (right - left) as f32 / scale,
                (bottom - top) as f32 / scale,
            );
            Some((dependency, false))
        })
        .collect();
    loop {
        let mut changed = false;
        for (dependency, processed) in &mut dependencies {
            if !*processed && regions.iter().any(|region| region.intersects(*dependency)) {
                regions.push(*dependency);
                regions = merge_damage(&regions, viewport, scale);
                *processed = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    regions
}
fn rounded_damage(rect: Rect, viewport: Rect, scale: f32) -> Option<Rect> {
    let rect = rect.intersection(viewport)?;
    let x = (rect.x * scale).floor() / scale;
    let y = (rect.y * scale).floor() / scale;
    Some(Rect::new(
        x,
        y,
        ((rect.x + rect.width) * scale).ceil() / scale - x,
        ((rect.y + rect.height) * scale).ceil() / scale - y,
    ))
}
/// Push a panel quad. An opaque one with rounded corners or a border paints
/// exactly its fill colour away from its edges (in `shade`: past the corner
/// radius, the border and the antialiasing band of at most two device
/// pixels): that interior goes as a flat opaque quad, drawn unblended so the
/// GPU skips shading whatever it covers, and only the four bands around it
/// need the distance field and blending. Every band keeps the
/// panel's own outer edges (the right and bottom ones measured back from the
/// far edge), so the same pixels are covered and antialiased; the seams
/// between pieces lie on whole device pixels, where rounding cannot move a
/// pixel centre across them.
fn push_panel(quads: &mut Vec<Quad>, quad: Quad, scale: f32, split: bool) {
    let radius = if quad.options[3] > 0.5 {
        quad.border.iter().copied().fold(0., f32::max)
    } else {
        quad.shape[0]
    };
    let band = radius.max(quad.shape[1] + 1.).max(0.) + 1.;
    let [x, y, width, height] = quad.rect;
    let (right, bottom) = (x + width, y + height);
    // The interior, on device pixels inside the bands.
    let (left, top) = (
        ((x + band) * scale).ceil() / scale,
        ((y + band) * scale).ceil() / scale,
    );
    let (inner_right, inner_bottom) = (
        ((right - band) * scale).floor() / scale,
        ((bottom - band) * scale).floor() / scale,
    );
    let (inner_width, inner_height) = (inner_right - left, inner_bottom - top);
    if !split
        || quad.color[3] < 1.
        || quad.options[..3] != [0.; 3]
        || quad.shape[2] > 0.
        || quad.shape[3] <= 0.5
        || quad.rect != quad.fade
        || inner_width <= 0.
        || inner_height <= 0.
        || inner_width * inner_height < PANEL_INTERIOR
    {
        quads.push(quad);
        return;
    }
    for rect in [
        [x, y, width, top - y],
        [x, bottom, width, inner_bottom - bottom],
        [x, top, left - x, inner_height],
        [right, top, inner_right - right, inner_height],
    ] {
        quads.push(Quad { rect, ..quad });
    }
    // Last, and opaque: drawn without blending, it hides what lies beneath
    // (the GPU skips shading fragments an opaque one covers), as the whole
    // panel's blended shader could not.
    let interior = [left, top, inner_width, inner_height];
    quads.push(Quad {
        rect: interior,
        fade: interior,
        options: [0.; 4],
        shape: [0.; 4],
        border: [0.; 4],
        ..quad
    });
}
/// Panels split only with an interior of at least this many logical pixels:
/// below, five quads cost more than shading one.
const PANEL_INTERIOR: f32 = 1024.;
/// Quads draw as four-vertex triangle strips, one per instance.
pub(crate) const QUAD_STRIP: wgpu::PrimitiveState = wgpu::PrimitiveState {
    topology: wgpu::PrimitiveTopology::TriangleStrip,
    strip_index_format: None,
    front_face: wgpu::FrontFace::Ccw,
    cull_mode: None,
    unclipped_depth: false,
    polygon_mode: wgpu::PolygonMode::Fill,
    conservative: false,
};
/// Regions a frame draws at most: each draws every batch it touches again.
const MAX_REGIONS: usize = 16;
/// Damage in at most `MAX_REGIONS` groups, each the bounds of its members:
/// rectangles sharing a cell of a grid, the finest of cells doubling from 16
/// that leaves few enough.
fn cluster_damage(damage: &[Rect], viewport: Rect) -> Vec<Rect> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let groups = |cell: f32| -> Vec<Rect> {
        let columns = (viewport.width / cell).ceil().max(1.) as usize;
        let rows = (viewport.height / cell).ceil().max(1.) as usize;
        let mut owner = vec![usize::MAX; columns * rows];
        let mut parent: Vec<usize> = (0..damage.len()).collect();
        for (i, r) in damage.iter().enumerate() {
            let first =
                |value: f32, count: usize| ((value / cell).floor().max(0.) as usize).min(count);
            let end =
                |value: f32, count: usize| ((value / cell).ceil().max(0.) as usize).min(count);
            for row in first(r.y, rows)..end(r.y + r.height, rows) {
                for column in first(r.x, columns)..end(r.x + r.width, columns) {
                    let slot = &mut owner[row * columns + column];
                    if *slot == usize::MAX {
                        *slot = i;
                    } else {
                        let (a, b) = (root(&mut parent, i), root(&mut parent, *slot));
                        parent[a] = b;
                    }
                }
            }
        }
        let mut bounds: Vec<Option<Rect>> = vec![None; damage.len()];
        for (i, r) in damage.iter().enumerate() {
            let group = &mut bounds[root(&mut parent, i)];
            *group = Some(group.map_or(*r, |g| g.union(*r)));
        }
        bounds.into_iter().flatten().collect()
    };
    let fits = |groups: &Vec<Rect>, cell: f32| {
        groups.len() <= MAX_REGIONS || cell >= viewport.width.max(viewport.height)
    };
    // Coarser cells only merge groups, so the answer is the finest cell that
    // fits. A frame's damage is much like the last one's: search from the
    // cell that fitted last time instead of from 16 (a dashboard changing
    // everywhere doubled five or six times every frame).
    thread_local! {
        static LAST: std::cell::Cell<f32> = const { std::cell::Cell::new(16.) };
    }
    let mut cell = LAST.get().max(16.);
    let mut found = groups(cell);
    if fits(&found, cell) {
        while cell > 16. {
            let finer = groups(cell / 2.);
            if !fits(&finer, cell / 2.) {
                break;
            }
            cell /= 2.;
            found = finer;
        }
    } else {
        loop {
            cell *= 2.;
            found = groups(cell);
            if fits(&found, cell) {
                break;
            }
        }
    }
    LAST.set(cell);
    found
}
fn merge_damage(damage: &[Rect], viewport: Rect, scale: f32) -> Vec<Rect> {
    // Each region redraws every batch it touches: nearby small ones (the
    // dots of a spinner) are fewer draws as one, for a few more pixels. The
    // pixels gained are bounded by the smaller region, so a large region
    // doesn't swallow every small one in reach, growing as it goes.
    let area = |r: Rect| r.width * r.height * scale * scale;
    let close = |a: Rect, b: Rect| {
        let (a, b) = (area(a), area(b));
        a + b + a.min(b) * 0.5 + NEAR_PIXELS
    };
    let close = |a: Rect, b: Rect| a.intersects(b) || area(a.union(b)) <= close(a, b);
    let mut regions: Vec<Rect> = Vec::new();
    let rounded: Vec<Rect>;
    let damage = if damage.len() > MAX_REGIONS {
        rounded = cluster_damage(damage, viewport);
        &rounded[..]
    } else {
        damage
    };
    for r in damage {
        let Some(mut r) = rounded_damage(*r, viewport, scale) else {
            continue;
        };
        let mut i = 0;
        while i < regions.len() {
            if close(regions[i], r) {
                r = r.union(regions.swap_remove(i));
                i = 0;
            } else {
                i += 1;
            }
        }
        regions.push(r);
    }
    regions
}

/// Text shaping and measurement without a GPU: what a renderer installs on
/// a scene (`GpuRenderer::install_text`), for laying out what is never drawn
/// (measuring a scene's update, layout and damage work on its own).
pub struct TextServices {
    fonts: Rc<RefCell<FontSystem>>,
    cache: Rc<RefCell<prepared::TextCache>>,
}
impl Default for TextServices {
    fn default() -> Self {
        Self::new()
    }
}
impl TextServices {
    /// The system's fonts (from the font cache) and an empty text cache.
    pub fn new() -> Self {
        Self {
            fonts: Rc::new(RefCell::new(font_cache::system_fonts())),
            cache: Default::default(),
        }
    }
    /// Shape and measure `scene`'s text with these, as a renderer would.
    pub fn install(&self, scene: &mut Scene) {
        install_text(scene, &self.fonts, &self.cache);
    }
}
/// Install text services sharing `shared_fonts` and `shared_cache` on `scene`.
fn install_text(
    scene: &mut Scene,
    shared_fonts: &Rc<RefCell<FontSystem>>,
    shared_cache: &Rc<RefCell<prepared::TextCache>>,
) {
    let fonts = shared_fonts.clone();
    scene.set_text_measurer(move |text: &str, size: f32, max: Option<f32>| {
        let _span = zgui::profile::nested(zgui::profile::Phase::Text);
        crate::measure_text(&mut fonts.borrow_mut(), text, size, max)
    });
    let fonts = shared_fonts.clone();
    scene.set_font_text_shaper(
        move |text: &str,
              size: f32,
              width: Option<f32>,
              font: &zgui::text_layout::FontStyle|
              -> Box<dyn zgui::text_layout::TextLayout> {
            let _span = zgui::profile::nested(zgui::profile::Phase::Text);
            Box::new(crate::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let fonts = shared_fonts.clone();
    scene.set_rich_text_shaper(move |rich, width| {
        let _span = zgui::profile::nested(zgui::profile::Phase::Text);
        Box::new(crate::text::ShapedText::with_runs(
            &mut fonts.borrow_mut(),
            rich,
            width,
        ))
    });
    // Prepared text: shaped once, then measured at any width by line
    // breaking alone; the renderer draws from the same lines.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_rich_text_measurer(move |rich, width| {
        let _span = zgui::profile::nested(zgui::profile::Phase::Text);
        cache
            .borrow_mut()
            .measure(&mut fonts.borrow_mut(), rich, width)
    });
    // Plain text nodes, from the same prepared text.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_font_text_measurer(move |text, size, width, font| {
        let _span = zgui::profile::nested(zgui::profile::Phase::Text);
        cache
            .borrow_mut()
            .measure_plain(&mut fonts.borrow_mut(), text, size, font, width)
    });
    // Apps measuring what they have not mounted keep no glyphs for it.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_detached_rich_text_measurer(move |rich, width| {
        let _span = zgui::profile::nested(zgui::profile::Phase::Text);
        cache
            .borrow_mut()
            .measure_detached(&mut fonts.borrow_mut(), rich, width)
    });
}

pub mod assets;

pub mod prepared;
pub mod text;

fn create_quad_pipelines(
    device: &wgpu::Device,
) -> (
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
    wgpu::ShaderModule,
    wgpu::PipelineLayout,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quads"),
        source: wgpu::ShaderSource::Wgsl(include_str!("draw.wgsl").into()),
    });
    let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("quads"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    // The viewport, and the frame's clip regions (`Region`).
    let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("viewport"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("quads"),
        bind_group_layouts: &[Some(&group_layout), Some(&viewport_layout)],
        immediate_size: 0,
    });
    (group_layout, viewport_layout, shader, layout)
}

impl surface::SurfaceSource for GpuRenderer {
    type Frame = wgpu::SurfaceTexture;
    fn acquire(&mut self) -> Result<surface::Acquisition<Self::Frame>, GpuError> {
        use surface::Acquisition;
        Ok(match self.surface.as_ref().unwrap().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Acquisition::Frame(frame),
            wgpu::CurrentSurfaceTexture::Timeout => Acquisition::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded => Acquisition::Occluded,
            wgpu::CurrentSurfaceTexture::Lost => Acquisition::Lost,
            wgpu::CurrentSurfaceTexture::Outdated => Acquisition::Outdated,
            other => return Err(GpuError(format!("surface acquisition: {other:?}"))),
        })
    }
    fn recreate(&mut self) -> Result<(), GpuError> {
        #[cfg(all(windows, feature = "composition"))]
        if let Some(target) = &self.composition {
            self.surface = Some(target.surface(&self.instance)?);
            return Ok(());
        }
        if let Some((display, window)) = self.raw {
            // SAFETY: as `for_raw_surface`'s caller promised.
            self.surface = Some(
                unsafe {
                    self.instance
                        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                            raw_display_handle: Some(display),
                            raw_window_handle: window,
                        })
                }
                .map_err(|e| GpuError(e.to_string()))?,
            );
            return Ok(());
        }
        self.surface = Some(
            self.instance
                .create_surface(self.window.as_ref().unwrap().clone())
                .map_err(|e| GpuError(e.to_string()))?,
        );
        #[cfg(target_os = "macos")]
        {
            self.transaction_present = false;
        }
        Ok(())
    }
    fn configure(&mut self) {
        self.surface
            .as_ref()
            .unwrap()
            .configure(&self.device, self.config.as_ref().unwrap());
        // A new swap chain shows in its visual once the tree commits.
        #[cfg(all(windows, feature = "composition"))]
        if let Some(target) = &self.composition {
            target.commit();
        }
        // Configuring restores the default count.
        #[cfg(target_os = "macos")]
        if self.wide_until.is_some() {
            self.set_drawable_count(3);
        }
    }
}

#[cfg(test)]
mod blur_damage_tests {
    use super::*;

    /// A filter samples only its own pixels: damage beside it stays local,
    /// damage inside it re-blurs the whole filter and nothing more.
    #[test]
    fn damage_outside_a_filter_is_local_and_inside_rebuilds_the_filter() {
        let viewport = Rect::new(0., 0., 500., 300.);
        let filter = (Rect::new(100., 100., 40., 30.), 2.);
        let distant = Rect::new(10., 10., 5., 5.);
        assert_eq!(
            blur_damage(&[distant], &[filter], viewport, 1.),
            vec![distant]
        );
        let beside = Rect::new(95., 110., 1., 1.);
        assert_eq!(
            blur_damage(&[beside], &[filter], viewport, 1.),
            vec![beside]
        );
        assert_eq!(
            blur_damage(&[Rect::new(101., 110., 1., 1.)], &[filter], viewport, 1.),
            vec![Rect::new(100., 100., 40., 30.)]
        );
        assert!(blur_damage(&[], &[filter], viewport, 1.).is_empty());
    }

    #[test]
    fn chained_filters_expand_to_fixed_point_independent_of_order() {
        let viewport = Rect::new(0., 0., 500., 300.);
        // Overlapping filters: rebuilding one damages the other.
        let filters = [
            (Rect::new(135., 100., 20., 20.), 2.),
            (Rect::new(120., 100., 20., 20.), 2.),
        ];
        let damage = [Rect::new(121., 105., 1., 1.)];
        let expected = vec![Rect::new(120., 100., 35., 20.)];
        assert_eq!(blur_damage(&damage, &filters, viewport, 1.), expected);
        assert_eq!(
            blur_damage(&damage, &[filters[1], filters[0]], viewport, 1.),
            expected
        );
    }

    #[test]
    fn reconstruction_covers_the_filter_pixels_across_fractional_scales() {
        for scale in [0.7, 1., 1.25, 1.3, 1.5, 1.7, 2.3] {
            let viewport = Rect::new(0., 0., 701. / scale, 503. / scale);
            for index in 0..127 {
                let output = Rect::new(
                    (index * 37 % 680) as f32 / scale + 0.017,
                    (index * 19 % 480) as f32 / scale + 0.031,
                    7.71,
                    8.13,
                );
                for radius in [0.0001, 0.33334, 1.17, 11.31, 1000.] {
                    let (x, y, w, h) = scissor(output, scale, 701, 503).unwrap();
                    let regions = blur_damage(&[output], &[(output, radius)], viewport, scale);
                    assert_eq!(regions.len(), 1);
                    let (dx, dy, dw, dh) = scissor(regions[0], scale, 701, 503).unwrap();
                    assert!(dx <= x && dy <= y);
                    assert!(dx + dw >= x + w && dy + dh >= y + h);
                }
            }
        }
    }

    #[test]
    fn fractional_output_rounds_to_whole_pixels_whatever_the_radius() {
        let viewport = Rect::new(0., 0., 400., 300.);
        // At 1.25x the output spans physical [12, 26).
        let regions = blur_damage(
            &[Rect::new(10.2, 10.2, 1., 1.)],
            &[(Rect::new(10.1, 10.1, 10.1, 10.1), 0.25)],
            viewport,
            1.25,
        );
        let (x, y, width, height) = scissor(regions[0], 1.25, 500, 375).unwrap();
        assert!(x <= 12 && y <= 12 && x + width >= 26 && y + height >= 26);
        assert!(width < 20 && height < 20);
        let huge = blur_damage(
            &[Rect::new(0., 0., 1., 1.)],
            &[(Rect::new(0., 0., 1., 1.), 10000.)],
            viewport,
            1.,
        );
        assert_eq!(huge, vec![Rect::new(0., 0., 1., 1.)]);
    }
}

pub mod canvas;

pub mod decoration;

pub mod svg;

#[cfg(test)]
mod native_surface_shader_tests {
    #[test]
    fn nv12_compute_shader_validates_on_the_available_gpu_backend() {
        let gpu = super::GpuRenderer::new(4, 4).unwrap();
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("NV12 portable validation"),
                source: wgpu::ShaderSource::Wgsl(include_str!("native_surface.wgsl").into()),
            });
        let _pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("NV12 validation"),
                layout: None,
                module: &shader,
                entry_point: Some("convert"),
                compilation_options: Default::default(),
                cache: None,
            });
    }
}

// macOS writes shared textures directly; the paths tested are the others'.
#[cfg(all(test, not(target_os = "macos")))]
mod texel_upload_tests {
    use super::*;
    use zgui::scene::{Color, Layout, Style};

    /// Images keep their channels whichever way texels reach the GPU: the
    /// adapter's own (an upload pass where CPU and GPU share memory) and the
    /// queue copy every other adapter takes, forced here.
    #[test]
    fn images_keep_red_and_blue_through_either_upload_path() {
        let image = Arc::new(
            zgui::image::ImageData::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap(),
        );
        for copied in [false, true] {
            let mut gpu = GpuRenderer::new(40, 20).unwrap();
            if copied {
                gpu.mapped = None;
            }
            let mut scene = Scene::new(40., 20.);
            scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
            scene.append(
                scene.root(),
                NodeKind::Rect(Color(0, 0, 0, 255)),
                Style {
                    width: Some(40.),
                    height: Some(20.),
                    ..Default::default()
                },
            );
            scene.append(
                scene.root(),
                NodeKind::Image(image.clone()),
                Style {
                    width: Some(40.),
                    height: Some(20.),
                    ..Default::default()
                },
            );
            let report = scene.flush();
            gpu.render(&scene, &report.damage).unwrap();
            let pixels = gpu.readback().unwrap();
            let pixel = |x: usize| &pixels[(10 * 40 + x) * 4..(10 * 40 + x) * 4 + 4];
            assert_eq!(pixel(2), &[255, 0, 0, 255], "left, copied={copied}");
            assert_eq!(pixel(37), &[0, 0, 255, 255], "right, copied={copied}");
        }
    }
}
