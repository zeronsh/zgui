//! Frames encoded with Metal directly (macOS, presenting through the layer).
//!
//! wgpu-core validates and tracks every render pass command, records it,
//! then replays it into Metal, and its submission and wait add bookkeeping
//! of their own: about half of an animating window's presentation CPU, for
//! a frame of a handful of draws. The common frame (quads and text sampling
//! the atlas, images and cached layers, no filters) needs none of it: the same pipelines, built here
//! from the same WGSL through naga, record straight into a command buffer on
//! wgpu's own queue, so ordering against wgpu's submissions holds. Backdrop
//! filters mixed by dual-source blending are drawn here too, their blur
//! passes splitting the frame's pass, and so are layer effects' passes
//! (effects.rs), ahead of it. Frames needing anything else take the wgpu
//! path.
use crate::{GpuError, Shading};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCompileOptions,
    MTLDevice, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLScissorRect, MTLStoreAction, MTLTexture, MTLVertexDescriptor,
    MTLVertexFormat, MTLVertexStepFunction,
};
use wgpu::naga;

type Pipeline = Retained<ProtocolObject<dyn MTLRenderPipelineState>>;
pub(crate) type Texture = Retained<ProtocolObject<dyn MTLTexture>>;
pub(crate) type CommandBuffer = Retained<ProtocolObject<dyn MTLCommandBuffer>>;
type Encoder = Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>;

/// The vertex buffer's slot; slot 0 holds the viewport.
const VERTICES: usize = 1;
/// The quads' clip regions (`crate::Region`) for their fragments, and in the
/// next slot their byte length (naga reads runtime-sized arrays' lengths
/// from there).
const REGIONS: usize = 2;
const REGION_SIZES: usize = 3;
/// Polyline strokes' points (polyline.wgsl), in the vertex buffer after the
/// quads; their byte length follows the regions' in `REGION_SIZES`.
const POINTS: usize = 4;

pub(crate) struct MetalFrame {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    full: Pipeline,
    full_clear: Pipeline,
    basic: Pipeline,
    opaque: Pipeline,
    copy_offset: Pipeline,
    /// Compiled with the first filter (None: not here).
    blur: std::cell::OnceCell<Option<Blur>>,
    /// Layer effects' pipelines (effects.wgsl), with the first effect.
    effects: std::cell::OnceCell<Option<Effects>>,
    /// Shader fills' pipelines by shader id (`None`: it did not compile).
    fills: std::cell::RefCell<std::collections::HashMap<u64, Option<Pipeline>>>,
    /// The image effects' pipeline (image_effects.wgsl), with the first
    /// image drawn with one (`None`: it did not compile).
    image_effects: std::cell::OnceCell<Option<Pipeline>>,
    /// Polyline strokes' library and pipelines, compiled with the first
    /// (None: not here).
    polyline: std::cell::OnceCell<Option<Polylines>>,
    /// The 1×1 texture `warm` clears, made with the first (`None`: Metal
    /// made none).
    warm: std::cell::OnceCell<Option<Texture>>,
}

/// Polyline strokes' pipeline (polyline.wgsl).
struct Polylines {
    plain: Pipeline,
}

/// effects.wgsl's passes: across into the scratch atlas, down into the
/// effect atlas.
struct Effects {
    across: Pipeline,
    down: Pipeline,
}

/// A pass of layer effects (`effects.rs`) into `target`: the effect atlas
/// (`down`, its other slots kept) or the scratch atlas.
pub(crate) struct EffectPass {
    pub(crate) target: Texture,
    pub(crate) down: bool,
    pub(crate) draws: Vec<EffectDraw>,
}

/// One job's quad in an effect pass: the frame's vertex buffer instance, the
/// texture it reads and its mask (a blank one where it has none).
pub(crate) struct EffectDraw {
    pub(crate) source: Texture,
    pub(crate) mask: Texture,
    pub(crate) instance: u32,
}

/// The backdrop blur pipelines (blur.wgsl), as `BlurPipelines`' mixed ones.
struct Blur {
    reduce: Pipeline,
    across: Pipeline,
    down: Pipeline,
    mix: Pipeline,
}

/// The blur's slots: its source in texture 1 (2 holds the area's original
/// pixels where copied), its filters and slots in buffers 2 and 3, and those
/// two records' lengths in buffer 4 (naga reads runtime-sized arrays'
/// lengths from there).
const BLUR_SOURCE: usize = 1;
const BLUR_RECORDS: usize = 2;
const BLUR_SIZES: usize = 4;

/// A batch of filters' records (`BlurRecord`s and `SlotRecord`s, as bytes).
pub(crate) struct BlurRecords<'a> {
    pub(crate) blurs: &'a [u8],
    pub(crate) slots: &'a [u8],
}

