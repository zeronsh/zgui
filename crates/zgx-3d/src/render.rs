//! Drawing a `World`: its view's painter. A frame is one pass with depth and
//! 4x multisampling (or 1 sample, `World::set_samples`): each geometry's
//! meshes in one instanced draw, opaque ones first, then the translucent
//! back to front. Pipelines and buffers are made with the first frame and
//! kept; a geometry's buffers are uploaded once and dropped a while after
//! the last frame drawing it. With shadows, a pass before it draws the
//! opaque meshes' depth as the sun sees them.
use crate::{
    math::{Mat4, Vec3},
    texture::MAPS,
    world::{Inner, Light, POINT_LIGHTS},
};
use std::{cell::RefCell, collections::HashMap, rc::Weak};
use zgx_gpu::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Frames a geometry's buffers stay after the last that drew it.
const KEEP_FRAMES: u64 = 240;
/// The shadow map's side, in texels.
const SHADOW_SIZE: u32 = 2048;

/// Per mesh: its world matrix, its normals' (the inverse transpose, its
/// fourth row metallic, roughness, unlit), its colour, its emission.
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Instance {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 3],
    color: [f32; 4],
    emissive: [f32; 4],
}
/// Per frame (see the shader's `Frame`).
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct FrameUniforms {
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    sun_direction: [f32; 4],
    sun_color: [f32; 4],
    sky: [f32; 4],
    ground: [f32; 4],
    points: [[[f32; 4]; 3]; POINT_LIGHTS],
    /// Point lights in use, whether to encode sRGB itself, exposure,
    /// whether the sun casts shadows.
    params: [f32; 4],
    /// The sun's view of the scene, onto the shadow map.
    shadow: [[f32; 4]; 4],
    /// A shadow map texel's size in the scene, and in the map (1 / side).
    shadow_texel: [f32; 4],
    /// The environment's intensity, its rotation's cosine and sine, and its
    /// cube's last level (zeros without one).
    environment: [f32; 4],
    /// Its diffuse light (`Prefiltered::irradiance`), a coefficient a row.
    irradiance: [[f32; 4]; 9],
    /// The tone mapping: 0 neutral, 1 ACES, 2 none.
    look: [f32; 4],
}

/// What the shadow map holds: the sun's view, each opaque mesh's geometry,
/// place and alpha cutoff, skinned ones' joint matrices and morphing ones'
/// weights.
type Casting = (Mat4, Vec<(u64, Mat4, u32)>, Vec<Mat4>, Vec<[f32; 2]>);

/// What sets a pipeline apart from the plain one (`Variant::default()`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Variant {
    textured: bool,
    skinned: bool,
    /// Lit by an environment.
    lit: bool,
    morphing: bool,
    /// Its back faces draw.
    two_sided: bool,
    /// Cut out by its base colour's alpha.
    masked: bool,
    /// Its maps placed (`World::set_map_placement`).
    placed: bool,
    /// Its geometry has vertex colours.
    colored: bool,
}
impl std::hash::Hash for Variant {
    /// As one byte: it keys the pipelines, looked up a run a frame, and each
    /// field hashed on its own costs them all another write.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Variant {
            textured,
            skinned,
            lit,
            morphing,
            two_sided,
            masked,
            placed,
            colored,
        } = *self;
        let fields = [
            textured, skinned, lit, morphing, two_sided, masked, placed, colored,
        ];
        state.write_u8(fields.iter().fold(0, |bits, &on| bits << 1 | u8::from(on)));
    }
}
/// Meshes drawn together: of one geometry, maps and variant.
struct Run {
    geometry: u64,
    glass: bool,
    maps: [u64; MAPS],
    variant: Variant,
    instances: std::ops::Range<u32>,
}

struct Buffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// A skinned geometry's joints and weights, a vertex's.
    skin: Option<wgpu::Buffer>,
    /// A morphing geometry's targets' deltas, bound.
    morphs: Option<wgpu::BindGroup>,
    /// Its second texture coordinates (its first where it has none), made
    /// with the first frame drawing it placed.
    second_uvs: Option<wgpu::Buffer>,
    /// Its vertices' colours, if it has any.
    colors: Option<wgpu::Buffer>,
    count: u32,
    used: u64,
}
struct Gpu {
    format: wgpu::TextureFormat,
    /// Opaque and translucent meshes', without textures: an untextured
    /// scene binds none (a bound texture costs Apple's driver 16 MB).
    plain: [wgpu::RenderPipeline; 2],
    /// Meshes' of any other variant, made with the first frame drawing one.
    variants: HashMap<Variant, [wgpu::RenderPipeline; 2]>,
    /// Skinned, morphing, cut-out or cut-out and placed meshes' depth from
    /// the sun, by which of those, made with the first such shadow.
    shadow_variants: HashMap<(bool, bool, bool, bool), wgpu::RenderPipeline>,
    shadow_layout: wgpu::BindGroupLayout,
    /// Group 1 of a skinned untextured pipeline: nothing.
    empty_layout: wgpu::BindGroupLayout,
    empty_bind: wgpu::BindGroup,
    /// Skinned meshes' joint matrices and morphing meshes' weights, all of
    /// a frame's in one buffer.
    palette_layout: wgpu::BindGroupLayout,
    /// Group 3 of a morphing pipeline: its geometry's deltas.
    morph_layout: wgpu::BindGroupLayout,
    palettes: Option<(wgpu::Buffer, usize, wgpu::BindGroup)>,
    /// Whether the device reads storage buffers in vertex shaders: without
    /// (some GL), skinned and morphing meshes draw as bound.
    skinning: bool,
    uniforms: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind: wgpu::BindGroup,
    /// Depth from the sun, and its pass's frame (the uniforms alone: the
    /// map can't be read while drawn).
    shadow: wgpu::RenderPipeline,
    shadow_bind: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    /// The shadow map while shadows are on; a 1 by 1 stand-in otherwise.
    shadow_map: Option<wgpu::TextureView>,
    stand_in: wgpu::TextureView,
    /// What the shadow map holds: the sun's view and each opaque mesh's
    /// geometry and place. A frame where only the camera moved keeps it.
    shadow_drawn: Option<Casting>,
    /// The frame's binding with the environment's cube, made with the first
    /// frame lit by one: its layout and sampler, the cube by environment
    /// id, and the binding (made again as the shadow map or cube changes).
    environment_layout: Option<(wgpu::BindGroupLayout, wgpu::Sampler)>,
    environment: Option<(u64, wgpu::TextureView)>,
    environment_bind: Option<wgpu::BindGroup>,
    /// Textures by id, with the frame that last drew each.
    texture_layout: wgpu::BindGroupLayout,
    texture_sampler: wgpu::Sampler,
    /// Uploaded textures by id and whether read as sRGB (a texture may be
    /// a colour map in one place, data in another).
    views: HashMap<(u64, bool), (wgpu::TextureView, u64)>,
    /// Group 1 with placements as well, and samplers wrapping other than
    /// by repeating: made with the first mesh needing them.
    placed_layout: Option<wgpu::BindGroupLayout>,
    wrapped_samplers: HashMap<[crate::Wrap; 2], wgpu::Sampler>,
    /// Each set of maps meshes draw with (`map_set`), bound.
    bindings: HashMap<[u64; MAPS], (wgpu::BindGroup, u64)>,
    /// What a map a mesh hasn't got reads: white (its factors as they are)
    /// and a flat normal. Made with the first textured frame.
    stand_ins: Option<[wgpu::TextureView; MAPS]>,
    instances: Option<(wgpu::Buffer, usize)>,
    geometries: HashMap<u64, Buffers>,
    /// Its size, and its multisampled colour (none drawing one sample a
    /// pixel: the frame's target itself) and depth.
    targets: Option<((u32, u32), Option<wgpu::TextureView>, wgpu::TextureView)>,
    /// Samples a pixel (`World::set_samples`).
    samples: u32,
    frame: u64,
}

pub(crate) struct Renderer {
    world: Weak<RefCell<Inner>>,
    gpu: Option<Gpu>,
    scratch: Scratch,
}
impl Renderer {
    pub(crate) fn new(world: Weak<RefCell<Inner>>) -> Self {
        Self {
            world,
            gpu: None,
            scratch: Scratch::default(),
        }
    }
}
/// A frame's lists, kept for the next to fill again: a frame allocates
/// none of its own, so what it costs doesn't follow where the heap is.
#[derive(Default)]
struct Scratch {
    poses: crate::world::Poses,
    all: Vec<crate::world::Drawn<'static>>,
    sorted: Vec<&'static crate::world::Drawn<'static>>,
    order: Vec<((u64, [u64; MAPS]), usize)>,
    palette: Vec<[[f32; 4]; 4]>,
    instances: Vec<Instance>,
    runs: Vec<Run>,
    /// What the shadow map held before, its lists to fill again.
    casting: Option<Casting>,
}
/// `list` emptied, as a list of another type of one size: its allocation
/// kept (a `Vec` collected in place from its own iterator reuses it).
fn recycle<T, U>(mut list: Vec<T>) -> Vec<U> {
    list.clear();
    list.into_iter().map(|_| unreachable!()).collect()
}

