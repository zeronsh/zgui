//! Damage-scissored retained GPU renderer for Vulkan (Linux) and Metal (macOS).
//! The output is premultiplied BGRA8 (read back as RGBA8). Text uses cosmic-text shaping and a bounded
//! GPU glyph atlas; stable text never reshapes on paint or compositor updates.
#![warn(missing_docs)]
use cosmic_text::{Buffer, CacheKey, FontSystem, Metrics, SwashCache, SwashContent};
use rustc_hash::FxHashMap;
use std::{cell::RefCell, fmt, rc::Rc, sync::Arc};
use zgx::scene::{Color, NodeId, NodeKind, Rect, Scene};

#[cfg(all(windows, feature = "composition"))]
mod composition;
#[cfg(windows)]
mod dxgi;
#[cfg(windows)]
mod dxgi_present;
mod effects;
mod font_cache;
#[cfg(target_os = "macos")]
#[doc(hidden)]
pub mod iosurface;
#[cfg(target_os = "macos")]
mod layer_present;
/// A caret Core Animation draws over the frames (`GpuRenderer::set_caret`).
#[cfg(target_os = "macos")]
pub use layer_present::Caret;
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

/// The corner radius of find's marks behind text.
const MARK_RADIUS: f32 = 2.;
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
    /// Quads canvases drew instead of a raster (`canvas_quads`).
    pub canvas_quads: usize,
    /// SVGs rasterized.
    pub svg_rasterizations: usize,
    /// Times the instance geometry was rebuilt.
    pub geometry_rebuilds: usize,
    /// Paint items patched into the window as kept, in a frame that drew
    /// the rest as they were (see `render_kept`); 0 for a frame drawn from
    /// the scene's walk.
    pub patched_items: usize,
    /// Vertex buffers allocated or grown.
    pub vertex_buffer_allocations: usize,
    /// Physical pixels repainted.
    pub damaged_pixels: u64,
    /// Cached layers repainted.
    pub layer_repaints: usize,
    /// Of those, the ones repainted only where the window's damage reaches
    /// them (`prepare_layer`'s `shown`).
    pub layer_partial_repaints: usize,
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
    /// The vertex buffer's bytes (all of the ring's, `vertex_buffers`).
    pub vertex_buffer_bytes: usize,
    /// Vertex buffers held: one with a staging copy; up to four with
    /// vertices written in place, a ring of buffers in flight.
    pub vertex_buffers: usize,
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
    /// where `tint[0]` is 1. `tint[1]` and `tint[2]`: the rounded boxes'
    /// feathers (`RoundRect::feather`), in lanes the record has anyway.
    color: [[f32; 4]; 3],
    tint: [f32; 4],
}
impl Region {
    /// A shader fill's uniforms as a record of the region table (`time`
    /// first, its `params` in the four lanes after; see `fill_source`).
    fn uniforms(uniforms: zgx::decoration::ShaderUniforms) -> Self {
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
    /// Whether it leaves every pixel of `bounds` as it is: no colour filter,
    /// and `bounds` inside each rounded box past its largest corner, its
    /// feather and half a device pixel's antialiasing, and inside every fade
    /// band (draw.wgsl's `region` is exactly 1 there).
    fn covers(&self, bounds: Rect, scale: f32) -> bool {
        // (Slack past the edges, so rounding never reads a band's last step.)
        const SLACK: f32 = 1e-3;
        let inside = |round: [f32; 4], radii: [f32; 4], feather: f32| {
            if round[2] < 0. {
                return true;
            }
            let corner = radii.into_iter().fold(0f32, f32::max);
            let margin = corner + (0.5 / scale).max(feather) + SLACK;
            bounds.x >= round[0] + margin
                && bounds.y >= round[1] + margin
                && bounds.x + bounds.width <= round[0] + round[2] - margin
                && bounds.y + bounds.height <= round[1] + round[3] - margin
        };
        let [x0, y0, x1, y1] = self.fade_box;
        let [top, right, bottom, left] = self.fade;
        self.tint[0] == 0.
            && inside(self.round0, self.radii0, self.tint[1])
            && inside(self.round1, self.radii1, self.tint[2])
            && (top <= 0. || bounds.y >= y0 + top + SLACK)
            && (right <= 0. || bounds.x + bounds.width <= x1 - right - SLACK)
            && (bottom <= 0. || bounds.y + bounds.height <= y1 - bottom - SLACK)
            && (left <= 0. || bounds.x >= x0 + left + SLACK)
    }
    /// `item`'s, if it lies in one.
    fn of(item: &zgx::scene::PaintItem<'_>) -> Option<Self> {
        if item.mask().is_none()
            && item.rounded().iter().all(Option::is_none)
            && item.color().is_none()
        {
            return None;
        }
        let round = |clip: Option<zgx::scene::RoundRect>| match clip {
            Some(clip) => (
                [clip.rect.x, clip.rect.y, clip.rect.width, clip.rect.height],
                clip.radii,
            ),
            None => ([0., 0., -1., -1.], [0.; 4]),
        };
        let feather =
            |clip: Option<zgx::scene::RoundRect>| clip.map_or(0., |clip| clip.feather.points());
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
            tint: [
                if item.color().is_some() { 1. } else { 0. },
                feather(item.rounded()[0]),
                feather(item.rounded()[1]),
                0.,
            ],
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
    /// A cached layer composited through its layer shader
    /// (`zgx::scene::LayerShader`, by shader id): its pipeline, `shade` for
    /// the rest. Never batched with other shadings.
    Filter(u64),
    /// A cached layer blended with the backdrop (`zgx::scene::BlendMode`,
    /// its mode in `mask.z`): blend.wgsl's pipeline, reading a copy of the
    /// target under it, and replacing the target's pixels. Drawn alone, at
    /// the start of a pass (see `render_flat`).
    Blend,
    /// `Blend` through a layer shader (by shader id).
    FilterBlend(u64),
    /// An image drawn with an effect (`ImageEffect`): image_effects.wgsl's
    /// pipeline, compiled when one first draws, `shade` for whatever else
    /// the draw holds.
    ImageEffect,
    /// A canvas's polyline strokes (polyline.wgsl): their own pipeline,
    /// reading their points, never joined with another shading's quads.
    Polyline,
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
        && quad.mask[0] <= 0.
        // Drawn without the region it lies in (`Region::covers`): full
        // still, batched as it was.
        && quad.mask[3] <= 0.;
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
    /// Their module (draw.wgsl), made with the first pipeline: a window
    /// whose frames Metal encodes directly (metal_frame.rs) needs none.
    shader: std::cell::OnceCell<wgpu::ShaderModule>,
    layout: wgpu::PipelineLayout,
    full: std::cell::OnceCell<wgpu::RenderPipeline>,
    full_clear: std::cell::OnceCell<wgpu::RenderPipeline>,
    basic: std::cell::OnceCell<wgpu::RenderPipeline>,
    basic_clear: std::cell::OnceCell<wgpu::RenderPipeline>,
    /// The quad pipelines' group layouts (texture, viewport), for the
    /// polyline layout.
    texture_layout: wgpu::BindGroupLayout,
    viewport_layout: wgpu::BindGroupLayout,
    polylines: std::cell::OnceCell<Polylines>,
}
/// The polyline strokes' module (draw.wgsl and polyline.wgsl), its layout
/// (the quads' groups, and their points as group 2) and its pipeline into
/// the retained target: made when a frame first draws one.
struct Polylines {
    module: wgpu::ShaderModule,
    points_layout: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    pipeline: wgpu::RenderPipeline,
}
impl QuadPipelines {
    /// The pipeline drawing quads of `shading`.
    fn get(&self, shading: Shading) -> &wgpu::RenderPipeline {
        let (cell, basic, blend) = match shading {
            Shading::Full
            | Shading::Fill(_)
            | Shading::Filter(_)
            | Shading::Blend
            | Shading::FilterBlend(_)
            | Shading::ImageEffect => (&self.full, false, true),
            Shading::Basic => (&self.basic, true, true),
            Shading::Opaque => (&self.basic_clear, true, false),
            Shading::Polyline => return &self.polylines().pipeline,
        };
        cell.get_or_init(|| {
            quad_pipeline(
                &self.device,
                self.shader(),
                &self.layout,
                blend,
                None,
                basic,
            )
        })
    }
    fn shader(&self) -> &wgpu::ShaderModule {
        self.shader.get_or_init(|| {
            self.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("quads"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("draw.wgsl").into()),
                })
        })
    }
    fn polylines(&self) -> &Polylines {
        self.polylines.get_or_init(|| {
            let device = &self.device;
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("polylines"),
                source: wgpu::ShaderSource::Wgsl(polyline_source().into()),
            });
            let points_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("polyline points"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("polylines"),
                bind_group_layouts: &[
                    Some(&self.texture_layout),
                    Some(&self.viewport_layout),
                    Some(&points_layout),
                ],
                immediate_size: 0,
            });
            let pipeline = polyline_pipeline(device, &module, &layout, None, false);
            Polylines {
                module,
                points_layout,
                layout,
                pipeline,
            }
        })
    }
    /// The pipeline clearing a region to the background quad.
    fn clear(&self, basic: bool) -> &wgpu::RenderPipeline {
        if basic {
            self.get(Shading::Opaque)
        } else {
            self.full_clear.get_or_init(|| {
                quad_pipeline(
                    &self.device,
                    self.shader(),
                    &self.layout,
                    false,
                    None,
                    false,
                )
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
/// A canvas's polyline strokes (when `polylines`) draw with their
/// own pipeline: their runs are draws of their own however the rest is.
#[inline(always)]
fn push_draw(
    draws: &mut Vec<Draw>,
    quads: &[Quad],
    draw: Draw,
    split: bool,
    polylines: bool,
    scale: f32,
    glyphs: Option<Shading>,
) {
    let split = split
        && draw.blur.is_none()
        && !matches!(draw.source, TextureSource::Layer(_))
        && !matches!(
            draw.shading,
            Shading::Fill(_)
                | Shading::Filter(_)
                | Shading::Blend
                | Shading::FilterBlend(_)
                | Shading::ImageEffect
        );
    if !(split || polylines) || draw.end <= draw.start {
        draws.push(draw);
        return;
    }
    // Quads of one shading, known already (a text's glyphs): one run.
    if let Some(shading) = glyphs.filter(|_| split && !polylines) {
        draws.push(Draw { shading, ..draw });
        return;
    }
    // A polyline stroke's quad holds its point count in `mask.z`.
    let kind_of = |quad: &Quad| match polylines && quad.mask[2] > 0. {
        true => Shading::Polyline,
        false if split => shading(quad, scale),
        false => draw.shading,
    };
    // Each run ends where the shading changes, so runs alternate shadings.
    let mut start = draw.start;
    while start < draw.end {
        let kind = kind_of(&quads[start as usize]);
        let mut end = start + 1;
        while end < draw.end && kind_of(&quads[end as usize]) == kind {
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
            // A backdrop filter goes under the first run alone.
            blur: draw.blur.clone().filter(|_| start == draw.start),
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
/// What keeps a scroll move from being a copy of the pixels drawn
/// (`copy_blocker`), for devtools' note on the frame (`zgx inspect frames`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum CopyBlocker {
    /// Nothing of the content paints in the clip.
    NoContent,
    /// Something not of the content paints between its items.
    Between(NodeId),
    /// A content item is masked, isolated (a layer) or blurred.
    Effect(NodeId),
    /// A node paints over the content and does not repaint this frame.
    Over(NodeId),
    /// What lies under the content shows through and does not repaint.
    Under(NodeId),
}
impl CopyBlocker {
    /// The note, its nodes as `zgx inspect tree` numbers them.
    fn describe(self, scene: &Scene) -> String {
        let node =
            |id: NodeId| format!("@{} ({})", id.number(), zgx::inspect::kind_name(scene, id));
        match self {
            Self::NoContent => "none of its content paints in its view".into(),
            Self::Between(id) => format!("{} paints between its content's items", node(id)),
            Self::Effect(id) => format!("{} in it is masked, a layer or blurred", node(id)),
            Self::Over(id) => format!(
                "{} draws over it, so moving its pixels would move that too (keep what draws \
                 over a scroll view outside its box)",
                node(id)
            ),
            Self::Under(id) => format!(
                "{} shows under it (give the scroll view an opaque fill of its own)",
                node(id)
            ),
        }
    }
}
/// Whether moving `scroll`'s pixels reproduces the frame (`None`), and what
/// keeps it from it otherwise: its content is one contiguous run of paint
/// items free of masks, layers and filters; beneath it the clip shows a
/// single opaque rectangle (or the window background); above it nothing
/// paints in the clip outside this frame's damage.
fn copy_blocker(
    scene: &Scene,
    scroll: &zgx::scene::ScrollMove,
    damage: &[Rect],
    scale: f32,
) -> Option<CopyBlocker> {
    let clip = scroll.clip;
    let items = scene.layer_items_within(None, Some(&[clip]));
    let content = |item: &zgx::scene::PaintItem<'_>| scene.is_within(item.id, scroll.node);
    let Some(first) = items.iter().position(content) else {
        return Some(CopyBlocker::NoContent);
    };
    let last = items.iter().rposition(content).expect("found above");
    let paints = |item: &zgx::scene::PaintItem<'_>| -> Option<Rect> {
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
        if !content(item) {
            return Some(CopyBlocker::Between(item.id));
        }
        if item.mask().is_some() || item.isolated || item.effects.blur_radius > 0. {
            return Some(CopyBlocker::Effect(item.id));
        }
    }
    if let Some(over) = items[last + 1..]
        .iter()
        .find(|item| paints(item).is_some_and(|rect| !repainted(rect)))
    {
        return Some(CopyBlocker::Over(over.id));
    }
    // Content opaque across the whole clip (a list's rows) hides whatever
    // lies under it, before the move and after.
    let opaque = |item: &&zgx::scene::PaintItem<'_>| {
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
        return None;
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
            return None;
        }
        if !repainted(rect) {
            return Some(CopyBlocker::Under(item.id));
        }
    }
    None
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
#[inline]
fn glyphs_of<'a>(
    glyphs: Vec<Glyph>,
    runs: impl Iterator<Item = cosmic_text::LayoutRun<'a>>,
    starts: &[usize],
    rich: Option<&zgx::rich_text::RichText>,
    scale: f32,
) -> Vec<Glyph> {
    match rich {
        // Glyphs come mostly in their run's order: the run of the last one
        // is tried first, and searched for (`run_at`) only when it isn't
        // this one's (runs cover the text in order, `RichText::new`).
        Some(rich) => {
            let mut current = 0;
            styled_glyphs(glyphs, runs, scale, |line, glyph| {
                let byte = starts[line] + glyph.start;
                if !rich.runs()[current].range.contains(&byte) {
                    current = text::run_at(rich, byte);
                }
                (current, Some(rich.runs()[current].color))
            })
        }
        None => styled_glyphs(glyphs, runs, scale, |_, glyph| {
            (
                0,
                glyph
                    .color_opt
                    .map(|c| zgx::scene::Color(c.r(), c.g(), c.b(), c.a())),
            )
        }),
    }
}
/// `glyphs_of`, each glyph's run and colour from `style` (given its line).
fn styled_glyphs<'a>(
    mut glyphs: Vec<Glyph>,
    runs: impl Iterator<Item = cosmic_text::LayoutRun<'a>>,
    scale: f32,
    mut style: impl FnMut(usize, &cosmic_text::LayoutGlyph) -> (usize, Option<zgx::scene::Color>),
) -> Vec<Glyph> {
    for run in runs {
        // Most texts are one line: one allocation at its size.
        glyphs.reserve(run.glyphs.len());
        for glyph in run.glyphs {
            let p = glyph.physical((0., 0.), scale);
            let (run_index, color) = style(run.line_i, glyph);
            glyphs.push(Glyph {
                key: p.cache_key,
                color,
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
                if draw.source != TextureSource::Atlas
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
    color: Option<zgx::scene::Color>,
    /// The rich run it was shaped from, to recolour paint-only changes.
    run: usize,
    x: f32,
    y: f32,
}
/// A text's cached quad, in its own place: its box, atlas cell, colour and
/// paint (corner radius, glyph flag, rounded-background flag). The rest of
/// the GPU `Quad` is the same for every one of a text's and is made where it
/// is placed (`render_flat`): half the bytes cached, copied and read.
#[derive(Clone, Copy)]
struct TextQuad {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    paint: [f32; 4],
}
/// A kept text's quads at `x`, `y`, each coloured by `color`.
fn place_text_quads(
    quads: &mut Vec<Quad>,
    source: &[TextQuad],
    x: f32,
    y: f32,
    fade: [f32; 4],
    edge_fade: f32,
    mut color: impl FnMut(&TextQuad) -> [f32; 4],
) {
    quads.extend(source.iter().map(|cached| Quad {
        rect: [
            cached.rect[0] + x,
            cached.rect[1] + y,
            cached.rect[2],
            cached.rect[3],
        ],
        uv: cached.uv,
        color: color(cached),
        fade,
        options: [edge_fade, cached.paint[1], 0., 0.],
        shape: [cached.paint[0], 0., 0., cached.paint[3]],
        border: [0.; 4],
        mask: NO_MASK,
    }));
}
struct Shaped {
    rich: Option<Arc<zgx::rich_text::RichText>>,
    text_options: zgx::text_layout::TextOptions,
    decorations: Vec<text::RichDecoration>,
    font: zgx::text_layout::FontStyle,
    text: Arc<str>,
    size: f32,
    width: f32,
    height: f32,
    /// Device pixels per logical pixel the glyphs were placed at: a layer's
    /// raster scale differs from the window's.
    scale: f32,
    glyphs: Vec<Glyph>,
    quads: Vec<TextQuad>,
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
            // (Charged as the GPU quads they are placed as: the cache keeps
            // as many shapes as when it held those.)
            + self.quads.capacity() * std::mem::size_of::<Quad>()
    }
}
/// The atlas's key: cosmic-text's, losslessly, with its scalar fields
/// packed into words so it hashes and compares in a few operations.
/// Four-byte aligned, its table's buckets are the size of those holding
/// cosmic-text's key.
#[derive(Clone, Copy, PartialEq, Eq)]
struct AtlasKey {
    font: cosmic_text::fontdb::ID,
    words: [u32; 4],
}
impl From<CacheKey> for AtlasKey {
    fn from(key: CacheKey) -> Self {
        // Bins take 2 bits each (4 variants), the glyph, weight and optical
        // size 16 each: 52 bits.
        let glyph = (key.x_bin as u64)
            | ((key.y_bin as u64) << 2)
            | (u64::from(key.glyph_id) << 4)
            | (u64::from(key.font_weight.0) << 20)
            | (u64::from(key.font_optical) << 36);
        Self {
            font: key.font_id,
            words: [
                key.font_size_bits,
                key.flags.bits(),
                glyph as u32,
                (glyph >> 32) as u32,
            ],
        }
    }
}
impl std::hash::Hash for AtlasKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.font, state);
        state.write_u64(u64::from(self.words[0]) | (u64::from(self.words[1]) << 32));
        state.write_u64(u64::from(self.words[2]) | (u64::from(self.words[3]) << 32));
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
    /// What it painted, in the scene's points (`Scene::layer_bounds`).
    content: Rect,
    revision: u64,
    root_origin: (f32, f32),
    bytes: usize,
    /// The content's extent in texture coordinates.
    uv: [f32; 2],
    /// Its raster scale over the device's, as steps of √2 (`raster_step`).
    step: i32,
    /// That scale's bits: a step's, or a zoom's exactly (`prepare_layer`).
    factor: u32,
    /// At a zoom's scale exactly: composited with its corner on a pixel,
    /// texel for pixel.
    exact: bool,
    /// Promoted by a local transform rather than isolation: kept a while
    /// once back at identity (`LAYER_PARK_FRAMES`).
    transformed: bool,
    /// The frame (`GpuRenderer::layer_frame`) it was last a layer in.
    used: u64,
    /// No longer a layer, kept for its pixels.
    parked: bool,
    /// Where in the window all of it showed when last drawn (`shown`),
    /// while every frame since that damaged it also drew it: its pixels
    /// then differ from its content's only where the window's damage
    /// says, and it can repaint just there. A frame damaging it without
    /// drawing it (hidden, culled) ends that.
    watched: Option<Rect>,
}
/// How a layer of the window's own shows there (`GpuRenderer::shown`).
#[derive(Clone, Copy)]
struct Shown<'a> {
    /// The window's damage this frame.
    damage: &'a [Rect],
    /// Its content onto the window.
    map: zgx::affine::Affine,
    /// What of the window it can show in.
    clip: Rect,
}
impl Shown<'_> {
    /// Where `content` shows in the window, if all of it does.
    fn window(&self, content: Rect) -> Option<Rect> {
        let window = self.map.bounds(content);
        within(self.clip, window).then_some(window)
    }
}
/// Whether `inner` lies within `outer`.
fn within(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}
/// A layer's repaint, counted in the frame's `stats`.
fn add_layer_stats(stats: &mut GpuStats, result: &GpuStats) {
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
/// A whole window's paint as `render_flat` made it, kept to patch where
/// the scene repaints (`render_kept`): its quads and draws, and where each
/// item's lie.
struct Kept {
    quads: Vec<Quad>,
    draws: Vec<Draw>,
    clips: Vec<Region>,
    items: Vec<KeptItem>,
    /// Each node's first item (a border pass follows it).
    by_node: KeptIndex,
    /// The scene's last flush it was brought up to date with.
    flushes: u64,
    /// What it was drawn for: the scene's nodes and identity, the atlas
    /// its glyphs lie in, and the target.
    revision: u64,
    identity: u64,
    atlas_epoch: u64,
    viewport: Rect,
    scale: f32,
    background: Color,
    /// Whether a frame has patched it yet.
    patched: bool,
    /// What its quads' patches wrote, for a vertex buffer still holding
    /// them to take only that (`MappedRing::write_kept`).
    upload: upload::KeptQuads,
    /// How many of its quads lie in paint order (as kept, or spliced):
    /// texts outgrowing their room move past them, to its end, where a
    /// subtree's quads no longer lie together.
    ordered: u32,
}
/// Each node's first item in a kept frame (`Kept::by_node`): a node's slot
/// number indexes a dense table while the frame's nodes are numbered close
/// together, a hash map otherwise. The item itself witnesses a slot's full
/// identity (number and generation), so a dense table can keep stale
/// entries after a splice: a node no longer there fails the item's bounds
/// or its identity check.
enum KeptIndex {
    Dense(Vec<u32>),
    Sparse(FxHashMap<NodeId, u32>),
}
impl KeptIndex {
    fn new(items: &[KeptItem]) -> Self {
        let span = items
            .iter()
            .map(|item| item.id.number() as usize + 1)
            .max()
            .unwrap_or(0);
        // A node has at most an ordinary item and a border item: six bytes
        // an item fit within the map's twelve-byte entry a node.
        let room = items.len().saturating_add(items.len() / 2);
        if span <= room {
            let mut slots = vec![0; room];
            for (index, item) in items.iter().enumerate().rev() {
                slots[item.id.number() as usize] = index as u32;
            }
            Self::Dense(slots)
        } else {
            let mut map = FxHashMap::default();
            Self::fill(&mut map, items);
            Self::Sparse(map)
        }
    }
    /// The distinct nodes among `items`: a node's second item is its border
    /// pass, at the end of its subtree.
    fn nodes(items: &[KeptItem]) -> usize {
        let borders = items
            .iter()
            .enumerate()
            .filter(|&(at, item)| {
                let end = item.end as usize;
                end > at + 1 && items[end - 1].id == item.id
            })
            .count();
        items.len() - borders
    }
    fn fill(map: &mut FxHashMap<NodeId, u32>, items: &[KeptItem]) {
        for (index, item) in items.iter().enumerate().rev() {
            map.insert(item.id, index as u32);
        }
    }
    #[inline]
    fn get(&self, id: NodeId, items: &[KeptItem]) -> Option<usize> {
        match self {
            Self::Dense(slots) => {
                let index = *slots.get(id.number() as usize)? as usize;
                (items.get(index)?.id == id).then_some(index)
            }
            Self::Sparse(map) => map.get(&id).map(|&index| index as usize),
        }
    }
    /// Brought up to date with `items` after splices (`renumbered`: all
    /// of them moved), `replaced` the nodes they took out (kept only for a
    /// sparse index).
    fn patch(
        &mut self,
        items: &[KeptItem],
        renumbered: bool,
        replaced: Vec<NodeId>,
        splices: &[(usize, usize, NodeId)],
    ) {
        match self {
            Self::Dense(slots) => {
                let span = if renumbered {
                    items.iter().map(|item| item.id.number() as usize + 1).max()
                } else {
                    splices
                        .iter()
                        .flat_map(|&(first, end, _)| &items[first..end])
                        .map(|item| item.id.number() as usize + 1)
                        .max()
                }
                .unwrap_or(0);
                if span > slots.len() {
                    let room = items.len().saturating_add(items.len() / 2);
                    if span > room {
                        let nodes = Self::nodes(items);
                        // The dense table goes first, and the map is made
                        // once at its size: growing it would hold two.
                        *self = Self::Sparse(FxHashMap::default());
                        let Self::Sparse(map) = self else {
                            unreachable!()
                        };
                        map.reserve(nodes);
                        Self::fill(map, items);
                        return;
                    }
                    slots.reserve_exact(room - slots.len());
                    slots.resize(room, 0);
                }
                if renumbered {
                    for (index, item) in items.iter().enumerate().rev() {
                        slots[item.id.number() as usize] = index as u32;
                    }
                } else {
                    for &(first, end, _) in splices {
                        for index in (first..end).rev() {
                            slots[items[index].id.number() as usize] = index as u32;
                        }
                    }
                }
            }
            Self::Sparse(map) => {
                if renumbered {
                    map.clear();
                    Self::fill(map, items);
                } else {
                    for id in replaced {
                        map.remove(&id);
                    }
                    for &(first, end, _) in splices {
                        for index in (first..end).rev() {
                            map.insert(items[index].id, index as u32);
                        }
                    }
                }
            }
        }
    }
}
/// What `render_flat` makes: a frame (`FRAME`, or the kept frame when
/// `draw_made`), or items' quads and draws alone, with where each
/// item's lie (`PATCH`: no frame begins, nothing is drawn), to patch a kept
/// frame with or, `record`ing, to keep. Frames drawn as usual run a loop
/// with nothing of keeping in it, so what it calls inlines as before.
const FRAME: u8 = 0;
const PATCH: u8 = 2;
/// One paint item's place in a `Kept`.
struct KeptItem {
    id: NodeId,
    inherited: zgx::scene::Inherited,
    /// Its quads: `len` of them, from `start`, in room for `room`. A text's
    /// value grows into spare quads after its own, kept empty (zero-sized,
    /// drawing nothing): a batch of draws in turn spans them.
    start: u32,
    len: u32,
    room: u32,
    /// Its draws: those of its quads' shadings, in turn.
    draws: std::ops::Range<u32>,
    /// A plain box's clip and compositing, to patch a move of it alone.
    boxed: Option<KeptBox>,
    /// The item after its subtree's (its own, its descendants' and its
    /// border pass): the run a splice replaces.
    end: u32,
}
/// Where each of `items`' subtrees ends (`KeptItem::end`), numbered from
/// `base`: items in paint order, each node's descendants after it and its
/// border pass after them.
fn subtree_ends(scene: &Scene, items: &mut [KeptItem], base: u32) {
    let mut open: Vec<(NodeId, usize)> = Vec::new();
    for at in 0..items.len() {
        let id = items[at].id;
        let after = base + at as u32;
        if open.iter().any(|&(node, _)| node == id) {
            // Its border pass closes it.
            while let Some((node, first)) = open.pop() {
                items[first].end = after + u32::from(node == id);
                if node == id {
                    break;
                }
            }
            items[at].end = after + 1;
            continue;
        }
        let parent = scene.parent(id);
        while let Some(&(node, first)) = open.last()
            && Some(node) != parent
        {
            items[first].end = after;
            open.pop();
        }
        open.push((id, at));
    }
    let after = base + items.len() as u32;
    for (_, first) in open {
        items[first].end = after;
    }
}
/// Paint that changes without the scene marking its node: a native
/// surface shows each frame its source gives it.
fn changes_unmarked(kind: &NodeKind) -> bool {
    #[cfg(target_os = "macos")]
    if matches!(kind, NodeKind::NativeSurface(_)) {
        return true;
    }
    let _ = kind;
    false
}
/// The share of the window a frame's damage covers from which it is drawn
/// from the window as kept (`render_kept`): sending every item's quads as
/// they are costs about 30 instructions an item, making a damaged one again
/// about 1,500.
const KEPT_SHARE: f32 = 0.1;
/// Frames in a row a kept window would have patched and drawn from (leaves
/// alone repainting, over `KEPT_SHARE` of it) after which a window that
/// lost its kept frame keeps one again (`render_once`): one alone may be a
/// change's last, and keeping costs a whole frame.
const KEEP_AGAIN: u8 = 2;
/// Kept frames lost in a row before they patched once, after which a frame
/// drawn whole keeps none (`GpuRenderer::unpatched_keeps`).
const UNPATCHED_KEEPS: u8 = 2;
/// The share of a kept window's items its spliced subtrees may hold
/// (`render_kept`): past it, walking them again costs more than drawing
/// the frame as usual.
const SPLICE_SHARE: f32 = 0.25;
/// The share of a window a frame's damage must cover for its repainted
/// subtrees to be spliced (`render_kept`): below it, drawing the frame as
/// usual walks less.
const SPLICE_DAMAGE: f32 = 0.5;
/// A plain box (`plain_box`) as `patch_box` makes it: what it paints, its
/// box, its clip and how it composites.
#[derive(Clone, Copy)]
struct PlainBox<'a> {
    kind: &'a NodeKind,
    bounds: Rect,
    clip: Option<Rect>,
    opacity: f32,
    edge_fade: f32,
}
impl<'a> PlainBox<'a> {
    fn of(item: &zgx::scene::PaintItem<'a>) -> Self {
        PlainBox {
            kind: item.kind,
            bounds: item.bounds,
            clip: item.clip,
            opacity: item.effects.opacity,
            edge_fade: item.effects.edge_fade,
        }
    }
}
/// What a kept box (a plain `Quad` or `Panel`) keeps to be patched when
/// layout alone moved or resized it: its clip (its parent's) and how it
/// composites, which only a repaint of more than its box changes.
#[derive(Clone, Copy)]
struct KeptBox {
    clip: Option<Rect>,
    opacity: f32,
    edge_fade: f32,
}
impl KeptBox {
    /// `item`'s, if it is one; `plain`: whether it is a plain box.
    fn of(item: &zgx::scene::PaintItem<'_>, plain: bool) -> Option<Self> {
        let boxed = matches!(item.kind, NodeKind::Quad(_) | NodeKind::Panel { .. });
        (boxed && plain).then_some(KeptBox {
            clip: item.clip,
            opacity: item.effects.opacity,
            edge_fade: item.effects.edge_fade,
        })
    }
}
/// Whether `kind` paints as a plain box would (`plain_box`): a box without
/// decoration, shadows or a pass over its content.
#[inline(always)]
fn plain_quad(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. }
        if style.decoration.is_none() && style.shadows().is_empty() && !style.paints_over())
}
/// Whether `item` is a box `render_kept` makes alone (`patch_box`): a
/// flat colour, or a box without decoration, shadows or a pass over its
/// content, drawn flat (no layer, filter, clip region or fade mask).
#[inline(always)]
fn plain_box(item: &zgx::scene::PaintItem<'_>) -> bool {
    let plain = match item.kind {
        NodeKind::Rect(_) => true,
        NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } => {
            style.decoration.is_none() && style.shadows().is_empty() && !style.paints_over()
        }
        _ => false,
    };
    plain
        && !item.isolated
        && !item.border_pass
        && item.effects.blur_radius <= 0.
        && item.matrix().is_none()
        && item.mask().is_none()
        && item.rounded().iter().all(Option::is_none)
        && item.color().is_none()
}
/// A flat colour's quad over `fade` (a box's place), as it shows.
#[inline(always)]
fn rect_quad(color: Color, fade: [f32; 4], effects: zgx::scene::Effects) -> Quad {
    Quad {
        rect: fade,
        uv: [0.; 4],
        color: rgba(color, effects.opacity),
        fade,
        options: [effects.edge_fade, 0., 0., 0.],
        shape: [0.; 4],
        border: [0.; 4],
        mask: NO_MASK,
    }
}
/// A box's quad with no decoration: its fill, corner radius and border
/// (none when drawn by its border pass, `later`), before `push_panel`.
#[inline(always)]
fn plain_panel_quad(
    style: &zgx::scene::QuadStyle,
    fade: [f32; 4],
    opacity: f32,
    edge_fade: f32,
    later: bool,
) -> Quad {
    let border = match later {
        true => 0.,
        false => style.border_width.max(0.),
    };
    Quad {
        rect: fade,
        uv: [0.; 4],
        color: rgba(style.fill, opacity),
        fade,
        options: [edge_fade, 0., 0., 0.],
        shape: [style.radius.max(0.), border, 0., 1.],
        border: rgba(style.border_color, opacity),
        mask: NO_MASK,
    }
}
/// When `clip` only culls whole quads of those from `start`, apply it on
/// the CPU and draw under the viewport, so a change of clip never splits a
/// batch: neighbouring text, icons and rows become one draw. Quads it
/// would cut and transformed quads keep the scissor. The draw's clip, and
/// the kept quads' bounds when culled here.
#[inline(always)]
fn cull_on_cpu(
    quads: &mut Vec<Quad>,
    start: usize,
    clip: Rect,
    viewport: Rect,
    scale: f32,
) -> (Rect, Option<Rect>) {
    let limit = snap_out(clip, scale);
    // One pass: each quad inside the clip, outside it, or cut.
    let (mut cut, mut outside, mut kept) = (false, false, None::<Rect>);
    for quad in &quads[start..] {
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
    if cut {
        return (clip, None);
    }
    if outside {
        let mut kept = start;
        for index in start..quads.len() {
            if inside(&quads[index], limit) {
                quads[kept] = quads[index];
                kept += 1;
            }
        }
        quads.truncate(kept);
    }
    (viewport, Some(kept.unwrap_or_default()))
}
/// `cull_on_cpu` of a text's glyphs alone (`glyphs_alone`): untransformed
/// quads of sizes never negative, each its own rectangle (`quad_rect`),
/// tested and joined as such in one pass.
#[inline(always)]
fn cull_glyphs(
    quads: &mut Vec<Quad>,
    start: usize,
    clip: Rect,
    viewport: Rect,
    scale: f32,
) -> (Rect, Option<Rect>) {
    let limit = snap_out(clip, scale);
    let (mut outside, mut kept) = (false, None::<Rect>);
    for quad in &quads[start..] {
        let [x, y, width, height] = quad.rect;
        if quad.options[2] >= 0.5 || width < 0. || height < 0. {
            return cull_on_cpu(quads, start, clip, viewport, scale);
        }
        // (As `quad_rect` has it: a zero size's sign dropped.)
        let rect = Rect::new(x, y, width.abs(), height.abs());
        if rect_inside(rect, limit) {
            kept = Some(kept.map_or(rect, |bounds| bounds.union(rect)));
        } else if rect_outside(rect, limit) {
            outside = true;
        } else {
            return (clip, None);
        }
    }
    if outside {
        let mut kept = start;
        for index in start..quads.len() {
            if inside(&quads[index], limit) {
                quads[kept] = quads[index];
                kept += 1;
            }
        }
        quads.truncate(kept);
    }
    (viewport, Some(kept.unwrap_or_default()))
}
/// Close the open item range (`render_flat`'s `item_ranges`): its quads'
/// count, its room (`pad`: a text's spare quads, added empty) and where
/// its draws end.
fn close_range(
    quads: &mut Vec<Quad>,
    draws: &[Draw],
    ranges: &mut [(u32, u32, u32, u32, u32)],
    pad: bool,
) {
    let Some(open) = ranges.last_mut().filter(|range| range.4 == u32::MAX) else {
        return;
    };
    let made = quads.len() as u32 - open.0;
    let room = made + if pad { text_room(made) } else { 0 };
    quads.resize((open.0 + room) as usize, bytemuck::Zeroable::zeroed());
    *open = (open.0, made, room, open.3, draws.len() as u32);
}
/// Frames in a row a kept window would have patched (`would_patch`), read
/// up to `flushes` and content `revision`; after a second in a row it
/// wouldn't, the scene's repaints go unread for `skip` frames (one, then
/// twice as many each time, up to 32), so that flushes stop keeping them
/// where nothing patches.
#[derive(Default)]
struct Patchable {
    run: u8,
    flushes: u64,
    revision: u64,
    skip: u8,
    backoff: u8,
    /// A kept frame was given up since one last drew from it, its splices
    /// walking too much of it (`SPLICE_SHARE`): frames repainting more than
    /// leaves don't count as frames that would patch until one does.
    refused: bool,
}
/// The items a frame loop leaves (`render_flat`, which may return early),
/// dropped out of line: their drop inlined at each of its exits grows the
/// loop, which then costs every frame more instructions (measured: 0.5%).
struct LeftItems<'a, T>(&'a mut std::vec::IntoIter<T>);
impl<T> Drop for LeftItems<'_, T> {
    #[inline(never)]
    fn drop(&mut self) {
        self.0.by_ref().for_each(drop);
    }
}
/// Spare quads a text that changes (one a kept frame patched) keeps for
/// its value to grow into, past its own: few, as a batch drawing the kept
/// frame spans them (empty, they still cost the GPU), and a text
/// outgrowing them moves to the kept frame's end (`render_kept`).
fn text_room(glyphs: u32) -> u32 {
    (glyphs / 4).max(2)
}
/// `map.get(key)`, or in a patch (`PATCH`) the same through `entry`: the
/// frame loop stays the one caller of each map's `get` (and of its search,
/// which `get_mut` shares), inlined there.
#[inline(always)]
fn get_in<'a, const MODE: u8, K: Eq + std::hash::Hash + Clone, V>(
    map: &'a mut FxHashMap<K, V>,
    key: &K,
) -> Option<&'a V> {
    match MODE {
        PATCH => match map.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
            std::collections::hash_map::Entry::Vacant(_) => None,
        },
        _ => map.get(key),
    }
}
/// `map.insert(key, value)`, or in a patch (`PATCH`) the same through
/// `entry`: the frame loop stays the one caller of each map's `insert`.
#[inline(always)]
fn insert_in<const MODE: u8, K: Eq + std::hash::Hash, V>(
    map: &mut FxHashMap<K, V>,
    key: K,
    value: V,
) {
    match MODE {
        PATCH => {
            map.entry(key).insert_entry(value);
        }
        _ => {
            map.insert(key, value);
        }
    }
}
impl GpuStats {
    /// Add what `made` made (shaped, uploaded, rasterized) in a pass that
    /// drew nothing.
    fn add_made(&mut self, made: &GpuStats) {
        self.shaped_nodes += made.shaped_nodes;
        self.glyph_uploads += made.glyph_uploads;
        self.image_uploads += made.image_uploads;
        self.canvas_rasterizations += made.canvas_rasterizations;
        self.canvas_quads += made.canvas_quads;
        self.svg_rasterizations += made.svg_rasterizations;
        self.geometry_rebuilds += made.geometry_rebuilds;
    }
}
/// A backdrop filter's bounds, effects and corner radii (`Draw::blur`).
type Backdrop = (Rect, zgx::scene::Effects, [f32; 4]);
/// What a draw's quads sample: the glyph atlas, an image (or the effect
/// atlas) by id, or a cached layer. One of them, never two.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TextureSource {
    Atlas,
    Image(u64),
    Layer(NodeId),
}
#[derive(Clone)]
struct Draw {
    /// How its quads are drawn; set by `push_draw`.
    shading: Shading,
    /// A backdrop filter: its bounds, effects and corner radii. Boxed: few
    /// draws have one, and a frame copies every draw a few times.
    blur: Option<Box<Backdrop>>,
    start: u32,
    end: u32,
    clip: Rect,
    /// Union of this draw's quads: nothing outside it is ever touched.
    bounds: Rect,
    source: TextureSource,
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
    /// The polyline strokes' points (group 2), if it draws any.
    points: Option<wgpu::BindGroup>,
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
    effects: zgx::scene::Effects,
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
/// A surface of the renderer's own a window presents through.
#[cfg(target_os = "macos")]
type OwnSurface = layer_present::Buffer;
#[cfg(windows)]
type OwnSurface = dxgi_present::Buffer;
/// Presentation through surfaces of the renderer's own, which keep what they
/// held: macOS's (`layer_present`) and Windows' swap chain
/// (`dxgi_present`).
#[cfg(own_surfaces)]
#[derive(Default)]
struct LayerPresent {
    #[cfg(target_os = "macos")]
    layer: Option<layer_present::Layer>,
    /// The caret the host draws over the frames (`GpuRenderer::set_caret`),
    /// for the layer made after it is set.
    #[cfg(target_os = "macos")]
    caret: Option<layer_present::Caret>,
    /// Windows: the swap chain whose buffers are the surfaces.
    #[cfg(windows)]
    chain: Option<dxgi_present::Chain>,
    /// Surfaces, each with the frame it holds (None: none yet).
    buffers: Vec<(OwnSurface, Option<u64>)>,
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
    #[cfg(target_os = "macos")]
    full_rate: bool,
    /// Since full-rate frames stopped, until when the video layer stays
    /// (`VIDEO_LINGER`); `None` while they come, or once it went.
    #[cfg(target_os = "macos")]
    video_until: Option<std::time::Instant>,
    /// When the window last changed size. Frames show on the plain layer
    /// until `RESIZE_SETTLE` after it (`Layer::show_plain`).
    #[cfg(target_os = "macos")]
    resized_at: Option<std::time::Instant>,
    /// The layer to present into instead of the window's (an overlay plane).
    #[cfg(target_os = "macos")]
    host: Option<objc2::rc::Retained<objc2_quartz_core::CALayer>>,
    /// Frames show on the plain layer until then (`present_plain_for`), or
    /// always (`plain_only`): they change in the caller's transaction.
    #[cfg(target_os = "macos")]
    plain_until: Option<std::time::Instant>,
    #[cfg(target_os = "macos")]
    plain_only: bool,
    /// Native views share the window (`present_with_native_views`): frames
    /// show on the plain layer, in the transaction that moves them.
    #[cfg(target_os = "macos")]
    with_natives: bool,
}
#[cfg(own_surfaces)]
impl LayerPresent {
    /// What surface `index` lacks of the latest frame before a frame draws
    /// into it: the changes of every frame since the one it holds (all of it
    /// once those are no longer kept); none when the frame repaints
    /// everything (`whole`) or it holds the latest.
    fn catch_up_for(&self, index: usize, whole: bool, width: u32, height: u32) -> Vec<Scissor> {
        let held = self.buffers[index].1;
        let latest = self.shown.and_then(|shown| self.buffers[shown].1);
        catch_up(&self.history, held, latest, whole, (width, height))
    }
    /// Surface `index` was shown: it holds the newest frame, whose changes
    /// join the history.
    fn presented(&mut self, index: usize) {
        self.frame += 1;
        let frame = self.frame;
        self.buffers[index].1 = Some(frame);
        self.shown = Some(index);
        let changed = std::mem::take(&mut self.changed);
        self.history.push_back((frame, changed));
        if self.history.len() > LAYER_HISTORY {
            self.history.pop_front();
        }
    }
}
/// What a surface holding frame `held` lacks of frame `latest` (the one
/// shown), from `history`, the changes of recent frames: see
/// `LayerPresent::catch_up_for`. All of a `size` surface once the frames
/// after `held` are no longer kept.
#[cfg(own_surfaces)]
fn catch_up(
    history: &std::collections::VecDeque<(u64, Vec<Scissor>)>,
    held: Option<u64>,
    latest: Option<u64>,
    whole: bool,
    (width, height): (u32, u32),
) -> Vec<Scissor> {
    match (whole, latest) {
        (true, _) | (false, None) => Vec::new(),
        (false, Some(latest)) if held == Some(latest) => Vec::new(),
        (false, Some(_)) => match held {
            // The changes of every frame since the one it holds.
            Some(held) if history.front().is_some_and(|(f, _)| *f <= held + 1) => history
                .iter()
                .filter(|(frame, _)| *frame > held)
                .flat_map(|(_, changed)| changed.iter().copied())
                .collect(),
            _ => vec![(0, 0, width, height)],
        },
    }
}
#[cfg(all(test, own_surfaces))]
mod catch_up_tests {
    use super::catch_up;
    #[test]
    fn a_surface_takes_the_changes_since_the_frame_it_holds() {
        let history: std::collections::VecDeque<_> = [
            (3, vec![(0, 0, 4, 4)]),
            (4, vec![(10, 0, 4, 4)]),
            (5, vec![(20, 0, 4, 4), (30, 0, 2, 2)]),
        ]
        .into();
        let size = (100, 50);
        // Two frames behind: the changes of 4 and 5.
        assert_eq!(
            catch_up(&history, Some(3), Some(5), false, size),
            [(10, 0, 4, 4), (20, 0, 4, 4), (30, 0, 2, 2)]
        );
        // Holding the shown frame, or drawn whole: nothing to take.
        assert!(catch_up(&history, Some(5), Some(5), false, size).is_empty());
        assert!(catch_up(&history, Some(3), Some(5), true, size).is_empty());
        // None shown yet: the frame draws whole.
        assert!(catch_up(&history, None, None, false, size).is_empty());
        // Never drawn, or behind what the history keeps: all of it.
        assert_eq!(
            catch_up(&history, None, Some(5), false, size),
            [(0, 0, 100, 50)]
        );
        assert_eq!(
            catch_up(&history, Some(1), Some(5), false, size),
            [(0, 0, 100, 50)]
        );
        // Just inside it: the frame after the one held is the oldest kept.
        assert_eq!(
            catch_up(&history, Some(2), Some(5), false, size).len(),
            4,
            "frames 3, 4 and 5"
        );
    }
}
/// Windows: the swap chain's buffers go before the chain they are of
/// (`dxgi_present::Chain::release`).
#[cfg(windows)]
impl Drop for LayerPresent {
    fn drop(&mut self) {
        if let Some(chain) = &self.chain {
            chain.release(self.buffers.drain(..).map(|(buffer, _)| buffer));
        }
    }
}
#[cfg(target_os = "macos")]
impl LayerPresent {
    /// For `ZGX_PRESENT_LOG`: which plane this presents.
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
#[cfg(own_surfaces)]
const LAYER_HISTORY: usize = 8;
/// How long after a size change frames keep to the plain layer, so a live
/// resize's frames land with its window sizes before queueing resumes.
#[cfg(target_os = "macos")]
const RESIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);
/// `ZGX_PRESENT_VIDEO`: frames at the display's full rate are queued on a
/// video layer (`AVSampleBufferDisplayLayer`) instead of shown as the plain
/// layer's tiles, as they were until tiles were measured better (2026-10-25,
/// animate at 120 Hz: CPU 4.84% → 4.03%, memory 253 → 242 MB, the 99th
/// percentile frame interval 20.6 → 17.7 ms, and 13.6 ms with the host
/// aligning frames to refreshes; GPU power and the window server's GPU the
/// same). Kept for a release, to compare with.
#[cfg(target_os = "macos")]
static VIDEO_AT_FULL_RATE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| zgx::env::var_os("ZGX_PRESENT_VIDEO").is_some());
/// `ZGX_GPU_WARM=0`: the first input after a window idled (`trim`) no longer
/// wakes the GPU ahead of its frame (`GpuRenderer::warm_up`). Measured
/// (2026-10-26, the mail example after idle, 8 gestures a cell): the first
/// frame's present 6.7 → 1.7 ms, the second frame's time and the CPU and
/// instructions per frame shown, at rest and animating, unchanged.
#[cfg(target_os = "macos")]
static WARM_AFTER_IDLE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| zgx::env::var_os("ZGX_GPU_WARM").is_none_or(|v| v != "0"));
/// `ZGX_DXGI_PRESENT=0`: a Windows window presents through wgpu's swap
/// chain, copying the retained frame into it each frame, as it did until
/// its own was measured better (2026-10-26, RTX 4090, against wgpu's:
/// app GPU 33 to 45% less in animate, vlist, lab, 12% in heavy; the
/// compositor's GPU 16% less in animate and its CPU 7 to 25% less; cycles
/// a frame 2 to 5% fewer; startup to the first text 346 → 280 ms). Kept
/// for a release, to compare with.
#[cfg(windows)]
static DXGI_PRESENT: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| zgx::env::var_os("ZGX_DXGI_PRESENT").is_none_or(|v| v != "0"));
/// How long the video layer outlasts full-rate frames (`LayerPresent::video_until`).
#[cfg(target_os = "macos")]
const VIDEO_LINGER: std::time::Duration = std::time::Duration::from_secs(1);
/// A pass's pipelines for each shading, and the one clearing a region.
struct PickedPipelines {
    full: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
    basic: wgpu::RenderPipeline,
    opaque: wgpu::RenderPipeline,
    /// Made when a frame first draws polylines (`picks`).
    polyline: std::cell::OnceCell<wgpu::RenderPipeline>,
}
impl PickedPipelines {
    /// Its pipelines, the polyline one made first if `make` and `polylines`.
    fn picks(&self, polylines: bool, make: impl FnOnce() -> wgpu::RenderPipeline) -> Picks<'_> {
        Picks {
            full: &self.full,
            clear: &self.clear,
            basic: &self.basic,
            opaque: &self.opaque,
            polyline: polylines.then(|| self.polyline.get_or_init(make)),
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
    /// For frames drawing polylines.
    polyline: Option<&'p wgpu::RenderPipeline>,
}
impl<'p> Picks<'p> {
    fn get(&self, shading: Shading) -> &'p wgpu::RenderPipeline {
        match shading {
            Shading::Full
            | Shading::Fill(_)
            | Shading::Filter(_)
            | Shading::Blend
            | Shading::FilterBlend(_)
            | Shading::ImageEffect => self.full,
            Shading::Basic => self.basic,
            Shading::Opaque => self.opaque,
            Shading::Polyline => self.polyline.expect("made for a frame with polylines"),
        }
    }
}
/// A run of draws encoded as one instanced call.
struct Batch {
    rect: Rect,
    source: TextureSource,
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
/// `shading`). Shader fills and image effects keep their own pipelines.
fn merged_shading(a: Shading, b: Shading) -> Option<Shading> {
    let cost = |shading| match shading {
        Shading::Opaque => Some(0),
        Shading::Basic => Some(1),
        Shading::Full => Some(2),
        Shading::Fill(_)
        | Shading::Filter(_)
        | Shading::Blend
        | Shading::FilterBlend(_)
        | Shading::ImageEffect
        | Shading::Polyline => None,
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
    zgx::affine::Affine { a, b, c, d, tx, ty }.bounds(rect)
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
/// With crates.io's wgpu-hal, which lacks it (`zgx_wgpu_hal_patched`, see
/// build.rs), none: `request_adapter` ranks the adapters.
#[cfg(all(windows, not(zgx_wgpu_hal_patched)))]
fn preferred_dx12_adapter(_: &wgpu::Instance) -> Option<wgpu::Adapter> {
    None
}
#[cfg(all(windows, zgx_wgpu_hal_patched))]
fn preferred_dx12_adapter(instance: &wgpu::Instance) -> Option<wgpu::Adapter> {
    // SAFETY: the hal instance is only read (DXGI's adapters listed, one
    // exposed), never destroyed or changed.
    let hal = unsafe { instance.as_hal::<wgpu::hal::api::Dx12>() }?;
    let exposed = hal.expose_preferred_adapter(POWER)?;
    // SAFETY: the adapter was exposed from this instance's own hal instance.
    Some(unsafe { instance.create_adapter_from_hal(exposed) })
}
/// Keep the shaders FXC compiles in the user's local app data between
/// launches (`vendor/wgpu-hal/ZGX_PATCH.md`): a window's first frame
/// compiled its pipelines' shaders every launch, 45 ms of a 280 ms start.
#[cfg(all(windows, zgx_wgpu_hal_patched))]
fn keep_compiled_shaders() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(|local| std::path::PathBuf::from(local).join("zgx\\shaders"));
        wgpu::hal::dx12::set_shader_cache_dir(dir);
    });
}
#[cfg(not(all(windows, zgx_wgpu_hal_patched)))]
fn keep_compiled_shaders() {}
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
    quad_layout: wgpu::PipelineLayout,
    mapped_uploads: bool,
    vertex_ring: bool,
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
    /// Vertex and small uniform uploads are CPU-mapped instead of copied
    /// (memory the CPU and GPU share).
    mapped_uploads: bool,
    /// Per-frame vertices are CPU-written in a ring of buffers the GPU reads
    /// (`upload::MappedRing`): with unified memory, and on Direct3D 12.
    vertex_ring: bool,
    /// For dual-output (single-pass presentation) pipelines.
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
                .name("zgx GPU setup".into())
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
        keep_compiled_shaders();
        let mut found = Err(GpuError("no graphics backend".into()));
        for &backends in attempts {
            let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = backends;
            // Windows: swap chains present through a DirectComposition
            // visual, whose alpha the compositor blends (transparent and
            // glass windows), where a window's own swap chain is opaque.
            descriptor.backend_options.dx12.presentation_system =
                wgpu::Dx12SwapchainKind::DxgiFromVisual;
            descriptor.flags = instance_flags(descriptor.flags, !cfg!(debug_assertions));
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
            // An adapter that presents to the window in no format draws
            // nothing there (Mesa's Vulkan, to a window of an X visual of 16
            // or 30 bits, which zgx's own windows no longer take): the next
            // backend, as where there is no driver, rather than a panic.
            let adapter = adapter
                .map_err(|e| GpuError(e.to_string()))
                .and_then(|adapter| match &surface {
                    Some(surface) if surface.get_capabilities(&adapter).formats.is_empty() => Err(
                        GpuError(format!("{backends:?} presents to the window in no format")),
                    ),
                    _ => Ok(adapter),
                });
            found = adapter.map(|adapter| (instance, adapter));
            if found.is_ok() {
                break;
            }
        }
        let (instance, adapter) = found?;
        let info = adapter.get_info();
        // CPU-written vertex buffers avoid a GPU copy per frame; only worth it
        // where CPU and GPU share memory.
        let mappable = adapter
            .features()
            .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS);
        let mapped_uploads = info.device_type == wgpu::DeviceType::IntegratedGpu && mappable;
        // Direct3D 12 also takes vertices written in place, in upload-heap
        // buffers the GPU reads directly (as Direct3D 11's dynamic buffers
        // are): a frame then needs no staging copy, and no command list for
        // it, each of which costs NVIDIA's driver some 30 K cycles. Measured
        // on an RTX 4090: 0.7 to 3.3% fewer cycles a frame, the GPU's time
        // the same. Textures and uniforms keep their copies.
        let vertex_ring = mappable && (mapped_uploads || info.backend == wgpu::Backend::Dx12);
        zgx::profile::mark("GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("zgx shared device"),
            required_features: if vertex_ring {
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
                        eprintln!("zgx: the GPU device was lost ({reason:?})");
                    } else {
                        eprintln!("zgx: the GPU device was lost ({reason:?}): {message}");
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
                eprintln!("zgx: GPU error: {error}");
            }));
        }
        let (texture_layout, viewport_layout, quad_layout) = create_quad_pipelines(&device);
        zgx::profile::mark("GPU device and pipelines");
        let fonts = font_cache::system_fonts();
        zgx::profile::mark("system fonts");
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
            quad_layout,
            mapped_uploads,
            vertex_ring,
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
            quad_layout,
            mapped_uploads,
            vertex_ring,
        } = prepared;
        let quads = QuadPipelines {
            device: device.clone(),
            shader: Default::default(),
            layout: quad_layout.clone(),
            full: Default::default(),
            full_clear: Default::default(),
            basic: Default::default(),
            basic_clear: Default::default(),
            texture_layout: texture_layout.clone(),
            viewport_layout: viewport_layout.clone(),
            polylines: Default::default(),
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
                quad_layout,
                mapped_uploads,
                vertex_ring,
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
/// Draws a [`zgx::scene::Scene`] with wgpu (Metal, Direct3D 12 or Vulkan),
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
    ramps: FxHashMap<RampKey, Arc<zgx::image::ImageData>>,
    node_ramps: FxHashMap<NodeId, RampKey>,
    /// SVG sources that failed to draw, reported once each.
    failed_svgs: rustc_hash::FxHashSet<u64>,
    /// The viewport of the target currently being rendered.
    viewport_bind: Option<wgpu::BindGroup>,
    /// Reused upload memory for per-frame vertices; `write_buffer` would
    /// allocate and free a staging buffer every frame.
    belt: RefCell<wgpu::util::StagingBelt>,
    /// Replaces the belt for vertices on unified-memory adapters and on
    /// Direct3D 12.
    mapped: Option<RefCell<upload::MappedRing>>,
    /// The CPU and GPU share memory: textures and small uniforms are written
    /// in place too (`texture_uploads`), not copied.
    unified: bool,
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
    /// DXGI's account of shown presents (`ZGX_PRESENT_LOG`).
    #[cfg(windows)]
    dxgi_stats: dxgi::PresentStats,
    /// Windows: the alpha mode of a swap chain that lets what is behind
    /// show, for a window made opaque whose background at the first frame
    /// does (`match_background`): DXGI keeps a swap chain's alpha mode for
    /// its life.
    #[cfg(windows)]
    translucent_alpha: wgpu::CompositeAlphaMode,
    /// Whether the first frame's background was checked against the swap
    /// chain's alpha.
    #[cfg(windows)]
    background_matched: bool,
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
    /// Passes waking the GPU for input after idling (`warm_up`).
    warms: u64,
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
    entries: FxHashMap<AtlasKey, Option<AtlasEntry>>,
    cursor: (u32, u32, u32),
    fonts: Rc<RefCell<FontSystem>>,
    text_cache: Rc<RefCell<prepared::TextCache>>,
    swash: SwashCache,
    layers: FxHashMap<NodeId, LayerCache>,
    /// Counts rendered frames, to age parked layers (`LAYER_PARK_FRAMES`).
    layer_frame: u64,
    layer_origin: (f32, f32),
    shapes: FxHashMap<NodeId, Shaped>,
    /// For text the prepared cache doesn't lay out, a node shaped from each
    /// of its inputs (hashed: see `render_flat`), to copy rather than shape.
    shaped_like: FxHashMap<u64, NodeId>,
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
    /// The points of the frame's polyline strokes (polyline.wgsl), kept for
    /// their allocation.
    frame_points: Vec<[f32; 2]>,
    /// The window's last whole paint, while only leaves repaint
    /// (`render_kept`).
    kept: Option<Box<Kept>>,
    /// What keeping the window up to date did in a frame drawn as usual.
    patch_stats: GpuStats,
    /// Whether the last frame patched the window as kept (`render_kept`):
    /// one that can't is drawn whole, to keep patching after it.
    patched: bool,
    /// While no frame is kept, how many in a row a kept one would have
    /// patched (`would_patch`).
    patchable: Patchable,
    /// Kept frames in a row lost before they patched once (`render_once`):
    /// past `UNPATCHED_KEEPS`, a frame drawn whole keeps none.
    unpatched_keeps: u8,
    /// Where each item's quads and draws lay in a frame made to keep or
    /// patch (`render_flat`): first quad, quads, room, first draw, draws' end.
    item_ranges: Vec<(u32, u32, u32, u32, u32)>,
    /// A kept frame's clip regions, while it is patched (`PATCH`).
    patch_clips: Vec<Region>,
    /// A plain box's quads and draws as `patch_box` makes them, kept for
    /// their room.
    box_quads: Vec<Quad>,
    box_draws: Vec<Draw>,
    /// While a frame draws from the kept frame: what its quads' patches
    /// wrote, and their bytes, leading the frame's (`render_kept`).
    kept_upload: Option<(upload::KeptQuads, usize)>,
    /// And how many of its quads lie in paint order (`Kept::ordered`).
    kept_ordered: u32,
    /// Texts a kept frame patched (at most `SHAPED_LIKE`): the ones a kept
    /// frame gives spare room to grow into (`text_room`).
    changed_texts: rustc_hash::FxHashSet<NodeId>,
    /// Whether the frame's quads, draws, polyline points and clips
    /// (`patch_clips`) are made already, for the next frame to draw as
    /// they are.
    draw_made: bool,
    /// The frame's draws, kept for their allocation.
    frame_draws: Vec<Draw>,
    /// How many quads and draws frames have used lately (`keep_room`).
    frame_peaks: (usize, usize),
    /// The quads of the lines of a text partly in its clip, made each frame
    /// (see `render_flat`), kept for their allocation.
    visible_glyphs: Vec<TextQuad>,
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
    /// Presentation through surfaces of the renderer's own (layer_present.rs,
    /// dxgi_present.rs).
    #[cfg(own_surfaces)]
    layer_present: Option<LayerPresent>,
    /// Pipelines for frames encoded with Metal directly (`render_direct`);
    /// `None` inside if they could not be built.
    #[cfg(target_os = "macos")]
    metal_frame: std::cell::OnceCell<Option<metal_frame::MetalFrame>>,
    /// A directly encoded frame, committed, until it is known complete.
    #[cfg(target_os = "macos")]
    metal_inflight: RefCell<Option<metal_frame::CommandBuffer>>,
    /// Trimmed, and nothing drawn since: the next input wakes the GPU
    /// (`warm_up`).
    #[cfg(target_os = "macos")]
    idle: bool,
    /// How long the GPU took to run the last direct frame (`take_gpu_time`).
    #[cfg(target_os = "macos")]
    gpu_time: std::cell::Cell<Option<std::time::Duration>>,
    /// The viewport uniform `select_viewport` chose last, for direct frames.
    #[cfg(target_os = "macos")]
    viewport_values: [f32; 4],
    /// Shifted texel copies drawn in a render pass (copy_offset.wgsl).
    #[cfg(own_surfaces)]
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
    /// The image effects' pipeline (image_effects.wgsl), made when an image
    /// first draws with one.
    image_effect_pipeline: std::cell::OnceCell<wgpu::RenderPipeline>,
    fill_shaders: FxHashMap<u64, Arc<zgx::decoration::FillShader>>,
    /// Layer shaders' pipelines by shader id (`None`: it failed to compile).
    filter_pipelines: FxHashMap<u64, Option<wgpu::RenderPipeline>>,
    /// Blended layers' pipelines (blend.wgsl): `BLEND_PLAIN`'s, and each
    /// layer shader's by its id (`None`: it failed to compile).
    blend_pipelines: FxHashMap<u64, Option<wgpu::RenderPipeline>>,
    /// Their group 2 (the backdrop) and layout, made with the first.
    blend_layouts: std::cell::OnceCell<(wgpu::BindGroupLayout, wgpu::PipelineLayout)>,
    /// Copies of the target under blended layers, one per target size
    /// (the window's, each layer's), with their bind groups.
    backdrops: FxHashMap<(u32, u32), (wgpu::Texture, wgpu::BindGroup)>,
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
    /// A renderer for `window` (on `context`, else a context made for it), at
    /// its size and scale, told whether the window is `transparent` (shows
    /// what is behind it where nothing is drawn). On Windows an opaque
    /// window's swap chain is opaque, so the compositor copies its frames
    /// rather than blending each over what lies behind (7 to 14% of its GPU
    /// time a frame); `for_window` and `for_window_with_context` make swap
    /// chains with alpha.
    pub fn for_window_with_transparency(
        window: Arc<winit::window::Window>,
        context: Option<&GpuContext>,
        transparent: bool,
    ) -> Result<Self, GpuError> {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let owned;
        let context = match context {
            Some(context) => context,
            None => {
                owned = GpuContext::create(Some(window.clone()))?;
                &owned
            }
        };
        let surface = context
            .inner
            .instance
            .create_surface(window.clone())
            .map_err(|e| GpuError(e.to_string()))?;
        let mut renderer = Self::create_on(
            Some(window),
            Some(surface),
            size.width,
            size.height,
            context,
            transparent,
            true,
        )?;
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
        let mut renderer =
            Self::create_on(None, Some(surface), width, height, context, true, false)?;
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
        Self::create_on(window, surface, width, height, context, true, true)
    }
    /// `create` presenting to `surface` (none: offscreen), with alpha where
    /// the window may be `transparent` (else opaque, on Windows). `own`:
    /// `surface` is `window`'s alone, so on Windows it may present through a
    /// swap chain of its own instead (not into a caller's visual).
    fn create_on(
        window: Option<Arc<winit::window::Window>>,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
        context: &GpuContext,
        transparent: bool,
        #[cfg_attr(not(windows), allow(unused))] own: bool,
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
        // Windows: a swap chain of the renderer's own, whose buffers are the
        // retained frame (dxgi_present.rs); wgpu's surface then stays
        // unconfigured. Without one, wgpu's.
        #[cfg(windows)]
        let own_chain = window
            .as_ref()
            .filter(|_| own && *DXGI_PRESENT && info.backend == wgpu::Backend::Dx12)
            .and_then(|window| {
                use wgpu::rwh::HasWindowHandle;
                let wgpu::rwh::RawWindowHandle::Win32(handle) =
                    window.window_handle().ok()?.as_raw()
                else {
                    return None;
                };
                let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut _);
                dxgi_present::Chain::new(&device, hwnd, width, height, transparent)
                    .inspect_err(|e| {
                        eprintln!("zgx: no swap chain of its own ({e}): presenting through wgpu's")
                    })
                    .ok()
            });
        #[cfg(windows)]
        let own_surfaces = own_chain.is_some();
        #[cfg(not(windows))]
        let own_surfaces = false;
        let mut translucent_alpha = wgpu::CompositeAlphaMode::Auto;
        let config = surface.as_ref().map(|s| {
            let mut c = s
                .get_default_config(adapter, width, height)
                .expect("supported surface");
            let caps = s.get_capabilities(adapter);
            translucent_alpha = [
                wgpu::CompositeAlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::Inherit,
                wgpu::CompositeAlphaMode::Opaque,
            ]
            .into_iter()
            .find(|mode| caps.alpha_modes.contains(mode))
            .unwrap_or(c.alpha_mode);
            // Windows: the compositor blends a swap chain with alpha over
            // what lies behind every frame, opaque or not.
            c.alpha_mode = if cfg!(windows)
                && !transparent
                && caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque)
            {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                translucent_alpha
            };
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
            if !own_surfaces {
                s.configure(&device, &c);
            }
            c
        });
        #[cfg(not(windows))]
        let _ = translucent_alpha;
        let copy_present = !own_surfaces
            && config
                .as_ref()
                .is_some_and(|c| c.usage.contains(wgpu::TextureUsages::COPY_DST));
        // A window on macOS presents surfaces of its own (see `LayerPresent`),
        // and on Windows with its own swap chain.
        let layer_presenting = own_surfaces
            || (cfg!(target_os = "macos")
                && config.is_some()
                && info.backend == wgpu::Backend::Metal);
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
            #[cfg(windows)]
            translucent_alpha,
            #[cfg(windows)]
            background_matched: false,
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
                .vertex_ring
                .then(|| RefCell::new(upload::MappedRing::new(&device, &queue))),
            unified: context.inner.mapped_uploads,
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
            #[cfg(windows)]
            layer_present: own_chain.map(|chain| {
                let mut present = LayerPresent::default();
                present.chain = Some(chain);
                present
            }),
            #[cfg(target_os = "macos")]
            metal_frame: Default::default(),
            #[cfg(target_os = "macos")]
            metal_inflight: RefCell::new(None),
            #[cfg(target_os = "macos")]
            idle: false,
            #[cfg(target_os = "macos")]
            gpu_time: std::cell::Cell::new(None),
            #[cfg(target_os = "macos")]
            viewport_values: [0.; 4],
            #[cfg(own_surfaces)]
            copy_offset: None,
            in_layer: false,
            present_unpremultiplies,
            copy_present,
            single_pass_frames: 0,
            direct_frames: 0,
            metal_frames: 0,
            warms: 0,
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
            shaped_like: FxHashMap::default(),
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
            frame_points: Vec::new(),
            kept: None,
            patch_stats: GpuStats::default(),
            patched: false,
            patchable: Patchable::default(),
            unpatched_keeps: 0,
            item_ranges: Vec::new(),
            patch_clips: Vec::new(),
            box_quads: Vec::new(),
            kept_upload: None,
            kept_ordered: 0,
            changed_texts: Default::default(),
            box_draws: Vec::new(),
            draw_made: false,
            frame_draws: Vec::new(),
            frame_peaks: (0, 0),
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
            filter_pipelines: FxHashMap::default(),
            blend_pipelines: FxHashMap::default(),
            blend_layouts: std::cell::OnceCell::new(),
            backdrops: FxHashMap::default(),
            image_effect_pipeline: Default::default(),
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
            vertex_buffers: self.mapped.as_ref().map_or(1, |ring| ring.borrow().len()),
            layer_textures: self.layers.len(),
            layer_bytes: self.layers.values().map(|l| l.bytes).sum(),
        }
    }
    /// At the first frame, a swap chain made opaque under a background that
    /// lets what is behind show is made again with alpha. (One with alpha
    /// under an opaque background is right as it is, if dearer to compose.)
    #[cfg(windows)]
    fn match_background(&mut self) -> Result<(), GpuError> {
        if std::mem::replace(&mut self.background_matched, true) {
            return Ok(());
        }
        let opaque = self
            .config
            .as_ref()
            .is_some_and(|c| c.alpha_mode == wgpu::CompositeAlphaMode::Opaque);
        if !opaque || self.background.3 == 255 {
            return Ok(());
        }
        if let Some(config) = &mut self.config {
            config.alpha_mode = self.translucent_alpha;
        }
        // Its own swap chain is made again with alpha, before a buffer of it
        // becomes the target (`begin_layer_frame`).
        if let Some(present) = &mut self.layer_present
            && let Some(chain) = present.chain.take()
        {
            chain.release(present.buffers.drain(..).map(|(buffer, _)| buffer));
            present.shown = None;
            present.history.clear();
            // Only before the first frame, which draws it whole.
            present.chain = Some(chain.remade(true)?);
            return Ok(());
        }
        // The old swap chain, and the window's composition target with it,
        // go before the new one is made.
        self.surface = None;
        surface::SurfaceSource::recreate(self)?;
        surface::SurfaceSource::configure(self);
        Ok(())
    }
    /// Whether its window's swap chain presents opaque (the compositor
    /// ignores its alpha). None offscreen.
    pub fn presents_opaque(&self) -> Option<bool> {
        (self.config.as_ref()).map(|c| c.alpha_mode == wgpu::CompositeAlphaMode::Opaque)
    }
    /// Show `caret` over the window's frames, drawn and blinked by Core
    /// Animation (`zgx::caret_blink::NativeCaret`), or none. Call inside the
    /// transaction that presents the frame which stops (or starts) drawing
    /// the caret itself, so the two never show together; alone, it makes
    /// its own. Nothing where frames show through a layer of the
    /// renderer's own.
    #[cfg(target_os = "macos")]
    pub fn set_caret(&mut self, caret: Option<Caret>) {
        if let Some(present) = self.layer_present.as_mut() {
            present.caret = caret;
            if let Some(layer) = present.layer.as_mut() {
                layer.set_caret(caret);
            }
        }
    }
    /// Whether a caret set with `set_caret` can show: frames show through
    /// a layer the renderer presents to.
    #[cfg(target_os = "macos")]
    pub fn shows_carets(&self) -> bool {
        self.layer_present.is_some()
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
            self.recycle_atlas();
            self.scale = scale;
            self.layers.clear();
            self.shapes.clear();
            self.fresh = true;
        }
    }
    /// Whether the size changed.
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) == (self.width, self.height) {
            return false;
        }
        #[cfg(own_surfaces)]
        if let Some(present) = &mut self.layer_present {
            present.current = None;
            present.changed.clear();
            present.catch_up.clear();
            #[cfg(target_os = "macos")]
            {
                present.resized_at = Some(std::time::Instant::now());
            }
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
        // A surface of the renderer's own becomes the target
        // (`begin_layer_frame`), resized there on Windows.
        #[cfg(own_surfaces)]
        let layered = self.layer_present.is_some();
        #[cfg(not(own_surfaces))]
        let layered = false;
        let (target_width, target_height) = if layered { (1, 1) } else { (width, height) };
        self.target = texture(&self.device, target_width, target_height, "retained output");
        self.view = self.target.create_view(&Default::default());
        #[cfg(target_os = "macos")]
        if self.layer_present.is_none() {
            written(&self.device, &self.queue, &self.view);
        }
        // Windows presenting through its own swap chain resizes it now, as
        // the window changes size (a swap chain left at the old size shows
        // black beyond it until the next frame), and leaves wgpu's surface
        // unconfigured. The target no longer holds an old buffer (above).
        #[cfg(windows)]
        if let Some(present) = &mut self.layer_present
            && let Some(chain) = &mut present.chain
        {
            chain.release(present.buffers.drain(..).map(|(buffer, _)| buffer));
            present.shown = None;
            present.history.clear();
            let resized = chain
                .resize(width, height)
                .and_then(|()| chain.buffers(&self.device, width, height));
            match resized {
                Ok(buffers) => present.buffers = buffers.into_iter().map(|b| (b, None)).collect(),
                // The next frame tries again (`begin_layer_frame`).
                Err(error) => eprintln!("zgx: {error}"),
            }
        }
        let own_chain = cfg!(windows) && layered;
        if !own_chain && let (Some(s), Some(c)) = (&self.surface, &mut self.config) {
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
    /// changed: a repaint for comparison (`ZGX_DAMAGE_CHECK`) must leave
    /// later frames to bring their surfaces up to date as they otherwise would.
    pub fn render_uncounted(
        &mut self,
        scene: &Scene,
        damage: &[Rect],
    ) -> Result<GpuStats, GpuError> {
        #[cfg(own_surfaces)]
        let changed = self
            .layer_present
            .as_ref()
            .map(|present| present.changed.clone());
        let stats = self.render(scene, damage);
        #[cfg(own_surfaces)]
        if let (Some(present), Some(changed)) = (self.layer_present.as_mut(), changed) {
            present.changed = changed;
        }
        stats
    }
    /// Draw `scene`'s `damage` (logical rectangles) to the target, and report
    /// what it did.
    pub fn render(&mut self, scene: &Scene, damage: &[Rect]) -> Result<GpuStats, GpuError> {
        #[cfg(target_os = "macos")]
        {
            self.idle = false;
        }
        let presented = std::mem::take(&mut self.present_stats);
        let mut stats = {
            let _span = zgx::profile::span(zgx::profile::Phase::Paint);
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
        // Offscreen, drawing is the whole frame; a window's ends once it
        // shows (`present_with_notify`), its wait for the GPU and for its
        // point in a refresh counted in it, not in the next.
        if !self.presents() {
            zgx::profile::frame();
        }
        Ok(stats)
    }
    /// Whether frames are shown (a window's surface, or surfaces of its
    /// own), so `present_with_notify` ends them.
    fn presents(&self) -> bool {
        #[cfg(own_surfaces)]
        if self.layer_present.is_some() {
            return true;
        }
        self.surface.is_some()
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
        // Only leaves repainted since the window was drawn whole: their
        // instances patched in place, the rest drawn as they were.
        let unpatched = self.kept.as_ref().is_some_and(|kept| !kept.patched);
        if let Some(regions) = &merged
            && let Some(stats) = self.render_kept(scene, damage, regions, viewport)?
        {
            self.unpatched_keeps = 0;
            self.patchable.refused = false;
            if let Some(kept) = &mut self.kept {
                kept.patched = true;
            }
            self.patched = true;
            self.rendered_serial = Some(scene.flush_serial());
            return Ok(stats);
        }
        // A kept frame lost before it patched once (one kept each frame a
        // fling repaints whole, lost to the next): after a few, frames drawn
        // whole keep none until frames in a row would patch.
        // (Still kept: patched, drawn as usual for its little damage.)
        match &mut self.kept {
            None if unpatched => {
                self.unpatched_keeps = self.unpatched_keeps.saturating_add(1);
            }
            Some(kept) if merged.is_some() => {
                kept.patched = true;
                self.unpatched_keeps = 0;
            }
            _ => {}
        }
        // Kept up to date, drawn as usual this frame (little damage).
        let maintained = self.kept.is_some();
        let was_patched = std::mem::replace(&mut self.patched, maintained);
        // Lost (a change it couldn't patch, before it had patched): kept
        // again once a few frames in a row show it would patch.
        let patchable = match (&merged, maintained) {
            (Some(regions), false) => self.would_patch(scene, regions, viewport),
            _ => 0,
        };
        // A frame that would patch: the next drawn whole keeps again.
        if patchable > 0 {
            self.unpatched_keeps = 0;
        }
        // Drawn whole and kept to patch from: a frame whole anyway (the
        // first, a full repaint), one after a patched frame, so that
        // patching goes on after a change it can't patch, or one after
        // frames it would have patched.
        let keep = !maintained
            && self.plane.is_none()
            && !self.in_layer
            && !self.atlas_filling()
            && !scene.has_backdrop_blur()
            && !scene.has_overflow_fades()
            && (self.fresh
                || was_patched
                || patchable >= KEEP_AGAIN
                || self.unpatched_keeps < UNPATCHED_KEEPS
                    && damage.iter().any(|d| {
                        d.x <= 0.
                            && d.y <= 0.
                            && d.x + d.width >= viewport.width
                            && d.y + d.height >= viewport.height
                    }));
        if !maintained {
            self.kept = None;
        }
        let mut inherited = Vec::new();
        let (items, merged) = match &merged {
            _ if keep => (
                scene.layer_items_recorded(&mut inherited),
                merged.map(|_| vec![viewport]),
            ),
            Some(regions) => (
                scene.layer_items_within(start, Some(&self.plane_world(regions))),
                merged,
            ),
            None => (scene.layer_items(start), merged),
        };
        // Around native views, only this plane's paint (planes.rs).
        let items = self.plane_items(items);
        let mut layer_stats = GpuStats::default();
        self.layer_frame += 1;
        for item in &items {
            if item.isolated && item.effects.opacity > 0. {
                let shown = self.shown(item, damage, viewport);
                self.prepare_layer(scene, item.id, 1., true, shown, &mut layer_stats)?;
            }
        }
        // Damaged where it shows but not drawn (hidden, culled): what changed
        // in it then is known nowhere, so it repaints whole next.
        let frame = self.layer_frame;
        for cache in self.layers.values_mut() {
            if cache.used != frame
                && cache
                    .watched
                    .is_some_and(|watched| damage.iter().any(|d| d.intersects(watched)))
            {
                cache.watched = None;
            }
        }
        self.prepare_effects(scene, &items)?;
        // Kept only if every item draws flat (no layer, no effect) from
        // what the scene marks when it changes.
        let keep = keep
            && self.effect_jobs.is_empty()
            && items
                .iter()
                .all(|item| !item.isolated && !changes_unmarked(item.kind));
        let mut stats = match keep {
            true => self.render_keeping(scene, items, inherited, damage, merged, viewport)?,
            false => self.render_flat::<FRAME>(scene, items, damage, merged, false)?,
        };
        stats.add_made(&std::mem::take(&mut self.patch_stats));
        self.rendered_serial = Some(scene.flush_serial());
        stats.layer_repaints = layer_stats.layer_repaints;
        stats.layer_partial_repaints = layer_stats.layer_partial_repaints;
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
                        zgx::decoration::Background::Image { image, .. } => Some(image.id()),
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
    /// `exact`: that magnification is zooms' alone (`Scene::set_zoom`), and
    /// the layer rasterizes at it exactly, showing texel for pixel.
    /// `shown`: how a layer of the window's own shows there, when all of it
    /// does: changed, it repaints only where the window's damage reaches it.
    fn prepare_layer(
        &mut self,
        scene: &Scene,
        id: NodeId,
        magnification: f32,
        exact: bool,
        shown: Option<Shown<'_>>,
        stats: &mut GpuStats,
    ) -> Result<(), GpuError> {
        let revision = scene.layer_revision(id);
        let root = scene.bounds(id);
        let local = scene.local_transform(id);
        let zoom = scene.zoom(id);
        // Parked a while once it has none (`LAYER_PARK_FRAMES`).
        let transformed = !local.is_identity()
            || zoom != 1.
            || scene.filter_blur(id) > 0.
            || scene.mask(id).is_some();
        let magnification = magnification * local.magnification() * zoom;
        // Zoomed (and not otherwise scaled): at its scale exactly, crisp.
        let exact = exact && local.is_identity();
        let current = self.layers.get(&id).map(|cache| cache.step);
        let mut step = raster_step(magnification, current);
        let mut factor = match exact && magnification != 1. {
            true => magnification,
            false => step_scale(step),
        };
        let frame = self.layer_frame;
        if let Some(cache) = self.layers.get_mut(&id)
            && cache.revision == revision
            && cache.step == step
            && cache.factor == factor.to_bits()
        {
            cache.bounds.x += root.x - cache.root_origin.0;
            cache.bounds.y += root.y - cache.root_origin.1;
            cache.content.x += root.x - cache.root_origin.0;
            cache.content.y += root.y - cache.root_origin.1;
            cache.root_origin = (root.x, root.y);
            cache.transformed = transformed;
            cache.used = frame;
            cache.parked = false;
            cache.watched = shown.and_then(|shown| shown.window(cache.content));
            stats.layer_cache_hits += 1;
            return Ok(());
        }
        let content = scene.layer_bounds(id);
        let limit = self.device.limits().max_texture_dimension_2d;
        // Magnified past what the texture limits or the budget allow, a
        // layer renders at the finest scale that fits.
        let pixels = |factor: f32| {
            let scale = self.scale * factor;
            (content.width + 2. / scale) * scale * (content.height + 2. / scale) * scale
        };
        let fits = |factor: f32| {
            (content.width.max(content.height) + 2.) * self.scale * factor <= limit as f32
                && pixels(factor) * 4. <= 16. * 1024. * 1024.
        };
        // Too large at a zoom's scale: the finest step that fits.
        let exact = exact && fits(factor);
        if !exact {
            while step > 0 && !fits(step_scale(step)) {
                step -= 1;
            }
            factor = step_scale(step);
        }
        let device_scale = self.scale;
        let scale = device_scale * factor;
        // A transparent texel border lets linear sampling preserve fractional
        // translation coverage (and smooth a rotated layer's edges) instead
        // of clamping opaque edge pixels.
        let bounds = content.expand(1. / scale);
        // At a zoom's scale, its texels on the device's pixels: counted from
        // its box's corner, which layout puts on a pixel.
        let (x, y) = match exact && factor != 1. {
            true => (
                root.x + ((bounds.x - root.x) * scale).floor() / scale,
                root.y + ((bounds.y - root.y) * scale).floor() / scale,
            ),
            false => (bounds.x, bounds.y),
        };
        let (width, height) = match (x, y) == (bounds.x, bounds.y) {
            true => (bounds.width, bounds.height),
            false => (bounds.x + bounds.width - x, bounds.y + bounds.height - y),
        };
        let width = (width * scale).ceil().max(1.) as u32;
        let height = (height * scale).ceil().max(1.) as u32;
        let bounds = Rect::new(x, y, width as f32 / scale, height as f32 / scale);
        // Where all of it shows in the window, if it does.
        let window = shown.and_then(|shown| shown.window(content));
        // Its window's damage, back in its own space: where its pixels can
        // have changed. All of it shows, so a change anywhere in it is
        // damage there (the window repaints from it nowhere else). A
        // blurred, masked or shaded layer's texels reach past where they
        // are: it repaints whole.
        let plain = || {
            scene.filter_blur(id) <= 0.
                && scene.mask(id).is_none()
                && scene.layer_shader(id).is_none()
        };
        let partial = shown
            .filter(|_| window.is_some() && plain())
            .and_then(|shown| {
                let cache = self.layers.get(&id)?;
                let kept = !cache.parked
                    && cache.watched.is_some()
                    && cache.step == step
                    && cache.factor == factor.to_bits()
                    && cache.bounds == bounds
                    && cache.root_origin == (root.x, root.y);
                let inverse = shown.map.inverse()?;
                kept.then(|| {
                    shown
                        .damage
                        .iter()
                        .filter_map(|damage| {
                            inverse
                                .bounds(*damage)
                                .expand(1. / scale)
                                .intersection(bounds)
                        })
                        .map(|rect| {
                            Rect::new(
                                rect.x - bounds.x,
                                rect.y - bounds.y,
                                rect.width,
                                rect.height,
                            )
                        })
                        .collect::<Vec<Rect>>()
                })
            });
        let mut items = scene.layer_items(Some(id));
        self.plane_filter(&mut items);
        for item in &items {
            if item.isolated && item.effects.opacity > 0. {
                self.prepare_layer(scene, item.id, magnification, exact, None, stats)?;
            }
        }
        self.prepare_effects(scene, &items)?;
        if let Some(damage) = partial {
            let cache = self.layers.remove(&id).expect("kept above");
            let (texture, result) =
                self.paint_layer(scene, items, bounds, cache.texture, scale, &damage, false);
            self.layers.insert(
                id,
                LayerCache {
                    texture,
                    revision,
                    transformed,
                    used: frame,
                    watched: window,
                    ..cache
                },
            );
            let result = result?;
            add_layer_stats(stats, &result);
            stats.layer_partial_repaints += 1;
            return Ok(());
        }
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
        let whole = [Rect::new(0., 0., bounds.width, bounds.height)];
        let (texture, result) =
            self.paint_layer(scene, items, bounds, texture, scale, &whole, true);
        let result = result?;
        add_layer_stats(stats, &result);
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
                factor: factor.to_bits(),
                exact: exact && factor != 1.,
                transformed,
                used: frame,
                parked: false,
                watched: window,
                content,
            },
        );
        Ok(())
    }
    /// Draw `items` into a layer's `texture` at `scale`, its content's
    /// corner at `bounds`' (nested layers find theirs from it): where
    /// `damage` says (in its own points from that corner), all of it
    /// when `fresh` (its slack too, so an earlier larger frame never bleeds
    /// into sampling at the content's edge).
    #[allow(clippy::too_many_arguments)]
    fn paint_layer(
        &mut self,
        scene: &Scene,
        mut items: Vec<zgx::scene::PaintItem<'_>>,
        bounds: Rect,
        texture: wgpu::Texture,
        scale: f32,
        damage: &[Rect],
        fresh: bool,
    ) -> (wgpu::Texture, Result<GpuStats, GpuError>) {
        let view = texture.create_view(&Default::default());
        let target = std::mem::replace(&mut self.target, texture);
        let old_view = std::mem::replace(&mut self.view, view);
        let old_size = (self.width, self.height);
        self.width = self.target.width();
        self.height = self.target.height();
        let background = self.background;
        self.background = Color(0, 0, 0, 0);
        let was_fresh = std::mem::replace(&mut self.fresh, fresh);
        // Everything inside draws at the layer's raster scale: glyphs,
        // canvases and nested layers rasterize as finely as it shows.
        let device_scale = std::mem::replace(&mut self.scale, scale);
        zgx::scene::translate_items(&mut items, -bounds.x, -bounds.y);
        // Nested layer images remain world-positioned in their cache; pass the offset
        // separately so the same texture can be composed into any ancestor layer.
        let old_origin = std::mem::replace(&mut self.layer_origin, (bounds.x, bounds.y));
        let in_layer = std::mem::replace(&mut self.in_layer, true);
        let result = self.render_flat::<FRAME>(scene, items, damage, None, false);
        self.in_layer = in_layer;
        self.layer_origin = old_origin;
        let texture = std::mem::replace(&mut self.target, target);
        self.view = old_view;
        self.width = old_size.0;
        self.height = old_size.1;
        self.scale = device_scale;
        self.background = background;
        self.fresh = was_fresh;
        (texture, result)
    }
    /// How `item`, a layer in the window's own pass, shows there: its map
    /// (none: as laid out), what of the window it can show in, and the
    /// window's damage this frame. `None` in perspective.
    fn shown<'a>(
        &self,
        item: &zgx::scene::PaintItem<'_>,
        damage: &'a [Rect],
        viewport: Rect,
    ) -> Option<Shown<'a>> {
        let map = match item.projective() {
            Some(map) => map.affine()?,
            None => zgx::affine::Affine::IDENTITY,
        };
        let clip = item.clip.unwrap_or(viewport).intersection(viewport)?;
        Some(Shown { damage, map, clip })
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
                self.unified,
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
    /// The pipeline drawing `shading` if a shader fill's or the image
    /// effects', once compiled.
    fn fill_pipeline(&self, shading: Shading) -> Option<&wgpu::RenderPipeline> {
        match shading {
            Shading::Fill(id) => self.fill_pipelines.get(&id)?.as_ref(),
            Shading::Filter(id) => self.filter_pipelines.get(&id)?.as_ref(),
            Shading::Blend => self.blend_pipelines.get(&BLEND_PLAIN)?.as_ref(),
            Shading::FilterBlend(id) => self.blend_pipelines.get(&id)?.as_ref(),
            Shading::ImageEffect => self.image_effect_pipeline.get(),
            _ => None,
        }
    }
    /// Compile the image effects' pipeline if not yet.
    fn prepare_image_effects(&self) {
        self.image_effect_pipeline.get_or_init(|| {
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("image effects"),
                    source: wgpu::ShaderSource::Wgsl(image_effect_source().into()),
                });
            fill_pipeline(
                &self.device,
                &module,
                &self.context.inner.quad_layout,
                "zgx_image_effect",
            )
        });
    }
    /// Compile `shader`'s pipeline if new; whether it can draw.
    fn prepare_fill(&mut self, shader: &Arc<zgx::decoration::FillShader>) -> bool {
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
            fill_pipeline(
                &self.device,
                &module,
                &self.context.inner.quad_layout,
                "zgx_fill",
            )
        });
        self.fill_shaders.insert(shader.id(), shader.clone());
        self.fill_pipelines.insert(shader.id(), pipeline);
        self.fill_pipelines[&shader.id()].is_some()
    }
    /// Compile layer shader `shader`'s pipeline if new; whether it can draw.
    fn prepare_filter(&mut self, shader: &zgx::decoration::FillShader) -> bool {
        if let Some(compiled) = self.filter_pipelines.get(&shader.id()) {
            return compiled.is_some();
        }
        let source = filter_source(shader.source());
        let pipeline = fill_validates(&source).then(|| {
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("layer shader"),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
            fill_pipeline(
                &self.device,
                &module,
                &self.context.inner.quad_layout,
                "zgx_filter",
            )
        });
        self.filter_pipelines.insert(shader.id(), pipeline);
        self.filter_pipelines[&shader.id()].is_some()
    }
    /// The blended layers' group 2 (the backdrop) and pipeline layout.
    fn blend_layouts(&self) -> &(wgpu::BindGroupLayout, wgpu::PipelineLayout) {
        self.blend_layouts.get_or_init(|| {
            let backdrop = self
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("blend backdrop"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    }],
                });
            let layout = self
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("blended layers"),
                    bind_group_layouts: &[
                        Some(&self.context.inner.texture_layout),
                        Some(&self.context.inner.viewport_layout),
                        Some(&backdrop),
                    ],
                    immediate_size: 0,
                });
            (backdrop, layout)
        })
    }
    /// Compile the blended layers' pipeline, plain or through layer shader
    /// `shader`, if new; whether it can draw.
    fn prepare_blend(&mut self, shader: Option<&zgx::decoration::FillShader>) -> bool {
        let key = shader.map_or(BLEND_PLAIN, |shader| shader.id());
        if let Some(compiled) = self.blend_pipelines.get(&key) {
            return compiled.is_some();
        }
        let (source, entry) = match shader {
            Some(shader) => (filter_source(shader.source()), "zgx_filter_blended"),
            None => (blend_source(), "zgx_blended"),
        };
        let pipeline = fill_validates(&source).then(|| {
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("blended layer"),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
            let (_, layout) = self.blend_layouts();
            blend_pipeline(&self.device, &module, layout, entry)
        });
        self.blend_pipelines.insert(key, pipeline);
        self.blend_pipelines[&key].is_some()
    }
    /// The backdrop for the current target (`target`'s size): made once
    /// per size, its bind group with it.
    fn backdrop(&mut self) -> &(wgpu::Texture, wgpu::BindGroup) {
        let size = (self.target.width(), self.target.height());
        if !self.backdrops.contains_key(&size) {
            let texture = texture(&self.device, size.0, size.1, "blend backdrop");
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blend backdrop"),
                layout: &self.blend_layouts().0,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                }],
            });
            self.backdrops.insert(size, (texture, bind));
        }
        &self.backdrops[&size]
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
    /// Frames in a row, this one included, whose changes a kept window
    /// would have patched and drawn from: leaves alone repainting (no
    /// composite change, nothing mounted or removed), over `KEPT_SHARE` of
    /// the window, with no flush unread between them.
    fn would_patch(&mut self, scene: &Scene, regions: &[Rect], viewport: Rect) -> u8 {
        let last = &mut self.patchable;
        if last.skip > 0 {
            last.skip -= 1;
            last.run = 0;
            return 0;
        }
        let (covered, repaints) = scene.repaints();
        let area = |r: &Rect| r.width * r.height;
        // (Flushes unread since, while skipping: read again from the next.)
        let read = *covered.start() == last.flushes + 1;
        // (Repainted subtrees and nodes mounted or removed are spliced;
        // since a splice walked too much, only leaves alone, patched in
        // place.)
        let patches = read
            && !repaints.is_empty()
            && regions.iter().map(area).sum::<f32>() >= KEPT_SHARE * area(&viewport)
            && (!last.refused
                || last.revision == scene.content_revision()
                    && repaints.iter().all(|&(id, flags)| {
                        flags & zgx::scene::repaint::COMPOSITE == 0
                            && scene.contains(id)
                            && scene.children(id).is_empty()
                    }));
        (last.run, last.skip, last.backoff) = match (read, patches) {
            (_, true) => (last.run.saturating_add(1), 0, 0),
            (false, false) => (0, 0, last.backoff),
            (true, false) => (0, last.backoff, (last.backoff * 2).clamp(1, 32)),
        };
        (last.flushes, last.revision) = (*covered.end(), scene.content_revision());
        last.run
    }
    /// Whether the glyph atlas is filling up: the next frame drawn as usual
    /// recycles it.
    #[inline(always)]
    fn atlas_filling(&self) -> bool {
        self.cursor.1 + self.cursor.2 > self.atlas_size * 3 / 4 || self.entries.len() >= 8192
    }
    /// Draw `items`, the window's whole paint as the walk found them with
    /// what they `inherited`, and keep it to patch: their quads and draws
    /// made with where each item's lie (`render_flat::<PATCH>`), then drawn
    /// as made.
    fn render_keeping(
        &mut self,
        scene: &Scene,
        items: Vec<zgx::scene::PaintItem<'_>>,
        inherited: Vec<zgx::scene::Inherited>,
        damage: &[Rect],
        merged: Option<Vec<Rect>>,
        viewport: Rect,
    ) -> Result<GpuStats, GpuError> {
        let ids: Vec<NodeId> = items.iter().map(|item| item.id).collect();
        let boxed: Vec<Option<KeptBox>> = items
            .iter()
            .map(|item| match item.kind {
                // (Only boxes keep one: no other item is weighed.)
                NodeKind::Quad(_) | NodeKind::Panel { .. } => KeptBox::of(item, plain_box(item)),
                _ => None,
            })
            .collect();
        self.patch_clips.clear();
        let made =
            self.render_flat::<PATCH>(scene, items, &[viewport], Some(vec![viewport]), true)?;
        let ranges = std::mem::take(&mut self.item_ranges);
        // Not shader fills or polylines, whose uniforms and points change
        // without the scene marking their nodes.
        self.draw_made = true;
        if ranges.len() != ids.len()
            || inherited.len() != ids.len()
            || !self.frame_points.is_empty()
            || self.frame_draws.iter().any(|draw| {
                matches!(
                    draw.shading,
                    Shading::Fill(_)
                        | Shading::Filter(_)
                        | Shading::Blend
                        | Shading::FilterBlend(_)
                        | Shading::Polyline
                )
            })
        {
            self.item_ranges = ranges;
            let mut drawn = self.render_flat::<FRAME>(scene, Vec::new(), damage, merged, false)?;
            drawn.add_made(&made);
            return Ok(drawn);
        }
        let len = self.frame_quads.len();
        let draws = self.frame_draws.clone();
        let clips = self.patch_clips.clone();
        let mut drawn = self.render_flat::<FRAME>(scene, Vec::new(), damage, merged, false)?;
        drawn.add_made(&made);
        let mut quads = std::mem::take(&mut self.frame_quads);
        // (Any copies a layer frame catches up with come after.)
        quads.truncate(len);
        let items: Vec<KeptItem> = ids
            .into_iter()
            .zip(inherited)
            .zip(&ranges)
            .zip(boxed)
            .map(
                |(((id, inherited), &(start, len, room, first, end)), boxed)| KeptItem {
                    id,
                    inherited,
                    start,
                    len,
                    room,
                    draws: first..end,
                    boxed,
                    end: 0,
                },
            )
            .collect();
        let mut items = items;
        subtree_ends(scene, &mut items, 0);
        let by_node = KeptIndex::new(&items);
        self.item_ranges = ranges;
        self.kept = Some(Box::new(Kept {
            quads,
            draws,
            clips,
            items,
            by_node,
            flushes: *scene.repaints().0.end(),
            revision: scene.content_revision(),
            identity: scene.identity(),
            atlas_epoch: self.atlas_epoch,
            viewport,
            scale: self.scale,
            background: self.background,
            patched: false,
            upload: upload::KeptQuads::new(),
            ordered: len as u32,
        }));
        Ok(drawn)
    }
    /// Draw the window as it was kept, where the scene repainted only
    /// leaves since: each one's instances made again (`render_flat::<PATCH>`,
    /// as a frame makes them) into its place, every other item's as they
    /// were. Only the repainted items are visited (the scene names them),
    /// not the window's. `None` when it can't: then the frame is drawn as
    /// usual.
    // Out of line: once a frame, and its loops over the repainted items then
    // compile alone, whatever `render_once` around it holds.
    #[inline(never)]
    fn render_kept(
        &mut self,
        scene: &Scene,
        damage: &[Rect],
        regions: &[Rect],
        viewport: Rect,
    ) -> Result<Option<GpuStats>, GpuError> {
        let Some(mut kept) = self.kept.take() else {
            return Ok(None);
        };
        let (covered, repaints) = scene.repaints();
        // Kept while the scene and target are those it was drawn for, and
        // the atlas keeps its glyphs where they were.
        // (Nodes mounted or removed lie in subtrees spliced below.)
        if *covered.start() != kept.flushes + 1
            || kept.identity != scene.identity()
            || kept.atlas_epoch != self.atlas_epoch
            || kept.viewport != viewport
            || kept.scale != self.scale
            || kept.background != self.background
            || self.fresh
            || self.plane.is_some()
            || self.in_layer
            || !self.effect_jobs.is_empty()
            || scene.has_backdrop_blur()
            || scene.has_overflow_fades()
            || self.atlas_filling()
        {
            return Ok(self.recycle_kept(*kept));
        }
        // (What this frame's patches write counts as one version of it.)
        kept.upload.patch();
        // The repainted items, worked out alone from what they inherited,
        // with their places: only leaves, painting as many items as before
        // (a box painting over what it holds has a border pass after it).
        let mut items = Vec::new();
        let mut places = Vec::new();
        // Plain boxes, made into their places as they are found
        // (`patch_box`), as `render_flat` makes them: whether all fit.
        let (mut boxes, mut boxes_fit) = (0, true);
        let (mut box_quads, mut box_draws) = (
            std::mem::take(&mut self.box_quads),
            std::mem::take(&mut self.box_draws),
        );
        // Subtrees repainted (a container, a composite change, a changed
        // list of children, a node not worked out alone): each walked again
        // whole from what its root inherited and spliced over the items it
        // had (`splice`). Kept items of removed nodes and mounted nodes not
        // kept must lie in one.
        let mut splices: Vec<(usize, usize, NodeId)> = Vec::new();
        let (mut leaving, mut entering) = (Vec::new(), Vec::new());
        for &(id, flags) in repaints {
            let first = kept.by_node.get(id, &kept.items);
            if !scene.contains(id) {
                leaving.extend(first);
                continue;
            }
            let Some(first) = first else {
                entering.push(id);
                continue;
            };
            if flags & (zgx::scene::repaint::COMPOSITE | zgx::scene::repaint::CHILDREN) != 0
                || !scene.children(id).is_empty()
            {
                splices.push((first, kept.items[first].end as usize, id));
                continue;
            }
            // A kept box layout alone moved or resized (`BOX_ONLY`): the same
            // paint and compositing at its new box, without working its item
            // out.
            if flags & zgx::scene::repaint::BOX_ONLY != 0
                && let Some(boxed) = kept.items[first].boxed
                && kept.items.get(first + 1).is_none_or(|next| next.id != id)
            {
                let (bounds, kind) = scene.box_in(id, &kept.items[first].inherited);
                if plain_quad(kind) {
                    boxes += 1;
                    if self.patch_box_moved(&mut kept, first, bounds, boxed.clip, viewport) {
                        continue;
                    }
                    let parts = PlainBox {
                        kind,
                        bounds,
                        clip: boxed.clip,
                        opacity: boxed.opacity,
                        edge_fade: boxed.edge_fade,
                    };
                    boxes_fit = boxes_fit
                        && self.patch_box(
                            &mut kept,
                            first,
                            parts,
                            viewport,
                            &mut box_quads,
                            &mut box_draws,
                        );
                    continue;
                }
            }
            let over = matches!(scene.kind(id),
                NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } if quad.paints_over());
            let count = 1 + usize::from(over);
            if kept.items[first..]
                .iter()
                .take_while(|item| item.id == id)
                .count()
                != count
            {
                splices.push((first, kept.items[first].end as usize, id));
                continue;
            }
            for place in first..first + count {
                let Some(item) = scene.item_in(id, &kept.items[place].inherited) else {
                    splices.push((first, kept.items[first].end as usize, id));
                    break;
                };
                let plain = plain_box(&item);
                kept.items[place].boxed = KeptBox::of(&item, plain);
                if plain {
                    boxes += 1;
                    boxes_fit = boxes_fit
                        && self.patch_box(
                            &mut kept,
                            place,
                            PlainBox::of(&item),
                            viewport,
                            &mut box_quads,
                            &mut box_draws,
                        );
                } else {
                    items.push(item);
                    places.push(place);
                }
            }
        }
        // Outermost first, nested ones dropped.
        splices.sort_unstable_by_key(|&(first, _, _)| first);
        splices.dedup_by(|inner, outer| inner.0 < outer.1);
        let spliced = |place: usize| {
            let at = splices.partition_point(|&(first, _, _)| first <= place);
            at > 0 && place < splices[at - 1].1
        };
        // Drawn from the kept frame only with damage over `KEPT_SHARE` of
        // the window: kept up to date otherwise only where that is cheap
        // (leaves alone).
        let area = |r: &Rect| r.width * r.height;
        let from_kept = regions.iter().map(area).sum::<f32>() >= KEPT_SHARE * area(&viewport);
        // Walked again, a subtree costs about what drawing it as usual does,
        // and drawing from the kept frame about what drawing half the window
        // does: spliced only while the subtrees are a small part of the
        // window and the damage most of it.
        let walked: usize = splices.iter().map(|&(first, end, _)| end - first).sum();
        let splices_pay = walked as f32 <= SPLICE_SHARE * kept.items.len() as f32
            && regions.iter().map(area).sum::<f32>() >= SPLICE_DAMAGE * area(&viewport);
        // (Such frames count as frames that wouldn't patch: keeping again
        // backs off, `would_patch`.)
        if !splices.is_empty() && !splices_pay {
            self.patchable.refused = true;
        }
        if (!splices.is_empty() && (!from_kept || !splices_pay))
            || !leaving.iter().all(|&place| spliced(place))
            || !entering.iter().all(|&id| {
                splices
                    .iter()
                    .any(|&(_, _, root)| scene.is_within(id, root))
            })
        {
            (self.box_quads, self.box_draws) = (box_quads, box_draws);
            return Ok(self.recycle_kept(*kept));
        }
        if !splices.is_empty() {
            let mut keeps = places.iter().map(|&place| !spliced(place));
            items.retain(|_| keeps.next().unwrap());
            places.retain(|&place| !spliced(place));
        }
        // (Plain boxes alone, as a bound colour's: nothing for it to make.)
        let mut stats = GpuStats::default();
        if !items.is_empty() {
            self.patch_clips = std::mem::take(&mut kept.clips);
            stats =
                self.render_flat::<PATCH>(scene, items, &[viewport], Some(vec![viewport]), false)?;
            kept.clips = std::mem::take(&mut self.patch_clips);
        } else {
            // (As a patch of no items leaves them.)
            self.item_ranges.clear();
            self.frame_points.clear();
        }
        let ranges = std::mem::take(&mut self.item_ranges);
        // Into their places, unless one grew past its room, drew more draws
        // or polylines (whose points a kept frame doesn't hold).
        // A text outgrowing its room moves to the kept frame's end: its
        // draws keep their places among the others, so it paints where it
        // did; its quads alone lie elsewhere. (A text's quads, textured,
        // never hide a region whole: `occluded_from` skips none for it.)
        let moves = |item: &KeptItem, len: u32| {
            len > item.room && matches!(scene.kind(item.id), NodeKind::Text { .. })
        };
        let fits = self.frame_points.is_empty()
            && ranges.len() == places.len()
            && ranges
                .iter()
                .zip(&places)
                .all(|(&(_, len, _, first, end), &place)| {
                    let item = &kept.items[place];
                    (len <= item.room || moves(item, len))
                        && end - first == item.draws.end - item.draws.start
                });
        if fits {
            for (&(start, len, _, first, end), &place) in ranges.iter().zip(&places) {
                let item = &mut kept.items[place];
                if matches!(scene.kind(item.id), NodeKind::Text { .. }) {
                    if self.changed_texts.len() >= SHAPED_LIKE {
                        self.changed_texts.clear();
                    }
                    self.changed_texts.insert(item.id);
                }
                if len > item.room {
                    // Emptied where it was; at the end, with room again.
                    let old = item.start as usize..(item.start + item.len) as usize;
                    kept.upload.wrote(item.start, item.start + item.len);
                    kept.quads[old].fill(bytemuck::Zeroable::zeroed());
                    let room = len + text_room(len);
                    (item.start, item.len, item.room) = (kept.quads.len() as u32, 0, room);
                    kept.quads.resize(
                        kept.quads.len() + room as usize,
                        bytemuck::Zeroable::zeroed(),
                    );
                }
                kept.upload
                    .wrote(item.start, item.start + len.max(item.len));
                let (at, made) = (item.start as usize, len as usize);
                kept.quads[at..at + made]
                    .copy_from_slice(&self.frame_quads[start as usize..(start + len) as usize]);
                // What a longer value left past this one's, emptied.
                if made < item.len as usize {
                    kept.quads[at + made..at + item.len as usize]
                        .fill(bytemuck::Zeroable::zeroed());
                }
                item.len = len;
                for (to, draw) in
                    (item.draws.start..).zip(&self.frame_draws[first as usize..end as usize])
                {
                    kept.draws[to as usize] = Draw {
                        start: draw.start - start + item.start,
                        end: draw.end - start + item.start,
                        ..draw.clone()
                    };
                }
            }
            stats.patched_items = places.len();
        }
        self.item_ranges = ranges;
        (self.box_quads, self.box_draws) = (box_quads, box_draws);
        let fits = fits && boxes_fit;
        if fits {
            stats.patched_items += boxes;
        }
        if !fits {
            // Drawn as usual, reporting what was made.
            stats.patched_items = 0;
            self.patch_stats = stats;
            return Ok(self.recycle_kept(*kept));
        }
        // (A subtree's quads lie together only until a text moves: kept
        // again, laid out in order.)
        if !splices.is_empty() && kept.quads.len() as u32 != kept.ordered {
            stats.patched_items = 0;
            self.patch_stats = stats;
            return Ok(self.recycle_kept(*kept));
        }
        if !splices.is_empty() {
            match self.splice(scene, &mut kept, &splices, viewport)? {
                Some(made) => {
                    kept.ordered = kept.quads.len() as u32;
                    stats.add_made(&made);
                    stats.patched_items += made.patched_items;
                }
                None => {
                    stats.patched_items = 0;
                    self.patch_stats = stats;
                    return Ok(self.recycle_kept(*kept));
                }
            }
        }
        kept.revision = scene.content_revision();
        kept.flushes = *covered.end();
        // Drawn from it when the damage covers enough of the window that
        // making each damaged item again costs more than sending all of
        // them as they are; otherwise drawn as usual, and kept up to date.
        let area = |r: &Rect| r.width * r.height;
        if regions.iter().map(area).sum::<f32>() < KEPT_SHARE * area(&viewport) {
            self.kept = Some(kept);
            stats.patched_items = 0;
            self.patch_stats = stats;
            return Ok(None);
        }
        let len = kept.quads.len();
        std::mem::swap(&mut self.frame_quads, &mut kept.quads);
        // Its draws lent too, unless a frame may be deferred to its present
        // (`Deferred`), which takes them.
        let (lent, draws) = (self.single_pass_format.is_none(), kept.draws.len());
        match lent {
            true => std::mem::swap(&mut self.frame_draws, &mut kept.draws),
            false => self.frame_draws.clone_from(&kept.draws),
        }
        self.patch_clips.clone_from(&kept.clips);
        self.frame_points.clear();
        self.draw_made = true;
        // Its quads lead the frame's: a vertex buffer still holding them
        // takes only what was written since.
        kept.upload.drawn(len * size_of::<Quad>());
        self.kept_ordered = kept.ordered;
        self.kept_upload = Some((std::mem::take(&mut kept.upload), len * size_of::<Quad>()));
        let drawn =
            self.render_flat::<FRAME>(scene, Vec::new(), damage, Some(regions.to_vec()), false);
        kept.upload = self
            .kept_upload
            .take()
            .map(|(upload, _)| upload)
            .unwrap_or_default();
        std::mem::swap(&mut self.frame_quads, &mut kept.quads);
        kept.quads.truncate(len);
        if lent {
            std::mem::swap(&mut self.frame_draws, &mut kept.draws);
            kept.draws.truncate(draws);
        }
        let mut drawn = drawn?;
        drawn.patched_items = stats.patched_items;
        drawn.add_made(&stats);
        self.kept = Some(kept);
        Ok(Some(drawn))
    }
    /// A kept frame given up: its quads' room goes back to the frame's,
    /// which keeping had taken (the next frame would grow it again from
    /// nothing). `None`, for `render_kept` to return.
    /// Whether `text`'s node is one a kept frame patched (`changed_texts`).
    fn changes(&self, text: Option<NodeId>) -> bool {
        text.is_some_and(|id| self.changed_texts.contains(&id))
    }
    fn recycle_kept(&mut self, kept: Kept) -> Option<GpuStats> {
        let mut quads = kept.quads;
        if quads.capacity() > self.frame_quads.capacity() {
            quads.clear();
            self.frame_quads = quads;
        }
        None
    }
    /// Move kept plain box `place`, drawn as one quad, to `bounds` (layout
    /// alone moved or resized it, `BOX_ONLY`): its quad's colours, corners
    /// and border are what the same style and compositing made, so only
    /// its box and what follows from it are made again (its shading, its
    /// cull against `clip`, its draw), as `patch_box` makes them. False,
    /// with nothing changed, unless it is drawn as one quad before and
    /// after: then `patch_box` makes it.
    fn patch_box_moved(
        &self,
        kept: &mut Kept,
        place: usize,
        bounds: Rect,
        clip: Option<Rect>,
        viewport: Rect,
    ) -> bool {
        let kept_item = &kept.items[place];
        if kept_item.len != 1 || kept_item.draws.end - kept_item.draws.start != 1 {
            return false;
        }
        let at = kept_item.start as usize;
        let mut quad = kept.quads[at];
        // (Made whole and untransformed: its box is its rectangle and fade.
        // Not drawn without its region for where it was: `Region::covers`.)
        if quad.rect != quad.fade || quad.options[2] >= 0.5 || quad.mask[3] > 0. {
            return false;
        }
        let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
        (quad.rect, quad.fade) = (fade, fade);
        // (A flat colour's quad isn't split; a box's is, past a size.)
        if quad.shape[3] > 0.5 && !panel_whole(&quad, self.split_shading) {
            return false;
        }
        let Some(clip) = clip
            .unwrap_or(viewport)
            .intersection(viewport)
            .filter(|clip| viewport.intersects(*clip) && viewport.intersects(bounds))
        else {
            return false;
        };
        let rect = quad_rect(&quad);
        let limit = snap_out(clip, self.scale);
        if rect_outside(rect, limit) {
            return false;
        }
        // Only its geometry moved: the rest of the quad stays in place.
        let stored = &mut kept.quads[at];
        stored.rect = quad.rect;
        stored.fade = quad.fade;
        kept.upload.wrote(at as u32, at as u32 + 1);
        let draw = &mut kept.draws[kept_item.draws.start as usize];
        draw.shading = match self.split_shading {
            true => shading(stored, self.scale),
            false => Shading::Full,
        };
        draw.clip = match rect_inside(rect, limit) {
            true => viewport,
            false => clip,
        };
        draw.bounds = rect;
        true
    }
    /// Make plain box `item`'s quads and draws (`plain_box`), as
    /// `render_flat::<PATCH>` makes them, into its place in `kept`: false
    /// if they don't fit there (more quads than its room, other draws).
    fn patch_box(
        &self,
        kept: &mut Kept,
        place: usize,
        boxed: PlainBox<'_>,
        viewport: Rect,
        quads: &mut Vec<Quad>,
        draws: &mut Vec<Draw>,
    ) -> bool {
        quads.clear();
        draws.clear();
        let PlainBox {
            kind,
            bounds,
            clip,
            opacity,
            edge_fade,
        } = boxed;
        let clip = clip.unwrap_or(viewport).intersection(viewport);
        if let Some(clip) = clip.filter(|_| opacity > 0.) {
            let painted = match kind {
                NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } => {
                    style.paint_bounds(bounds)
                }
                _ => bounds,
            };
            if viewport.intersects(clip) && viewport.intersects(painted) {
                let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
                match kind {
                    NodeKind::Rect(color) => quads.push(rect_quad(
                        *color,
                        fade,
                        zgx::scene::Effects {
                            opacity,
                            edge_fade,
                            ..Default::default()
                        },
                    )),
                    NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } => push_panel(
                        quads,
                        plain_panel_quad(style, fade, opacity, edge_fade, false),
                        self.scale,
                        self.split_shading,
                    ),
                    _ => unreachable!("a plain box"),
                }
                // One quad, not culled whole (the usual box): its draw as
                // `cull_on_cpu` and `push_draw` make it, written into place.
                if let [quad] = quads[..]
                    && quad.options[2] < 0.5
                    && let rect = quad_rect(&quad)
                    && let limit = snap_out(clip, self.scale)
                    && !rect_outside(rect, limit)
                {
                    let kept_item = &mut kept.items[place];
                    if kept_item.room == 0 || kept_item.draws.end - kept_item.draws.start != 1 {
                        return false;
                    }
                    kept.upload
                        .wrote(kept_item.start, kept_item.start + kept_item.len.max(1));
                    let at = kept_item.start as usize;
                    kept.quads[at] = quad;
                    // What a longer value left past this one's, emptied.
                    if kept_item.len > 1 {
                        kept.quads[at + 1..at + kept_item.len as usize]
                            .fill(bytemuck::Zeroable::zeroed());
                    }
                    kept_item.len = 1;
                    kept.draws[kept_item.draws.start as usize] = Draw {
                        shading: match self.split_shading {
                            true => shading(&quad, self.scale),
                            false => Shading::Full,
                        },
                        source: TextureSource::Atlas,
                        blur: None,
                        start: at as u32,
                        end: at as u32 + 1,
                        clip: match rect_inside(rect, limit) {
                            true => viewport,
                            false => clip,
                        },
                        bounds: rect,
                    };
                    return true;
                }
                let (draw_clip, culled) = cull_on_cpu(quads, 0, clip, viewport, self.scale);
                if !quads.is_empty() {
                    let draw = Draw {
                        shading: Shading::Full,
                        source: TextureSource::Atlas,
                        blur: None,
                        start: 0,
                        end: quads.len() as u32,
                        clip: draw_clip,
                        bounds: culled.unwrap_or_else(|| quad_bounds(quads)),
                    };
                    push_draw(
                        draws,
                        quads,
                        draw,
                        self.split_shading,
                        false,
                        self.scale,
                        None,
                    );
                }
            }
        }
        let kept_item = &mut kept.items[place];
        let len = quads.len() as u32;
        if len > kept_item.room || draws.len() as u32 != kept_item.draws.end - kept_item.draws.start
        {
            return false;
        }
        kept.upload
            .wrote(kept_item.start, kept_item.start + len.max(kept_item.len));
        let at = kept_item.start as usize;
        kept.quads[at..at + quads.len()].copy_from_slice(quads);
        // What a longer value left past this one's, emptied.
        if len < kept_item.len {
            kept.quads[at + quads.len()..at + kept_item.len as usize]
                .fill(bytemuck::Zeroable::zeroed());
        }
        kept_item.len = len;
        for (to, draw) in (kept_item.draws.start..).zip(draws.drain(..)) {
            kept.draws[to as usize] = Draw {
                start: draw.start + kept_item.start,
                end: draw.end + kept_item.start,
                ..draw
            };
        }
        true
    }
    /// Walk each of `splices`' subtrees again (`first` and `end` of its
    /// items in `kept`, and its root) from what its root inherited, make
    /// their items (`render_flat::<PATCH>`, recording as a kept frame does)
    /// and put them over the old ones, moving what follows. `None` when
    /// they can't be kept (a layer, an effect, a shader fill or polyline):
    /// then the frame is drawn as usual.
    fn splice(
        &mut self,
        scene: &Scene,
        kept: &mut Kept,
        splices: &[(usize, usize, NodeId)],
        viewport: Rect,
    ) -> Result<Option<GpuStats>, GpuError> {
        let mut walked = Vec::new();
        let mut inherited = Vec::new();
        let mut counts = Vec::with_capacity(splices.len());
        let mut out = Vec::new();
        for &(first, _, root) in splices {
            let items = scene.layer_items_from(root, &kept.items[first].inherited, &mut out);
            if out.len() != items.len()
                || items
                    .iter()
                    .any(|item| item.isolated || changes_unmarked(item.kind))
            {
                return Ok(None);
            }
            counts.push(items.len());
            walked.extend(items);
            inherited.append(&mut out);
        }
        let ids: Vec<NodeId> = walked.iter().map(|item| item.id).collect();
        self.patch_clips = std::mem::take(&mut kept.clips);
        let made =
            self.render_flat::<PATCH>(scene, walked, &[viewport], Some(vec![viewport]), true)?;
        kept.clips = std::mem::take(&mut self.patch_clips);
        let ranges = std::mem::take(&mut self.item_ranges);
        let keeps = ranges.len() == ids.len()
            && self.frame_points.is_empty()
            && !self.frame_draws.iter().any(|draw| {
                matches!(
                    draw.shading,
                    Shading::Fill(_)
                        | Shading::Filter(_)
                        | Shading::Blend
                        | Shading::FilterBlend(_)
                        | Shading::Polyline
                )
            });
        if !keeps {
            self.item_ranges = ranges;
            return Ok(None);
        }
        // Each splice's items, numbered where they were made.
        let mut ids = ids.into_iter();
        let mut inherited = inherited.into_iter();
        let mut made_items: Vec<Vec<KeptItem>> = Vec::with_capacity(splices.len());
        let mut at = 0;
        for &count in &counts {
            let new: Vec<KeptItem> = ranges[at..at + count]
                .iter()
                .map(|&(start, len, room, first, end)| KeptItem {
                    id: ids.next().expect("an item"),
                    inherited: inherited.next().expect("an item"),
                    start,
                    len,
                    room,
                    draws: first..end,
                    boxed: None,
                    end: 0,
                })
                .collect();
            made_items.push(new);
            at += count;
        }
        // The nodes whose items were spliced over, and whether any splice
        // changed how many items there are: if none did, only the spliced
        // items are found again by node (`by_node`).
        let mut replaced = Vec::new();
        let mut renumbered = false;
        let mut shifted_from = u32::MAX;
        // Last first: what a splice moves lies after it, spliced already.
        for (&(first, end, _), mut new) in splices.iter().zip(made_items).rev() {
            let old = &kept.items[first..end];
            if matches!(kept.by_node, KeptIndex::Sparse(_)) {
                replaced.extend(old.iter().map(|item| item.id));
            }
            let quads_from = old[0].start;
            let quads_to = old.last().map_or(quads_from, |item| item.start + item.room);
            let draws_from = old[0].draws.start;
            let draws_to = old.last().map_or(draws_from, |item| item.draws.end);
            // The new items' quads and draws, where they were made.
            let (made_from, made_to) = match (new.first(), new.last()) {
                (Some(a), Some(b)) => (a.start, b.start + b.room),
                _ => (0, 0),
            };
            let (made_draws_from, made_draws_to) = match (new.first(), new.last()) {
                (Some(a), Some(b)) => (a.draws.start, b.draws.end),
                _ => (0, 0),
            };
            kept.quads.splice(
                quads_from as usize..quads_to as usize,
                self.frame_quads[made_from as usize..made_to as usize]
                    .iter()
                    .copied(),
            );
            let moved = |at: u32| at - made_from + quads_from;
            kept.draws.splice(
                draws_from as usize..draws_to as usize,
                self.frame_draws[made_draws_from as usize..made_draws_to as usize]
                    .iter()
                    .map(|draw| Draw {
                        start: moved(draw.start),
                        end: moved(draw.end),
                        ..draw.clone()
                    }),
            );
            let quad_shift = (made_to - made_from) as i64 - (quads_to - quads_from) as i64;
            // Written over: its own quads, or from them on when what
            // follows moves.
            match quad_shift {
                0 => kept.upload.wrote(quads_from, quads_to),
                _ => shifted_from = shifted_from.min(quads_from),
            }
            let draw_shift =
                (made_draws_to - made_draws_from) as i64 - (draws_to - draws_from) as i64;
            let item_shift = new.len() as i64 - (end - first) as i64;
            let shift = |value: &mut u32, by: i64| *value = (*value as i64 + by) as u32;
            for item in &mut new {
                item.start = moved(item.start);
                item.draws = item.draws.start - made_draws_from + draws_from
                    ..item.draws.end - made_draws_from + draws_from;
            }
            subtree_ends(scene, &mut new, first as u32);
            renumbered |= item_shift != 0;
            // What follows moves; the subtrees holding this one end later.
            // (Nothing, when it is as long as what it replaces.)
            if (quad_shift, draw_shift, item_shift) != (0, 0, 0) {
                for item in &mut kept.items[end..] {
                    shift(&mut item.start, quad_shift);
                    shift(&mut item.draws.start, draw_shift);
                    shift(&mut item.draws.end, draw_shift);
                    shift(&mut item.end, item_shift);
                }
                for item in &mut kept.items[..first] {
                    if item.end as usize >= end {
                        shift(&mut item.end, item_shift);
                    }
                }
                let moved_from = draws_from as usize + (made_draws_to - made_draws_from) as usize;
                for draw in &mut kept.draws[moved_from..] {
                    shift(&mut draw.start, quad_shift);
                    shift(&mut draw.end, quad_shift);
                }
            }
            kept.items.splice(first..end, new);
        }
        if shifted_from != u32::MAX {
            kept.upload.wrote(shifted_from, kept.quads.len() as u32);
        }
        // (Each node's first item: a box's border pass comes after it.)
        kept.by_node
            .patch(&kept.items, renumbered, replaced, splices);
        self.item_ranges = ranges;
        let mut made = made;
        made.patched_items = counts.iter().sum();
        Ok(Some(made))
    }
    /// `merged`: `damage` merged into regions already (`merge_damage`).
    fn render_flat<const MODE: u8>(
        &mut self,
        scene: &Scene,
        items: Vec<zgx::scene::PaintItem<'_>>,
        damage: &[Rect],
        merged: Option<Vec<Rect>>,
        record: bool,
    ) -> Result<GpuStats, GpuError> {
        let mut stats = GpuStats::default();
        // Made already (a kept frame, patched, or one made to keep): drawn
        // as it is (it holds its background).
        let made = MODE != PATCH && std::mem::take(&mut self.draw_made);
        let copies = if self.in_layer || self.fresh || MODE == PATCH {
            Vec::new()
        } else {
            std::mem::take(&mut self.copies)
        };
        // Effect slots `prepare_effects` planned for these items: drawn
        // before anything composites them.
        let effect_jobs = match MODE {
            PATCH => Vec::new(),
            _ => std::mem::take(&mut self.effect_jobs),
        };
        if damage.is_empty() && !self.fresh && copies.is_empty() {
            self.forget_effects(&effect_jobs);
            return Ok(stats);
        }
        // Earlier commands read the vertex and uniform buffers rewritten below.
        if MODE != PATCH {
            self.flush_pending();
        }
        let viewport = Rect::new(
            0.,
            0.,
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        );
        if MODE != PATCH {
            self.select_viewport(viewport.width, viewport.height);
        }
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
        } else if MODE != PATCH && has_blur {
            // (Patching draws nothing: its damage is the window's.)
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
        if MODE != PATCH {
            self.frame = self.frame.wrapping_add(1);
        }
        // (A kept frame's glyphs stay where they are: patching it gave way
        // before the atlas filled, `render_kept`.)
        if MODE != PATCH && !made && self.atlas_filling() {
            // Filling again soon after the last recycle means the visible
            // working set does not fit: grow rather than re-rasterize every
            // glyph and icon each time it fills.
            let thrashing = self.frame.wrapping_sub(self.atlas_recycled_at) < ATLAS_THRASH_FRAMES;
            if thrashing && self.atlas_size < MAX_ATLAS && self.entries.len() < 8192 {
                self.reset_atlas((self.atlas_size * 2).min(MAX_ATLAS));
            } else {
                self.recycle_atlas();
            }
            self.atlas_recycled_at = self.frame;
        }
        let mut quads = std::mem::take(&mut self.frame_quads);
        if !made {
            quads.clear();
        }
        if !made && (MODE != PATCH || record) {
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
        }
        let mut points = std::mem::take(&mut self.frame_points);
        if !made {
            points.clear();
        }
        let mut draws = std::mem::take(&mut self.frame_draws);
        // The clip regions quads lie in (`Quad::mask`): a kept frame's, to
        // patch it, or those of a frame made already.
        let mut clips: Vec<Region> = match MODE {
            PATCH => std::mem::take(&mut self.patch_clips),
            _ if made => std::mem::take(&mut self.patch_clips),
            _ => Vec::new(),
        };
        if !made {
            draws.clear();
        }
        // To keep or patch: where each item's quads and draws lie, noted as
        // the next begins (an item may end anywhere in the loop's body).
        let mut ranges = match MODE {
            PATCH => std::mem::take(&mut self.item_ranges),
            _ => Vec::new(),
        };
        ranges.clear();
        // The open range's text, if one: its spare room, if it changes.
        let mut text = None;
        let mut items = items.into_iter();
        let items = LeftItems(&mut items);
        for item in items.0.by_ref() {
            if MODE == PATCH {
                close_range(
                    &mut quads,
                    &draws,
                    &mut ranges,
                    record && self.changes(text),
                );
                ranges.push((quads.len() as u32, 0, 0, draws.len() as u32, u32::MAX));
                text = matches!(item.kind, NodeKind::Text { .. }).then_some(item.id);
            }
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
            let matrix = item.projective().map(|matrix| {
                use zgx::affine::{Affine, Projective};
                let (x, y) = self.layer_origin;
                Projective::from(Affine::translation(x, y))
                    .then(matrix)
                    .then(Projective::from(Affine::translation(-x, -y)))
            });
            // A layer composited through its layer shader, once that compiles.
            let filter = item
                .isolated
                .then(|| scene.layer_shader(item.id))
                .flatten()
                .and_then(|layer| Some((layer.shader.clone()?, layer.uniforms)))
                .filter(|(shader, _)| self.prepare_filter(shader));
            // And blended with its backdrop, once that compiles.
            let blend = item
                .isolated
                .then(|| scene.blend(item.id))
                .filter(|mode| *mode != zgx::scene::BlendMode::Normal)
                .filter(|_| self.prepare_blend(filter.as_ref().map(|(shader, _)| &**shader)));
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
                // as an affine image's; edge fades stay in its own frame. In
                // perspective (`options.z` 2), divided by `w` there too.
                let mut m = matrix.unwrap_or_default();
                let projective = matrix.is_some_and(|m| m.affine().is_none());
                // Zoomed: its corner on a pixel of this target, so its texels
                // land on pixels (`prepare_layer`).
                if layer.exact && !projective {
                    let (x, y) = m.point(item.bounds.x, item.bounds.y);
                    m.tx += (x * self.scale).round() / self.scale - x;
                    m.ty += (y * self.scale).round() / self.scale - y;
                }
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
                        match (matrix.is_some(), projective) {
                            (_, true) => 2.,
                            (true, false) => 1.,
                            (false, false) => 0.,
                        },
                        0.,
                    ],
                    shape: match matrix {
                        Some(_) => [m.a, m.c, m.b, m.d],
                        None => [0.; 4],
                    },
                    border: match (matrix, projective) {
                        (Some(_), true) => [m.tx, m.ty, m.px, m.py],
                        (Some(_), false) => [m.tx, m.ty, 0., 0.],
                        (None, _) => [0.; 4],
                    },
                    mask: [
                        region_index(&mut clips, Region::of(&item)),
                        filter.as_ref().map_or(0., |(_, uniforms)| {
                            region_index(&mut clips, Some(Region::uniforms(*uniforms)))
                        }),
                        blend.map_or(0., |mode| mode as u8 as f32),
                        0.,
                    ],
                });
                draws.push(Draw {
                    shading: match (&filter, blend) {
                        (Some((shader, _)), Some(_)) => Shading::FilterBlend(shader.id()),
                        (None, Some(_)) => Shading::Blend,
                        (Some((shader, _)), None) => Shading::Filter(shader.id()),
                        (None, None) => Shading::Full,
                    },
                    start,
                    end: quads.len() as u32,
                    clip,
                    bounds: quad_bounds(&quads[start as usize..]),
                    // Effect layers sample the effect atlas, and batch.
                    source: match effect {
                        Some(_) => TextureSource::Image(effects::EFFECT_ATLAS),
                        None => TextureSource::Layer(item.id),
                    },
                    blur: (item.effects.blur_radius > 0.).then(|| {
                        Box::new((
                            item.bounds,
                            item.effects,
                            blur_corners(item.kind, item.bounds),
                        ))
                    }),
                });
                continue;
            }
            let bounds = item.bounds;
            let fade = [bounds.x, bounds.y, bounds.width, bounds.height];
            let mut canvas_image = None;
            // A canvas drawn as quads clips them to its box, as its raster is.
            let mut canvas_clip = None;
            // And its polyline strokes, if any, drawn apart (`push_draw`).
            let mut polylines = false;
            // And a text whose quads are its glyphs alone (no selection, no
            // decorations): drawn with one shading (`push_draw`).
            let mut glyphs_alone = false;
            match item.kind {
                NodeKind::Canvas(canvas) => {
                    if bounds.width <= 0. || bounds.height <= 0. {
                        continue;
                    }
                    // A grid under its drawing (a board's dots), clipped to
                    // its box: marks placed by the grid's view, none drawn.
                    if scene.has_grid_fills()
                        && let Some(grid) = scene.grid_fill(item.id)
                    {
                        grid_quads(grid, bounds, item.effects.opacity, &mut quads);
                        canvas_clip = Some(bounds);
                    }
                    let view = scene.canvas_view(item.id);
                    // Its own opacity and edge fade apply to it as a whole:
                    // drawn in quads, each would take them alone.
                    if item.effects.opacity >= 1.
                        && item.effects.edge_fade <= 0.
                        && let Some(drawn) =
                            canvas_quads(canvas, (bounds.x, bounds.y), view, &mut points)
                    {
                        if let Some(old) = self.node_images.remove(&item.id) {
                            self.images.remove(&old);
                        }
                        self.canvases.forget(item.id);
                        for quad in drawn {
                            match quad {
                                CanvasQuad::Fill(quad) => {
                                    push_panel(&mut quads, quad, self.scale, self.split_shading)
                                }
                                CanvasQuad::Ring(quad) => quads.push(quad),
                                CanvasQuad::Polyline(quad) => {
                                    polylines = true;
                                    quads.push(quad)
                                }
                            }
                        }
                        canvas_clip = Some(bounds);
                        stats.canvas_quads += quads.len() - start as usize;
                    } else {
                        let before = self.canvases.rasterizations();
                        let (width, height, scale) = (bounds.width, bounds.height, self.scale);
                        let id = self.canvas_texture(item.id, &mut stats, |cache| {
                            cache.get_viewed(item.id, canvas, view, width, height, scale)
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
                }
                NodeKind::Rect(color) => quads.push(rect_quad(*color, fade, item.effects)),
                NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } => {
                    if style.shadows().len() > 32 {
                        return Err(GpuError("panel exceeds 32 shadows".into()));
                    }
                    // A solid border (any widths per side, any radii) is a
                    // ring the quad shader draws; dashed ones rasterize with
                    // the fill.
                    let ring = style.has_border()
                        && style.decoration.as_ref().is_none_or(|detail| {
                            detail.border_style == zgx::decoration::BorderStyle::Solid
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
                                    zgx::decoration::Background::Image { .. }
                                    | zgx::decoration::Background::Shader(_),
                                ) => style.fill,
                                Some(zgx::decoration::Background::Brush(
                                    zgx::canvas::Brush::Solid(color),
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
                            if let Some(zgx::decoration::Background::Image { image, fit }) = style
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
                            if let Some(zgx::decoration::Background::Shader(shader)) = style
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
                            let ramp = match get_in::<MODE, _, _>(&mut self.ramps, &key) {
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
                            insert_in::<MODE, _, _>(&mut self.node_ramps, item.id, key);
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
                                true => std::borrow::Cow::Owned(zgx::scene::QuadStyle {
                                    border_width: 0.,
                                    decoration: style.decoration.as_deref().map(|detail| {
                                        Arc::new(zgx::decoration::Decoration {
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
                            push_panel(
                                &mut quads,
                                plain_panel_quad(style, fade, opacity, edge_fade, later),
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
                                    eprintln!("zgx: an SVG doesn't draw: {}", error.0);
                                }
                                continue;
                            }
                        };
                    stats.svg_rasterizations += (self.svgs.rasterizations() - before) as usize;
                    // Nodes share SVG rasters: `prune` releases their textures
                    // once no node shows them (a new source prunes).
                    let transform = image.paint_transform(bounds);
                    let transformed = image.transform() != zgx::affine::Affine::IDENTITY;
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
                    // (Never kept, so never patched: `changes_unmarked`.)
                    if MODE != PATCH && !self.images.contains_key(&frame.id()) {
                        let bytes = frame.width() as usize
                            * frame.height() as usize
                            * if frame.format() == zgx::native_surface::SurfaceFormat::Bgra {
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
                            frame.format() == zgx::native_surface::SurfaceFormat::Nv12FullRange,
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
                    let transformed = image.transform() != zgx::affine::Affine::IDENTITY;
                    // An effect: its own quad, drawn with its own pipeline.
                    if !matches!(image.effect(), zgx::image::ImageEffect::Plain) {
                        let reveal = scene.shader_uniforms(item.id).params;
                        let (opacity, edge) = (item.effects.opacity, item.effects.edge_fade);
                        quads.push(image_effect_quad(
                            image, transform, opacity, edge, fade, reveal,
                        ));
                        self.prepare_image_effects();
                        fill_shading = Some(Shading::ImageEffect);
                    } else {
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
                            zgx::scene::Color(255, 255, 255, 255),
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
                    // A shape made this frame, held here until placed: into
                    // the map once, not in and out again to place it.
                    let mut fresh = None;
                    if stale {
                        // The old shape's glyphs and quads hold the new one's:
                        // a changed value is as many allocations fewer.
                        let (mut spare_glyphs, mut spare_quads) = (Vec::new(), Vec::new());
                        if let Some(old) = self.shapes.remove(&item.id) {
                            self.shaped_bytes -= old.bytes();
                            (spare_glyphs, spare_quads) = (old.glyphs, old.quads);
                            spare_glyphs.clear();
                            spare_quads.clear();
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
                        // Text the prepared cache doesn't lay out (with clamping
                        // or an ellipsis): another node's shape of the same
                        // inputs, copied rather than shaped again (a list moved
                        // by a row shows in each what the next showed).
                        let content = (!plain && prepared.is_none()).then(|| {
                            use std::hash::{Hash, Hasher};
                            let mut hasher = rustc_hash::FxHasher::default();
                            text.hash(&mut hasher);
                            font_size.to_bits().hash(&mut hasher);
                            bounds.width.to_bits().hash(&mut hasher);
                            bounds.height.to_bits().hash(&mut hasher);
                            hasher.finish()
                        });
                        let twin = content
                            .and_then(|content| self.shaped_like.get(&content))
                            .and_then(|id| self.shapes.get(id))
                            .filter(|twin| {
                                twin.text == *text
                                    && twin.size == font_size
                                    && twin.width == bounds.width
                                    && twin.height == bounds.height
                                    && twin.scale == self.scale
                                    && twin.text_options == item.text_options
                                    && twin.font == *item.font
                                    && twin.rich == rich
                            })
                            .map(|twin| {
                                spare_glyphs.extend_from_slice(&twin.glyphs);
                                let placed = twin.atlas_epoch == self.atlas_epoch;
                                if placed {
                                    spare_quads.extend_from_slice(&twin.quads);
                                }
                                (twin.decorations.clone(), placed.then_some(twin.atlas_epoch))
                            });
                        let b = if let Some((decorations, epoch)) = twin {
                            Err((std::mem::take(&mut spare_glyphs), decorations, None, epoch))
                        } else if plain {
                            let mut cache = self.text_cache.borrow_mut();
                            let laid = cache.layout_plain(
                                &mut fonts,
                                text,
                                font_size,
                                item.font,
                                Some(bounds.width),
                            );
                            // Plain text has one run: no line starts to find it by.
                            let glyphs = glyphs_of(
                                std::mem::take(&mut spare_glyphs),
                                laid.runs(Some(bounds.height)),
                                &[],
                                None,
                                self.scale,
                            );
                            let underlines = item.font.underline.map_or_else(Vec::new, |line| {
                                text::plain_underlines(
                                    laid.runs(Some(bounds.height)),
                                    font_size,
                                    line,
                                )
                            });
                            Err((glyphs, underlines, Some(laid.key()), None))
                        } else if let Some(rich) = prepared {
                            let mut cache = self.text_cache.borrow_mut();
                            let laid = cache.layout(&mut fonts, rich, Some(bounds.width));
                            let starts = laid.line_starts();
                            let glyphs = glyphs_of(
                                std::mem::take(&mut spare_glyphs),
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
                            Err((glyphs, decorations, Some(laid.key()), None))
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
                        let (glyphs, decorations, prepared, epoch) = match b {
                            Err(made) => made,
                            Ok(b) => (
                                glyphs_of(
                                    std::mem::take(&mut spare_glyphs),
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
                            quads: spare_quads,
                            atlas_epoch: epoch.unwrap_or(0),
                            last_used: self.frame,
                            prepared,
                        };
                        fresh = Some(shaped);
                        stats.shaped_nodes += 1;
                        if let Some(content) = content {
                            if self.shaped_like.len() >= SHAPED_LIKE {
                                self.shaped_like.clear();
                            }
                            insert_in::<MODE, _, _>(&mut self.shaped_like, content, item.id);
                        }
                    }
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
                    // Find's marks, under the selection: rounded a little,
                    // as a browser's.
                    let marks = scene.text_marks(item.id);
                    quads.extend(marks.iter().map(|m| {
                        let rect = [
                            bounds.x + m.rect.x,
                            bounds.y + m.rect.y,
                            m.rect.width,
                            m.rect.height,
                        ];
                        Quad {
                            rect,
                            uv: [0.; 4],
                            color: rgba(m.color, item.effects.opacity),
                            fade: rect,
                            options: [item.effects.edge_fade, 0., 0., 0.],
                            shape: [MARK_RADIUS, 0., 0., 1.],
                            border: [0.; 4],
                            mask: NO_MASK,
                        }
                    }));
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
                    // Its quads from the shape's, placed and coloured.
                    // (One extend, written straight into the room it makes: a
                    // push a glyph would check that room each time.)
                    let place = |quads: &mut Vec<Quad>, source: &[TextQuad]| {
                        // Plain and unselected: one colour, chosen once.
                        if selected.is_none() && rich.is_none() {
                            place_text_quads(
                                quads,
                                source,
                                x,
                                y,
                                fade,
                                item.effects.edge_fade,
                                |_| plain_color,
                            );
                        } else {
                            place_text_quads(
                                quads,
                                source,
                                x,
                                y,
                                fade,
                                item.effects.edge_fade,
                                |cached| {
                                    let recolor =
                                        selected.filter(|_| cached.paint[1] > 0.5).and_then(|h| {
                                            h.glyph_color(
                                                cached.rect[0] + cached.rect[2] * 0.5,
                                                cached.rect[1] + cached.rect[3] * 0.5,
                                            )
                                        });
                                    if let Some(color) = recolor {
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
                                    }
                                },
                            );
                        }
                    };
                    // Its shape as it was, inside its clip and with the atlas it
                    // was placed in: its quads, borrowed where they are kept.
                    let whole =
                        !self.cull_glyphs || (holds(viewport) && item.clip.is_none_or(holds));
                    if whole
                        && let Some(mut shaped) =
                            fresh.take_if(|shaped| shaped.atlas_epoch == self.atlas_epoch)
                    {
                        // (A twin's, placed in this atlas already.)
                        place(&mut quads, &shaped.quads);
                        glyphs_alone = marks.is_empty()
                            && highlight.is_none()
                            && shaped.decorations.is_empty();
                        shaped.last_used = self.frame;
                        self.shaped_bytes += shaped.bytes();
                        self.shapes.insert(item.id, shaped);
                    } else if whole
                        && fresh.is_none()
                        && let Some(shaped) = self.shapes.get_mut(&item.id)
                        && shaped.atlas_epoch == self.atlas_epoch
                    {
                        place(&mut quads, &shaped.quads);
                        glyphs_alone = marks.is_empty()
                            && highlight.is_none()
                            && shaped.decorations.is_empty();
                        shaped.last_used = self.frame;
                    } else {
                        let mut shaped = match fresh {
                            Some(shaped) => shaped,
                            None => {
                                let shaped = self.shapes.remove(&item.id).unwrap();
                                self.shaped_bytes -= shaped.bytes();
                                shaped
                            }
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
                                if let Some(a) = self.glyph::<MODE>(glyph.key, &mut stats)? {
                                    shaped.quads.push(TextQuad {
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
                                        paint: [0., 1., 0., 0.],
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
                        let visible = std::mem::take(&mut self.visible_glyphs);
                        place(&mut quads, if partial { &visible } else { &shaped.quads });
                        glyphs_alone = marks.is_empty()
                            && highlight.is_none()
                            && shaped.decorations.is_empty();
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
                }
                NodeKind::Container(_) => {}
            }
            // A canvas drawn as quads: clipped to its box.
            let clip = match canvas_clip.map(|canvas: Rect| clip.intersection(canvas)) {
                None => clip,
                Some(Some(inside)) => inside,
                Some(None) => {
                    quads.truncate(start as usize);
                    continue;
                }
            };
            // Inside rounded clips or a `fade_edges` container: cut and faded
            // in the shader, unless the item lies wholly where the region
            // changes nothing (a row well inside a rounded card): then drawn
            // without it, as the plain quads it is. `mask.w` marks those, so
            // a box moved alone (`patch_box_moved`) is made again instead.
            let item_quads = &mut quads[start as usize..];
            let in_region = Region::of(&item);
            let drawn_in = in_region.filter(|region| {
                !(item_quads.iter().all(|quad| quad.options[2] < 0.5)
                    && region.covers(quad_bounds(item_quads), self.scale))
            });
            let region = region_index(&mut clips, drawn_in);
            if region > 0. {
                for quad in item_quads {
                    quad.mask[0] = region;
                }
            } else if in_region.is_some() {
                for quad in item_quads {
                    quad.mask[3] = 1.;
                }
            }
            // When the clip only culls whole quads, apply it on the CPU and
            // draw under the viewport, so a change of clip never splits a
            // batch: neighbouring text, icons and rows become one draw. Quads
            // it would cut, transformed quads and filters keep the scissor.
            // (The kept quads' bounds, when the clip culls on the CPU.)
            let (draw_clip, culled_bounds) = match item.effects.blur_radius <= 0. {
                true if glyphs_alone => {
                    cull_glyphs(&mut quads, start as usize, clip, viewport, self.scale)
                }
                true => cull_on_cpu(&mut quads, start as usize, clip, viewport, self.scale),
                false => (clip, None),
            };
            if quads.len() as u32 > start || item.effects.blur_radius > 0. {
                let draw = Draw {
                    shading: fill_shading.unwrap_or(Shading::Full),
                    blur: (item.effects.blur_radius > 0.).then(|| {
                        Box::new((bounds, item.effects, blur_corners(item.kind, item.bounds)))
                    }),
                    start,
                    end: quads.len() as u32,
                    clip: draw_clip,
                    bounds: culled_bounds.unwrap_or_else(|| quad_bounds(&quads[start as usize..])),
                    source: if let Some(image) = canvas_image {
                        TextureSource::Image(image)
                    } else if let NodeKind::Image(image) = item.kind {
                        TextureSource::Image(image.id())
                    } else {
                        TextureSource::Atlas
                    },
                };
                let split = self.split_shading;
                // A glyph's quad (`glyph_quad`) is plain unless faded or in a
                // clip region, and textured: one shading for all of them.
                let glyphs =
                    glyphs_alone.then_some(match item.effects.edge_fade != 0. || in_region.is_some() {
                        true => Shading::Full,
                        false => Shading::Basic,
                    });
                push_draw(
                    &mut draws, &quads, draw, split, polylines, self.scale, glyphs,
                );
            }
        }
        if MODE == PATCH {
            close_range(
                &mut quads,
                &draws,
                &mut ranges,
                record && self.changes(text),
            );
            self.item_ranges = ranges;
            self.frame_quads = quads;
            self.frame_draws = draws;
            self.frame_points = points;
            self.patch_clips = clips;
            return Ok(stats);
        }
        // Effect passes' quads ride in the frame's vertex buffer.
        let effect_first = quads.len() as u32;
        quads.extend(Self::effect_quads(&effect_jobs));
        // The surface this frame draws into, when presenting through the
        // layer: shown surfaces stay as they are.
        #[cfg(own_surfaces)]
        let shown = if self.in_layer {
            None
        } else {
            self.begin_layer_frame(self.fresh || whole)?;
            self.layer_present
                .as_ref()
                .and_then(|present| present.shown)
        };
        #[cfg(not(own_surfaces))]
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
        #[cfg(own_surfaces)]
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
        #[cfg(own_surfaces)]
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
                draw.blur.as_deref().is_some_and(|&(bounds, _, _)| {
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
            let &(bounds, _, _) = draw.blur.as_deref()?;
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
                    .as_deref()
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
                let Some(&(bounds, effects, _)) = draw.blur.as_deref() else {
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
                                    && before.blur.as_deref().is_some_and(|&(under, beneath, _)| {
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
        // Polyline strokes read their points (group 2, or Metal's `POINTS`).
        // Every stroke appends at least two points before making its quad.
        let polylines =
            !points.is_empty() && draws.iter().any(|draw| draw.shading == Shading::Polyline);
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
            && draws.iter().all(|draw| match draw.source {
                TextureSource::Atlas => true,
                TextureSource::Layer(id) => self.layers.contains_key(&id),
                TextureSource::Image(id) => {
                    self.images.get(&id).is_some_and(|image| image.2.is_some())
                }
            })
            && (batches.is_empty()
                || self
                    .device
                    .features()
                    .contains(wgpu::Features::DUAL_SOURCE_BLENDING))
            // Shader fills and polylines compiled for Metal too.
            && (!polylines
                || self
                    .metal_frame
                    .get_or_init(|| metal_frame::MetalFrame::new(&self.device, &self.queue).ok())
                    .as_ref()
                    .is_some_and(|frame| frame.polylines()))
            && draws.iter().all(|draw| match draw.shading {
                Shading::Fill(id) => self.fill_shaders.get(&id).is_some_and(|shader| {
                    self.metal_frame
                        .get_or_init(|| metal_frame::MetalFrame::new(&self.device, &self.queue).ok())
                        .as_ref()
                        .is_some_and(|frame| frame.fills(shader))
                }),
                Shading::ImageEffect => self
                    .metal_frame
                    .get_or_init(|| metal_frame::MetalFrame::new(&self.device, &self.queue).ok())
                    .as_ref()
                    .is_some_and(|frame| frame.image_effects()),
                // Layer shaders and blend modes take the wgpu path.
                Shading::Filter(_) | Shading::Blend | Shading::FilterBlend(_) => false,
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
            let strokes = polylines.then_some(&points[..]);
            let stats = self.render_direct(
                quads, draws, &regions, &batches, catch_up, shown, effects, &clips, strokes, stats,
            );
            self.frame_points = points;
            return Ok(stats);
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
        // The polyline strokes' points too, bound as group 2 where drawn.
        let points_range = polylines.then(|| record(bytemuck::cast_slice(&points)));
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
            let parts = [bytes, &records];
            let (vertices, allocated) = match &self.kept_upload {
                Some((kept, kept_bytes)) => {
                    mapped.write_kept(&parts, kept, *kept_bytes, size_of::<Quad>())
                }
                None => mapped.write_parts(&parts),
            };
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
        let points_bind = points_range.map(|(offset, size)| {
            let polylines = self.context.inner.quads.polylines();
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("polyline points"),
                layout: &polylines.points_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &vertices,
                        offset,
                        size: wgpu::BufferSize::new(size),
                    }),
                }],
            })
        });
        self.encode_effects(
            &mut encoder,
            &vertices,
            &effect_jobs,
            effect_first,
            &mut stats,
        );
        // The pass splits at each batch of backdrop filters (blurred between
        // passes) and at each blended layer a region reaches (the target
        // under it copied between passes, for it to read): `Some(batch)` or
        // `None`, by draw.
        let mut cuts: Vec<(usize, Option<usize>)> = batches
            .iter()
            .enumerate()
            .map(|(batch, (start, _))| (*start, Some(batch)))
            // A blended draw is made only once `prepare_blend` made its
            // pipeline: with none made, no draw splits the pass.
            .chain(
                (0..if self.blend_pipelines.is_empty() {
                    0
                } else {
                    draws.len()
                })
                    .filter_map(|i| {
                        let draw = &draws[i];
                        (matches!(draw.shading, Shading::Blend | Shading::FilterBlend(_))
                            && regions
                                .iter()
                                .any(|(region, _)| region.intersects(draw.bounds)))
                        .then_some((i, None))
                    }),
            )
            .collect();
        // A filter's blur before a blend at the same draw.
        cuts.sort_by_key(|&(draw, batch)| (draw, batch.is_none()));
        let splits: Vec<usize> = cuts.iter().map(|(draw, _)| *draw).collect();
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
        // Shader fills and image effects have no pipelines writing two
        // targets at once.
        let defer = !self.in_layer
            && !draws.iter().any(|draw| {
                matches!(
                    draw.shading,
                    Shading::Fill(_)
                        | Shading::Filter(_)
                        | Shading::Blend
                        | Shading::FilterBlend(_)
                        | Shading::ImageEffect
                )
            })
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
        // its blur passes; each blended layer draws first in the pass after
        // its backdrop's copy (the draw, and the copy's bind group).
        let mut vertical = None;
        let mut backdrop: Option<(usize, wgpu::BindGroup)> = None;
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
                    points: points_bind.clone(),
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
                #[cfg(own_surfaces)]
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
                if let Some(points) = &points_bind {
                    pass.set_bind_group(2, points, &[]);
                }
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
                    let mut rest = &draws[from..end];
                    if let Some((at, bind)) = &backdrop
                        && *at == from
                        && let Some(draw) = rest.first()
                    {
                        self.encode_blended(&mut pass, *region, draw, bind, &mut stats);
                        if let Some(points) = &points_bind {
                            pass.set_bind_group(2, points, &[]);
                        }
                        rest = &rest[1..];
                    }
                    self.encode_draws(
                        &mut pass,
                        &|shading| {
                            self.fill_pipeline(shading)
                                .unwrap_or_else(|| quads.get(shading))
                        },
                        *region,
                        rest,
                        first,
                        &mut stats,
                    );
                }
            }
            if segment == cuts.len() {
                break;
            }
            match cuts[segment] {
                (_, Some(batch)) => {
                    vertical =
                        plans[batch]
                            .as_ref()
                            .zip(ranges[batch])
                            .and_then(|(plan, ranges)| {
                                self.blur(&mut encoder, plan, &vertices, ranges, &mut stats)
                            });
                }
                (at, None) => {
                    backdrop = Some((at, self.copy_backdrop(&mut encoder, &draws[at], &regions)));
                }
            }
            // The first filtered draw's own quads open the next pass.
            from = end;
        }
        *self.pending.get_mut() = Some(encoder);
        self.fresh = false;
        keep_room(&mut quads, &mut self.frame_peaks.0);
        keep_room(&mut draws, &mut self.frame_peaks.1);
        self.frame_quads = quads;
        self.frame_points = points;
        // (A kept frame's, lent: given back as they are.)
        if self.kept_upload.is_none() {
            draws.clear();
        }
        self.frame_draws = draws;
        Ok(stats)
    }
    /// Record this frame with Metal directly (`metal_frame`): the pass the
    /// wgpu path records for a frame of plain draws, committed at once.
    #[cfg(target_os = "macos")]
    #[allow(clippy::too_many_arguments)]
    fn render_direct(
        &mut self,
        mut quads: Vec<Quad>,
        mut draws: Vec<Draw>,
        regions: &[(Rect, Scissor)],
        batches: &[(usize, Vec<usize>)],
        catch_up: Option<std::ops::Range<u32>>,
        shown: Option<usize>,
        (effect_jobs, effect_first): (&[effects::Job], u32),
        clips: &[Region],
        points: Option<&[[f32; 2]]>,
        mut stats: GpuStats,
    ) -> GpuStats {
        let _span = zgx::profile::span(zgx::profile::Phase::Encode);
        let vertices = {
            use objc2::Message;
            let mapped = self
                .mapped
                .as_mut()
                .expect("checked by the caller")
                .get_mut();
            // Polyline strokes' points after the quads, where drawn.
            let parts = [
                bytemuck::cast_slice(&quads),
                points.map_or(&[][..], bytemuck::cast_slice),
            ];
            let (_, allocated) = match &self.kept_upload {
                Some((kept, bytes)) => mapped.write_kept(&parts, kept, *bytes, size_of::<Quad>()),
                None => mapped.write_parts(&parts),
            };
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
                // After the quads (as written above).
                points.map(|points| (quads.len() * size_of::<Quad>(), size_of_val(points))),
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
            let mut sampled = TextureSource::Atlas;
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
                                TextureSource::Image(image) => {
                                    self.images.get(&image).and_then(|image| image.2.as_ref())
                                }
                                TextureSource::Layer(layer) => {
                                    self.layers.get(&layer).map(|layer| &layer.texture)
                                }
                                TextureSource::Atlas => None,
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
                sampled = TextureSource::Atlas;
                stats.draw_calls += 3 * rects.iter().filter(|r| r.2 > 0 && r.3 > 0).count();
                stats.render_passes += 4;
                stats.blur_passes += 3;
                blurred = Some((plan, slots, rects));
                from = end;
            }
            let _span = zgx::profile::span(zgx::profile::Phase::Submit);
            pass.commit()
        };
        *self.metal_inflight.get_mut() = Some(committed);
        self.metal_frames += 1;
        stats.render_passes += 1;
        self.fresh = false;
        keep_room(&mut quads, &mut self.frame_peaks.0);
        keep_room(&mut draws, &mut self.frame_peaks.1);
        self.frame_quads = quads;
        // (A kept frame's, lent: given back as they are.)
        if self.kept_upload.is_none() {
            draws.clear();
        }
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
        // over 100 MB of graphics memory (see vendor/wgpu-hal/ZGX_PATCH.md):
        // repaint there, unless presenting surfaces of its own, which copy
        // by drawing from the shown surface. That holds the previous flush
        // only once it was presented.
        #[cfg(own_surfaces)]
        let copyable = match self.layer_present.as_ref() {
            Some(present) => present.current.is_none(),
            None => !cfg!(target_os = "macos"),
        };
        #[cfg(not(own_surfaces))]
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
            // Why it repaints, for devtools (`zgx inspect frames`).
            let refused: Result<_, fn(&Scene) -> String> = match pixels {
                _ if !zgx::switches::SCROLL_COPY.on() => {
                    Err(|_| "switched off (zgx inspect switch scroll_copy on)".into())
                }
                None => Err(|_| "its view or its move falls between device pixels".into()),
                Some(_) if !copyable => Err(|_| {
                    "the window presents through the system's surface, whose first copy holds \
                     100 MB of graphics memory (macOS)"
                        .into()
                }),
                Some(_) if self.fresh => {
                    Err(|_| "the window's pixels are new (a first frame, a resize)".into())
                }
                Some(_) if self.rendered_serial != Some(serial - 1) => {
                    Err(|_| "the frame before it was not drawn".into())
                }
                Some(copy) => Ok(copy),
            };
            match refused {
                Ok(copy) => match copy_blocker(scene, scroll, damage, self.scale) {
                    None => {
                        zgx::profile::note(|| {
                            format!("scroll @{} copied its pixels", scroll.node.number())
                        });
                        copies.push(copy);
                    }
                    Some(blocker) => {
                        zgx::profile::note(|| {
                            format!(
                                "scroll @{} repainted its view: {}",
                                scroll.node.number(),
                                blocker.describe(scene)
                            )
                        });
                        repaint.push(scroll.clip);
                    }
                },
                Err(why) => {
                    zgx::profile::note(|| {
                        format!(
                            "scroll @{} repainted its view: {}",
                            scroll.node.number(),
                            why(scene)
                        )
                    });
                    repaint.push(scroll.clip);
                }
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
                polyline: deferred
                    .points
                    .is_some()
                    .then(|| quads.get(Shading::Polyline)),
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
    /// The polyline pipeline for single-pass presentation to `format`:
    /// into the retained target too, or the drawable `alone`.
    fn polyline_drawing(&self, format: wgpu::TextureFormat, alone: bool) -> wgpu::RenderPipeline {
        let polylines = self.context.inner.quads.polylines();
        polyline_pipeline(
            &self.device,
            &polylines.module,
            &polylines.layout,
            Some(format),
            alone,
        )
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
        if let Some(points) = &frame.points {
            pass.set_bind_group(2, points, &[]);
        }
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
                    inner.quads.shader(),
                    &inner.quad_layout,
                    blend,
                    Some(format),
                    basic,
                )
            };
            let direct = |blend, basic| {
                drawable_pipeline(
                    &self.device,
                    inner.quads.shader(),
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
                    polyline: Default::default(),
                },
                direct: PickedPipelines {
                    full: direct(true, false),
                    clear: direct(false, false),
                    basic: direct(true, true),
                    opaque: direct(false, true),
                    polyline: Default::default(),
                },
            });
        }
        if !frame.regions.is_empty() {
            self.single_pass_frames += 1;
        }
        let polylines = frame.points.is_some();
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
                    pipelines
                        .direct
                        .picks(polylines, || self.polyline_drawing(format, true))
                } else {
                    pipelines
                        .both
                        .picks(polylines, || self.polyline_drawing(format, false))
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
            let picks = pipelines
                .both
                .picks(polylines, || self.polyline_drawing(format, false));
            self.encode_damage(&mut pass, frame, picks, &mut stats);
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
    /// Copy what the target holds under blended `draw`, within `regions`,
    /// into the backdrop for its size; the backdrop's bind group.
    fn copy_backdrop(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        draw: &Draw,
        regions: &[(Rect, Scissor)],
    ) -> wgpu::BindGroup {
        let under = draw.bounds.intersection(draw.clip);
        let area = regions
            .iter()
            .filter_map(|(region, _)| region.intersection(under?))
            .reduce(Rect::union);
        let copied = area.and_then(|area| scissor(area, self.scale, self.width, self.height));
        let target = self.target.clone();
        let (texture, bind) = self.backdrop();
        if let Some((x, y, width, height)) = copied {
            let at = |texture| wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            };
            encoder.copy_texture_to_texture(
                at(&target),
                at(texture),
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        bind.clone()
    }
    /// Draw blended `draw` (a layer's composite) within `region`, reading
    /// the backdrop `bind`: its own pipeline, replacing the target's pixels.
    fn encode_blended(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        region: Rect,
        draw: &Draw,
        bind: &wgpu::BindGroup,
        stats: &mut GpuStats,
    ) {
        let Some(pipeline) = self.fill_pipeline(draw.shading) else {
            return;
        };
        let Some((x, y, w, h)) = region
            .intersection(draw.clip)
            .and_then(|rect| rect.intersection(draw.bounds))
            .and_then(|rect| scissor(rect, self.scale, self.width, self.height))
        else {
            return;
        };
        pass.set_pipeline(pipeline);
        pass.set_scissor_rect(x, y, w, h);
        pass.set_bind_group(0, self.source_bind(draw.source), &[]);
        pass.set_bind_group(2, bind, &[]);
        pass.draw(0..4, draw.start..draw.end);
        stats.draw_calls += 1;
        stats.instances += (draw.end - draw.start) as usize;
    }
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
    fn batches(&self, region: Rect, draws: &[Draw], first: u32, emit: impl FnMut(Batch)) {
        match self.kept_upload.is_some() {
            true => self.batches_of::<true>(region, draws, first, emit),
            false => self.batches_of::<false>(region, draws, first, emit),
        }
    }
    /// `batches`, of the kept frame's draws (`KEPT`) or a frame's.
    #[inline(always)]
    fn batches_of<const KEPT: bool>(
        &self,
        region: Rect,
        draws: &[Draw],
        first: u32,
        mut emit: impl FnMut(Batch),
    ) {
        let mut batch: Option<Batch> = None;
        // Skipping blending pays for its extra draws only over many pixels:
        // a small region (a scrolled-in strip) blends plain quads in one draw.
        let pixels = region.width * region.height * self.scale * self.scale;
        let unblended = pixels >= SPLIT_PIXELS;
        for draw in draws {
            let shading = match draw.shading {
                Shading::Opaque if !unblended => Shading::Basic,
                // Drawn alone, after their backdrop's copy (`encode_blended`).
                Shading::Blend | Shading::FilterBlend(_) => continue,
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
            let source = draw.source;
            let pixels = draw
                .bounds
                .intersection(rect)
                .map_or(0., |covered| covered.width * covered.height)
                * self.scale
                * self.scale;
            if let Some(open) = &mut batch
                && open.rect == rect
                && open.source == source
                // From the kept frame, the instances it would span to this
                // draw (draws outside `region`, a kept text's spare room),
                // set up by the GPU to be scissored away: under
                // `SKIPPED_SPLIT` (past it, a draw call costs less), and only
                // below those in paint order (past them lie texts moved
                // there, painted elsewhere in order).
                && (!KEPT
                    || start == open.end
                    || start > open.end
                        && start - open.end < SKIPPED_SPLIT
                        && draw.end <= self.kept_ordered)
            {
                // Another shading joins when what it promotes (quads drawn
                // with a costlier shader than theirs) stays small.
                let merged = match open.shading == shading {
                    true => Some((shading, open.promoted)),
                    false => match merged_shading(open.shading, shading) {
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
                    },
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
        pass.set_bind_group(0, self.source_bind(batch.source), &[]);
        pass.draw(0..4, batch.start..batch.end);
        stats.draw_calls += 1;
        stats.instances += (batch.end - batch.start) as usize;
    }
    /// The bind group of what `source` samples (the atlas's where it is gone).
    #[inline(always)]
    fn source_bind(&self, source: TextureSource) -> &wgpu::BindGroup {
        match source {
            TextureSource::Atlas => &self.bind,
            TextureSource::Image(id) => self.images.get(&id).map_or(&self.bind, |entry| &entry.0),
            TextureSource::Layer(id) => {
                self.layers.get(&id).map_or(&self.bind, |layer| &layer.bind)
            }
        }
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
    fn upload_image(&mut self, image: &zgx::image::ImageData) -> Result<(), GpuError> {
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
            self.unified,
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
        if self.unified {
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
    /// Start the glyph atlas over in place: every glyph and icon cell
    /// (`entries`, `image_cells`) given up together, so the next to take a
    /// cell never draws over one still listed. (A scale change once kept
    /// the icons' cells: back at a scale, an icon drew the glyphs written
    /// over it meanwhile.)
    fn recycle_atlas(&mut self) {
        self.release_atlas_cells();
        self.entries.clear();
        self.image_cells.clear();
        self.atlas_epoch = self.atlas_epoch.wrapping_add(1);
        self.cursor = (0, 0, 0);
    }
    fn reset_atlas(&mut self, size: u32) {
        self.atlas = sampled_texture(&self.device, self.unified, size, size, "glyph atlas");
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
        let lines = |quad: &TextQuad| quad.rect[1] + quad.rect[3] >= top && quad.rect[1] <= bottom;
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
            let entry = match self.entries.get_mut(&AtlasKey::from(glyph.key)) {
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
    #[inline(always)]
    fn glyph<const MODE: u8>(
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
        if let Some(entry) = get_in::<MODE, _, _>(&mut self.entries, &AtlasKey::from(key)) {
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
            self.entries.insert(AtlasKey::from(key), None);
            return Ok(None);
        };
        let p = image.placement;
        if p.width == 0 || p.height == 0 {
            self.entries.insert(AtlasKey::from(key), None);
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
        self.entries.insert(AtlasKey::from(key), Some(entry));
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
    #[cfg(own_surfaces)]
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
        present.catch_up = present.catch_up_for(index, whole, width, height);
        // How many areas the surface copies from the one shown before the
        // frame draws (`catch_up_draws` takes them as it renders).
        present_log::log(|| {
            format!(
                "{} draw whole={whole} held={held:?} latest={latest:?} catch_up={}",
                present.plane(),
                present.catch_up.len()
            )
        });
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
        if let Some(layer) = present.layer.as_mut() {
            layer.set_caret(present.caret);
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
            // Every frame is a tile of the plain layer, at the display's
            // full rate too (`layer_present::Layer`), unless the video
            // layer is asked for (`ZGX_PRESENT_VIDEO`).
            || !*VIDEO_AT_FULL_RATE
            // With the video layer: slower frames than the display's (a
            // 30 fps shimmer, a clock) are still the plain layer's, as the
            // video layer queues frames on threads of its own, a cost worth
            // paying only for a full rate. It stays a second after one, so
            // a pause in a scroll does not drop and rebuild it.
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
                "{} present frame={} index={index} held={:?} path={} full_rate={} opaque={opaque}",
                plane,
                present.frame + 1,
                present.buffers[index].1,
                if plain { "plain" } else { "show" },
                present.full_rate,
            )
        });
        let layer = present.layer.as_mut().expect("attached above");
        let buffer = &present.buffers[index].0;
        if plain {
            layer.show_plain(buffer, opaque);
        } else {
            layer.show(buffer, opaque, present.full_rate);
        }
        layer.keep(
            present
                .buffers
                .iter()
                .map(|(buffer, _)| buffer.surface.id()),
        );
        present.presented(index);
        self.layer_present = Some(present);
        Ok(PresentationStatus::Presented)
    }
    /// Make the swap chain's current buffer the retained target for the
    /// frame about to draw, unless one is: once the compositor lets a frame
    /// start (the swap chain's waitable), with the regions it lacks of the
    /// last frame queued to copy from the shown buffer (none when `whole`).
    #[cfg(windows)]
    fn begin_layer_frame(&mut self, whole: bool) -> Result<(), GpuError> {
        let (width, height) = (self.width, self.height);
        let Some(present) = self.layer_present.as_ref() else {
            return Ok(());
        };
        if present.current.is_some() {
            return Ok(());
        }
        if !self.background_matched {
            // The first frame, drawn whole: an opaque window whose background
            // lets what is behind show presents with alpha from it. Its
            // buffers may be the target: let go of them first.
            self.target = texture(&self.device, 1, 1, "retained output placeholder");
            self.view = self.target.create_view(&Default::default());
            self.target_copy_bind = None;
            self.match_background()?;
        }
        let Some(present) = self.layer_present.as_ref() else {
            return Ok(());
        };
        if present
            .buffers
            .first()
            .is_none_or(|(b, _)| (b.texture.width(), b.texture.height()) != (width, height))
        {
            // Made or resized: buffers of the window's size. The target may
            // be one of the old ones; it lets go first.
            self.target = texture(&self.device, 1, 1, "retained output placeholder");
            self.view = self.target.create_view(&Default::default());
            self.target_copy_bind = None;
            let present = self.layer_present.as_mut().expect("presenting");
            let chain = present.chain.as_mut().expect("a swap chain");
            chain.release(present.buffers.drain(..).map(|(buffer, _)| buffer));
            present.shown = None;
            present.history.clear();
            if chain.buffers_made() {
                chain.resize(width, height)?;
            }
            present.buffers = chain
                .buffers(&self.device, width, height)?
                .into_iter()
                .map(|buffer| (buffer, None))
                .collect();
        }
        let present = self.layer_present.as_mut().expect("presenting");
        let chain = present.chain.as_ref().expect("a swap chain");
        let started = std::time::Instant::now();
        let ready = chain.wait(1000);
        let index = chain.current();
        if !ready || started.elapsed().as_micros() > 500 {
            present_log::log(|| {
                format!(
                    "dxgi acquire index={index} waited_us={} ready={ready}",
                    started.elapsed().as_micros()
                )
            });
        }
        present.catch_up = present.catch_up_for(index, whole, width, height);
        present_log::log(|| {
            format!(
                "dxgi draw whole={whole} held={:?} catch_up={}",
                present.buffers[index].1,
                present.catch_up.len()
            )
        });
        let buffer = &present.buffers[index].0;
        self.target = buffer.texture.clone();
        self.view = buffer.view.clone();
        present.current = Some(index);
        Ok(())
    }
    /// Show the retained frame (the swap chain's current buffer), the
    /// regions it changed as the present's dirty rectangles.
    #[cfg(windows)]
    fn present_layer(
        &mut self,
        before_present: impl FnOnce(),
    ) -> Result<PresentationStatus, GpuError> {
        let Some(index) = self
            .layer_present
            .as_mut()
            .and_then(|present| present.current.take())
        else {
            // Nothing drawn since: the shown frame stands.
            return Ok(PresentationStatus::Presented);
        };
        let mut encoder = self
            .pending
            .get_mut()
            .take()
            .unwrap_or_else(|| self.device.create_command_encoder(&Default::default()));
        {
            let present = self.layer_present.as_ref().expect("presenting");
            // Presented, a buffer is in the common state the compositor reads.
            encoder.transition_resources(
                std::iter::empty(),
                std::iter::once(wgpu::TextureTransition {
                    texture: &present.buffers[index].0.texture,
                    selector: None,
                    state: wgpu::TextureUses::PRESENT,
                }),
            );
        }
        self.queue.submit([encoder.finish()]);
        self.belt.get_mut().recall();
        self.submitted();
        if let Some(mapped) = self.mapped.as_mut() {
            mapped.get_mut().submitted();
        }
        before_present();
        let present = self.layer_present.as_mut().expect("presenting");
        present_log::log(|| {
            format!(
                "dxgi present frame={} index={index} held={:?} dirty={}",
                present.frame + 1,
                present.buffers[index].1,
                present.changed.len()
            )
        });
        let chain = present.chain.as_mut().expect("a swap chain");
        let seen = chain.present(&present.changed)?;
        self.dxgi_stats.presented_chain(chain.swap_chain());
        // The buffer holds the frame either way: the swap chain's index says
        // which the next draws into.
        present.presented(index);
        Ok(if seen {
            PresentationStatus::Presented
        } else {
            PresentationStatus::Occluded
        })
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
        let frame = self.shown_frame()?;
        // Direct frames are waited for before they are shown; wgpu's here.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        frame.read(0, 0, u32::MAX, u32::MAX)
    }
    /// The surface its layer shows now, to read a region of later
    /// (`ShownFrame::read`): what the window last showed. Holding it keeps
    /// only that surface, which the renderer does not draw into while it is
    /// shown; take a new one after each frame.
    #[cfg(target_os = "macos")]
    pub fn shown_frame(&self) -> Option<ShownFrame> {
        let present = self.layer_present.as_ref()?;
        let (buffer, _) = present.buffers.get(present.shown?)?;
        Some(ShownFrame {
            surface: buffer.surface.clone(),
        })
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
    /// scrolling). On macOS they may keep a third surface, when the window
    /// server still holds both others as the next frame starts (with
    /// `ZGX_PRESENT_VIDEO`, the video layer's queue does); slower frames make
    /// do with two. Elsewhere ignored.
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
        let status = {
            let _span = zgx::profile::span(zgx::profile::Phase::Present);
            self.present_frame(before_present)
        };
        // The frame is done once shown (see `render`).
        if self.presents() {
            zgx::profile::frame();
        }
        status
    }
    fn present_frame(
        &mut self,
        before_present: impl FnOnce(),
    ) -> Result<PresentationStatus, GpuError> {
        if let Some(reason) = self.context.lost() {
            return Err(GpuError::device_lost(&reason));
        }
        #[cfg(own_surfaces)]
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
        // A frame's quads and draws: the next frame makes them again.
        self.frame_quads = Vec::new();
        self.frame_points = Vec::new();
        self.frame_draws = Vec::new();
        self.visible_glyphs = Vec::new();
        self.frame_peaks = (0, 0);
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
                layer.keep(std::iter::once(present.buffers[0].0.surface.id()));
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
            self.idle = true;
        }
    }
    /// Input arrived: if the window idled (`trim`) and has drawn nothing
    /// since, wake the GPU now with a trivial pass, committed without
    /// waiting, so the frame the input makes need not wait for it to wake.
    /// On macOS, unless `ZGX_GPU_WARM=0`; elsewhere nothing.
    #[doc(hidden)]
    pub fn warm_up(&mut self) {
        #[cfg(target_os = "macos")]
        if *WARM_AFTER_IDLE
            && std::mem::take(&mut self.idle)
            && let Some(Some(frame)) = self.metal_frame.get()
        {
            present_log::log(|| "warm".into());
            frame.warm();
            self.warms += 1;
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
    /// Frames drawn through a stencil of their damage: none, as frames no
    /// longer draw so (kept for 0.1.0's callers).
    #[deprecated(
        since = "0.1.1",
        note = "frames no longer draw through a stencil: always 0"
    )]
    #[doc(hidden)]
    pub fn debug_stamped_frames(&self) -> u64 {
        0
    }
    /// Test hook kept for 0.1.0's callers: frames no longer draw through a
    /// stencil, so it does nothing.
    #[deprecated(
        since = "0.1.1",
        note = "frames no longer draw through a stencil: does nothing"
    )]
    #[doc(hidden)]
    pub fn debug_stencil(&mut self, _stencil: bool) {}
    /// Passes that woke the GPU for input after idling (`warm_up`).
    #[doc(hidden)]
    pub fn debug_warms(&self) -> u64 {
        self.warms
    }
    /// Test hook: a renderer presenting through surfaces of its own, as a
    /// window's does, into `host` (a layer of the caller's), without a
    /// window: `width` by `height` pixels.
    #[cfg(target_os = "macos")]
    #[doc(hidden)]
    pub fn debug_for_layer(
        host: objc2::rc::Retained<objc2_quartz_core::CALayer>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        let context = GpuContext::new()?;
        let mut renderer = Self::create(None, width, height, &context)?;
        if renderer.info.backend != wgpu::Backend::Metal {
            return Err(GpuError(
                "layer presentation needs the Metal backend".into(),
            ));
        }
        renderer.layer_present = Some(LayerPresent {
            host: Some(host),
            plain_only: true,
            ..Default::default()
        });
        Ok(renderer)
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
/// A frame a window showed (`GpuRenderer::shown_frame`), its pixels
/// readable while it is held.
#[cfg(target_os = "macos")]
pub struct ShownFrame {
    surface: objc2_core_foundation::CFRetained<objc2_io_surface::IOSurfaceRef>,
}
#[cfg(target_os = "macos")]
impl ShownFrame {
    /// The region `x, y, width, height` (pixels, from its top left, clipped
    /// to it) as premultiplied BGRA rows, with its size; `None` for a region
    /// outside it.
    pub fn read(&self, x: u32, y: u32, width: u32, height: u32) -> Option<(u32, u32, Vec<u8>)> {
        let surface = objc2_core_foundation::CFRetained::as_ptr(&self.surface)
            .as_ptr()
            .cast::<std::ffi::c_void>()
            .cast_const();
        // SAFETY: a live surface (held), BGRA. It was shown, so the GPU
        // finished drawing it.
        unsafe {
            iosurface::with_pixels(surface, |bytes, full_w, full_h, stride| {
                let (x, y) = (x as usize, y as usize);
                let w = (width as usize).min(full_w.saturating_sub(x));
                let h = (height as usize).min(full_h.saturating_sub(y));
                (w > 0 && h > 0).then(|| {
                    let mut pixels = Vec::with_capacity(w * h * 4);
                    for row in y..y + h {
                        pixels.extend_from_slice(&bytes[row * stride + x * 4..][..w * 4]);
                    }
                    (w as u32, h as u32, pixels)
                })
            })
            .flatten()
        }
    }
}
/// `rect` less `cut`, as up to four rectangles pushed to `out`.
/// The widest a filter of `bounds` and `effects` blurs anywhere in `area`:
/// its radius, less under an edge fade (as blur.wgsl's `sigma`).
fn peak_blur(bounds: Rect, effects: &zgx::scene::Effects, area: Rect) -> f32 {
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
            let &(bounds, effects, corners) = draw.blur.as_deref().expect("batched filter");
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
// Used by surfaces of the renderer's own only.
#[cfg_attr(not(own_surfaces), allow(dead_code))]
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
#[cfg_attr(not(own_surfaces), allow(dead_code))]
fn scissor_holds(outer: Scissor, inner: Scissor) -> bool {
    outer.0 <= inner.0
        && outer.1 <= inner.1
        && outer.0 + outer.2 >= inner.0 + inner.2
        && outer.1 + outer.3 >= inner.1 + inner.3
}
/// A texture draws sample and the CPU writes (glyph cells, images): with
/// `cpu_writes` on macOS, in shared memory written in place
/// (`upload::shared_texture`).
/// Keep `vec`'s room for the frames to come (its length is what this one
/// used), unless a spike left it more than twice what frames have used
/// lately (`peak`, which follows the length up at once and down a 64th a
/// frame) and it holds more than 64 KB: a frame of a long page scrolled past
/// keeps none of it.
fn keep_room<T>(vec: &mut Vec<T>, peak: &mut usize) {
    *peak = vec.len().max(*peak - *peak / 64);
    if vec.capacity() > 2 * *peak && vec.capacity() * std::mem::size_of::<T>() > 64 << 10 {
        vec.shrink_to(*peak);
    }
}
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
fn glyph_quad(glyph: &Glyph, a: AtlasEntry, scale: f32, atlas_size: u32) -> TextQuad {
    TextQuad {
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
        paint: [0., 1., 0., 0.],
    }
}
fn rich_decoration_quad(d: &text::RichDecoration) -> TextQuad {
    let rect = [d.bounds.x, d.bounds.y, d.bounds.width, d.bounds.height];
    TextQuad {
        rect,
        color: rgba(d.color, 1.),
        uv: [0.; 4],
        // A rounded background (an inline code chip): a rounded rect.
        paint: if d.radius > 0. {
            [d.radius, 0., 0., 1.]
        } else {
            [0.; 4]
        },
    }
}
/// A shader fill's module: the quad shader (draw.wgsl), the `FillInput` its
/// `fill` takes, the user's source and the fragment entry `zgx_fill`, which
/// shades a quad carrying uniforms (`mask.y`: their record in the region
/// table, plus one) with `fill`'s colour and the rest as `shade` does.
pub(crate) fn fill_source(user: &str) -> String {
    format!(
        "{}\nstruct FillInput {{ p:vec2<f32>, size:vec2<f32>, time:f32, scale:f32, params:array<vec4<f32>,4> }}\n{user}\n{}",
        include_str!("draw.wgsl"),
        "@fragment fn zgx_fill(input:Out)->@location(0) vec4<f32> {
 if input.mask.y<.5 {return shade(input);}
 let u=regions[u32(input.mask.y)-1u]; var v=input;
 let point=select(v.position.xy/viewport.z,v.point,v.options.z>0.5);
 let c=fill(FillInput(point-v.fade.xy,v.fade.zw,u.round0.x,viewport.z,array<vec4<f32>,4>(u.radii0,u.round1,u.radii1,u.fade_box)));
 v.color=vec4(c.rgb,c.a*v.color.a);
 return shade(v);
}"
    )
}
/// A layer shader's module (`zgx::scene::LayerShader`): the quad shader,
/// the `LayerInput` its `layer` takes, `zgx_sample`, the user's source and
/// the fragment entry `zgx_filter`, which passes the layer's texel under a
/// layer's composite quad (`mask.y`: its uniforms' record, plus one) through
/// `layer` and shades the result as `shade` does. The quad's `fade` is the
/// node's box and `point` its untransformed position, so `p` is the node's.
pub(crate) fn filter_source(user: &str) -> String {
    format!(
        "{}\n{}\nstruct LayerInput {{ color:vec4<f32>, p:vec2<f32>, size:vec2<f32>, time:f32, scale:f32, params:array<vec4<f32>,4> }}\n{}\n{user}\n{}",
        include_str!("draw.wgsl"),
        include_str!("blend.wgsl"),
        "var<private> zgx_frame:vec4<f32>;
// The layer's straight-alpha pixel at logical `p` from the node's top-left.
fn zgx_sample(p:vec2<f32>)->vec4<f32> {
 let uv=zgx_frame.xy+p*zgx_frame.zw;
 let t=textureSampleLevel(atlas,tex_sampler,uv,0.);
 return select(vec4(0.),vec4(t.rgb/t.a,t.a),t.a>0.);
}",
        "fn zgx_filtered(input:Out)->vec4<f32> {
 var v=input;
 let point=select(v.position.xy/viewport.z,v.point,v.options.z>0.5);
 // The texture's uv per logical point of the node (the quad maps one
 // linearly to the other), so `zgx_sample` reaches any p: from the
 // screen derivatives of both, taken before any branch.
 let du=mat2x2(dpdx(v.uv),dpdy(v.uv)); let dp=mat2x2(dpdx(point),dpdy(point));
 let det=dp[0].x*dp[1].y-dp[1].x*dp[0].y;
 let inverse=mat2x2(vec2(dp[1].y,-dp[0].y),vec2(-dp[1].x,dp[0].x))*(1./select(det,1e-6,abs(det)<1e-12));
 let k=du*inverse;
 if v.mask.y<.5 {return shade(v);}
 let u=regions[u32(v.mask.y)-1u];
 let p=point-v.fade.xy;
 // The shader runs over the node's box; the layer's margin past it (its
 // resampling edge, overflow) composites as painted.
 if any(p<vec2(0.))||any(p>=v.fade.zw) {return shade(v);}
 zgx_frame=vec4(v.uv-vec2(k[0].x,k[1].y)*p,k[0].x,k[1].y);
 let c=clamp(layer(LayerInput(zgx_sample(p),p,v.fade.zw,u.round0.x,viewport.z,array<vec4<f32>,4>(u.radii0,u.round1,u.radii1,u.fade_box))),vec4(0.),vec4(1.));
 return shade_texel(v,vec4(c.rgb*c.a,c.a),true);
}
@fragment fn zgx_filter(input:Out)->@location(0) vec4<f32> { return zgx_filtered(input); }
@fragment fn zgx_filter_blended(input:Out)->@location(0) vec4<f32> {
 return zgx_blend(zgx_filtered(input),zgx_beneath(input.position),u32(input.mask.z));
}"
    )
}
/// The key of the blended layers' plain pipeline in `blend_pipelines`
/// (layer shaders' ids count up from 1).
const BLEND_PLAIN: u64 = 0;
/// The blended layers' module: the quad shader, blend.wgsl, and the
/// fragment entry `zgx_blended`, which shades a layer's composite quad as
/// `shade` does and blends it with the backdrop under it by `mask.z`.
pub(crate) fn blend_source() -> String {
    format!(
        "{}\n{}\n{}",
        include_str!("draw.wgsl"),
        include_str!("blend.wgsl"),
        "@fragment fn zgx_blended(input:Out)->@location(0) vec4<f32> {
 return zgx_blend(shade(input),zgx_beneath(input.position),u32(input.mask.z));
}"
    )
}
/// Checks a layer shader's source (the user's part) as the renderer will
/// compile it, as `check_fill_shader` does a fill's.
pub fn check_layer_shader(user: &str) -> Result<(), String> {
    check_module(user, filter_source)
}
/// Checks a shader fill's source (the user's part, as `FillShader` holds
/// it) as the renderer will compile it: `Err` reads like a compiler
/// message, with lines counted in `user`, for an app or script to show.
pub fn check_fill_shader(user: &str) -> Result<(), String> {
    check_module(user, fill_source)
}
/// Checks `user` in the module `module` builds around it.
fn check_module(user: &str, module: fn(&str) -> String) -> Result<(), String> {
    let source = module(user);
    // Lines before the user's code in the module `module` builds.
    let template = module("\u{1}");
    let before = template[..template.find('\u{1}').unwrap_or(0)]
        .matches('\n')
        .count();
    let place = |location: Option<wgpu::naga::SourceLocation>| match location {
        Some(at) if at.line_number as usize > before => {
            format!("line {}: ", at.line_number as usize - before)
        }
        Some(_) => "in zgx's part of the module (a reserved name?): ".to_owned(),
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
#[inline(always)]
fn axis_gradient(style: &zgx::scene::QuadStyle, bounds: Rect) -> Option<(RampKey, [f32; 4])> {
    use zgx::{canvas::Point, decoration::Background};
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
        Background::Brush(zgx::canvas::Brush::Linear { start, end, stops }) => {
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
fn ramp_image(key: &RampKey) -> Result<zgx::image::ImageData, GpuError> {
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
    zgx::image::ImageData::new(width, height, pixels).map_err(|e| GpuError(e.into()))
}
fn background_quad(
    image: &zgx::image::ImageData,
    fit: zgx::style::ObjectFit,
    style: &zgx::scene::QuadStyle,
    bounds: Rect,
    opacity: f32,
    edge_fade: f32,
) -> Option<Quad> {
    use zgx::style::ObjectFit;
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
#[inline(always)]
fn outline_quad(
    outline: &zgx::scene::Outline,
    style: &zgx::scene::QuadStyle,
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
/// What a canvas draws as quads (`canvas_quads`): fills (pushed as panels),
/// rings and polyline strokes, in order.
enum CanvasQuad {
    Fill(Quad),
    Ring(Quad),
    Polyline(Quad),
}
/// How many segments a pixel of a polyline stroke may have in its reach
/// (`zgx_polyline`'s loop): denser strokes rasterize.
const POLYLINE_REACH_SEGMENTS: usize = 32;
/// A polyline stroke's quad (polyline.wgsl), its points added to `points`:
/// a subpath of straight lines whose x never decreases (or never
/// increases: drawn the other way), as a sparkline's or a chart's, each
/// pixel finding its segments by a search. `None` for anything else.
fn polyline_quad(
    subpath: &[zgx::canvas::Segment],
    style: &zgx::canvas::Stroke,
    color: Color,
    origin: (f32, f32),
    zoom: f32,
    points: &mut Vec<[f32; 2]>,
) -> Option<Quad> {
    use zgx::canvas::{LineCap, LineJoin, Segment};
    let mut line: Vec<[f32; 2]> = Vec::with_capacity(subpath.len());
    for (k, segment) in subpath.iter().enumerate() {
        let point = match (k, segment) {
            (0, Segment::Move(p)) | (1.., Segment::Line(p)) => p,
            _ => return None,
        };
        let point = [point.x * zoom + origin.0, point.y * zoom + origin.1];
        if !point.iter().all(|v| v.is_finite()) {
            return None;
        }
        // A repeated point adds nothing to the outline but a join's corner.
        if line.last() != Some(&point) {
            line.push(point);
        }
    }
    if line.len() < 2 {
        return None;
    }
    if line.windows(2).all(|w| w[1][0] <= w[0][0]) {
        line.reverse();
    }
    if !line.windows(2).all(|w| w[1][0] >= w[0][0]) {
        return None;
    }
    let half = style.width * zoom / 2.;
    let limit = match style.join {
        LineJoin::Miter => style.miter_limit.max(1.),
        _ => 1.,
    };
    // As far as its pixels lie from its points: a miter's tip, a square
    // cap's corner, and the antialiased edge (as the shader reckons it).
    let reach = half * limit.max(1.42) + 1.;
    // Every pixel searches its reach: the points within any span of twice
    // it, and one segment either side, are at most the loop's.
    let mut end = 0;
    for (k, point) in line.iter().enumerate() {
        end = end.max(k);
        while end < line.len() && line[end][0] < point[0] + 2. * reach + 1. {
            end += 1;
        }
        if end - k + 1 > POLYLINE_REACH_SEGMENTS {
            return None;
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for [x, y] in &line {
        (x0, y0, x1, y1) = (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y));
    }
    let bounds = Rect::new(x0, y0, x1 - x0, y1 - y0).expand(reach);
    let first = points.len();
    points.extend_from_slice(&line);
    let cap = match style.cap {
        LineCap::Butt => 0.,
        LineCap::Square => 1.,
        LineCap::Round => 2.,
    };
    let join = match style.join {
        LineJoin::Miter => 0.,
        LineJoin::Bevel => 1.,
        LineJoin::Round => 2.,
    };
    let rect = [bounds.x, bounds.y, bounds.width, bounds.height];
    Some(Quad {
        rect,
        uv: [0.; 4],
        color: rgba(color, 1.),
        fade: rect,
        options: [0.; 4],
        shape: [0.; 4],
        border: [half, cap, join, limit],
        mask: [0., first as f32 + 1., line.len() as f32, 0.],
    })
}
/// A grid fill's marks over a canvas's box (`bounds`), as plain quads.
fn grid_quads(grid: &zgx::scene::GridFill, bounds: Rect, opacity: f32, quads: &mut Vec<Quad>) {
    let color = rgba(grid.color, opacity);
    quads.extend(grid.marks(bounds).into_iter().map(|r| {
        let rect = [r.x, r.y, r.width, r.height];
        Quad {
            rect,
            uv: [0.; 4],
            color,
            fade: rect,
            options: [0.; 4],
            shape: [0.; 4],
            border: [0.; 4],
            mask: NO_MASK,
        }
    }));
}
/// A canvas at `origin`, under `view` (its pan and zoom), drawn as quads,
/// when each of its commands is a solid
/// fill or stroke of a simple shape (`Path::shape`) the quad shader draws
/// as a rasterizer would: a rectangle, its corners rounded or not, a circle,
/// a line along an axis. No raster is made or uploaded, and moving or
/// scaling what it draws costs its quads alone. `None` for anything else
/// (curves, ellipses that aren't circles, gradients, patterns, dashes,
/// clips), which rasterizes as before.
fn canvas_quads(
    canvas: &zgx::canvas::Canvas,
    origin: (f32, f32),
    view: zgx::scene::CanvasView,
    points: &mut Vec<[f32; 2]>,
) -> Option<Vec<CanvasQuad>> {
    let mark = points.len();
    let quads = canvas_quads_of(canvas, origin, view, points);
    if quads.is_none() {
        points.truncate(mark);
    }
    quads
}
fn canvas_quads_of(
    canvas: &zgx::canvas::Canvas,
    origin: (f32, f32),
    view: zgx::scene::CanvasView,
    points: &mut Vec<[f32; 2]>,
) -> Option<Vec<CanvasQuad>> {
    use zgx::canvas::{Brush, Draw, LineCap, LineJoin, Segment, Shape};
    let mut out = Vec::with_capacity(canvas.commands().len());
    // The view scales what it draws about the box's origin, then moves it:
    // shapes stay shapes, only placed and sized as they show.
    let zoom = view.scale;
    let origin = (origin.0 + view.x, origin.1 + view.y);
    let at = |r: Rect| {
        Rect::new(
            r.x * zoom + origin.0,
            r.y * zoom + origin.1,
            r.width * zoom,
            r.height * zoom,
        )
    };
    // A box given by a corner and a negative size, the other way round.
    let positive = |r: Rect| {
        Rect::new(
            r.x.min(r.x + r.width),
            r.y.min(r.y + r.height),
            r.width.abs(),
            r.height.abs(),
        )
    };
    // Its pixels reach past its box by its antialiased edge: a quad shades
    // the pixels whose centres it covers, and a shape starting a quarter of
    // a pixel in would lose the quarter-covered pixel before it.
    let reach = |r: Rect| {
        let r = r.expand(1.);
        [r.x, r.y, r.width, r.height]
    };
    // A filled box with corners rounded by `radius`: the panel fill.
    let fill = |r: Rect, radius: f32, color: Color| {
        CanvasQuad::Fill(Quad {
            rect: reach(r),
            uv: [0.; 4],
            color: rgba(color, 1.),
            fade: [r.x, r.y, r.width, r.height],
            options: [0., 0., 0., 1.],
            shape: [0., 0., 0., 1.],
            border: [radius; 4],
            mask: NO_MASK,
        })
    };
    for command in canvas.commands() {
        match command {
            Draw::Fill {
                path,
                brush: Brush::Solid(color),
                ..
            } => match path.shape()? {
                Shape::Rect { rect, radius } => {
                    let rect = positive(rect);
                    out.push(fill(at(rect), radius * zoom, *color));
                }
                Shape::Ellipse { rect } if rect.width == rect.height => {
                    let rect = positive(rect);
                    out.push(fill(at(rect), rect.width * zoom / 2., *color));
                }
                Shape::Ellipse { .. } => return None,
                // A line has no inside.
                Shape::Line { .. } => {}
            },
            Draw::Stroke {
                path,
                brush: Brush::Solid(color),
                style,
            } if style.dash.is_empty() && style.width > 0. && style.width * zoom < 255. => {
                let half = style.width / 2.;
                // As it shows: the stroke scales with what it outlines.
                let (width, shown_half) = (style.width * zoom, style.width * zoom / 2.);
                let Some(shape) = path.shape() else {
                    // Each subpath a polyline stroke.
                    let segments = path.segments();
                    let mut start = 0;
                    while start < segments.len() {
                        let end = segments[start + 1..]
                            .iter()
                            .position(|s| matches!(s, Segment::Move(_)))
                            .map_or(segments.len(), |at| start + 1 + at);
                        let quad = polyline_quad(
                            &segments[start..end],
                            style,
                            *color,
                            origin,
                            zoom,
                            points,
                        )?;
                        out.push(CanvasQuad::Polyline(quad));
                        start = end;
                    }
                    continue;
                };
                match shape {
                    // A ring as wide as the line, half outside the outline.
                    shape @ (Shape::Rect { .. } | Shape::Ellipse { .. }) => {
                        let (rect, radius) = match shape {
                            Shape::Rect { rect, radius } => (positive(rect), radius),
                            Shape::Ellipse { rect } if rect.width == rect.height => {
                                (positive(rect), rect.width.abs() / 2.)
                            }
                            _ => return None,
                        };
                        // Square corners join as the stroke asks: a miter
                        // (within its limit) stays square, a round join
                        // rounds by half the width, a bevel cuts them.
                        let outer = match (radius > 0., style.join) {
                            (true, _) => radius * zoom + shown_half,
                            (false, LineJoin::Round) => shown_half,
                            (false, LineJoin::Miter)
                                if style.miter_limit >= std::f32::consts::SQRT_2 =>
                            {
                                0.
                            }
                            (false, _) => return None,
                        };
                        let box_ = at(rect).expand(shown_half);
                        let side = (width * 16.).round();
                        let fade = [box_.x, box_.y, box_.width, box_.height];
                        out.push(CanvasQuad::Ring(Quad {
                            rect: reach(box_),
                            uv: [0.; 4],
                            color: rgba(*color, 1.),
                            fade,
                            options: [0., 0., 0., 1.],
                            shape: [side + 4096. * side, -1., side + 4096. * side, 1.],
                            border: [outer; 4],
                            mask: NO_MASK,
                        }));
                    }
                    // Along an axis, a line is a box as wide as it is, its
                    // ends as its caps say.
                    Shape::Line { from, to } if from.x == to.x || from.y == to.y => {
                        let reach = match style.cap {
                            LineCap::Butt => 0.,
                            LineCap::Square | LineCap::Round => half,
                        };
                        let (x0, x1) = (from.x.min(to.x), from.x.max(to.x));
                        let (y0, y1) = (from.y.min(to.y), from.y.max(to.y));
                        let rect = if from.y == to.y {
                            Rect::new(x0 - reach, y0 - half, x1 - x0 + 2. * reach, style.width)
                        } else {
                            Rect::new(x0 - half, y0 - reach, style.width, y1 - y0 + 2. * reach)
                        };
                        if rect.width <= 0. || rect.height <= 0. {
                            continue;
                        }
                        let radius = if style.cap == LineCap::Round {
                            shown_half
                        } else {
                            0.
                        };
                        out.push(fill(at(rect), radius, *color));
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    Some(out)
}
/// `style`'s border alone at `bounds`: a ring of its side widths (top,
/// right, bottom, left) inside its rounded box (`draw.wgsl`'s ring mode,
/// two widths a lane in 16ths of a pixel, up to 255 px).
fn ring_quad(style: &zgx::scene::QuadStyle, bounds: Rect, opacity: f32, edge_fade: f32) -> Quad {
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
    style: &zgx::scene::QuadStyle,
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
/// A shader fill's pipeline, or the image effects': quads blended over the
/// target, `module`'s `entry` shading them (`zgx_fill`, see `fill_source`;
/// `zgx_image_effect`, see `image_effect_source`).
fn fill_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &str,
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
            entry_point: Some(entry),
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
/// A blended layer's pipeline (`prepare_blend`): its quad replaces the
/// target's pixels with the blend of the layer and the backdrop it read.
fn blend_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &str,
) -> wgpu::RenderPipeline {
    let all = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blended layer"),
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
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
/// An image's quad drawn with its effect (image_effects.wgsl): the image's
/// lanes, its effect's size in the texture lane (premultiplied, `2 + size`),
/// characters theirs in `options.w` (-1: its positive values are per-corner
/// radii) and their reveal (their node's shader parameters) in the colour,
/// white for any image, and `border.zw`: no lane more for any quad.
#[cold]
#[inline(never)]
fn image_effect_quad(
    image: &zgx::image::ImageData,
    transform: zgx::affine::Affine,
    opacity: f32,
    edge_fade: f32,
    fade: [f32; 4],
    reveal: [[f32; 4]; 4],
) -> Quad {
    let transformed = image.transform() != zgx::affine::Affine::IDENTITY;
    let characters = matches!(image.effect(), zgx::image::ImageEffect::Characters(_));
    let ([front, band, ragged, _], [dx, dy, _, _]) = match characters {
        true => (reveal[0], reveal[1]),
        false => ([1.; 4], [0.; 4]),
    };
    Quad {
        rect: fade,
        uv: [0., 0., 1., 1.],
        color: [front, band, ragged, opacity],
        fade,
        options: [
            edge_fade,
            2. + image.effect().size(),
            if transformed { 1. } else { 0. },
            if characters { -1. } else { 0. },
        ],
        shape: match transformed {
            true => [transform.a, transform.c, transform.b, transform.d],
            false => [0.; 4],
        },
        border: match transformed {
            true => [transform.tx, transform.ty, dx, dy],
            false => [0., 0., dx, dy],
        },
        mask: NO_MASK,
    }
}
/// The image effects' module: the quad shader (draw.wgsl), whose vertex
/// stage, clip regions and shading they share, and image_effects.wgsl.
pub(crate) fn image_effect_source() -> String {
    format!(
        "{}\n{}",
        include_str!("draw.wgsl"),
        include_str!("image_effects.wgsl")
    )
}
/// The polyline strokes' module: the quad shader (draw.wgsl), whose vertex
/// stage and clip regions they share, and polyline.wgsl.
fn polyline_source() -> String {
    format!(
        "{}\n{}",
        include_str!("draw.wgsl"),
        include_str!("polyline.wgsl")
    )
}
/// The polyline strokes' pipeline: into the retained target, with
/// `drawable` also into that in the same pass (`fs_polyline_both`), or into
/// it `alone` (`Deferred::direct`).
fn polyline_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    drawable: Option<wgpu::TextureFormat>,
    alone: bool,
) -> wgpu::RenderPipeline {
    let all = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
    let target = |format| {
        Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })
    };
    let targets = match (drawable, alone) {
        (Some(format), true) => vec![target(format)],
        (Some(format), false) => vec![target(FORMAT), target(format)],
        (None, _) => vec![target(FORMAT)],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("polylines"),
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
            entry_point: Some(if targets.len() == 2 {
                "fs_polyline_both"
            } else {
                "fs_polyline"
            }),
            compilation_options: Default::default(),
            targets: &targets,
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
    if panel_whole(&quad, split) {
        quads.push(quad);
        return;
    }
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
    if inner_width <= 0. || inner_height <= 0. || inner_width * inner_height < PANEL_INTERIOR {
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
/// Whether `push_panel` draws `quad` whole, known before its bands are
/// worked out: the interior lies inside the box, so a box under
/// `PANEL_INTERIOR` has none large enough.
#[inline(always)]
fn panel_whole(quad: &Quad, split: bool) -> bool {
    !split
        || quad.color[3] < 1.
        || quad.options[..3] != [0.; 3]
        || quad.shape[2] > 0.
        || quad.shape[3] <= 0.5
        || quad.rect != quad.fade
        || quad.rect[2] * quad.rect[3] < PANEL_INTERIOR
}
/// Nodes `shaped_like` remembers at most by what they show: past it, it
/// starts again.
const SHAPED_LIKE: usize = 4096;

/// Instances between draws for a region (of draws outside it, or empty
/// spare room) past which a batch from the kept frame ends rather than
/// spanning them (`batches`): holding the window's every draw, a batch
/// would otherwise take blocks of rows the damage misses. (A frame drawn
/// as usual holds what its damage reaches alone.)
const SKIPPED_SPLIT: u32 = 32;
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
// Out of line: inlined, it grew `merge_damage`'s other paths (scroll's).
#[inline(never)]
fn cluster_damage(damage: &[Rect], viewport: Rect) -> Vec<Rect> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    // A grid's groups, or `None` if it leaves too many (counted as they
    // join, so a grid the search discards makes no bounds).
    let groups = |cell: f32| -> Option<Vec<Rect>> {
        let columns = (viewport.width / cell).ceil().max(1.) as usize;
        let rows = (viewport.height / cell).ceil().max(1.) as usize;
        let mut owner = vec![usize::MAX; columns * rows];
        let mut parent: Vec<usize> = (0..damage.len()).collect();
        let mut count = damage.len();
        for (i, r) in damage.iter().enumerate() {
            // The root of `i`'s group: `i` until a cell joins it to another,
            // then that one's root (each join keeps it a root).
            let mut group = i;
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
                        let other = root(&mut parent, *slot);
                        count -= usize::from(group != other);
                        parent[group] = other;
                        group = other;
                    }
                }
            }
        }
        if count > MAX_REGIONS && cell < viewport.width.max(viewport.height) {
            return None;
        }
        let mut bounds: Vec<Option<Rect>> = vec![None; damage.len()];
        for (i, r) in damage.iter().enumerate() {
            let group = &mut bounds[root(&mut parent, i)];
            *group = Some(group.map_or(*r, |g| g.union(*r)));
        }
        Some(bounds.into_iter().flatten().collect())
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
    if found.is_some() {
        while cell > 16. {
            let finer = groups(cell / 2.);
            if finer.is_none() {
                break;
            }
            cell /= 2.;
            found = finer;
        }
    } else {
        loop {
            cell *= 2.;
            found = groups(cell);
            if found.is_some() {
                break;
            }
        }
    }
    LAST.set(cell);
    // A cell as large as the viewport always fits.
    found.expect("a grid fits")
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
    /// The font system it shapes and measures text with (a renderer's
    /// `text_system`): register bundled fonts here.
    pub fn text_system(&self) -> Rc<RefCell<FontSystem>> {
        self.fonts.clone()
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
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
        crate::measure_text(&mut fonts.borrow_mut(), text, size, max)
    });
    let fonts = shared_fonts.clone();
    scene.set_font_text_shaper(
        move |text: &str,
              size: f32,
              width: Option<f32>,
              font: &zgx::text_layout::FontStyle|
              -> Box<dyn zgx::text_layout::TextLayout> {
            let _span = zgx::profile::nested(zgx::profile::Phase::Text);
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
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
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
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
        cache
            .borrow_mut()
            .measure(&mut fonts.borrow_mut(), rich, width)
    });
    // Plain text nodes, from the same prepared text.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_font_text_measurer(move |text, size, width, font| {
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
        cache
            .borrow_mut()
            .measure_plain(&mut fonts.borrow_mut(), text, size, font, width)
    });
    // Plain text's min-content size, from the words it was shaped into.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_font_min_content_measurer(move |text, size, font| {
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
        cache
            .borrow_mut()
            .min_content_plain(&mut fonts.borrow_mut(), text, size, font)
    });
    // Rich text's, likewise.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_rich_min_content_measurer(move |rich| {
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
        cache
            .borrow_mut()
            .min_content(&mut fonts.borrow_mut(), rich)
    });
    // Apps measuring what they have not mounted keep no glyphs for it.
    let (fonts, cache) = (shared_fonts.clone(), shared_cache.clone());
    scene.set_detached_rich_text_measurer(move |rich, width| {
        let _span = zgx::profile::nested(zgx::profile::Phase::Text);
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
    wgpu::PipelineLayout,
) {
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
    (group_layout, viewport_layout, layout)
}

impl surface::SurfaceSource for GpuRenderer {
    type Frame = wgpu::SurfaceTexture;
    fn acquire(&mut self) -> Result<surface::Acquisition<Self::Frame>, GpuError> {
        use surface::Acquisition;
        #[cfg(windows)]
        self.match_background()?;
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
    use zgx::scene::{Color, Layout, Style};

    /// Images keep their channels whichever way texels reach the GPU: the
    /// adapter's own (an upload pass where CPU and GPU share memory) and the
    /// queue copy every other adapter takes, forced here.
    #[test]
    fn images_keep_red_and_blue_through_either_upload_path() {
        let image = Arc::new(
            zgx::image::ImageData::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap(),
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
/// wgpu's instance flags for the build, as zgx sets them: no indirect-call
/// validation (zgx draws nothing indirect; it would compile a compute shader
/// while creating the device, a few milliseconds of startup), and in release
/// builds no names for the driver. wgpu names every command list and object
/// it makes otherwise, several a frame (Direct3D 12's `SetName`: 16 K cycles
/// a frame of `animation_workload` on Windows). `WGPU_DISCARD_HAL_LABELS=0`
/// brings them back for a capture (`with_env`, applied after).
fn instance_flags(build: wgpu::InstanceFlags, release: bool) -> wgpu::InstanceFlags {
    let mut flags = build - wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL;
    flags.set(wgpu::InstanceFlags::DISCARD_HAL_LABELS, release);
    flags
}

#[cfg(test)]
mod instance_flags_tests {
    use super::instance_flags;
    use wgpu::InstanceFlags as F;

    #[test]
    fn release_builds_give_the_driver_no_names() {
        for build in [F::from_build_config(), F::debugging(), F::empty()] {
            let release = instance_flags(build, true);
            assert!(release.contains(F::DISCARD_HAL_LABELS));
            assert!(!release.contains(F::VALIDATION_INDIRECT_CALL));
            let debug = instance_flags(build, false);
            assert!(!debug.contains(F::DISCARD_HAL_LABELS));
            assert!(!debug.contains(F::VALIDATION_INDIRECT_CALL));
            // Everything else is wgpu's for the build.
            let rest = !(F::DISCARD_HAL_LABELS | F::VALIDATION_INDIRECT_CALL);
            assert_eq!(release & rest, build & rest);
        }
    }
}

#[cfg(test)]
mod keep_room_tests {
    use super::keep_room;

    #[test]
    fn a_spike_of_quads_is_given_back_and_steady_frames_keep_their_room() {
        let mut quads: Vec<[u8; 128]> = Vec::new();
        let mut peak = 0;
        let frame = |quads: &mut Vec<[u8; 128]>, peak: &mut usize, used: usize| {
            quads.clear();
            quads.resize(used, [0; 128]);
            keep_room(quads, peak);
        };
        // Steady frames, a little above and below each other: no shrinking.
        for i in 0..200 {
            frame(&mut quads, &mut peak, 3000 + (i % 7) * 100);
        }
        let steady = quads.capacity();
        for i in 0..200 {
            frame(&mut quads, &mut peak, 3000 + (i % 7) * 100);
            assert_eq!(quads.capacity(), steady, "steady frames keep their room");
        }
        // One frame of a long page: the room grows, then goes back.
        frame(&mut quads, &mut peak, 60_000);
        assert!(quads.capacity() >= 60_000);
        for _ in 0..300 {
            frame(&mut quads, &mut peak, 3000);
        }
        assert!(
            quads.capacity() <= 2 * 3000,
            "a spike's room is given back: {}",
            quads.capacity()
        );
        // Small vectors are left alone.
        let mut small: Vec<[u8; 128]> = Vec::with_capacity(400);
        let mut small_peak = 0;
        for _ in 0..200 {
            small.clear();
            small.push([0; 128]);
            keep_room(&mut small, &mut small_peak);
        }
        assert_eq!(small.capacity(), 400);
    }
}

#[cfg(test)]
mod copy_blocker_tests {
    use super::*;
    use zgx::{
        compose::prelude::{rgba, *},
        widgets::Ui,
    };

    /// A node painting over a scroll view keeps its scroll from being a copy
    /// of the pixels drawn, and devtools' note names it; with nothing over
    /// it, the same scroll copies.
    #[test]
    fn a_node_over_a_scroll_view_is_named_as_what_keeps_it_from_copying() {
        for covered in [false, true] {
            let mut ui = Ui::new(160., 120.);
            let offset = ui.signal(0_f32);
            let rows = virtual_list(
                offset.clone(),
                24.,
                2,
                || 1000,
                |i| i,
                |_, key, _| {
                    div().size(144., 24.).bg(if key % 2 == 0 {
                        rgb(0x223344)
                    } else {
                        rgb(0x334455)
                    })
                },
            )
            .w(160.)
            .h(100.)
            .p(8.)
            .bg(rgb(0x101820));
            let mut root = overlay().w(160.).h(120.).bg(rgb(0x000000)).child(rows);
            if covered {
                root = root.child(
                    div()
                        .absolute()
                        .left(40.)
                        .top(40.)
                        .size(30., 30.)
                        .bg(rgba(0xffffff80)),
                );
            }
            ui.mount(root);
            ui.prepare_frame();
            ui.scene.borrow_mut().flush();
            offset.set(14.);
            ui.prepare_frame();
            let damage = ui.scene.borrow_mut().flush().damage;
            let scene = ui.scene.borrow();
            let scroll = scene.scroll_moves().last().expect("it moved").clone();
            let blocker = copy_blocker(&scene, &scroll, &damage, 1.);
            if covered {
                let Some(blocker @ CopyBlocker::Over(_)) = blocker else {
                    panic!("the square over the list keeps it from copying: {blocker:?}");
                };
                let said = blocker.describe(&scene);
                assert!(
                    said.contains("draws over it") && said.starts_with('@'),
                    "{said}"
                );
            } else {
                assert_eq!(blocker, None);
            }
        }
    }
}

#[cfg(test)]
mod kept_index_tests {
    use super::*;

    fn scene_ids(count: usize) -> (Scene, Vec<NodeId>, zgx::scene::Inherited) {
        let mut scene = Scene::new(100., 100.);
        let mut ids = vec![scene.root()];
        for _ in 1..count {
            ids.push(scene.append(
                scene.root(),
                NodeKind::Rect(Color(20, 30, 40, 255)),
                zgx::scene::Style::default(),
            ));
        }
        scene.flush();
        let mut inherited = Vec::new();
        scene.layer_items_recorded(&mut inherited);
        (scene, ids, inherited.remove(0))
    }

    fn items(ids: &[NodeId], inherited: &zgx::scene::Inherited) -> Vec<KeptItem> {
        ids.iter()
            .enumerate()
            .map(|(at, &id)| KeptItem {
                id,
                inherited: inherited.clone(),
                start: 0,
                len: 0,
                room: 0,
                draws: 0..0,
                boxed: None,
                end: (ids[at..].iter().rposition(|&other| other == id).unwrap() + at + 1) as u32,
            })
            .collect()
    }

    fn agrees(index: &KeptIndex, kept: &[KeptItem], ids: &[NodeId]) {
        for &id in ids {
            assert_eq!(
                index.get(id, kept),
                kept.iter().position(|item| item.id == id)
            );
        }
    }

    #[test]
    fn subtree_ends_count_unique_nodes_through_nested_border_passes() {
        let (mut scene, ids, _) = scene_ids(2);
        let quad = zgx::scene::QuadStyle {
            border_over: true,
            border_width: 1.,
            border_color: Color(255, 255, 255, 255),
            ..Default::default()
        };
        scene.set_kind(ids[1], NodeKind::Quad(quad.clone()));
        let style = zgx::scene::Style {
            width: Some(80.),
            height: Some(80.),
            ..Default::default()
        };
        scene.set_style(ids[1], style.clone());
        let child = scene.append(ids[1], NodeKind::Quad(quad), style.clone());
        scene.append(child, NodeKind::Rect(Color(1, 2, 3, 255)), style);
        scene.flush();
        let mut inherited = Vec::new();
        let painted = scene.layer_items_recorded(&mut inherited);
        let expected = painted.iter().filter(|item| !item.border_pass).count();
        let ids: Vec<_> = painted.iter().map(|item| item.id).collect();
        let mut kept = items(&ids, &inherited[0]);
        subtree_ends(&scene, &mut kept, 0);
        assert_eq!(KeptIndex::nodes(&kept), expected);
        assert!(
            kept.len() > expected,
            "the test must include border duplicates"
        );
        let mut sparse = FxHashMap::default();
        sparse.reserve(KeptIndex::nodes(&kept));
        let room = sparse.capacity();
        KeptIndex::fill(&mut sparse, &kept);
        assert_eq!(
            sparse.capacity(),
            room,
            "fallback fills without another allocation"
        );
    }

    #[test]
    fn dense_index_rejects_recycled_generations_and_keeps_the_first_border_item() {
        let (mut scene, ids, inherited) = scene_ids(3);
        let root = ids[0];
        let mut live = ids[1];
        let spare = ids[2];
        let mut kept = items(&[root, live, spare, live], &inherited);
        let mut index = KeptIndex::new(&kept);
        assert!(matches!(index, KeptIndex::Dense(_)));
        let mut history = ids.clone();
        for _ in 0..32 {
            let old = live;
            scene.remove(old);
            live = scene.append(
                root,
                NodeKind::Rect(Color(0, 0, 0, 255)),
                zgx::scene::Style::default(),
            );
            assert_eq!(live.number(), old.number());
            assert_ne!(live, old);
            kept = items(&[root, live, spare, live], &inherited);
            index.patch(&kept, false, vec![old], &[(1, 4, root)]);
            history.push(live);
            agrees(&index, &kept, &history);
            assert_eq!(index.get(live, &kept), Some(1));
        }
    }

    #[test]
    fn stale_dense_offsets_are_checked_after_removal_reorder_and_growth() {
        let (_, ids, inherited) = scene_ids(10);
        let mut kept = items(&ids[..6], &inherited);
        let mut index = KeptIndex::new(&kept);
        for order in [vec![0, 2, 4], vec![4, 0, 2, 6, 8, 9], vec![0], vec![]] {
            let selected: Vec<_> = order.into_iter().map(|at| ids[at]).collect();
            kept = items(&selected, &inherited);
            index.patch(&kept, true, Vec::new(), &[]);
            agrees(&index, &kept, &ids);
        }
    }

    #[test]
    fn sparse_slots_fall_back_and_keep_generation_and_duplicate_semantics() {
        let (_, ids, inherited) = scene_ids(100);
        let small = items(&ids[..4], &inherited);
        let mut index = KeptIndex::new(&small);
        assert!(matches!(index, KeptIndex::Dense(_)));
        let far = items(&[ids[0], ids[99], ids[99]], &inherited);
        index.patch(&far, true, Vec::new(), &[]);
        assert!(matches!(index, KeptIndex::Sparse(_)));
        agrees(&index, &far, &ids);
        let direct = KeptIndex::new(&far);
        assert!(matches!(direct, KeptIndex::Sparse(_)));
        agrees(&direct, &far, &ids);
        let next = items(&[ids[0], ids[98], ids[98]], &inherited);
        index.patch(&next, false, vec![ids[99]], &[(1, 3, ids[0])]);
        agrees(&index, &next, &ids);
        assert_eq!(
            std::mem::size_of::<KeptIndex>(),
            std::mem::size_of::<FxHashMap<NodeId, u32>>()
        );
    }
}