impl MetalFrame {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, GpuError> {
        // SAFETY: The raw device is retained, never destroyed; the guard is
        // dropped at once.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }
            .ok_or_else(|| GpuError("not a Metal device".into()))?;
        let raw = hal.raw_device().clone();
        drop(hal);
        // SAFETY: The raw queue is retained, never destroyed. Command buffers
        // committed to it (`Frame::commit`) retain what they reference
        // (`commandBuffer`, not the unretained kind), so a wgpu resource
        // dropped meanwhile stays alive for them.
        let hal_queue = unsafe { queue.as_hal::<wgpu::hal::api::Metal>() }
            .ok_or_else(|| GpuError("not a Metal queue".into()))?;
        let queue = {
            use objc2::Message;
            hal_queue.as_raw().retain()
        };
        drop(hal_queue);
        let draw = Library::translated(&raw, DRAW_MSL, Kind::Quads)?;
        let copy = Library::translated(&raw, COPY_MSL, Kind::Copies)?;
        let pipeline = |library: &Library, vertex: &str, fragment: &str, blend: Blend| {
            library.pipeline(&raw, vertex, fragment, MTLPixelFormat::BGRA8Unorm, blend)
        };
        Ok(Self {
            full: pipeline(&draw, "vs", "fs", Blend::Over)?,
            full_clear: pipeline(&draw, "vs", "fs", Blend::Replace)?,
            basic: pipeline(&draw, "vs_basic", "fs_basic", Blend::Over)?,
            opaque: pipeline(&draw, "vs_basic", "fs_basic", Blend::Replace)?,
            copy_offset: pipeline(&copy, "vs", "fs", Blend::Replace)?,
            device: raw,
            queue,
            blur: std::cell::OnceCell::new(),
            effects: std::cell::OnceCell::new(),
            fills: Default::default(),
            image_effects: std::cell::OnceCell::new(),
            polyline: std::cell::OnceCell::new(),
            warm: std::cell::OnceCell::new(),
        })
    }