impl Viewport for Renderer {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        let Some(world) = self.world.upgrade() else {
            return;
        };
        let world = world.borrow();
        // Shade in linear light: written through the sRGB view where there is
        // one, encoded by the shader where not.
        let (target, format, encode) = match frame.target_srgb {
            Some(srgb) => (srgb, frame.format.add_srgb_suffix(), false),
            None => (frame.target, frame.format, !frame.format.is_srgb()),
        };
        let samples = world.samples;
        if self
            .gpu
            .as_ref()
            .is_none_or(|gpu| gpu.format != format || gpu.samples != samples)
        {
            self.gpu = Some(Gpu::new(frame.device, format, samples));
        }
        let gpu = self.gpu.as_mut().unwrap();
        gpu.frame += 1;
        let size = frame.size;
        if gpu.targets.as_ref().is_none_or(|(at, ..)| *at != size) {
            let texture = |format, label| {
                frame
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: size.0,
                            height: size.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            gpu.targets = Some((
                size,
                (samples > 1).then(|| texture(format, "3D colour")),
                texture(DEPTH, "3D depth"),
            ));
        }

        let camera = world.camera;
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let view = camera.view();
        let view_projection = camera.projection(aspect) * view;
        let mut uniforms = frame_uniforms(&world, view_projection, encode);
        let scratch = &mut self.scratch;
        let mut all = recycle(std::mem::take(&mut scratch.all));
        world.drawn_into(&mut scratch.poses, &mut all);
        let sun = world
            .shadows
            .then(|| {
                let [x, y, z, _] = uniforms.sun_direction;
                (uniforms.sun_color != [0.; 4]).then_some(crate::vec3(x, y, z))
            })
            .flatten();
        let shadow = sun.and_then(|sun| sun_view(&all, sun));
        if let Some((matrix, texel)) = shadow {
            uniforms.shadow = matrix.columns;
            uniforms.shadow_texel = [texel, 1. / SHADOW_SIZE as f32, 0., 0.];
            uniforms.params[3] = 1.;
        }
        gpu.shadows(frame.device, world.shadows);
        let lit = world.environment.as_ref();
        if let Some(environment) = lit {
            gpu.environment(frame.device, frame.queue, environment);
        }
        let lit = lit.is_some();
        frame
            .queue
            .write_buffer(&gpu.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Meshes by geometry, translucent ones after, back to front: by a
        // key of one integer (a sort of integers, not of meshes).
        let depth = |m: &Mat4| {
            let p = view.project(Vec3::from([
                m.columns[3][0],
                m.columns[3][1],
                m.columns[3][2],
            ]));
            -p.z
        };
        let translucent = |d: &crate::world::Drawn| d.material.color[3] < 1.;
        // Opaque ones by geometry, then maps.
        let key = |d: &crate::world::Drawn| {
            if translucent(d) {
                // Its depth's bits in total order, the farthest least.
                let bits = depth(&d.matrix).to_bits();
                let ordered = if bits >> 31 == 1 {
                    !bits
                } else {
                    bits | 1 << 31
                };
                (1 << 63 | u64::from(!ordered), map_set(d))
            } else {
                (d.geometry.id() & !(1 << 63), map_set(d))
            }
        };
        let order = &mut scratch.order;
        order.clear();
        order.extend(all.iter().map(key).zip(0..));
        order.sort_unstable();
        let mut drawn: Vec<&crate::world::Drawn> = recycle(std::mem::take(&mut scratch.sorted));
        drawn.extend(order.iter().map(|&(_, i)| &all[i]));
        // Skinned meshes' joint matrices, one after another; each mesh's
        // instance says where its own start.
        let skinning = gpu.skinning;
        let posing = skinning && world.posing();
        let skinned = |d: &crate::world::Drawn| posing && d.palette.is_some();
        let morphing = |d: &crate::world::Drawn| posing && d.morph.is_some();
        let palette = &mut scratch.palette;
        palette.clear();
        let instances = &mut scratch.instances;
        instances.clear();
        instances.extend(drawn.iter().map(|d| {
            let base = palette.len() as u32;
            if morphing(d) {
                morph_block(d, palette);
            }
            if skinned(d) {
                palette.extend(d.palette.iter().copied().flatten().map(|m| m.columns));
            }
            instance(d, base)
        }));
        if !palette.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(palette);
            if gpu
                .palettes
                .as_ref()
                .is_none_or(|(_, capacity, _)| *capacity < bytes.len())
            {
                let capacity = bytes.len().next_power_of_two().max(4096);
                let buffer = frame.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("3D joints"),
                    size: capacity as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind = frame.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("3D joints"),
                    layout: &gpu.palette_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                gpu.palettes = Some((buffer, capacity, bind));
            }
            frame
                .queue
                .write_buffer(&gpu.palettes.as_ref().unwrap().0, 0, bytes);
        }
        if !instances.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(instances);
            if gpu
                .instances
                .as_ref()
                .is_none_or(|(_, capacity)| *capacity < bytes.len())
            {
                let capacity = bytes.len().next_power_of_two().max(4096);
                gpu.instances = Some((
                    frame.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("3D instances"),
                        size: capacity as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    capacity,
                ));
            }
            frame
                .queue
                .write_buffer(&gpu.instances.as_ref().unwrap().0, 0, bytes);
        }
        // What they draw with, kept and marked used. (Meshes sharing it are
        // mostly neighbours by now.)
        let mut last = None;
        for (d, ((_, set), _)) in drawn.iter().zip(order.iter()) {
            if last == Some((d.geometry.id(), set)) {
                continue;
            }
            last = Some((d.geometry.id(), set));
            let entry = gpu.geometries.entry(d.geometry.id()).or_insert_with(|| {
                use wgpu::util::DeviceExt;
                Buffers {
                    vertices: frame
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("3D vertices"),
                            contents: bytemuck::cast_slice(d.geometry.vertices()),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                    indices: frame
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("3D indices"),
                            contents: bytemuck::cast_slice(d.geometry.indices()),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                    skin: d.geometry.skin().map(|skin| {
                        frame
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("3D joints and weights"),
                                contents: bytemuck::cast_slice(skin),
                                usage: wgpu::BufferUsages::VERTEX,
                            })
                    }),
                    second_uvs: None,
                    colors: d.geometry.colors().map(|colors| {
                        frame
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("3D vertex colours"),
                                contents: bytemuck::cast_slice(colors),
                                usage: wgpu::BufferUsages::VERTEX,
                            })
                    }),
                    morphs: d.geometry.morphs().filter(|_| skinning).map(|morphs| {
                        let buffer =
                            frame
                                .device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("3D morph targets"),
                                    contents: bytemuck::cast_slice(morphs.deltas()),
                                    usage: wgpu::BufferUsages::STORAGE,
                                });
                        frame.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("3D morph targets"),
                            layout: &gpu.morph_layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: buffer.as_entire_binding(),
                            }],
                        })
                    }),
                    count: d.geometry.indices().len() as u32,
                    used: 0,
                }
            });
            entry.used = gpu.frame;
            if placed(d) && entry.second_uvs.is_none() {
                use wgpu::util::DeviceExt;
                let first: Vec<[f32; 2]>;
                let uvs = match d.geometry.second_uvs() {
                    Some(uvs) => uvs,
                    None => {
                        first = d.geometry.vertices().iter().map(|v| v.uv).collect();
                        &first
                    }
                };
                entry.second_uvs = Some(frame.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("3D second texture coordinates"),
                        contents: bytemuck::cast_slice(uvs),
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                ));
            }
            for (map, texture) in crate::Map::ALL.into_iter().zip(d.maps) {
                if let Some(texture) = texture {
                    // A texture that can't be read draws as if it had none.
                    let key = (texture.id(), map.srgb());
                    if !gpu.views.contains_key(&key)
                        && let Some(view) = upload(frame.device, frame.queue, texture, map.srgb())
                    {
                        gpu.views.insert(key, (view, 0));
                    }
                    if let Some(entry) = gpu.views.get_mut(&key) {
                        entry.1 = gpu.frame;
                    }
                }
            }
        }
        // Each set of maps bound once, against stand-ins where a map is
        // missing. (Meshes sharing one are mostly neighbours by now.)
        let mut last = &[0; MAPS];
        for (d, ((_, key), _)) in drawn.iter().zip(order.iter()) {
            if std::mem::replace(&mut last, key) == key || *key == [0; MAPS] {
                continue;
            }
            let key = *key;
            let stand_ins = gpu.stand_ins.get_or_insert_with(|| {
                let flat = |rgba: [u8; 4], srgb| {
                    let texture = crate::Texture::from_rgba8(1, 1, rgba.to_vec()).unwrap();
                    upload(frame.device, frame.queue, &texture, srgb).expect("its pixels, held")
                };
                [
                    flat([255; 4], true),
                    flat([255; 4], false),
                    flat([128, 128, 255, 255], false),
                    flat([255; 4], true),
                    flat([255; 4], false),
                ]
            });
            let views = &gpu.views;
            let (layout, sampler) = (&gpu.texture_layout, &gpu.texture_sampler);
            let (placed_layout, wrapped) = (&mut gpu.placed_layout, &mut gpu.wrapped_samplers);
            let entry = gpu.bindings.entry(key).or_insert_with(|| {
                let view = |i: usize| {
                    d.maps[i]
                        .and_then(|t| views.get(&(t.id(), crate::Map::ALL[i].srgb())))
                        .map_or(&stand_ins[i], |(view, _)| view)
                };
                let wrap = wrap_of(d);
                let sampler = if wrap == [crate::Wrap::Repeat; 2] {
                    sampler
                } else {
                    wrapped
                        .entry(wrap)
                        .or_insert_with(|| texture_sampler(frame.device, wrap))
                };
                let rows = d.placement.filter(|_| placed(d)).map(|placement| {
                    use wgpu::util::DeviceExt;
                    let rows = placement.map(|p| p.rows());
                    frame
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("3D map placements"),
                            contents: bytemuck::cast_slice(&rows),
                            usage: wgpu::BufferUsages::UNIFORM,
                        })
                });
                let layout = match rows {
                    Some(_) => placed_layout.get_or_insert_with(|| map_layout(frame.device, true)),
                    None => layout,
                };
                (
                    map_binding(
                        frame.device,
                        layout,
                        sampler,
                        std::array::from_fn(view),
                        rows.as_ref(),
                    ),
                    0,
                )
            });
            entry.1 = gpu.frame;
        }
        let now = gpu.frame;
        gpu.geometries
            .retain(|_, buffers| now - buffers.used <= KEEP_FRAMES);
        gpu.views.retain(|_, (_, used)| now - *used <= KEEP_FRAMES);
        gpu.bindings
            .retain(|_, (_, used)| now - *used <= KEEP_FRAMES);

        // Runs of one geometry and texture, of one pass (opaque, then
        // translucent).
        let variant_of = |d: &crate::world::Drawn| Variant {
            textured: d.maps.iter().any(Option::is_some),
            skinned: skinned(d),
            lit,
            morphing: morphing(d),
            two_sided: d.surface.double_sided,
            // Cut by a base colour texture's alpha: without one, whole.
            masked: d.surface.alpha_cutoff.is_some() && d.maps[0].is_some(),
            placed: placed(d),
            colored: d.geometry.colors().is_some(),
        };
        // Of one geometry and maps, a world without poses or surfaces has
        // one variant.
        let one_variant = !posing && !world.has_surfaces();
        let runs = &mut scratch.runs;
        runs.clear();
        let mut start = 0;
        while start < drawn.len() {
            let first = drawn[start];
            let (geometry, glass, maps, variant) = (
                first.geometry.id(),
                translucent(first),
                order[start].0.1,
                variant_of(first),
            );
            let end = start
                + drawn[start..]
                    .iter()
                    .zip(&order[start..])
                    .take_while(|(d, ((_, set), _))| {
                        d.geometry.id() == geometry
                            && translucent(d) == glass
                            && *set == maps
                            && (one_variant || variant_of(d) == variant)
                    })
                    .count();
            runs.push(Run {
                geometry,
                glass,
                maps,
                variant,
                instances: start as u32..end as u32,
            });
            start = end;
        }
        // What the map would hold now; drawn only if it holds something else.
        let spare = &mut scratch.casting;
        let mut casting = shadow.map(|(matrix, _)| {
            let (_, mut meshes, mut joints, mut weights) = spare.take().unwrap_or_default();
            meshes.clear();
            joints.clear();
            weights.clear();
            let opaque = || drawn.iter().filter(|d| !translucent(d));
            meshes.extend(opaque().map(|d| {
                let cutoff = d.surface.alpha_cutoff.map_or(0, f32::to_bits);
                (d.geometry.id(), d.matrix, cutoff)
            }));
            // Poses: an empty palette has none, so no mesh need be asked.
            if !palette.is_empty() {
                joints.extend(opaque().flat_map(|d| d.palette.iter().copied().flatten().copied()));
                weights.extend(opaque().flat_map(|d| d.morph.iter().copied().flatten().copied()));
            }
            (matrix, meshes, joints, weights)
        });
        if casting.is_some() {
            for run in runs.iter() {
                let Variant {
                    skinned,
                    morphing,
                    masked,
                    placed,
                    ..
                } = run.variant;
                // Only a cut-out shadow reads a map.
                let placed = masked && placed;
                let key = (skinned, morphing, masked, placed);
                if run.glass
                    || key == (false, false, false, false)
                    || gpu.shadow_variants.contains_key(&key)
                {
                    continue;
                }
                make_shadow_pipeline(gpu, frame.device, run.variant);
            }
        }
        if casting.is_some()
            && casting != gpu.shadow_drawn
            && let (Some(map), Some((instances_buffer, _))) = (&gpu.shadow_map, &gpu.instances)
        {
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("3D shadows"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: map,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
            pass.set_bind_group(0, &gpu.shadow_bind, &[]);
            pass.set_vertex_buffer(1, instances_buffer.slice(..));
            // Translucent meshes cast none.
            for run in runs.iter().filter(|run| !run.glass) {
                let buffers = &gpu.geometries[&run.geometry];
                let Variant {
                    skinned,
                    morphing,
                    masked,
                    placed,
                    ..
                } = run.variant;
                let placed = masked && placed;
                match gpu
                    .shadow_variants
                    .get(&(skinned, morphing, masked, placed))
                {
                    Some(pipeline) => {
                        pass.set_pipeline(pipeline);
                        let maps = gpu.bindings.get(&run.maps).filter(|_| masked);
                        pass.set_bind_group(
                            1,
                            maps.map_or(&gpu.empty_bind, |(binding, _)| binding),
                            &[],
                        );
                        if let Some((_, _, joints)) =
                            gpu.palettes.as_ref().filter(|_| skinned || morphing)
                        {
                            pass.set_bind_group(2, joints, &[]);
                        }
                        if let Some(skin) = buffers.skin.as_ref().filter(|_| skinned) {
                            pass.set_vertex_buffer(2, skin.slice(..));
                        }
                        if let Some(uvs) = buffers.second_uvs.as_ref().filter(|_| placed) {
                            pass.set_vertex_buffer(2 + u32::from(skinned), uvs.slice(..));
                        }
                        if let Some(morphs) = buffers.morphs.as_ref().filter(|_| morphing) {
                            pass.set_bind_group(3, morphs, &[]);
                        }
                    }
                    None => pass.set_pipeline(&gpu.shadow),
                }
                pass.set_vertex_buffer(0, buffers.vertices.slice(..));
                pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..buffers.count, 0, run.instances.clone());
            }
            drop(pass);
            *spare = std::mem::replace(&mut gpu.shadow_drawn, casting.take());
        }
        if casting.is_some() {
            *spare = casting;
        }

        for run in runs.iter() {
            let variant = run.variant;
            if variant == Variant::default() || gpu.variants.contains_key(&variant) {
                continue;
            }
            make_main_pipelines(gpu, frame.device, variant);
        }
        let (_, colour, depth_view) = gpu.targets.as_ref().unwrap();
        let clear = world
            .background
            .map_or(wgpu::Color::TRANSPARENT, |[r, g, b, a]| wgpu::Color {
                r: f64::from(r),
                g: f64::from(g),
                b: f64::from(b),
                a: f64::from(a),
            });
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D"),
                // Multisampled, resolved into the target; else into it.
                color_attachments: &[Some(match colour {
                    Some(colour) => wgpu::RenderPassColorAttachment {
                        view: colour,
                        depth_slice: None,
                        resolve_target: Some(target),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Discard,
                        },
                    },
                    None => wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        let Some((instances_buffer, _)) = &gpu.instances else {
            return;
        };
        let frame_bind = match &gpu.environment_bind {
            Some(bind) if lit => bind,
            _ => &gpu.bind,
        };
        pass.set_bind_group(0, frame_bind, &[]);
        pass.set_vertex_buffer(1, instances_buffer.slice(..));
        // Each run sets only what differs from the run before: wgpu records
        // and checks every call, the same pipeline or buffer again too.
        let mut bound = Bound::default();
        for run in runs.iter() {
            let buffers = &gpu.geometries[&run.geometry];
            let maps = gpu.bindings.get(&run.maps).map(|(binding, _)| binding);
            let skin = buffers.skin.as_ref().filter(|_| run.variant.skinned);
            let morphs = buffers.morphs.as_ref().filter(|_| run.variant.morphing);
            let palette = gpu
                .palettes
                .as_ref()
                .filter(|_| run.variant.skinned || run.variant.morphing);
            match gpu.variants.get(&run.variant) {
                Some(pipelines) => {
                    bound.pipeline(&mut pass, &pipelines[usize::from(run.glass)]);
                    bound.group(&mut pass, 1, maps.unwrap_or(&gpu.empty_bind));
                    if let Some((_, _, palette)) = palette {
                        bound.group(&mut pass, 2, palette);
                    }
                    if let Some(skin) = skin {
                        bound.vertices(&mut pass, 2, skin);
                    }
                    if let Some(uvs) = buffers.second_uvs.as_ref().filter(|_| run.variant.placed) {
                        let slot = 2 + u32::from(run.variant.skinned);
                        bound.vertices(&mut pass, slot, uvs);
                    }
                    if let Some(colors) = buffers.colors.as_ref().filter(|_| run.variant.colored) {
                        let Variant {
                            skinned, placed, ..
                        } = run.variant;
                        let slot = 2 + u32::from(skinned) + u32::from(placed);
                        bound.vertices(&mut pass, slot, colors);
                    }
                    if let Some(morphs) = morphs {
                        bound.group(&mut pass, 3, morphs);
                    }
                }
                None => bound.pipeline(&mut pass, &gpu.plain[usize::from(run.glass)]),
            }
            bound.vertices(&mut pass, 0, &buffers.vertices);
            bound.indices(&mut pass, &buffers.indices);
            pass.draw_indexed(0..buffers.count, 0, run.instances.clone());
        }
        drop(pass);
        scratch.sorted = recycle(drawn);
        scratch.all = recycle(all);
    }
    fn device_changed(&mut self) {
        self.gpu = None;
    }
}

