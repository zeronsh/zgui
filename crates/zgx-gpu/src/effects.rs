//! Layer effects: content blur (`Effects::filter_blur`) and masks
//! (`Scene::set_mask`). A layer with either composites from its slot of the
//! effect atlas, which effects.wgsl fills from the layer's cached texture: a
//! blur across into a scratch atlas, then down into the effect atlas,
//! masking as it goes. Every layer blurred or masked anew in a frame shares
//! those two passes, and a slot whose inputs did not change composites as
//! it is: an animated sigma or mask offset costs the passes and the
//! composite, never the layer's content.
//!
//! A wide blur runs on a slot averaged down by a power of two, where sigma
//! spans two to four texels, so its taps stay few however wide it is; the
//! composite samples it back up bilinearly.
use crate::{GpuError, GpuRenderer, GpuStats, Quad};
use rustc_hash::FxHashMap;
use zgx::scene::{NodeId, PaintItem, Rect, Scene};

/// The effect atlas's entry in `GpuRenderer::images`: effect layers
/// composite as an image does, batching with each other.
pub(crate) const EFFECT_ATLAS: u64 = u64::MAX;
/// The largest effect atlas side.
const MAX_SIDE: u32 = 4096;
/// The widest blur, in layer texels.
const MAX_SIGMA: f32 = 128.;