    /// Wake the GPU: a pass clearing a 1×1 private texture, committed without
    /// waiting (an empty command buffer may wake nothing). Nothing if Metal
    /// refuses any of it (a lost device): the frame that follows reports that.
    pub(crate) fn warm(&self) {
        let target = self.warm.get_or_init(|| {
            use objc2_metal::{MTLStorageMode, MTLTextureDescriptor, MTLTextureUsage};
            // SAFETY: A plain 2D texture descriptor of one texel.
            let descriptor = unsafe {
                MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                    MTLPixelFormat::BGRA8Unorm,
                    1,
                    1,
                    false,
                )
            };
            descriptor.setUsage(MTLTextureUsage::RenderTarget);
            descriptor.setStorageMode(MTLStorageMode::Private);
            self.device.newTextureWithDescriptor(&descriptor)
        });
        let (Some(target), Some(buffer)) = (target, self.queue.commandBuffer()) else {
            return;
        };
        let pass = MTLRenderPassDescriptor::new();
        // SAFETY: Index 0 of a render pass descriptor's colour attachments.
        let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        color.setTexture(Some(target));
        color.setLoadAction(MTLLoadAction::Clear);
        color.setStoreAction(MTLStoreAction::Store);
        let Some(encoder) = buffer.renderCommandEncoderWithDescriptor(&pass) else {
            return;
        };
        encoder.endEncoding();
        buffer.commit();
    }

    /// Whether polyline strokes can be drawn here: their pipeline, compiled
    /// now from the same module as wgpu's (`crate::polyline_source`) if new.
    pub(crate) fn polylines(&self) -> bool {
        self.polyline_pipelines().is_some()
    }

    fn polyline_pipelines(&self) -> Option<&Polylines> {
        self.polyline
            .get_or_init(|| {
                let library =
                    Library::new(&self.device, &crate::polyline_source(), Kind::Quads).ok()?;
                let format = MTLPixelFormat::BGRA8Unorm;
                let plain = library
                    .pipeline(&self.device, "vs", "fs_polyline", format, Blend::Over)
                    .ok()?;
                Some(Polylines { plain })
            })
            .as_ref()
    }

    /// Whether `shader`'s fill can be drawn here: its pipeline, compiled
    /// now from the same module as wgpu's (`crate::fill_source`) if new.
    pub(crate) fn fills(&self, shader: &zgx::decoration::FillShader) -> bool {
        if let Some(compiled) = self.fills.borrow().get(&shader.id()) {
            return compiled.is_some();
        }
        let source = crate::fill_source(shader.source());
        let pipeline = crate::fill_validates(&source)
            .then(|| Library::new(&self.device, &source, Kind::Quads).ok())
            .flatten()
            .and_then(|library| {
                library
                    .pipeline(
                        &self.device,
                        "vs",
                        "zgx_fill",
                        MTLPixelFormat::BGRA8Unorm,
                        Blend::Over,
                    )
                    .ok()
            });
        let ready = pipeline.is_some();
        self.fills.borrow_mut().insert(shader.id(), pipeline);
        ready
    }

    /// Whether images can be drawn with effects here: their pipeline,
    /// compiled now from the same module as wgpu's
    /// (`crate::image_effect_source`) if new.
    pub(crate) fn image_effects(&self) -> bool {
        self.image_effects
            .get_or_init(|| {
                Library::new(&self.device, &crate::image_effect_source(), Kind::Quads)
                    .ok()?
                    .pipeline(
                        &self.device,
                        "vs",
                        "zgx_image_effect",
                        MTLPixelFormat::BGRA8Unorm,
                        Blend::Over,
                    )
                    .ok()
            })
            .is_some()
    }

    /// Whether layer effects can be drawn here: their pipelines, compiled
    /// now if they are not yet.
    pub(crate) fn effects(&self) -> bool {
        self.effect_pipelines().is_some()
    }

    fn effect_pipelines(&self) -> Option<&Effects> {
        self.effects
            .get_or_init(|| {
                let library =
                    Library::new(&self.device, include_str!("effects.wgsl"), Kind::Effects).ok()?;
                let pipeline = |fragment: &str| {
                    library
                        .pipeline(
                            &self.device,
                            "vs",
                            fragment,
                            MTLPixelFormat::BGRA8Unorm,
                            Blend::Replace,
                        )
                        .ok()
                };
                Some(Effects {
                    across: pipeline("fs_across")?,
                    down: pipeline("fs_down")?,
                })
            })
            .as_ref()
    }

    /// Whether filters can be drawn here: their pipelines, compiled now if
    /// they are not yet.
    pub(crate) fn blurs(&self) -> bool {
        self.blur().is_some()
    }

    fn blur(&self) -> Option<&Blur> {
        self.blur
            .get_or_init(|| {
                let source = format!(
                    "enable dual_source_blending;\n{}\n{}",
                    include_str!("blur.wgsl"),
                    crate::BLUR_MIX
                );
                let library = Library::new(&self.device, &source, Kind::Blur).ok()?;
                let pipeline = |fragment: &str, format, blend| {
                    library
                        .pipeline(&self.device, "vs", fragment, format, blend)
                        .ok()
                };
                let half = MTLPixelFormat::RGBA16Float;
                Some(Blur {
                    reduce: pipeline("fs_reduce", half, Blend::Replace)?,
                    across: pipeline("fs_across", half, Blend::Replace)?,
                    down: pipeline("fs_down", half, Blend::Replace)?,
                    mix: pipeline("fs_mix", MTLPixelFormat::BGRA8Unorm, Blend::Mix)?,
                })
            })
            .as_ref()
    }

    /// Open a frame drawing into `target`, its pixels kept (loaded), after
    /// `effects`' passes (`effects` checked first) drawing from `vertices`;
    /// quads lie in `regions` (`crate::Region`s, as bytes), and polyline
    /// strokes' `points` (`polylines` checked first) at a byte offset and
    /// length of `vertices`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin(
        &self,
        target: &ProtocolObject<dyn MTLTexture>,
        vertices: &ProtocolObject<dyn objc2_metal::MTLBuffer>,
        viewport: [f32; 4],
        regions: &[u8],
        points: Option<(usize, usize)>,
        effects: &[EffectPass],
    ) -> Frame {
        let buffer = self.queue.commandBuffer().expect("a command buffer");
        for effect in effects {
            let pipelines = self.effect_pipelines().expect("checked by `effects`");
            let load = match effect.down {
                true => MTLLoadAction::Load,
                false => MTLLoadAction::Clear,
            };
            let encoder = pass(&buffer, &effect.target, load);
            encoder.setRenderPipelineState(if effect.down {
                &pipelines.down
            } else {
                &pipelines.across
            });
            // SAFETY: The frame's live vertex buffer, live textures for both
            // of effects.wgsl's slots, and four-vertex strips of instances
            // the buffer holds (`metal_effect_passes` numbers them from the
            // effect quads written after the frame's own).
            unsafe {
                encoder.setVertexBuffer_offset_atIndex(Some(vertices), 0, VERTICES);
                for draw in &effect.draws {
                    encoder.setFragmentTexture_atIndex(Some(&draw.source), 0);
                    encoder.setFragmentTexture_atIndex(Some(&draw.mask), 1);
                    encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                        MTLPrimitiveType::TriangleStrip,
                        0,
                        4,
                        1,
                        draw.instance as usize,
                    );
                }
            }
            encoder.endEncoding();
        }
        let encoder = pass(&buffer, target, MTLLoadAction::Load);
        // Small tables ride inline with each pass; larger ones in a buffer.
        let regions = match regions.len() {
            0 => Regions::Inline(vec![0; size_of::<crate::Region>()]),
            len if len <= 4096 => Regions::Inline(regions.to_vec()),
            len => Regions::Buffer(
                // SAFETY: `len` bytes of `regions`, read by Metal during the
                // call (it copies them into the new buffer).
                unsafe {
                    self.device.newBufferWithBytes_length_options(
                        std::ptr::NonNull::from(regions).cast(),
                        len,
                        objc2_metal::MTLResourceOptions::StorageModeShared,
                    )
                }
                .expect("a regions buffer"),
                len,
            ),
        };
        let mut frame = Frame {
            buffer,
            encoder,
            current: None,
            scissor: None,
            vertices: {
                use objc2::Message;
                vertices.retain()
            },
            viewport,
            regions,
            points,
        };
        frame.quad_state();
        frame
    }

    fn pipeline(&self, shading: Shading) -> Pipeline {
        match shading {
            Shading::Full => self.full.clone(),
            Shading::Basic => self.basic.clone(),
            Shading::Opaque => self.opaque.clone(),
            Shading::Polyline => self
                .polyline_pipelines()
                .expect("checked by `polylines`")
                .plain
                .clone(),
            Shading::Fill(id) => self
                .fills
                .borrow()
                .get(&id)
                .cloned()
                .flatten()
                .unwrap_or_else(|| self.full.clone()),
            Shading::ImageEffect => self
                .image_effects
                .get()
                .cloned()
                .flatten()
                .unwrap_or_else(|| self.full.clone()),
        }
    }
}