/// `d`'s instance: its place, normals' matrix, material, and where its joint
/// matrices start (`base`) if it is skinned.
fn instance(d: &crate::world::Drawn, base: u32) -> Instance {
    let m = &d.matrix.columns;
    // The inverse transpose of the upper 3x3 (its cofactors, over the
    // determinant): normals stay square to surfaces under any scale.
    let (a, b, c) = (
        Vec3::from([m[0][0], m[0][1], m[0][2]]),
        Vec3::from([m[1][0], m[1][1], m[1][2]]),
        Vec3::from([m[2][0], m[2][1], m[2][2]]),
    );
    let (x, y, z) = (b.cross(c), c.cross(a), a.cross(b));
    let determinant = a.dot(x);
    let inv = if determinant.abs() > 1e-12 {
        1. / determinant
    } else {
        0.
    };
    let material = d.material;
    let column = |v: Vec3, w: f32| [v.x * inv, v.y * inv, v.z * inv, w];
    Instance {
        model: d.matrix.columns,
        normal: [
            column(x, material.metallic.clamp(0., 1.)),
            column(y, material.roughness.clamp(0.02, 1.)),
            column(z, surface_word(d)),
        ],
        color: material
            .color
            .map(|c| if c.is_finite() { c.max(0.) } else { 0. }),
        emissive: [
            material.emissive[0],
            material.emissive[1],
            material.emissive[2],
            base as f32,
        ],
    }
}