/// effects.wgsl's pipelines (see `GpuRenderer::encode_effects`).
pub(crate) struct EffectPipelines {
    across: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    /// Group 1: the mask.
    mask_layout: wgpu::BindGroupLayout,
}
impl EffectPipelines {
    fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("layer effects"),
            source: wgpu::ShaderSource::Wgsl(include_str!("effects.wgsl").into()),
        });
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effect mask"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layer effects"),
            bind_group_layouts: &[Some(texture_layout), Some(&mask_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4];
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("layer effects"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 128,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attributes,
                    }],
                },
                primitive: crate::QUAD_STRIP,
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(crate::FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            across: pipeline("fs_across"),
            down: pipeline("fs_down"),
            mask_layout,
        }
    }
}
/// A row of slots, filled left to right.
struct Shelf {
    y: u32,
    height: u32,
    end: u32,
}
/// What a slot was drawn from: equal, it composites as it is.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    revision: u64,
    step: i32,
    content: (u32, u32),
    sigma: u32,
    /// The image, and where its texels fall (a 64th of a texel, and the
    /// step per slot texel's bits).
    mask: Option<(u64, [i32; 2], [u32; 2])>,
}
/// A layer's slot of the effect atlas.
pub(crate) struct Slot {
    /// Its top-left texel (its gutter's) and the room it holds.
    at: (u32, u32),
    room: (u32, u32),
    key: Option<Key>,
    /// Where its first texel inside the gutter starts, from the layer's
    /// bounds' origin, and a texel's size (logical pixels).
    origin: (f32, f32),
    texel: f32,
    /// How far the blur shows past the layer's bounds (logical pixels).
    pub(crate) spread: f32,
}
impl Slot {
    /// The quad a layer at `bounds` composites with, and the part of an
    /// atlas of `size` it samples (uv rectangle).
    pub(crate) fn composite(&self, bounds: Rect, size: (u32, u32)) -> (Rect, [f32; 4]) {
        let rect = bounds.expand(self.spread);
        let start = (bounds.x + self.origin.0, bounds.y + self.origin.1);
        let u = |at: u32, value: f32, start: f32| at as f32 + 1. + (value - start) / self.texel;
        (
            rect,
            [
                u(self.at.0, rect.x, start.0) / size.0 as f32,
                u(self.at.1, rect.y, start.1) / size.1 as f32,
                rect.width / self.texel / size.0 as f32,
                rect.height / self.texel / size.1 as f32,
            ],
        )
    }
}
/// A slot to draw this frame: its layer, its mask image, and its passes'
/// quads (see effects.wgsl): across, where it blurs, then down.
pub(crate) struct Job {
    pub(crate) node: NodeId,
    pub(crate) mask: Option<u64>,
    pub(crate) across: Option<Quad>,
    pub(crate) down: Quad,
}
/// The effect atlas and its scratch twin, of one size; slots packed on
/// shelves until one does not fit, then everything wanted is packed afresh.
pub(crate) struct EffectAtlas {
    pub(crate) output: wgpu::Texture,
    pub(crate) scratch: wgpu::Texture,
    scratch_bind: wgpu::BindGroup,
    /// Bound as the mask of jobs without one.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) blank: wgpu::Texture,
    blank_bind: wgpu::BindGroup,
    masks: FxHashMap<u64, wgpu::BindGroup>,
    shelves: Vec<Shelf>,
    bottom: u32,
    pub(crate) slots: FxHashMap<NodeId, Slot>,
    /// The frame (`GpuRenderer::layer_frame`) a slot was last wanted in.
    pub(crate) used: u64,
}
impl EffectAtlas {
    pub(crate) fn size(&self) -> (u32, u32) {
        (self.output.width(), self.output.height())
    }
    /// Room for `room` texels, if a shelf or the space below them has it.
    fn place(&mut self, room: (u32, u32)) -> Option<(u32, u32)> {
        let (width, height) = self.size();
        let room = (room.0.next_multiple_of(8), room.1.next_multiple_of(8));
        if let Some(shelf) = self.shelves.iter_mut().find(|shelf| {
            shelf.height >= room.1
                && shelf.height <= room.1 + room.1 / 2
                && shelf.end + room.0 <= width
        }) {
            shelf.end += room.0;
            return Some((shelf.end - room.0, shelf.y));
        }
        if self.bottom + room.1 > height || room.0 > width {
            return None;
        }
        self.shelves.push(Shelf {
            y: self.bottom,
            height: room.1,
            end: room.0,
        });
        self.bottom += room.1;
        Some((0, self.bottom - room.1))
    }
}
/// A layer's slot as this frame wants it.
struct Want {
    node: NodeId,
    key: Key,
    /// Texels inside the gutter.
    interior: (u32, u32),
    content: (u32, u32),
    origin: (f32, f32),
    texel: f32,
    spread: f32,
    /// Source texels per slot texel, the blur's pad (layer texels) and its
    /// sigma in slot texels (0: none).
    factor: f32,
    pad: u32,
    sigma: f32,
    /// The image, the mask texel at the slot's corner and its step.
    mask: Option<(std::sync::Arc<zgx::image::ImageData>, [f32; 4])>,
}
impl Want {
    fn room(&self) -> (u32, u32) {
        (self.interior.0 + 2, self.interior.1 + 2)
    }
}
/// Whether `rooms` pack into an empty atlas of `width` × `height`, as
/// `EffectAtlas::place` packs them.
fn packs(width: u32, height: u32, rooms: &[(u32, u32)]) -> bool {
    let mut shelves: Vec<Shelf> = Vec::new();
    let mut bottom = 0;
    for room in rooms {
        let room = (room.0.next_multiple_of(8), room.1.next_multiple_of(8));
        if let Some(shelf) = shelves.iter_mut().find(|shelf| {
            shelf.height >= room.1
                && shelf.height <= room.1 + room.1 / 2
                && shelf.end + room.0 <= width
        }) {
            shelf.end += room.0;
            continue;
        }
        if bottom + room.1 > height || room.0 > width {
            return false;
        }
        shelves.push(Shelf {
            y: bottom,
            height: room.1,
            end: room.0,
        });
        bottom += room.1;
    }
    true
}
impl GpuRenderer {
    /// Plan the effect layers among `items` (prepared layers, positioned as
    /// their layers are): each slot drawn as it is, or a job drawing it in
    /// the next `render_flat`.
    pub(crate) fn prepare_effects(
        &mut self,
        scene: &Scene,
        items: &[PaintItem<'_>],
    ) -> Result<(), GpuError> {
        let mut wanted = Vec::new();
        for item in items {
            if !item.isolated || item.effects.opacity <= 0. {
                continue;
            }
            let mask = scene.mask(item.id);
            if item.effects.filter_blur <= 0. && mask.is_none() {
                continue;
            }
            let Some(layer) = self.layers.get(&item.id) else {
                continue;
            };
            let content = (
                (layer.uv[0] * layer.texture.width() as f32).round() as u32,
                (layer.uv[1] * layer.texture.height() as f32).round() as u32,
            );
            if content.0 == 0 || content.1 == 0 || layer.bounds.width <= 0. {
                continue;
            }
            // Layer texels per logical pixel.
            let scale = content.0 as f32 / layer.bounds.width;
            let sigma = (item.effects.filter_blur * scale).min(MAX_SIGMA);
            let (factor, pad, slot_sigma) = if sigma > 0. {
                // Averaged down where sigma spans 2 to 4 texels; the boxes
                // averaged and the bilinear upsampling blur too, so the
                // Gaussian makes up the rest.
                let factor = match sigma < 4. {
                    true => 1.,
                    false => (sigma / 2.).log2().floor().exp2().min(16.),
                };
                let extra = match factor > 1. {
                    true => (factor * factor - 1.) / 12. + factor * factor / 6.,
                    false => 0.,
                };
                let gaussian = (sigma * sigma - extra).max(0.0025).sqrt();
                (factor, (3. * sigma).ceil() as u32, gaussian / factor)
            } else {
                (1., 0, 0.)
            };
            let interior = (
                ((content.0 + 2 * pad) as f32 / factor).ceil() as u32,
                ((content.1 + 2 * pad) as f32 / factor).ceil() as u32,
            );
            let texel = factor / scale;
            let origin = (-(pad as f32) / scale, -(pad as f32) / scale);
            // The quad's first texel is the gutter's: one before the slot's.
            let mask = mask.map(|mask| {
                let image = &mask.image;
                let along = |size: f32, pixels: u32, start: f32, node: f32, offset: f32| {
                    if size <= 0. {
                        return (-1., 0.);
                    }
                    let per = pixels as f32 / size;
                    ((start - texel - node - offset) * per, texel * per)
                };
                let (x, step_x) = along(
                    mask.size.0,
                    image.width(),
                    layer.bounds.x + origin.0,
                    item.bounds.x,
                    mask.offset.0,
                );
                let (y, step_y) = along(
                    mask.size.1,
                    image.height(),
                    layer.bounds.y + origin.1,
                    item.bounds.y,
                    mask.offset.1,
                );
                (image.clone(), [x, y, step_x, step_y])
            });
            let key = Key {
                revision: layer.revision,
                step: layer.step,
                content,
                sigma: sigma.to_bits(),
                mask: mask.as_ref().map(|(image, [x, y, step_x, step_y])| {
                    (
                        image.id(),
                        [(x * 64.).round() as i32, (y * 64.).round() as i32],
                        [step_x.to_bits(), step_y.to_bits()],
                    )
                }),
            };
            wanted.push(Want {
                node: item.id,
                key,
                interior,
                content,
                origin,
                texel,
                spread: 3. * sigma / scale,
                factor,
                pad,
                sigma: slot_sigma,
                mask,
            });
        }
        if wanted.is_empty() {
            return Ok(());
        }
        for want in &wanted {
            if let Some((image, _)) = &want.mask
                && !self.images.contains_key(&image.id())
            {
                self.upload_image(image)?;
            }
        }
        let frame = self.layer_frame;
        // Unchanged slots composite as they are; the rest go where they fit.
        let mut fresh = Vec::new();
        let mut overflow = self.effects.is_none();
        if let Some(atlas) = &mut self.effects {
            atlas.used = frame;
            for (index, want) in wanted.iter().enumerate() {
                let room = want.room();
                match atlas.slots.get(&want.node) {
                    Some(slot) if slot.key == Some(want.key) => continue,
                    Some(slot) if slot.room.0 >= room.0 && slot.room.1 >= room.1 => {}
                    _ => match atlas.place(room) {
                        Some(at) => {
                            atlas.slots.insert(
                                want.node,
                                Slot {
                                    at,
                                    room: (room.0.next_multiple_of(8), room.1.next_multiple_of(8)),
                                    key: None,
                                    origin: want.origin,
                                    texel: want.texel,
                                    spread: want.spread,
                                },
                            );
                        }
                        None => {
                            overflow = true;
                            break;
                        }
                    },
                }
                fresh.push(index);
            }
        }
        if overflow {
            // Everything wanted, packed afresh, tallest first, into an atlas
            // grown to hold it.
            let mut order: Vec<usize> = (0..wanted.len()).collect();
            order.sort_by_key(|&index| std::cmp::Reverse(wanted[index].room().1));
            let rooms: Vec<(u32, u32)> = order.iter().map(|&index| wanted[index].room()).collect();
            let limit = self.device.limits().max_texture_dimension_2d.min(MAX_SIDE);
            let widest = rooms.iter().map(|room| room.0).max().unwrap_or(1);
            let (mut width, mut height) =
                self.effects.as_ref().map_or((256, 256), EffectAtlas::size);
            width = width.max(widest.next_power_of_two()).min(limit);
            while !packs(width, height, &rooms) && (width < limit || height < limit) {
                if height < width {
                    height = (height * 2).min(limit);
                } else {
                    width = (width * 2).min(limit);
                }
            }
            if self
                .effects
                .as_ref()
                .is_none_or(|atlas| atlas.size() != (width, height))
            {
                self.effects = Some(self.effect_atlas(width, height));
            }
            let atlas = self.effects.as_mut().expect("made above");
            atlas.used = frame;
            atlas.slots.clear();
            atlas.shelves.clear();
            atlas.bottom = 0;
            fresh.clear();
            // What still does not fit draws without its effect.
            for index in order {
                let want = &wanted[index];
                let room = want.room();
                if let Some(at) = atlas.place(room) {
                    atlas.slots.insert(
                        want.node,
                        Slot {
                            at,
                            room: (room.0.next_multiple_of(8), room.1.next_multiple_of(8)),
                            key: None,
                            origin: want.origin,
                            texel: want.texel,
                            spread: want.spread,
                        },
                    );
                    fresh.push(index);
                }
            }
        }
        let atlas = self.effects.as_mut().expect("made above");
        let size = atlas.size();
        let size = [size.0 as f32, size.1 as f32, 0., 0.];
        for index in fresh {
            let want = &wanted[index];
            let slot = atlas.slots.get_mut(&want.node).expect("placed above");
            slot.key = Some(want.key);
            slot.origin = want.origin;
            slot.texel = want.texel;
            slot.spread = want.spread;
            let (x, y) = (slot.at.0 as f32, slot.at.1 as f32);
            let room = want.room();
            let rect = [x, y, room.0 as f32, room.1 as f32];
            let inside = [
                x + 1.,
                y + 1.,
                x + 1. + want.interior.0 as f32,
                y + 1. + want.interior.1 as f32,
            ];
            let quad = |color: [f32; 4], options: [f32; 4]| Quad {
                rect,
                uv: size,
                color,
                fade: inside,
                options,
                shape: want.mask.as_ref().map_or([0.; 4], |(_, mapping)| *mapping),
                border: [if want.mask.is_some() { 1. } else { 0. }, 0., 0., 0.],
                mask: crate::NO_MASK,
            };
            let blurred = want.sigma > 0.;
            let start = -(want.pad as f32) - want.factor;
            atlas_jobs_push(
                &mut self.effect_jobs,
                Job {
                    node: want.node,
                    mask: want.mask.as_ref().map(|(image, _)| image.id()),
                    across: blurred.then(|| quad([start, start, want.factor, want.sigma], [0.; 4])),
                    down: match blurred {
                        true => quad([x, y, 0., want.sigma], inside),
                        false => quad(
                            [-1., -1., 0., 0.],
                            [0., 0., want.content.0 as f32, want.content.1 as f32],
                        ),
                    },
                },
            );
        }
        Ok(())
    }
    /// A new effect atlas of `width` × `height`, bound for composites.
    fn effect_atlas(&mut self, width: u32, height: u32) -> EffectAtlas {
        let output = crate::texture(&self.device, width, height, "effect atlas");
        let scratch = crate::texture(&self.device, width, height, "effect scratch");
        let blank = crate::texture(&self.device, 1, 1, "effect blank mask");
        let view = |texture: &wgpu::Texture| texture.create_view(&Default::default());
        // Frames drawn with Metal directly write these behind wgpu's back.
        #[cfg(target_os = "macos")]
        for texture in [&output, &scratch, &blank] {
            crate::written(&self.device, &self.queue, &view(texture));
        }
        let bind = self.texture_bind(&view(&output), "effect atlas");
        self.images
            .insert(EFFECT_ATLAS, (bind, 0, Some(output.clone()), (0, 0)));
        let pipelines = self.effect_pipelines();
        let mask_layout = pipelines.mask_layout.clone();
        let blank_bind = self.mask_bind(&mask_layout, &view(&blank));
        EffectAtlas {
            scratch_bind: self.texture_bind(&view(&scratch), "effect scratch"),
            output,
            scratch,
            blank,
            blank_bind,
            masks: FxHashMap::default(),
            shelves: Vec::new(),
            bottom: 0,
            slots: FxHashMap::default(),
            used: self.layer_frame,
        }
    }
    fn effect_pipelines(&mut self) -> &EffectPipelines {
        let layout = &self.context.inner.texture_layout;
        self.effect_pipelines
            .get_or_insert_with(|| EffectPipelines::new(&self.device, layout))
    }
    fn mask_bind(
        &self,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect mask"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        })
    }
    /// Jobs given up before they were drawn: their slots draw again.
    pub(crate) fn forget_effects(&mut self, jobs: &[Job]) {
        if let Some(atlas) = &mut self.effects {
            for job in jobs {
                if let Some(slot) = atlas.slots.get_mut(&job.node) {
                    slot.key = None;
                }
            }
        }
    }
    /// The slot `id` composites from, if drawn (or drawn this frame).
    pub(crate) fn effect_slot(&self, id: NodeId) -> Option<&Slot> {
        self.effects
            .as_ref()?
            .slots
            .get(&id)
            .filter(|slot| slot.key.is_some())
    }
    /// The quads of `jobs`' passes, as `encode_effects` and the direct Metal
    /// frame draw them: every across quad, then every down quad.
    pub(crate) fn effect_quads(jobs: &[Job]) -> impl Iterator<Item = Quad> + '_ {
        jobs.iter()
            .filter_map(|job| job.across)
            .chain(jobs.iter().map(|job| job.down))
    }
    /// Draw `jobs` (their quads at instance `first` of `vertices` on, as
    /// `effect_quads` lays them out): across into the scratch atlas, then
    /// down into the effect atlas.
    pub(crate) fn encode_effects(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        vertices: &wgpu::Buffer,
        jobs: &[Job],
        first: u32,
        stats: &mut GpuStats,
    ) {
        if jobs.is_empty() {
            return;
        }
        self.effect_pipelines();
        // Mask bind groups for images new to the atlas.
        let mask_layout = self
            .effect_pipelines
            .as_ref()
            .expect("made above")
            .mask_layout
            .clone();
        for job in jobs {
            if let Some(image) = job.mask
                && !self
                    .effects
                    .as_ref()
                    .is_some_and(|atlas| atlas.masks.contains_key(&image))
                && let Some(texture) = self.images.get(&image).and_then(|entry| entry.2.clone())
            {
                let bind = self.mask_bind(&mask_layout, &texture.create_view(&Default::default()));
                self.effects
                    .as_mut()
                    .expect("planned")
                    .masks
                    .insert(image, bind);
            }
        }
        let pipelines = self.effect_pipelines.as_ref().expect("made above");
        let atlas = self.effects.as_ref().expect("planned");
        let across = jobs.iter().filter(|job| job.across.is_some()).count() as u32;
        let views = [&atlas.scratch, &atlas.output].map(|t| t.create_view(&Default::default()));
        for (pass, view) in views.iter().enumerate() {
            let down = pass == 1;
            if !down && across == 0 {
                continue;
            }
            let mut encoded = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if down {
                    "effects down"
                } else {
                    "effects across"
                }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if down {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            encoded.set_pipeline(if down {
                &pipelines.down
            } else {
                &pipelines.across
            });
            encoded.set_vertex_buffer(0, vertices.slice(..));
            stats.render_passes += 1;
            stats.effect_passes += 1;
            let mut instance = first + if down { across } else { 0 };
            for job in jobs {
                if !down && job.across.is_none() {
                    continue;
                }
                let layer = &self.layers[&job.node].bind;
                let source = if down && job.across.is_some() {
                    &atlas.scratch_bind
                } else {
                    layer
                };
                let mask = match (down, job.mask) {
                    (true, Some(image)) => atlas.masks.get(&image).unwrap_or(&atlas.blank_bind),
                    _ => &atlas.blank_bind,
                };
                encoded.set_bind_group(0, source, &[]);
                encoded.set_bind_group(1, mask, &[]);
                encoded.draw(0..4, instance..instance + 1);
                stats.draw_calls += 1;
                instance += 1;
            }
        }
    }
}
#[cfg(target_os = "macos")]
impl GpuRenderer {
    /// `jobs`' passes for a frame drawn with Metal directly (their quads at
    /// instance `first` on, as `effect_quads` lays them out), each texture
    /// taken through `raw`.
    pub(crate) fn metal_effect_passes(
        &self,
        jobs: &[Job],
        first: u32,
        raw: &dyn Fn(&wgpu::Texture) -> crate::metal_frame::Texture,
    ) -> Vec<crate::metal_frame::EffectPass> {
        let Some(atlas) = self.effects.as_ref().filter(|_| !jobs.is_empty()) else {
            return Vec::new();
        };
        let blank = raw(&atlas.blank);
        let mut instance = first;
        let mut passes = Vec::new();
        for down in [false, true] {
            let mut draws = Vec::new();
            for job in jobs.iter().filter(|job| down || job.across.is_some()) {
                let source = match down && job.across.is_some() {
                    true => &atlas.scratch,
                    false => &self.layers[&job.node].texture,
                };
                let mask = match (down, job.mask) {
                    (true, Some(id)) => self.images[&id].2.as_ref().map(raw),
                    _ => None,
                };
                draws.push(crate::metal_frame::EffectDraw {
                    source: raw(source),
                    mask: mask.unwrap_or_else(|| blank.clone()),
                    instance,
                });
                instance += 1;
            }
            if !draws.is_empty() {
                let target = if down { &atlas.output } else { &atlas.scratch };
                passes.push(crate::metal_frame::EffectPass {
                    target: raw(target),
                    down,
                    draws,
                });
            }
        }
        passes
    }
}
/// Queue `job`, replacing an earlier one for its layer this frame.
fn atlas_jobs_push(jobs: &mut Vec<Job>, job: Job) {
    jobs.retain(|queued| queued.node != job.node);
    jobs.push(job);
}