/// What a frame's encoder draws with now.
#[derive(Clone, Copy, PartialEq)]
enum Bound {
    Quads(Shading),
    FullClear,
    CopyOffset,
}

/// A render pass of `buffer` into `target`, stored.
fn pass(
    buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    target: &ProtocolObject<dyn MTLTexture>,
    load: MTLLoadAction,
) -> Encoder {
    let pass = MTLRenderPassDescriptor::new();
    // SAFETY: Index 0 of a render pass descriptor's colour attachments.
    let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
    color.setTexture(Some(target));
    color.setLoadAction(load);
    color.setStoreAction(MTLStoreAction::Store);
    buffer
        .renderCommandEncoderWithDescriptor(&pass)
        .expect("a render encoder")
}

/// One frame's command buffer and its render pass into the target: one,
/// unless backdrop filters split it.
pub(crate) struct Frame {
    buffer: CommandBuffer,
    encoder: Encoder,
    current: Option<Bound>,
    /// The scissor last set on `encoder`: setting it again costs the
    /// driver as much as a change (most batches share their region's).
    scissor: Option<(u32, u32, u32, u32)>,
    vertices: Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    viewport: [f32; 4],
    regions: Regions,
    points: Option<(usize, usize)>,
}

/// A frame's clip regions, as Metal takes them.
enum Regions {
    Inline(Vec<u8>),
    Buffer(Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>, usize),
}