/// `d`'s morph weights into the frame's `palette`: a matrix saying how many
/// are active and how many targets it has, then matrices of `[target,
/// weight]`s, one a column (the shader's `morph`). Joints follow.
fn morph_block(d: &crate::world::Drawn, palette: &mut Vec<[[f32; 4]; 4]>) {
    let active = d.morph.unwrap_or_default();
    let targets = d.geometry.morphs().map_or(0, |m| m.targets());
    palette.push([
        [active.len() as f32, targets as f32, 0., 0.],
        [0.; 4],
        [0.; 4],
        [0.; 4],
    ]);
    for pairs in active.chunks(4) {
        palette.push(std::array::from_fn(|column| {
            let [target, weight] = pairs.get(column).copied().unwrap_or([0.; 2]);
            [target, weight, 0., 0.]
        }));
    }
}

/// A skinned, morphing or cut-out shadow's pipeline for `variant`'s meshes.
/// Made once, rarely: out of the frame's code.
#[cold]
#[inline(never)]
fn make_shadow_pipeline(gpu: &mut Gpu, device: &wgpu::Device, variant: Variant) {
    let Variant {
        skinned,
        morphing,
        masked,
        placed,
        ..
    } = variant;
    let placed = masked && placed;
    let key = (skinned, morphing, masked, placed);
    let variant = Variant {
        textured: masked,
        skinned,
        morphing,
        masked,
        placed,
        ..Variant::default()
    };
    let source = shader_source(variant);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D skinned, morphing or cut-out shadows"),
        source: wgpu::ShaderSource::Wgsl(compact(source).into()),
    });
    let group1 = match &gpu.placed_layout {
        Some(layout) if placed => layout,
        _ if masked => &gpu.texture_layout,
        _ => &gpu.empty_layout,
    };
    let layouts = [
        Some(&gpu.shadow_layout),
        Some(group1),
        Some(&gpu.palette_layout),
        Some(&gpu.morph_layout),
    ];
    let count = match (skinned, morphing) {
        (_, true) => 4,
        (true, false) => 3,
        (false, false) => 2,
    };
    let pipeline = shadow_pipeline(device, &shader, &layouts[..count], variant);
    gpu.shadow_variants.insert(key, pipeline);
}
/// `variant`'s pipelines, opaque and translucent. Made once, rarely: out
/// of the frame's code.
#[cold]
#[inline(never)]
fn make_main_pipelines(gpu: &mut Gpu, device: &wgpu::Device, variant: Variant) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D mesh variant"),
        source: wgpu::ShaderSource::Wgsl(
            compact(antialiased(shader_source(variant), gpu.samples)).into(),
        ),
    });
    let group0 = match &gpu.environment_layout {
        Some((layout, _)) if variant.lit => layout,
        _ => &gpu.layout,
    };
    let group1 = match &gpu.placed_layout {
        Some(layout) if variant.placed => layout,
        _ if variant.textured => &gpu.texture_layout,
        _ => &gpu.empty_layout,
    };
    let layouts = [
        Some(group0),
        Some(group1),
        Some(&gpu.palette_layout),
        Some(&gpu.morph_layout),
    ];
    let count = match (variant.skinned, variant.morphing) {
        (_, true) => 4,
        (true, false) => 3,
        (false, false) => 2,
    };
    let pipelines = main_pipelines(
        device,
        (gpu.format, gpu.samples),
        &shader,
        &layouts[..count],
        variant,
    );
    gpu.variants.insert(variant, pipelines);
}
/// What a pass has bound, by address, so a run sets only what differs from
/// the run before (setting the same again changes nothing but costs wgpu a
/// recorded and checked command).
#[derive(Default)]
struct Bound {
    pipeline: Option<*const wgpu::RenderPipeline>,
    groups: [Option<*const wgpu::BindGroup>; 4],
    vertices: [Option<*const wgpu::Buffer>; 6],
    indices: Option<*const wgpu::Buffer>,
}
impl Bound {
    fn pipeline(&mut self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline) {
        if self.pipeline.replace(pipeline) != Some(pipeline as *const _) {
            pass.set_pipeline(pipeline);
        }
    }
    fn group(&mut self, pass: &mut wgpu::RenderPass<'_>, index: u32, group: &wgpu::BindGroup) {
        if self.groups[index as usize].replace(group) != Some(group as *const _) {
            pass.set_bind_group(index, group, &[]);
        }
    }
    fn vertices(&mut self, pass: &mut wgpu::RenderPass<'_>, slot: u32, buffer: &wgpu::Buffer) {
        if self.vertices[slot as usize].replace(buffer) != Some(buffer as *const _) {
            pass.set_vertex_buffer(slot, buffer.slice(..));
        }
    }
    fn indices(&mut self, pass: &mut wgpu::RenderPass<'_>, buffer: &wgpu::Buffer) {
        if self.indices.replace(buffer) != Some(buffer as *const _) {
            pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint32);
        }
    }
}
/// Whether any of `d`'s maps reads other than its first coordinates as they
/// are: those need the placed pipelines, a wrap alone only a sampler.
fn placed(d: &crate::world::Drawn) -> bool {
    d.placement.is_some_and(|placement| {
        let mut maps = placement.iter().zip(d.maps);
        maps.any(|(p, map)| map.is_some() && !p.in_place())
    })
}
/// How `d`'s maps repeat: its first map's wrap, its base colour's if it has
/// one (a mesh's maps share one sampler).
fn wrap_of(d: &crate::world::Drawn) -> [crate::Wrap; 2] {
    let Some(placement) = d.placement else {
        return [crate::Wrap::Repeat; 2];
    };
    let mut maps = placement.iter().zip(d.maps);
    maps.find(|(_, map)| map.is_some())
        .map_or([crate::Wrap::Repeat; 2], |(p, _)| p.wrap)
}
/// The maps `d` draws with, as a key: their ids (`Map::ALL`'s, 0 for none),
/// the first mixed with its placements' numbers, so meshes placed alike share
/// a binding. (No `Hasher`: this runs for every mesh, a frame, and a second
/// `DefaultHasher` caller outlined the hash of the lookups it's for.)
fn map_set(d: &crate::world::Drawn) -> [u64; MAPS] {
    let mut maps = d.maps.map(|map| map.map_or(0, crate::Texture::id));
    if let Some(placement) = d.placement
        && maps != [0; MAPS]
    {
        maps[0] ^= placement_key(placement);
    }
    maps
}
/// A number for what `placement` says, never 0.
fn placement_key(placement: &[crate::MapPlacement; MAPS]) -> u64 {
    let numbers = placement.iter().flat_map(crate::MapPlacement::numbers);
    numbers.fold(0xcbf2_9ce4_8422_2325, |key, number| {
        (key ^ u64::from(number)).wrapping_mul(0x0100_0000_01b3)
    }) | 1
}

/// The shader's third material word: unlit from 1 up, lit below, with the
/// alpha cutoff as how far from there (`cutoff_of` undoes it): 1 + c, or −c.
fn surface_word(d: &crate::world::Drawn) -> f32 {
    let cutoff = d.surface.alpha_cutoff.map_or(0., |c| c.max(0.));
    if d.material.unlit {
        1. + cutoff
    } else {
        -cutoff
    }
}

fn frame_uniforms(world: &Inner, view_projection: Mat4, encode: bool) -> FrameUniforms {
    let mut uniforms = FrameUniforms {
        view_projection: view_projection.columns,
        camera: [
            world.camera.position.x,
            world.camera.position.y,
            world.camera.position.z,
            1.,
        ],
        sun_direction: [0., -1., 0., 0.],
        sun_color: [0.; 4],
        sky: [0.; 4],
        ground: [0.; 4],
        points: [[[0.; 4]; 3]; POINT_LIGHTS],
        params: [0., if encode { 1. } else { 0. }, 1., 0.],
        shadow: Mat4::IDENTITY.columns,
        shadow_texel: [0.; 4],
        environment: [0.; 4],
        irradiance: [[0.; 4]; 9],
        look: [
            match world.tone_mapping {
                crate::ToneMapping::Neutral => 0.,
                crate::ToneMapping::Aces => 1.,
                crate::ToneMapping::Linear => 2.,
            },
            0.,
            0.,
            0.,
        ],
    };
    uniforms.params[2] = world.exposure.exp2();
    if let Some(environment) = &world.environment {
        let (sin, cos) = environment.rotation.sin_cos();
        uniforms.environment = [
            environment.intensity,
            cos,
            sin,
            (crate::environment::LEVELS - 1) as f32,
        ];
        for (row, k) in uniforms
            .irradiance
            .iter_mut()
            .zip(&environment.data.irradiance)
        {
            *row = [k[0], k[1], k[2], 0.];
        }
    }
    // The point lights nearest the camera's target, nearest first, past the
    // eight a frame shades with (ties in the order they were added).
    let target = world.camera.target;
    // Nearest first, in place: `shaded` of them so far.
    let mut points = [(f32::INFINITY, [[0.; 4]; 3]); POINT_LIGHTS];
    let mut shaded = 0;
    world.for_each_light(|light, spot| {
        match light {
            // The first sun shades; others add to it as one.
            Light::Directional {
                direction,
                color,
                intensity,
            } => {
                let d = direction.normalized();
                uniforms.sun_direction = [d.x, d.y, d.z, 0.];
                for (sum, c) in uniforms.sun_color.iter_mut().zip(color) {
                    *sum += c * intensity;
                }
            }
            Light::Hemisphere {
                sky,
                ground,
                intensity,
            } => {
                for (sum, c) in uniforms.sky.iter_mut().zip(sky) {
                    *sum += c * intensity;
                }
                for (sum, c) in uniforms.ground.iter_mut().zip(ground) {
                    *sum += c * intensity;
                }
            }
            Light::Point {
                position,
                color,
                intensity,
                range,
            } => {
                let distance = (position - target).length();
                let at = points[..shaded]
                    .iter()
                    .position(|(nearer, _)| distance.total_cmp(nearer).is_lt())
                    .unwrap_or(shaded);
                if at < POINT_LIGHTS {
                    // A spot's cone as glTF fades it: (cos θ · scale +
                    // offset)², θ from its direction; all round, 1.
                    let cone = spot.map_or([0., 0., 0., 1.], |spot| {
                        let (inner, outer) = (spot.inner.cos(), spot.outer.cos());
                        let scale = 1. / (inner - outer).max(1e-3);
                        let d = spot.direction.normalized() * scale;
                        [d.x, d.y, d.z, -outer * scale]
                    });
                    let slot = [
                        [position.x, position.y, position.z, range.max(1e-3)],
                        [color[0], color[1], color[2], intensity],
                        cone,
                    ];
                    // The farthest falls off the end when they're all taken.
                    points.copy_within(at..POINT_LIGHTS - 1, at + 1);
                    points[at] = (distance, slot);
                    shaded = (shaded + 1).min(POINT_LIGHTS);
                }
            }
        }
    });
    for (slot, (_, light)) in uniforms.points.iter_mut().zip(&points[..shaded]) {
        *slot = *light;
    }
    uniforms.params[0] = shaded as f32;
    uniforms
}