impl Frame {
    /// The viewport, vertex buffer and clip regions quads draw with.
    fn quad_state(&mut self) {
        let bytes = std::ptr::NonNull::from(&self.viewport).cast();
        // SAFETY: Sixteen bytes of viewport for slot 0 of both stages, the
        // frame's live vertex buffer, and its regions (copied by Metal, or
        // a buffer the encoder retains) with their length.
        unsafe {
            self.encoder
                .setVertexBytes_length_atIndex(bytes, size_of_val(&self.viewport), 0);
            self.encoder
                .setFragmentBytes_length_atIndex(bytes, size_of_val(&self.viewport), 0);
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&self.vertices), 0, VERTICES);
            let len = match &self.regions {
                Regions::Inline(bytes) => {
                    let pointer = std::ptr::NonNull::from(&bytes[..]).cast();
                    self.encoder
                        .setFragmentBytes_length_atIndex(pointer, bytes.len(), REGIONS);
                    bytes.len()
                }
                Regions::Buffer(buffer, len) => {
                    self.encoder
                        .setFragmentBuffer_offset_atIndex(Some(buffer), 0, REGIONS);
                    *len
                }
            };
            let sizes = [len as u32, self.points.map_or(0, |(_, len)| len as u32)];
            let sizes = match self.points {
                Some((offset, _)) => {
                    self.encoder.setFragmentBuffer_offset_atIndex(
                        Some(&self.vertices),
                        offset,
                        POINTS,
                    );
                    &sizes[..]
                }
                None => &sizes[..1],
            };
            self.encoder.setFragmentBytes_length_atIndex(
                std::ptr::NonNull::from(sizes).cast(),
                size_of_val(sizes),
                REGION_SIZES,
            );
        }
    }
    /// Bind a batch's records for the blur's fragments: as bytes where Metal
    /// takes them inline, else in a buffer.
    fn records(&self, frame: &MetalFrame, records: &BlurRecords<'_>) {
        let sizes = [records.blurs.len() as u32, records.slots.len() as u32];
        // SAFETY: Eight bytes of lengths, copied by Metal.
        unsafe {
            self.encoder.setFragmentBytes_length_atIndex(
                std::ptr::NonNull::from(&sizes).cast(),
                size_of_val(&sizes),
                BLUR_SIZES,
            );
        }
        for (index, bytes) in [records.blurs, records.slots].into_iter().enumerate() {
            let slot = BLUR_RECORDS + index;
            let pointer = std::ptr::NonNull::from(bytes).cast();
            // SAFETY: The bytes outlive the call (Metal copies them), or a
            // buffer made from them is retained by the encoder.
            unsafe {
                if bytes.len() <= 4096 {
                    self.encoder
                        .setFragmentBytes_length_atIndex(pointer, bytes.len(), slot);
                } else {
                    let buffer = frame
                        .device
                        .newBufferWithBytes_length_options(
                            pointer,
                            bytes.len(),
                            objc2_metal::MTLResourceOptions::StorageModeShared,
                        )
                        .expect("a records buffer");
                    self.encoder
                        .setFragmentBuffer_offset_atIndex(Some(&buffer), 0, slot);
                }
            }
        }
    }
    /// A batch of backdrop filters' blur passes (`GpuRenderer::plan_blur`):
    /// `target` averaged down into `atlas[0]`, blurred across into
    /// `atlas[1]` and down into `atlas[0]`, each slot (atlas x, y, width,
    /// height) drawn in each. The pass into the target ends; `resume` opens
    /// the next.
    pub(crate) fn blur(
        &mut self,
        frame: &MetalFrame,
        target: &ProtocolObject<dyn MTLTexture>,
        atlas: [&ProtocolObject<dyn MTLTexture>; 2],
        records: &BlurRecords<'_>,
        slots: &[(u32, u32, u32, u32)],
    ) {
        let blur = frame.blur().expect("checked by `blurs`");
        let passes = [
            (&blur.reduce, target, atlas[0]),
            (&blur.across, atlas[0], atlas[1]),
            (&blur.down, atlas[1], atlas[0]),
        ];
        for (pipeline, source, into) in passes {
            self.encoder.endEncoding();
            self.encoder = pass(&self.buffer, into, MTLLoadAction::Clear);
            self.encoder.setRenderPipelineState(pipeline);
            // SAFETY: Live textures for the blur's source slot.
            unsafe {
                self.encoder
                    .setFragmentTexture_atIndex(Some(source), BLUR_SOURCE);
            }
            self.records(frame, records);
            for (index, &(x, y, width, height)) in slots.iter().enumerate() {
                if width == 0 || height == 0 {
                    continue;
                }
                self.scissor((x, y, width, height));
                self.triangle(index as u32);
            }
        }
        self.current = None;
    }
    /// Open the next pass into `target` after a batch's blur passes.
    pub(crate) fn resume(&mut self, target: &ProtocolObject<dyn MTLTexture>) {
        self.encoder.endEncoding();
        self.encoder = pass(&self.buffer, target, MTLLoadAction::Load);
        self.current = None;
        self.scissor = None;
        self.quad_state();
    }
    /// Draw a batch of filters back over the target, mixed by dual-source
    /// blending: per filter (its index in the batch) and output.
    pub(crate) fn mixes(
        &mut self,
        frame: &MetalFrame,
        atlas: &ProtocolObject<dyn MTLTexture>,
        records: &BlurRecords<'_>,
        draws: &[(u32, crate::Scissor)],
    ) {
        let blur = frame.blur().expect("checked by `blurs`");
        self.encoder.setRenderPipelineState(&blur.mix);
        self.current = None;
        // SAFETY: A live texture for the blur's source slot.
        unsafe {
            self.encoder
                .setFragmentTexture_atIndex(Some(atlas), BLUR_SOURCE);
        }
        self.records(frame, records);
        for &(index, scissor) in draws {
            self.scissor(scissor);
            self.triangle(index);
        }
        // The records took the regions' slots: quads drawn after the blur
        // (shader fills' uniforms, rounded clips, fades) read their own again.
        self.quad_state();
    }
    /// A full-target triangle as instance `index` (blur.wgsl's `vs`).
    fn triangle(&mut self, index: u32) {
        // SAFETY: Three vertices made from their indices, no buffers; the
        // instance indexes the batch's records bound by `records` (callers
        // pass a slot's or draw's index in that batch).
        unsafe {
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    MTLPrimitiveType::Triangle,
                    0,
                    3,
                    1,
                    index as usize,
                )
        };
    }
    fn bind(&mut self, frame: &MetalFrame, bound: Bound) {
        if self.current == Some(bound) {
            return;
        }
        self.current = Some(bound);
        let pipeline = match bound {
            Bound::Quads(shading) => frame.pipeline(shading),
            Bound::FullClear => frame.full_clear.clone(),
            Bound::CopyOffset => frame.copy_offset.clone(),
        };
        self.encoder.setRenderPipelineState(&pipeline);
    }
    /// The texture fragments sample: the atlas, or for catch-up copies the
    /// shown surface.
    pub(crate) fn texture(&mut self, texture: &ProtocolObject<dyn MTLTexture>) {
        // SAFETY: A live texture for slot 0, as both libraries bind it (the
        // copy's vertex stage reads its size).
        unsafe {
            self.encoder.setVertexTexture_atIndex(Some(texture), 0);
            self.encoder.setFragmentTexture_atIndex(Some(texture), 0);
        }
    }
    pub(crate) fn scissor(&mut self, rect: (u32, u32, u32, u32)) {
        if self.scissor == Some(rect) {
            return;
        }
        self.scissor = Some(rect);
        let (x, y, width, height) = rect;
        self.encoder.setScissorRect(MTLScissorRect {
            x: x as usize,
            y: y as usize,
            width: width as usize,
            height: height as usize,
        });
    }
    /// Draw `instances` quads of `shading`; `clear` for the pipeline that
    /// replaces rather than blends (a region's background).
    pub(crate) fn quads(
        &mut self,
        frame: &MetalFrame,
        shading: Shading,
        clear: bool,
        instances: std::ops::Range<u32>,
    ) {
        let bound = match (shading, clear) {
            (Shading::Full | Shading::Fill(_) | Shading::ImageEffect | Shading::Polyline, true) => {
                Bound::FullClear
            }
            (Shading::Basic | Shading::Opaque, true) => Bound::Quads(Shading::Opaque),
            (shading, false) => Bound::Quads(shading),
        };
        self.bind(frame, bound);
        self.draw(instances);
    }
    /// Draw `instances` shifted copies from the texture bound with `texture`.
    pub(crate) fn copies(&mut self, frame: &MetalFrame, instances: std::ops::Range<u32>) {
        self.bind(frame, Bound::CopyOffset);
        self.draw(instances);
    }
    fn draw(&mut self, instances: std::ops::Range<u32>) {
        // SAFETY: Four-vertex strips over instances the vertex buffer holds:
        // `render_direct` draws ranges of the quads it wrote to it.
        unsafe {
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                    instances.len(),
                    instances.start as usize,
                )
        };
    }
    /// End the pass and commit (the frame then runs after everything already
    /// submitted to wgpu's queue), presenting `drawable` once its work
    /// completes: Metal hands it to the window server for the refresh after.
    pub(crate) fn commit_presenting(
        self,
        drawable: Option<&ProtocolObject<dyn objc2_metal::MTLDrawable>>,
        at: Option<f64>,
    ) -> CommandBuffer {
        self.encoder.endEncoding();
        match (drawable, at) {
            (Some(drawable), Some(at)) => self.buffer.presentDrawable_atTime(drawable, at),
            (Some(drawable), None) => self.buffer.presentDrawable(drawable),
            (None, _) => {}
        }
        self.buffer.commit();
        self.buffer
    }
}

/// What a library's shaders draw, which fixes their slots.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Quads: eight attributes, the viewport uniform in buffer 0, the
    /// sampled texture in texture 0, its sampler inline.
    Quads,
    /// Copies: two attributes, the texture in texture 0.
    Copies,
    /// Backdrop blur (see `Blur`): no attributes, its sampler inline.
    Blur,
    /// Layer effects: the quads' attributes, the source in texture 0, the
    /// mask in texture 1, the sampler inline.
    Effects,
}

/// How a pipeline writes its colour.
#[derive(Clone, Copy)]
enum Blend {
    Replace,
    /// Premultiplied alpha, as wgpu's PREMULTIPLIED_ALPHA_BLENDING.
    Over,
    /// The second output's complement keeps the target (`BLUR_MIX`).
    Mix,
}

/// A WGSL module compiled for Metal with zgx's fixed slots (`Kind`), any
/// sampler inline (linear and clamped, as `GpuRenderer::texture_bind`'s).
struct Library {
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
    names: std::collections::HashMap<String, String>,
    kind: Kind,
}

impl Library {
    fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        source: &str,
        kind: Kind,
    ) -> Result<Self, GpuError> {
        let (msl, names) = translate(source, kind)?;
        Self::compile(device, &msl, names, kind)
    }

    /// The library of a module translated ahead (`translated`).
    fn translated(
        device: &ProtocolObject<dyn MTLDevice>,
        translated: &str,
        kind: Kind,
    ) -> Result<Self, GpuError> {
        let names = translated
            .lines()
            .next()
            .and_then(|line| line.strip_prefix(ENTRY_POINTS))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|pair| pair.split_once('='))
            .map(|(entry, name)| (entry.to_owned(), name.to_owned()))
            .collect();
        Self::compile(device, translated, names, kind)
    }

    fn compile(
        device: &ProtocolObject<dyn MTLDevice>,
        msl: &str,
        names: std::collections::HashMap<String, String>,
        kind: Kind,
    ) -> Result<Self, GpuError> {
        let library = device
            .newLibraryWithSource_options_error(
                &NSString::from_str(msl),
                Some(&MTLCompileOptions::new()),
            )
            .map_err(|e| GpuError(format!("direct Metal shaders: {e}")))?;
        Ok(Self {
            library,
            names,
            kind,
        })
    }
}