/// The box holding the box `(low, high)` after `matrix`, which has no
/// perspective: what its eight corners would give, each axis's least and
/// greatest added up.
fn box_through(matrix: &Mat4, (low, high): (Vec3, Vec3)) -> (Vec3, Vec3) {
    let c = &matrix.columns;
    let (mut from, mut to) = ([c[3][0], c[3][1], c[3][2]], [c[3][0], c[3][1], c[3][2]]);
    for (column, (l, h)) in c
        .iter()
        .zip([(low.x, high.x), (low.y, high.y), (low.z, high.z)])
    {
        for row in 0..3 {
            let (a, b) = (column[row] * l, column[row] * h);
            from[row] += a.min(b);
            to[row] += a.max(b);
        }
    }
    (Vec3::from(from), Vec3::from(to))
}

/// The sun going `direction` as it sees the opaque meshes of `all`: an
/// orthographic view holding them all, onto the shadow map, and how wide a
/// map texel is in the scene. `None` with nothing to cast a shadow.
fn sun_view(all: &[crate::world::Drawn], direction: Vec3) -> Option<(Mat4, f32)> {
    // A mesh's box, and what places it. A skinned mesh's is placed by each
    // of its joints: about where its pose can take it.
    fn placed<'a>(d: &'a crate::world::Drawn) -> ((Vec3, Vec3), &'a [Mat4]) {
        let bounds = match &d.morph {
            Some(active) => d.geometry.morphed_bounds(active),
            None => d.geometry.bounds(),
        };
        let matrices = d.palette;
        (bounds, matrices.unwrap_or(std::slice::from_ref(&d.matrix)))
    }
    let opaque = || all.iter().filter(|d| d.material.color[3] >= 1.);
    let (mut low, mut high) = (Vec3::from([f32::MAX; 3]), Vec3::from([f32::MIN; 3]));
    for d in opaque() {
        let (bounds, matrices) = placed(d);
        for matrix in matrices {
            let (from, to) = box_through(matrix, bounds);
            low = low.min(from);
            high = high.max(to);
        }
    }
    let direction = direction.normalized();
    if !(low.is_finite() && high.is_finite()) || direction == Vec3::ZERO {
        return None;
    }
    let centre = (low + high) * 0.5;
    let radius = ((high - low).length() * 0.5).max(1e-3);
    let view = Mat4::look_at(centre - direction * (radius * 2.), centre, Vec3::Y);
    let (mut near, mut far) = (Vec3::from([f32::MAX; 3]), Vec3::from([f32::MIN; 3]));
    for d in opaque() {
        let (bounds, matrices) = placed(d);
        for matrix in matrices {
            let (from, to) = box_through(&(view * *matrix), bounds);
            near = near.min(from);
            far = far.max(to);
        }
    }
    // A texel's margin about them, so the filter's edge falls outside.
    let margin = (far.x - near.x).max(far.y - near.y) / SHADOW_SIZE as f32 * 2.;
    let (l, r) = (near.x - margin, far.x + margin);
    let (b, t) = (near.y - margin, far.y + margin);
    // View space looks down -z: the nearest is the greatest z.
    let (n, f) = (-far.z - margin, -near.z + margin);
    let projection = Mat4 {
        columns: [
            [2. / (r - l), 0., 0., 0.],
            [0., 2. / (t - b), 0., 0.],
            [0., 0., 1. / (n - f), 0.],
            [-(r + l) / (r - l), -(t + b) / (t - b), n / (n - f), 1.],
        ],
    };
    let texel = (r - l).max(t - b) / SHADOW_SIZE as f32;
    Some((projection * view, texel))
}

impl Gpu {
    /// The shadow map made while `on` (dropped while off), and the frame's
    /// binding following it.
    fn shadows(&mut self, device: &wgpu::Device, on: bool) {
        if on == self.shadow_map.is_some() {
            return;
        }
        self.shadow_map = on.then(|| depth_map(device, SHADOW_SIZE, "3D shadow map"));
        self.shadow_drawn = None;
        let map = self.shadow_map.as_ref().unwrap_or(&self.stand_in);
        self.bind = frame_binding(
            device,
            &self.layout,
            &self.uniforms,
            map,
            &self.sampler,
            None,
        );
        self.environment_bind = None;
    }
    /// The environment `environment`'s cube uploaded (once each) and the
    /// frame's binding with it.
    fn environment(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        environment: &crate::Environment,
    ) {
        let data = &environment.data;
        if self
            .environment
            .as_ref()
            .is_none_or(|(id, _)| *id != data.id)
        {
            let levels: Vec<&[u8]> = data
                .levels
                .iter()
                .map(|level| bytemuck::cast_slice(level.as_slice()))
                .collect();
            let cube = zgx_gpu::viewport::cube_with_levels(
                device,
                queue,
                crate::environment::SIDE,
                &levels,
                "3D environment",
            );
            let view = cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            });
            self.environment = Some((data.id, view));
            self.environment_bind = None;
        }
        let (layout, sampler) = self
            .environment_layout
            .get_or_insert_with(|| environment_layout(device));
        if self.environment_bind.is_none() {
            let map = self.shadow_map.as_ref().unwrap_or(&self.stand_in);
            let cube = self.environment.as_ref().map(|(_, view)| (view, &*sampler));
            self.environment_bind = Some(frame_binding(
                device,
                layout,
                &self.uniforms,
                map,
                &self.sampler,
                cube,
            ));
        }
    }
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, samples: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D"),
            source: wgpu::ShaderSource::Wgsl(
                compact(antialiased(shader_source(Variant::default()), samples)).into(),
            ),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D frame"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D frame"),
            entries: &[
                uniform_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("3D shadow map"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let stand_in = depth_map(device, 1, "3D shadow stand-in");
        let bind = frame_binding(device, &layout, &uniforms, &stand_in, &sampler, None);
        let texture_layout = map_layout(device, false);
        let texture_sampler = texture_sampler(device, [crate::Wrap::Repeat; 2]);
        let plain = main_pipelines(
            device,
            (format, samples),
            &shader,
            &[Some(&layout)],
            Variant::default(),
        );
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D shadow frame"),
            entries: &[uniform_entry],
        });
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D shadow frame"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let shadow = shadow_pipeline(device, &shader, &[Some(&shadow_layout)], Variant::default());
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D nothing"),
            entries: &[],
        });
        let empty_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D nothing"),
            layout: &empty_layout,
            entries: &[],
        });
        let palette_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D joints"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let morph_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D morph targets"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        Self {
            format,
            plain,
            variants: HashMap::new(),
            morph_layout,
            environment_layout: None,
            environment: None,
            environment_bind: None,
            shadow_variants: HashMap::new(),
            shadow_layout,
            empty_layout,
            empty_bind,
            palette_layout,
            palettes: None,
            skinning: device.limits().max_storage_buffers_per_shader_stage > 0,
            uniforms,
            layout,
            bind,
            shadow,
            shadow_bind,
            sampler,
            shadow_map: None,
            stand_in,
            shadow_drawn: None,
            texture_layout,
            texture_sampler,
            placed_layout: None,
            wrapped_samplers: HashMap::new(),
            views: HashMap::new(),
            bindings: HashMap::new(),
            stand_ins: None,
            instances: None,
            geometries: HashMap::new(),
            targets: None,
            samples,
            frame: 0,
        }
    }
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4,
    7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4
];
const SKIN_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![12 => Uint16x4, 13 => Float32x4];
const SECOND_UV_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![14 => Float32x2];
const COLOR_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![15 => Float32x4];
/// The shader, sampling textures or not, skinning or not, lit by an
/// environment or not: its lines marked `//T ` are a textured pipeline's
/// alone (its maps' bindings, samples and normal mapping), `//S ` a skinned
/// one's (its joints), `//M ` a morphing one's (its targets), `//P ` either's
/// (the frame's joints and weights), `//E ` an environment-lit one's (its
/// cube and diffuse light), `//U ` a placed one's (its maps' placements and
/// second coordinates), `//C ` a colored one's (its vertex colours), `//K ` a cut-out one's and `//N ` any other's,
/// comments in others'. (A cut-out or placed one is textured; a line with
/// two prefixes is for a variant with both.)
fn shader_source(variant: Variant) -> String {
    let mut source = SHADER.to_owned();
    if variant.textured {
        source = source.replace("//T ", "");
    }
    if variant.skinned {
        source = source.replace("//S ", "");
    }
    if variant.morphing {
        source = source.replace("//M ", "");
    }
    if variant.skinned || variant.morphing {
        source = source.replace("//P ", "");
    }
    if variant.lit {
        source = source.replace("//E ", "");
    }
    if variant.placed {
        source = source.replace("//U ", "");
    }
    if variant.colored {
        source = source.replace("//C ", "");
    }
    source.replace(if variant.masked { "//K " } else { "//N " }, "")
}
/// `source` drawing `samples` a pixel: multisampled, its `//A ` lines
/// (geometric specular anti-aliasing) are its own; at one sample they stay
/// comments, and cost nothing.
/// `source` without its comment lines: naga reads every byte of a shader,
/// and a variant's are half other variants' lines, commented out.
fn compact(source: String) -> String {
    let mut out = String::with_capacity(source.len() / 2);
    for line in source.lines().filter(|l| !l.trim_start().starts_with("//")) {
        out.push_str(line);
        out.push('\n');
    }
    out
}
fn antialiased(source: String, samples: u32) -> String {
    if samples > 1 {
        source.replace("//A ", "")
    } else {
        source
    }
}
/// Group 1's layout: the maps, their sampler and, `placed`, their placements.
fn map_layout(device: &wgpu::Device, placed: bool) -> wgpu::BindGroupLayout {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = (0..MAPS as u32)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: MAPS as u32,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    if placed {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: MAPS as u32 + 1,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("3D maps"),
        entries: &entries,
    })
}
/// A mipmapped, anisotropic sampler wrapping as `wrap` says along u and v.
fn texture_sampler(device: &wgpu::Device, wrap: [crate::Wrap; 2]) -> wgpu::Sampler {
    let mode = |wrap| match wrap {
        crate::Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
        crate::Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
        _ => wgpu::AddressMode::Repeat,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("3D texture"),
        address_mode_u: mode(wrap[0]),
        address_mode_v: mode(wrap[1]),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    })
}
/// The vertex buffers a pipeline reads: vertices, instances, a skinned
/// one's joints and weights, and a placed one's second coordinates.
fn vertex_buffers(variant: Variant) -> Vec<wgpu::VertexBufferLayout<'static>> {
    let mut buffers = vec![
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::geometry::Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        },
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        },
    ];
    if variant.skinned {
        buffers.push(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::geometry::Influence>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SKIN_ATTRIBUTES,
        });
    }
    if variant.placed {
        buffers.push(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SECOND_UV_ATTRIBUTES,
        });
    }
    if variant.colored {
        buffers.push(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 4]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &COLOR_ATTRIBUTES,
        });
    }
    buffers
}
/// Depth from the sun with `shader`, bound as `layouts` say; a cut-out
/// `variant`'s by its alpha in a fragment shader (depth alone needs none
/// otherwise).
fn shadow_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layouts: &[Option<&wgpu::BindGroupLayout>],
    variant: Variant,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("3D shadows"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("3D shadows"),
                bind_group_layouts: layouts,
                immediate_size: 0,
            }),
        ),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("shadow_vertex"),
            compilation_options: Default::default(),
            buffers: &vertex_buffers(variant),
        },
        fragment: variant.masked.then(|| wgpu::FragmentState {
            module: shader,
            entry_point: Some("shadow_fragment"),
            compilation_options: Default::default(),
            targets: &[],
        }),
        // Both faces: a plane or an open mesh casts from either side.
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            // Kept off the surfaces they're cast on (acne), more where
            // they slope from the sun.
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.,
                clamp: 0.,
            },
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
/// The opaque and translucent meshes' pipelines with `shader`, bound as
/// `layouts` say, drawing into `format` at `samples` a pixel.
fn main_pipelines(
    device: &wgpu::Device,
    (format, samples): (wgpu::TextureFormat, u32),
    shader: &wgpu::ShaderModule,
    layouts: &[Option<&wgpu::BindGroupLayout>],
    variant: Variant,
) -> [wgpu::RenderPipeline; 2] {
    let buffers = vertex_buffers(variant);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D"),
        bind_group_layouts: layouts,
        immediate_size: 0,
    });
    let pipeline = |translucent: bool| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if translucent {
                "3D translucent"
            } else {
                "3D opaque"
            }),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: translucent.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: (!translucent && !variant.two_sided).then_some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(!translucent),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
    };
    [pipeline(false), pipeline(true)]
}