/// The first line of a module translated ahead: its entry points' names in
/// MSL (`entry=name`, separated by spaces).
const ENTRY_POINTS: &str = "// zgx entry points:";

/// `draw.wgsl` and `copy_offset.wgsl` as `translate` writes them, behind
/// their entry points' names (`ENTRY_POINTS`): every window's first frame
/// needs both, and naga's translation was about 10 M of its instructions.
/// `translated_shaders_match` keeps them current
/// (`ZGX_BLESS=1 cargo test -p zgx-gpu --lib translated_shaders`).
const DRAW_MSL: &str = include_str!("draw.metal");
const COPY_MSL: &str = include_str!("copy_offset.metal");

/// A WGSL module in MSL with zgx's fixed slots (`Kind`) and any sampler
/// inline, and its entry points' names there.
fn translate(
    source: &str,
    kind: Kind,
) -> Result<(String, std::collections::HashMap<String, String>), GpuError> {
    let failed = |what: String| GpuError(format!("direct Metal shaders: {what}"));
    let module = naga::front::wgsl::parse_str(source).map_err(|e| failed(e.to_string()))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::DUAL_SOURCE_BLENDING,
    )
    .validate(&module)
    .map_err(|e| failed(e.to_string()))?;
    let binding = |group, binding| naga::ResourceBinding { group, binding };
    let mut resources = naga::back::msl::BindingMap::new();
    let texture = |slot| naga::back::msl::BindTarget {
        texture: Some(slot),
        ..Default::default()
    };
    let buffer = |slot| naga::back::msl::BindTarget {
        buffer: Some(slot),
        ..Default::default()
    };
    if kind == Kind::Blur {
        resources.insert(binding(0, 0), texture(BLUR_SOURCE as u8));
        resources.insert(binding(0, 1), texture(BLUR_SOURCE as u8 + 1));
        resources.insert(binding(0, 2), buffer(BLUR_RECORDS as u8));
        resources.insert(binding(0, 3), buffer(BLUR_RECORDS as u8 + 1));
        resources.insert(
            binding(0, 4),
            naga::back::msl::BindTarget {
                sampler: Some(naga::back::msl::BindSamplerTarget::Inline(0)),
                ..Default::default()
            },
        );
    } else if kind == Kind::Effects {
        resources.insert(binding(0, 1), texture(0));
        resources.insert(
            binding(0, 2),
            naga::back::msl::BindTarget {
                sampler: Some(naga::back::msl::BindSamplerTarget::Inline(0)),
                ..Default::default()
            },
        );
        resources.insert(binding(1, 0), texture(1));
    } else if kind == Kind::Quads {
        resources.insert(
            binding(1, 0),
            naga::back::msl::BindTarget {
                buffer: Some(0),
                ..Default::default()
            },
        );
        resources.insert(binding(1, 1), buffer(REGIONS as u8));
        resources.insert(binding(2, 0), buffer(POINTS as u8));
        resources.insert(
            binding(0, 1),
            naga::back::msl::BindTarget {
                texture: Some(0),
                ..Default::default()
            },
        );
        resources.insert(
            binding(0, 2),
            naga::back::msl::BindTarget {
                sampler: Some(naga::back::msl::BindSamplerTarget::Inline(0)),
                ..Default::default()
            },
        );
    } else {
        resources.insert(
            binding(0, 0),
            naga::back::msl::BindTarget {
                texture: Some(0),
                ..Default::default()
            },
        );
    }
    let per_entry_point_map = module
        .entry_points
        .iter()
        .map(|entry| {
            (
                entry.name.clone(),
                naga::back::msl::EntryPointResources {
                    resources: resources.clone(),
                    sizes_buffer: match kind {
                        Kind::Blur => Some(BLUR_SIZES as u8),
                        Kind::Quads => Some(REGION_SIZES as u8),
                        _ => None,
                    },
                    ..Default::default()
                },
            )
        })
        .collect();
    let options = naga::back::msl::Options {
        lang_version: (2, 4),
        per_entry_point_map,
        inline_samplers: vec![naga::back::msl::sampler::InlineSampler {
            coord: naga::back::msl::sampler::Coord::Normalized,
            address: [naga::back::msl::sampler::Address::ClampToEdge; 3],
            border_color: naga::back::msl::sampler::BorderColor::TransparentBlack,
            mag_filter: naga::back::msl::sampler::Filter::Linear,
            min_filter: naga::back::msl::sampler::Filter::Linear,
            mip_filter: None,
            lod_clamp: None,
            max_anisotropy: None,
            compare_func: naga::back::msl::sampler::CompareFunc::Never,
        }],
        fake_missing_bindings: false,
        zero_initialize_workgroup_memory: false,
        ..Default::default()
    };
    let pipeline_options = naga::back::msl::PipelineOptions {
        entry_point: None,
        allow_and_force_point_size: false,
        vertex_pulling_transform: false,
        vertex_buffer_mappings: Vec::new(),
    };
    let (msl, translated) =
        naga::back::msl::write_string(&module, &info, &options, &pipeline_options)
            .map_err(|e| failed(e.to_string()))?;
    let names = module
        .entry_points
        .iter()
        .zip(translated.entry_point_names)
        .filter_map(|(entry, name)| Some((entry.name.clone(), name.ok()?)))
        .collect();
    Ok((msl, names))
}

impl Library {
    fn pipeline(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        vertex: &str,
        fragment: &str,
        format: MTLPixelFormat,
        blend: Blend,
    ) -> Result<Pipeline, GpuError> {
        let function = |name: &str| {
            self.names
                .get(name)
                .and_then(|name| self.library.newFunctionWithName(&NSString::from_str(name)))
                .ok_or_else(|| GpuError(format!("direct Metal shaders: no `{name}`")))
        };
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&*function(vertex)?));
        descriptor.setFragmentFunction(Some(&*function(fragment)?));
        // Per-instance quads, 128 bytes each: eight vec4s, as `Quad` (a
        // copy reads its first two).
        let attributes = match self.kind {
            Kind::Quads | Kind::Effects => 8,
            Kind::Copies => 2,
            Kind::Blur => 0,
        };
        if attributes > 0 {
            let vertices = MTLVertexDescriptor::vertexDescriptor();
            // SAFETY: Attribute and layout indices within Metal's limits.
            unsafe {
                for index in 0..attributes {
                    let attribute = vertices.attributes().objectAtIndexedSubscript(index);
                    attribute.setFormat(MTLVertexFormat::Float4);
                    attribute.setOffset(16 * index);
                    attribute.setBufferIndex(VERTICES);
                }
                let layout = vertices.layouts().objectAtIndexedSubscript(VERTICES);
                layout.setStride(128);
                layout.setStepFunction(MTLVertexStepFunction::PerInstance);
                layout.setStepRate(1);
            }
            descriptor.setVertexDescriptor(Some(&vertices));
        }
        // SAFETY: Index 0 of the pipeline's colour attachments.
        let color = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
        color.setPixelFormat(format);
        let destination = match blend {
            Blend::Replace => None,
            Blend::Over => Some((
                MTLBlendFactor::OneMinusSourceAlpha,
                MTLBlendFactor::OneMinusSourceAlpha,
            )),
            Blend::Mix => Some((
                MTLBlendFactor::OneMinusSource1Color,
                MTLBlendFactor::OneMinusSource1Alpha,
            )),
        };
        if let Some((rgb, alpha)) = destination {
            color.setBlendingEnabled(true);
            color.setSourceRGBBlendFactor(MTLBlendFactor::One);
            color.setSourceAlphaBlendFactor(MTLBlendFactor::One);
            color.setDestinationRGBBlendFactor(rgb);
            color.setDestinationAlphaBlendFactor(alpha);
        }
        device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|e| GpuError(format!("direct Metal pipeline: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module as `DRAW_MSL` and `COPY_MSL` hold it.
    fn ahead(source: &str, kind: Kind) -> String {
        let (msl, names) = translate(source, kind).unwrap();
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort();
        let names: String = names
            .iter()
            .map(|(entry, name)| format!(" {entry}={name}"))
            .collect();
        format!("{ENTRY_POINTS}{names}\n{msl}")
    }

    #[test]
    fn translated_shaders_match() {
        let bless = std::env::var_os("ZGX_BLESS").is_some_and(|v| v != "0");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for (wgsl, kind, msl, file) in [
            (
                include_str!("draw.wgsl"),
                Kind::Quads,
                DRAW_MSL,
                "draw.metal",
            ),
            (
                include_str!("copy_offset.wgsl"),
                Kind::Copies,
                COPY_MSL,
                "copy_offset.metal",
            ),
        ] {
            let now = ahead(wgsl, kind);
            if bless {
                std::fs::write(dir.join(file), &now).unwrap();
            } else {
                assert!(
                    now == msl,
                    "src/{file} is not its WGSL's translation: \
                     `ZGX_BLESS=1 cargo test -p zgx-gpu --lib translated_shaders`"
                );
            }
        }
    }
}