/// `texture` and its mipmaps on the GPU (written by the CPU where memory
/// is shared: no copy, see `texture_with_levels`), read as sRGB colours or
/// as data.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &crate::Texture,
    srgb: bool,
) -> Option<wgpu::TextureView> {
    let image = texture.pixels()?;
    let smaller = crate::texture::mipmaps(texture.width(), texture.height(), image.pixels());
    let levels: Vec<&[u8]> = std::iter::once(image.pixels())
        .chain(smaller.iter().map(|(_, _, pixels)| pixels.as_slice()))
        .collect();
    let made = zgx_gpu::viewport::texture_with_levels(
        device,
        queue,
        (texture.width(), texture.height()),
        if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        &levels,
        "3D map",
    );
    texture.uploaded();
    Some(made.create_view(&Default::default()))
}
/// A mesh's maps (`Map::ALL`'s order) bound for sampling.
fn map_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    maps: [&wgpu::TextureView; MAPS],
    placements: Option<&wgpu::Buffer>,
) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry> = maps
        .iter()
        .enumerate()
        .map(|(binding, view)| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .chain(std::iter::once(wgpu::BindGroupEntry {
            binding: MAPS as u32,
            resource: wgpu::BindingResource::Sampler(sampler),
        }))
        .chain(placements.map(|buffer| wgpu::BindGroupEntry {
            binding: MAPS as u32 + 1,
            resource: buffer.as_entire_binding(),
        }))
        .collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3D maps"),
        layout,
        entries: &entries,
    })
}
/// A depth texture `side` texels square, drawn to and sampled.
fn depth_map(device: &wgpu::Device, side: u32, label: &str) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
/// The main pass's frame: the uniforms, the shadow map (or stand-in), and
/// an environment-lit frame's cube and its sampler.
fn frame_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    map: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    cube: Option<(&wgpu::TextureView, &wgpu::Sampler)>,
) -> wgpu::BindGroup {
    let mut entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(map),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
    ];
    if let Some((view, cube_sampler)) = cube {
        entries.push(wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(view),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::Sampler(cube_sampler),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3D frame"),
        layout,
        entries: &entries,
    })
}
/// An environment-lit frame's layout (the frame's, then the cube and its
/// trilinear sampler) and that sampler.
fn environment_layout(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::Sampler) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("3D frame, environment-lit"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("3D environment"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    (layout, sampler)
}

/// Metallic-roughness shading (GGX, Smith, Schlick) under a sun, up to four
/// point lights and a hemisphere's ambient, mapped to display range by
/// Khronos's PBR Neutral, premultiplied.
const SHADER: &str = r#"
struct Frame {
    view_projection: mat4x4<f32>,
    camera: vec4<f32>,
    sun_direction: vec4<f32>,
    sun_color: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
    points: array<array<vec4<f32>, 3>, 8>,
    params: vec4<f32>,
    shadow: mat4x4<f32>,
    shadow_texel: vec4<f32>,
    environment: vec4<f32>,
    irradiance: array<vec4<f32>, 9>,
    look: vec4<f32>,
}
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
//E @group(0) @binding(3) var environment_cube: texture_cube<f32>;
//E @group(0) @binding(4) var environment_sampler: sampler;
//T @group(1) @binding(0) var base_texture: texture_2d<f32>;
//T @group(1) @binding(1) var metal_rough_texture: texture_2d<f32>;
//T @group(1) @binding(2) var normal_texture: texture_2d<f32>;
//T @group(1) @binding(3) var emission_texture: texture_2d<f32>;
//T @group(1) @binding(4) var occlusion_texture: texture_2d<f32>;
//T @group(1) @binding(5) var map_sampler: sampler;
//U struct Placement {
//U     u: vec4<f32>,
//U     v: vec4<f32>,
//U }
//U @group(1) @binding(6) var<uniform> placements: array<Placement, 5>;
//U // Where map `i` reads: its coordinates (the second, if its row says)
//U // through its rows.
//U fn placed(i: u32, first: vec2<f32>, second: vec2<f32>) -> vec2<f32> {
//U     let p = placements[i];
//U     let uv = vec3(select(first, second, p.u.w > 0.5), 1.0);
//U     return vec2(dot(p.u.xyz, uv), dot(p.v.xyz, uv));
//U }
//P @group(2) @binding(0) var<storage, read> palette: array<mat4x4<f32>>;
//M @group(3) @binding(0) var<storage, read> morphs: array<vec4<f32>>;
//M struct Morphed {
//M     position: vec3<f32>,
//M     normal: vec3<f32>,
//M     joints: u32,
//M }
//M // A vertex blended by the mesh's weights (from `base`: how many, how many
//M // targets, then `[target, weight]`s, four a matrix), and where its
//M // joints' matrices start.
//M fn morph(base: u32, vertex: u32, position: vec3<f32>, normal: vec3<f32>) -> Morphed {
//M     let head = palette[base][0];
//M     let blending = u32(head.x);
//M     let targets = u32(head.y);
//M     var out = Morphed(position, normal, base + 1u + (blending + 3u) / 4u);
//M     for (var i = 0u; i < blending; i++) {
//M         let pair = palette[base + 1u + i / 4u][i % 4u];
//M         let at = (vertex * targets + u32(pair.x)) * 2u;
//M         out.position += morphs[at].xyz * pair.y;
//M         out.normal += morphs[at + 1u].xyz * pair.y;
//M     }
//M     return out;
//M }
//S // A vertex's place from its joints' matrices (from `base`), weighted.
//S fn skinned(base: u32, joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
//S     return palette[base + joints.x] * weights.x + palette[base + joints.y] * weights.y
//S         + palette[base + joints.z] * weights.z + palette[base + joints.w] * weights.w;
//S }

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) material: vec3<f32>,
    @location(5) uv: vec2<f32>,
//U     @location(6) second_uv: vec2<f32>,
}

@vertex fn vertex(
    @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
    @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>, @location(5) m2: vec4<f32>, @location(6) m3: vec4<f32>,
    @location(7) n0: vec4<f32>, @location(8) n1: vec4<f32>, @location(9) n2: vec4<f32>,
    @location(10) color: vec4<f32>, @location(11) emissive: vec4<f32>,
//S     @location(12) joints: vec4<u32>, @location(13) weights: vec4<f32>,
//U     @location(14) second_uv: vec2<f32>,
//C     @location(15) vertex_color: vec4<f32>,
//M     @builtin(vertex_index) index: u32,
) -> Out {
    var model = mat4x4(m0, m1, m2, m3);
    var normals = mat3x3(n0.xyz, n1.xyz, n2.xyz);
    var at = position;
    var facing = normal;
    var joints_from = u32(emissive.w);
//M     let morphed = morph(joints_from, index, position, normal);
//M     at = morphed.position;
//M     facing = morphed.normal;
//M     joints_from = morphed.joints;
//S     model = skinned(joints_from, joints, weights);
//S     normals = mat3x3(model[0].xyz, model[1].xyz, model[2].xyz);
    let world = model * vec4(at, 1.0);
    let n = normals * facing;
    var tint = color;
//C     tint *= vertex_color;
    return Out(
        frame.view_projection * world, world.xyz, n, tint, emissive.xyz, vec3(n0.w, n1.w, n2.w), uv,
//U         second_uv,
    );
}

@vertex fn shadow_vertex(
    @location(0) position: vec3<f32>,
    @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>, @location(5) m2: vec4<f32>, @location(6) m3: vec4<f32>,
//P     @location(11) emissive: vec4<f32>,
//S     @location(12) joints: vec4<u32>, @location(13) weights: vec4<f32>,
//M     @builtin(vertex_index) index: u32,
//U     @location(14) second_uv: vec2<f32>,
//K     @location(2) uv: vec2<f32>, @location(9) n2: vec4<f32>,
//N ) -> @builtin(position) vec4<f32> {
//K ) -> ShadowOut {
    var model = mat4x4(m0, m1, m2, m3);
    var at = position;
//P     var joints_from = u32(emissive.w);
//M     let morphed = morph(joints_from, index, position, vec3(0.0));
//M     at = morphed.position;
//M     joints_from = morphed.joints;
//S     model = skinned(joints_from, joints, weights);
//N     return frame.shadow * model * vec4(at, 1.0);
//K     var cut_uv = uv;
//K //U     cut_uv = placed(0u, uv, second_uv);
//K     return ShadowOut(frame.shadow * model * vec4(at, 1.0), cut_uv, n2.w);
}
//K struct ShadowOut {
//K     @builtin(position) position: vec4<f32>,
//K     @location(0) uv: vec2<f32>,
//K     @location(1) word: f32,
//K }
//K // A surface's alpha cutoff from its third material word (`surface_word`).
//K fn cutoff_of(word: f32) -> f32 {
//K     return select(-word, word - 1.0, word > 0.5);
//K }
//K @fragment fn shadow_fragment(in: ShadowOut) {
//K     if textureSample(base_texture, map_sampler, in.uv).a < cutoff_of(in.word) {
//K         discard;
//K     }
//K }

const PI: f32 = 3.14159265;

// How much of the sun reaches `world` (facing `n`): 1 lit, 0 in shadow,
// between at a shadow's edge (four filtered comparisons half a texel
// about it: each blends 2 by 2 texels, so 3 by 3 in all).
fn sunlit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    // Off the surface by a texel or so, against its own shadow.
    let p = frame.shadow * vec4(world + n * frame.shadow_texel.x * 1.5, 1.0);
    let uv = vec2(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) || p.z > 1.0 {
        return 1.0;
    }
    let half = frame.shadow_texel.y * 0.5;
    var sum = 0.0;
    sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2(-half, -half), p.z);
    sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2(half, -half), p.z);
    sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2(-half, half), p.z);
    sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2(half, half), p.z);
    return sum * 0.25;
}

fn ggx(n_h: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = n_h * n_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}
fn smith(n_v: f32, n_l: f32, a: f32) -> f32 {
    let k = a * 0.5;
    return (n_v / (n_v * (1.0 - k) + k)) * (n_l / (n_l * (1.0 - k) + k));
}
fn shade(base: vec3<f32>, metallic: f32, roughness: f32, n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, radiance: vec3<f32>) -> vec3<f32> {
    let h = normalize(v + l);
    let n_l = max(dot(n, l), 0.0);
    let n_v = max(dot(n, v), 1e-4);
    let n_h = max(dot(n, h), 0.0);
    let a = roughness * roughness;
    let f0 = mix(vec3(0.04), base, metallic);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    let specular = fresnel * ggx(n_h, a) * smith(n_v, n_l, a) / (4.0 * n_v * max(n_l, 1e-4));
    let diffuse = (1.0 - fresnel) * (1.0 - metallic) * base / PI;
    return (diffuse + specular) * radiance * n_l;
}
fn neutral(color: vec3<f32>) -> vec3<f32> {
    let start = 0.76;
    let desaturation = 0.15;
    let x = min(color.r, min(color.g, color.b));
    let offset = select(0.04, x - 6.25 * x * x, x < 0.08);
    var c = color - offset;
    let peak = max(c.r, max(c.g, c.b));
    if peak < start {
        return c;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    c = c * (new_peak / peak);
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(c, vec3(new_peak), g);
}
//E // Where the environment's light along `d` comes from: the room turned
//E // by its rotation about the vertical.
//E fn turned(d: vec3<f32>) -> vec3<f32> {
//E     let c = frame.environment.y;
//E     let s = frame.environment.z;
//E     return vec3(c * d.x - s * d.z, d.y, s * d.x + c * d.z);
//E }
//E // Its diffuse light over π facing `n` (nine spherical harmonics).
//E fn irradiance(n: vec3<f32>) -> vec3<f32> {
//E     let k = frame.irradiance;
//E     return k[0].rgb + k[1].rgb * n.y + k[2].rgb * n.z + k[3].rgb * n.x
//E         + k[4].rgb * (n.x * n.y) + k[5].rgb * (n.y * n.z) + k[6].rgb * (3.0 * n.z * n.z - 1.0)
//E         + k[7].rgb * (n.x * n.z) + k[8].rgb * (n.x * n.x - n.y * n.y);
//E }
//E // The split sum's second factor, the specular's scale and bias by
//E // angle and roughness: Karis's fit to its table (no texture).
//E fn reflectance(f0: vec3<f32>, roughness: f32, n_v: f32) -> vec3<f32> {
//E     let r = roughness * vec4(-1.0, -0.0275, -0.572, 0.022) + vec4(1.0, 0.0425, 1.04, -0.04);
//E     let a004 = min(r.x * r.x, exp2(-9.28 * n_v)) * r.x + r.y;
//E     let ab = vec2(-1.04, 1.04) * a004 + r.zw;
//E     return f0 * ab.x + ab.y;
//E }
// ACES's filmic curve, Stephen Hill's fit: into its working space, its
// reference transform and display transform as one rational curve, back.
// Exposed by 1 / 0.6 first, as three.js does, so mid grey stays about mid.
fn aces(color: vec3<f32>) -> vec3<f32> {
    let into = mat3x3(vec3(0.59719, 0.07600, 0.02840), vec3(0.35458, 0.90834, 0.13383), vec3(0.04823, 0.01566, 0.83777));
    let back = mat3x3(vec3(1.60475, -0.10208, -0.00327), vec3(-0.53108, 1.10813, -0.07276), vec3(-0.07367, -0.00605, 1.07602));
    let v = into * (color / 0.6);
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return clamp(back * (a / b), vec3(0.0), vec3(1.0));
}
fn tone_map(color: vec3<f32>) -> vec3<f32> {
    if frame.look.x > 1.5 {
        return color;
    }
    if frame.look.x > 0.5 {
        return aces(color);
    }
    return neutral(color);
}
fn encode(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3(0.0031308));
}

//T // The surface's normal tilted by a normal map's texel (0 to 1 each, half
//T // way flat), its frame from the screen's derivatives of the point and its
//T // texture coordinates (as Khronos's viewer makes it without tangents).
//T fn bumped(ng: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>, duvx: vec2<f32>, duvy: vec2<f32>, texel: vec3<f32>) -> vec3<f32> {
//T     let det = duvx.x * duvy.y - duvy.x * duvx.y;
//T     let along = (duvy.y * dpx - duvx.y * dpy) * sign(det);
//T     let t = along - ng * dot(ng, along);
//T     if abs(det) < 1e-12 || dot(t, t) < 1e-20 {
//T         return ng;
//T     }
//T     let tangent = normalize(t);
//T     let m = texel * 2.0 - 1.0;
//T     return normalize(tangent * m.x + cross(ng, tangent) * m.y + ng * m.z);
//T }

@fragment fn fragment(in: Out, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Its maps (none in an untextured pipeline): its colour (white without
    // one), in linear light; metal and roughness; which way it faces.
    var texel = vec4(1.0);
    var surface = vec4(1.0);
    var bump = vec3(0.5, 0.5, 1.0);
    var glow = vec3(1.0);
    var occlusion = 1.0;
//T     var uvs = array<vec2<f32>, 5>(in.uv, in.uv, in.uv, in.uv, in.uv);
//U     for (var i = 0u; i < 5u; i++) {
//U         uvs[i] = placed(i, in.uv, in.second_uv);
//U     }
//T     texel = textureSample(base_texture, map_sampler, uvs[0]);
//T     surface = textureSample(metal_rough_texture, map_sampler, uvs[1]);
//T     bump = textureSample(normal_texture, map_sampler, uvs[2]).xyz;
//T     glow = textureSample(emission_texture, map_sampler, uvs[3]).rgb;
//T     occlusion = textureSample(occlusion_texture, map_sampler, uvs[4]).r;
//T     let dpx = dpdx(in.world);
//T     let dpy = dpdy(in.world);
//T     let duvx = dpdx(uvs[2]);
//T     let duvy = dpdy(uvs[2]);
    // How fast the surface's normal turns across the pixel, in uniform
    // control flow: widened into its roughness below (geometric specular
    // anti-aliasing, Tokuyoshi and Kaplanyan), so a highlight on a curve
    // too fine for the pixels blurs instead of flickering.
    // Multisampled pipelines' alone: one drawing a sample a pixel skips it.
    var widening = 0.0;
//A let facing = normalize(in.normal);
//A let turning_x = dpdx(facing);
//A let turning_y = dpdy(facing);
//A widening = min(0.3 * (dot(turning_x, turning_x) + dot(turning_y, turning_y)), 0.18);
//K     if texel.a < cutoff_of(in.material.z) {
//K         discard;
//K     }
    let base = in.color.rgb * texel.rgb;
    var lit: vec3<f32>;
    if in.material.z > 0.5 {
        lit = base;
    } else {
        var n = normalize(in.normal);
        if !front {
            n = -n;
        }
//T         n = bumped(n, dpx, dpy, duvx, duvy, bump);
        let v = normalize(frame.camera.xyz - in.world);
        let metallic = in.material.x * surface.b;
        let given = clamp(in.material.y * surface.g, 0.02, 1.0);
        let a = given * given;
        let roughness = sqrt(sqrt(clamp(a * a + widening, 0.0, 1.0)));
        var sun = frame.sun_color.rgb;
        if frame.params.w > 0.5 {
            sun *= sunlit(in.world, n);
        }
        lit = shade(base, metallic, roughness, n, v, -frame.sun_direction.xyz, sun);
        for (var i = 0u; i < u32(frame.params.x); i++) {
            let light = frame.points[i];
            let to = light[0].xyz - in.world;
            let distance = length(to);
            let falloff = pow(clamp(1.0 - pow(distance / light[0].w, 4.0), 0.0, 1.0), 2.0) / max(distance * distance, 1e-4);
            // Within a spot's cone (all round: 1).
            let cone = clamp(dot(light[2].xyz, -to / distance) + light[2].w, 0.0, 1.0);
            lit += shade(base, metallic, roughness, n, v, to / distance, light[1].rgb * light[1].w * falloff * cone * cone);
        }
        // Ambient light: the hemisphere seen by the surface for its diffuse
        // part, and along its reflection, by Fresnel, for its specular one
        // (what makes metals their colour away from the lights).
        let ambient = mix(frame.ground.rgb, frame.sky.rgb, n.y * 0.5 + 0.5);
        let r = reflect(-v, n);
        let reflected = mix(frame.ground.rgb, frame.sky.rgb, r.y * 0.5 + 0.5);
        let f0 = mix(vec3(0.04), base, metallic);
        let n_v = max(dot(n, v), 0.0);
        let rough = roughness * roughness;
        let fresnel = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - n_v, 5.0);
        var surround = ambient * base * (1.0 - metallic) * (1.0 - fresnel);
        surround += reflected * fresnel * (1.0 - 0.5 * rough);
        // Lit by an environment instead: its diffuse light, and its light
        // along the reflection blurred as rough as the surface (the cube's
        // level) times the split sum's factor.
//E     let along = textureSampleLevel(environment_cube, environment_sampler, turned(r), roughness * frame.environment.w).rgb;
//E     surround = irradiance(turned(n)) * base * (1.0 - metallic) * (1.0 - fresnel);
//E     surround = (surround + along * reflectance(f0, roughness, n_v)) * frame.environment.x;
        lit += surround * occlusion;
        lit += in.emissive * glow;
        lit = tone_map(lit * frame.params.z);
    }
    if frame.params.y > 0.5 {
        lit = encode(clamp(lit, vec3(0.0), vec3(1.0)));
    }
    // A translucent mesh's texture adds its own alpha; an opaque one's
    // is ignored (glTF's OPAQUE).
    let alpha = clamp(select(1.0, in.color.a * texel.a, in.color.a < 1.0), 0.0, 1.0);
    return vec4(lit * alpha, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Camera, World, math::vec3};

    #[test]
    fn a_box_through_a_matrix_is_what_its_corners_give() {
        let (low, high) = (crate::vec3(-1., 0.5, -2.), crate::vec3(3., 2., 0.25));
        let turned = crate::Quat::from_euler(0.7, -1.2, 2.1);
        let matrix =
            Mat4::from_transform(crate::vec3(4., -3., 0.5), turned, crate::vec3(2., -1., 0.5));
        let view = Mat4::look_at(crate::vec3(3., 9., -6.), Vec3::ZERO, Vec3::Y);
        for matrix in [Mat4::IDENTITY, matrix, view * matrix] {
            let (mut from, mut to) = (Vec3::from([f32::MAX; 3]), Vec3::from([f32::MIN; 3]));
            for i in 0..8 {
                let p = matrix.project(crate::vec3(
                    if i & 1 == 0 { low.x } else { high.x },
                    if i & 2 == 0 { low.y } else { high.y },
                    if i & 4 == 0 { low.z } else { high.z },
                ));
                from = from.min(p);
                to = to.max(p);
            }
            let (a, b) = box_through(&matrix, (low, high));
            for (got, want) in [(a, from), (b, to)] {
                assert!((got - want).length() < 1e-4, "{got:?} {want:?}");
            }
        }
    }

    #[test]
    fn an_untextured_shader_binds_no_texture_an_unskinned_no_joints_and_an_unlit_no_cube() {
        let source = |change: fn(&mut Variant)| {
            let mut variant = Variant::default();
            change(&mut variant);
            shader_source(variant)
        };
        let plain = source(|_| {});
        let textured = source(|v| v.textured = true);
        let skinned = source(|v| v.skinned = true);
        let lit = source(|v| v.lit = true);
        let morphing = source(|v| v.morphing = true);
        let masked = source(|v| {
            v.textured = true;
            v.masked = true;
        });
        // In the plain shader every line naming a map, the joints or the
        // environment's cube is a comment.
        for line in plain.lines().filter(|l| {
            l.contains("_texture")
                || l.contains("@group(1)")
                || l.contains("palette")
                || l.contains("environment_cube")
                || l.contains("morphs")
                || l.contains("placements")
                || l.contains("second_uv")
                || l.contains("vertex_color")
        }) {
            assert!(line.trim_start().starts_with("//"), "{line}");
        }
        assert!(textured.contains("@group(1) @binding(2) var normal_texture"));
        assert!(!textured.contains("//T") && textured.contains("//S"));
        assert!(skinned.contains("var<storage, read> palette"));
        assert!(!skinned.contains("//S") && skinned.contains("//T"));
        assert!(lit.contains("@group(0) @binding(3) var environment_cube: texture_cube<f32>"));
        assert!(!lit.contains("//E") && lit.contains("//T"));
        assert!(morphing.contains("var<storage, read> morphs"));
        assert!(morphing.contains("var<storage, read> palette"));
        assert!(!morphing.contains("//M") && morphing.contains("//S"));
        assert!(masked.contains("discard;") && masked.contains("-> ShadowOut"));
        for line in textured.lines().filter(|l| l.contains("second_uv")) {
            assert!(line.trim_start().starts_with("//"), "{line}");
        }
        let placed = source(|v| {
            v.textured = true;
            v.placed = true;
        });
        assert!(placed.contains("var<uniform> placements"));
        let colored = source(|v| v.colored = true);
        assert!(colored.contains("@location(15) vertex_color: vec4<f32>"));
        assert!(!colored.contains("//C") && colored.contains("//S"));
        assert!(!placed.contains("//U") && placed.contains("//S"));
        for line in plain.lines().filter(|l| l.contains("discard")) {
            assert!(line.trim_start().starts_with("//"), "{line}");
        }
    }

    #[test]
    fn specular_anti_aliasing_is_a_multisampled_shaders_alone() {
        let source = shader_source(Variant::default());
        let (one, four) = (antialiased(source.clone(), 1), antialiased(source, 4));
        for line in one.lines().filter(|l| l.contains("dpdx(facing)")) {
            assert!(line.trim_start().starts_with("//"), "{line}");
        }
        assert!(four.contains("\nlet turning_x = dpdx(facing);"));
    }

    #[test]
    fn a_compact_shader_keeps_its_code_and_drops_its_comments() {
        let skinned = shader_source(Variant {
            skinned: true,
            ..Variant::default()
        });
        let compact = compact(antialiased(skinned.clone(), 4));
        assert!(compact.len() < skinned.len() / 2 + skinned.len() / 4);
        assert!(compact.lines().all(|l| !l.trim_start().starts_with("//")));
        assert!(compact.contains("var<storage, read> palette"));
        assert!(compact.contains("fn fragment"));
        // Antialiasing's lines, uncommented before, are kept.
        let antialiased = antialiased(skinned, 4);
        let code = |s: &str| {
            let lines = s.lines().filter(|l| !l.trim_start().starts_with("//"));
            lines.filter(|l| l.contains("dpdx")).count()
        };
        assert!(code(&antialiased) > 0);
        assert_eq!(code(&compact), code(&antialiased));
    }

    #[test]
    fn a_frame_shades_with_the_eight_point_lights_nearest_the_target() {
        let world = World::new();
        world.set_camera(Camera::looking_at([0., 0., 9.], [0., 0., 0.]));
        // Along x at these distances from the target; the second 1 is
        // added after the first.
        let lights = [
            (9., 1.),
            (5., 1.),
            (1., 1.),
            (7., 1.),
            (3., 1.),
            (2., 1.),
            (8., 1.),
        ];
        for (x, intensity) in lights
            .into_iter()
            .chain([(4., 1.), (6., 1.), (10., 1.), (1., 2.)])
        {
            world.add_light(Light::Point {
                position: vec3(x, 0., 0.),
                color: [1.; 3],
                intensity,
                range: 10.,
            });
        }
        let uniforms = frame_uniforms(&world.inner(), Mat4::IDENTITY, false);
        let shaded: Vec<(f32, f32)> = uniforms
            .points
            .iter()
            .map(|[position, color, _]| (position[0], color[3]))
            .collect();
        let nearest = [1., 1., 2., 3., 4., 5., 6., 7.];
        let intensities = [1., 2., 1., 1., 1., 1., 1., 1.];
        assert_eq!(
            shaded,
            nearest.into_iter().zip(intensities).collect::<Vec<_>>()
        );
        assert_eq!(uniforms.params[0], 8.);
    }
}
