//! Drawing a `World`: its view's painter. A frame is one pass with depth and
//! 4x multisampling: each geometry's meshes in one instanced draw, opaque
//! ones first, then the translucent back to front. Pipelines and buffers are
//! made with the first frame and kept; a geometry's buffers are uploaded
//! once and dropped a while after the last frame drawing it. With shadows,
//! a pass before it draws the opaque meshes' depth as the sun sees them.
use crate::{
    math::{Mat4, Vec3},
    world::{Inner, Light, POINT_LIGHTS},
};
use std::{cell::RefCell, collections::HashMap, rc::Weak};
use zgx_gpu::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

const SAMPLES: u32 = 4;
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
    points: [[[f32; 4]; 2]; POINT_LIGHTS],
    /// Point lights in use, whether to encode sRGB itself, exposure,
    /// whether the sun casts shadows.
    params: [f32; 4],
    /// The sun's view of the scene, onto the shadow map.
    shadow: [[f32; 4]; 4],
    /// A shadow map texel's size in the scene, and in the map (1 / side).
    shadow_texel: [f32; 4],
}

struct Buffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    used: u64,
}
struct Gpu {
    format: wgpu::TextureFormat,
    /// Opaque and translucent meshes', without textures: an untextured
    /// scene binds none (a bound texture costs Apple's driver 16 MB).
    plain: [wgpu::RenderPipeline; 2],
    /// Textured meshes', made with the first textured frame.
    textured: Option<[wgpu::RenderPipeline; 2]>,
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
    shadow_drawn: Option<(Mat4, Vec<(u64, Mat4)>)>,
    /// Textures by id, with the frame that last drew each.
    texture_layout: wgpu::BindGroupLayout,
    texture_sampler: wgpu::Sampler,
    /// Uploaded textures by id and whether read as sRGB (a texture may be
    /// a colour map in one place, data in another).
    views: HashMap<(u64, bool), (wgpu::TextureView, u64)>,
    /// Each set of maps meshes draw with (`Map::ALL`'s ids, 0 for none),
    /// bound.
    bindings: HashMap<[u64; 3], (wgpu::BindGroup, u64)>,
    /// What a map a mesh hasn't got reads: white (its factors as they are)
    /// and a flat normal. Made with the first textured frame.
    stand_ins: Option<[wgpu::TextureView; 3]>,
    instances: Option<(wgpu::Buffer, usize)>,
    geometries: HashMap<u64, Buffers>,
    targets: Option<((u32, u32), wgpu::TextureView, wgpu::TextureView)>,
    frame: u64,
}

pub(crate) struct Renderer {
    world: Weak<RefCell<Inner>>,
    gpu: Option<Gpu>,
}
impl Renderer {
    pub(crate) fn new(world: Weak<RefCell<Inner>>) -> Self {
        Self { world, gpu: None }
    }
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
        if self.gpu.as_ref().is_none_or(|gpu| gpu.format != format) {
            self.gpu = Some(Gpu::new(frame.device, format));
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
                        sample_count: SAMPLES,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            gpu.targets = Some((
                size,
                texture(format, "3D colour"),
                texture(DEPTH, "3D depth"),
            ));
        }

        let camera = world.camera;
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let view = camera.view();
        let view_projection = camera.projection(aspect) * view;
        let mut uniforms = frame_uniforms(&world, view_projection, encode);
        let all = world.drawn();
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
        // Opaque ones by geometry, then texture.
        let texture_of =
            |d: &crate::world::Drawn| d.maps.map(|map| map.map_or(0, crate::Texture::id));
        let key = |d: &crate::world::Drawn| {
            if translucent(d) {
                // Its depth's bits in total order, the farthest least.
                let bits = depth(&d.matrix).to_bits();
                let ordered = if bits >> 31 == 1 {
                    !bits
                } else {
                    bits | 1 << 31
                };
                (1 << 63 | u64::from(!ordered), texture_of(d))
            } else {
                (d.geometry.id() & !(1 << 63), texture_of(d))
            }
        };
        let mut order: Vec<((u64, [u64; 3]), usize)> = all.iter().map(key).zip(0..).collect();
        order.sort_unstable();
        let drawn: Vec<&crate::world::Drawn> = order.iter().map(|&(_, i)| &all[i]).collect();
        let instances: Vec<Instance> = drawn.iter().map(|d| instance(d)).collect();
        if !instances.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&instances);
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
        for d in &drawn {
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
                    count: d.geometry.indices().len() as u32,
                    used: 0,
                }
            });
            entry.used = gpu.frame;
            for (map, texture) in crate::Map::ALL.into_iter().zip(d.maps) {
                if let Some(texture) = texture {
                    let entry = gpu
                        .views
                        .entry((texture.id(), map.srgb()))
                        .or_insert_with(|| {
                            (upload(frame.device, frame.queue, texture, map.srgb()), 0)
                        });
                    entry.1 = gpu.frame;
                }
            }
        }
        // Each set of maps bound once, against stand-ins where a map is
        // missing.
        for d in &drawn {
            let key = texture_of(d);
            if key == [0; 3] {
                continue;
            }
            let stand_ins = gpu.stand_ins.get_or_insert_with(|| {
                let flat = |rgba: [u8; 4], srgb| {
                    let texture = crate::Texture::from_rgba8(1, 1, rgba.to_vec()).unwrap();
                    upload(frame.device, frame.queue, &texture, srgb)
                };
                [
                    flat([255; 4], true),
                    flat([255; 4], false),
                    flat([128, 128, 255, 255], false),
                ]
            });
            let views = &gpu.views;
            let (layout, sampler) = (&gpu.texture_layout, &gpu.texture_sampler);
            let entry = gpu.bindings.entry(key).or_insert_with(|| {
                let view = |i: usize| {
                    d.maps[i]
                        .and_then(|t| views.get(&(t.id(), crate::Map::ALL[i].srgb())))
                        .map_or(&stand_ins[i], |(view, _)| view)
                };
                (
                    map_binding(frame.device, layout, sampler, [view(0), view(1), view(2)]),
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
        let mut runs = Vec::new();
        let mut start = 0;
        while start < drawn.len() {
            let first = drawn[start];
            let (geometry, glass, texture) =
                (first.geometry.id(), translucent(first), texture_of(first));
            let end = start
                + drawn[start..]
                    .iter()
                    .take_while(|d| {
                        d.geometry.id() == geometry
                            && translucent(d) == glass
                            && texture_of(d) == texture
                    })
                    .count();
            runs.push((geometry, glass, texture, start as u32..end as u32));
            start = end;
        }
        // What the map would hold now; drawn only if it holds something else.
        let casting = shadow.map(|(matrix, _)| {
            let meshes = drawn
                .iter()
                .filter(|d| !translucent(d))
                .map(|d| (d.geometry.id(), d.matrix))
                .collect::<Vec<_>>();
            (matrix, meshes)
        });
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
            pass.set_pipeline(&gpu.shadow);
            pass.set_bind_group(0, &gpu.shadow_bind, &[]);
            pass.set_vertex_buffer(1, instances_buffer.slice(..));
            // Translucent meshes cast none.
            for (geometry, _, _, instances) in runs.iter().filter(|(_, glass, ..)| !glass) {
                let buffers = &gpu.geometries[geometry];
                pass.set_vertex_buffer(0, buffers.vertices.slice(..));
                pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..buffers.count, 0, instances.clone());
            }
            drop(pass);
            gpu.shadow_drawn = casting;
        }

        if gpu.textured.is_none() && runs.iter().any(|(_, _, maps, _)| *maps != [0; 3]) {
            let shader = frame
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("3D textured"),
                    source: wgpu::ShaderSource::Wgsl(shader_source(true).into()),
                });
            gpu.textured = Some(main_pipelines(
                frame.device,
                gpu.format,
                &shader,
                &[Some(&gpu.layout), Some(&gpu.texture_layout)],
            ));
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
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: colour,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Discard,
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
        pass.set_bind_group(0, &gpu.bind, &[]);
        pass.set_vertex_buffer(1, instances_buffer.slice(..));
        for (geometry, glass, texture, instances) in runs {
            let buffers = &gpu.geometries[&geometry];
            match (gpu.bindings.get(&texture), &gpu.textured) {
                (Some((binding, _)), Some(textured)) => {
                    pass.set_pipeline(&textured[usize::from(glass)]);
                    pass.set_bind_group(1, binding, &[]);
                }
                _ => pass.set_pipeline(&gpu.plain[usize::from(glass)]),
            }
            pass.set_vertex_buffer(0, buffers.vertices.slice(..));
            pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..buffers.count, 0, instances);
        }
    }
    fn device_changed(&mut self) {
        self.gpu = None;
    }
}

fn instance(d: &crate::world::Drawn) -> Instance {
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
            column(z, if material.unlit { 1. } else { 0. }),
        ],
        color: material
            .color
            .map(|c| if c.is_finite() { c.max(0.) } else { 0. }),
        emissive: [
            material.emissive[0],
            material.emissive[1],
            material.emissive[2],
            0.,
        ],
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
        points: [[[0.; 4]; 2]; POINT_LIGHTS],
        params: [0., if encode { 1. } else { 0. }, 1., 0.],
        shadow: Mat4::IDENTITY.columns,
        shadow_texel: [0.; 4],
    };
    // The point lights nearest the camera's target, nearest first, past the
    // four a frame shades with (ties in the order they were added).
    let target = world.camera.target;
    let mut points: Vec<(f32, [[f32; 4]; 2])> = Vec::with_capacity(POINT_LIGHTS + 1);
    for light in world.lights() {
        match *light {
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
                let at = points
                    .iter()
                    .position(|(nearer, _)| distance.total_cmp(nearer).is_lt())
                    .unwrap_or(points.len());
                if at < POINT_LIGHTS {
                    let slot = [
                        [position.x, position.y, position.z, range.max(1e-3)],
                        [color[0], color[1], color[2], intensity],
                    ];
                    points.insert(at, (distance, slot));
                    points.truncate(POINT_LIGHTS);
                }
            }
        }
    }
    for (slot, (_, light)) in uniforms.points.iter_mut().zip(&points) {
        *slot = *light;
    }
    uniforms.params[0] = points.len() as f32;
    uniforms
}

/// The sun going `direction` as it sees the opaque meshes of `all`: an
/// orthographic view holding them all, onto the shadow map, and how wide a
/// map texel is in the scene. `None` with nothing to cast a shadow.
fn sun_view(all: &[crate::world::Drawn], direction: Vec3) -> Option<(Mat4, f32)> {
    let corners = |d: &crate::world::Drawn| {
        let (low, high) = d.geometry.bounds();
        let matrix = d.matrix;
        (0..8).map(move |i| {
            matrix.project(crate::vec3(
                if i & 1 == 0 { low.x } else { high.x },
                if i & 2 == 0 { low.y } else { high.y },
                if i & 4 == 0 { low.z } else { high.z },
            ))
        })
    };
    let opaque = || all.iter().filter(|d| d.material.color[3] >= 1.);
    let (mut low, mut high) = (Vec3::from([f32::MAX; 3]), Vec3::from([f32::MIN; 3]));
    for p in opaque().flat_map(corners) {
        low = low.min(p);
        high = high.max(p);
    }
    let direction = direction.normalized();
    if !(low.is_finite() && high.is_finite()) || direction == Vec3::ZERO {
        return None;
    }
    let centre = (low + high) * 0.5;
    let radius = ((high - low).length() * 0.5).max(1e-3);
    let view = Mat4::look_at(centre - direction * (radius * 2.), centre, Vec3::Y);
    let (mut near, mut far) = (Vec3::from([f32::MAX; 3]), Vec3::from([f32::MIN; 3]));
    for p in opaque().flat_map(corners) {
        let p = view.project(p);
        near = near.min(p);
        far = far.max(p);
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
        self.bind = frame_binding(device, &self.layout, &self.uniforms, map, &self.sampler);
    }
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D"),
            source: wgpu::ShaderSource::Wgsl(shader_source(false).into()),
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
        let bind = frame_binding(device, &layout, &uniforms, &stand_in, &sampler);
        let map_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D maps"),
            entries: &[
                map_entry(0),
                map_entry(1),
                map_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("3D texture"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let plain = main_pipelines(device, format, &shader, &[Some(&layout)]);
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
        let (vertex_attributes, instance_attributes) = (VERTEX_ATTRIBUTES, INSTANCE_ATTRIBUTES);
        let shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D shadows"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("3D shadows"),
                    bind_group_layouts: &[Some(&shadow_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("shadow_vertex"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<crate::geometry::Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attributes,
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &instance_attributes,
                    },
                ],
            },
            fragment: None,
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
        });
        Self {
            format,
            plain,
            textured: None,
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
            views: HashMap::new(),
            bindings: HashMap::new(),
            stand_ins: None,
            instances: None,
            geometries: HashMap::new(),
            targets: None,
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
/// The shader, sampling textures or not: its lines marked `//T ` are a
/// textured pipeline's alone (its maps' bindings, samples and normal
/// mapping), comments in an untextured one's.
fn shader_source(textured: bool) -> String {
    if textured {
        SHADER.replace("//T ", "")
    } else {
        SHADER.to_owned()
    }
}
/// The opaque and translucent meshes' pipelines with `shader`, bound as
/// `layouts` say.
fn main_pipelines(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    layouts: &[Option<&wgpu::BindGroupLayout>],
) -> [wgpu::RenderPipeline; 2] {
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
                buffers: &[
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
                ],
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
                cull_mode: (!translucent).then_some(wgpu::Face::Back),
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
                count: SAMPLES,
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
) -> wgpu::TextureView {
    let levels = crate::texture::mipmaps(texture.width(), texture.height(), texture.pixels());
    let levels: Vec<&[u8]> = levels
        .iter()
        .map(|(_, _, pixels)| pixels.as_slice())
        .collect();
    zgx_gpu::viewport::texture_with_levels(
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
    )
    .create_view(&Default::default())
}
/// A mesh's maps (`Map::ALL`'s order) bound for sampling.
fn map_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    maps: [&wgpu::TextureView; 3],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3D maps"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(maps[0]),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(maps[1]),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(maps[2]),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
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
/// The main pass's frame: the uniforms, and the shadow map (or stand-in).
fn frame_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    map: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3D frame"),
        layout,
        entries: &[
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
        ],
    })
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
    points: array<array<vec4<f32>, 2>, 4>,
    params: vec4<f32>,
    shadow: mat4x4<f32>,
    shadow_texel: vec4<f32>,
}
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
//T @group(1) @binding(0) var base_texture: texture_2d<f32>;
//T @group(1) @binding(1) var metal_rough_texture: texture_2d<f32>;
//T @group(1) @binding(2) var normal_texture: texture_2d<f32>;
//T @group(1) @binding(3) var map_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) material: vec3<f32>,
    @location(5) uv: vec2<f32>,
}

@vertex fn vertex(
    @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
    @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>, @location(5) m2: vec4<f32>, @location(6) m3: vec4<f32>,
    @location(7) n0: vec4<f32>, @location(8) n1: vec4<f32>, @location(9) n2: vec4<f32>,
    @location(10) color: vec4<f32>, @location(11) emissive: vec4<f32>,
) -> Out {
    let model = mat4x4(m0, m1, m2, m3);
    let world = model * vec4(position, 1.0);
    let n = mat3x3(n0.xyz, n1.xyz, n2.xyz) * normal;
    return Out(frame.view_projection * world, world.xyz, n, color, emissive.xyz, vec3(n0.w, n1.w, n2.w), uv);
}

@vertex fn shadow_vertex(
    @location(0) position: vec3<f32>,
    @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>, @location(5) m2: vec4<f32>, @location(6) m3: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    return frame.shadow * mat4x4(m0, m1, m2, m3) * vec4(position, 1.0);
}

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
//T     texel = textureSample(base_texture, map_sampler, in.uv);
//T     surface = textureSample(metal_rough_texture, map_sampler, in.uv);
//T     bump = textureSample(normal_texture, map_sampler, in.uv).xyz;
//T     let dpx = dpdx(in.world);
//T     let dpy = dpdy(in.world);
//T     let duvx = dpdx(in.uv);
//T     let duvy = dpdy(in.uv);
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
        let roughness = clamp(in.material.y * surface.g, 0.02, 1.0);
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
            lit += shade(base, metallic, roughness, n, v, to / distance, light[1].rgb * light[1].w * falloff);
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
        lit += ambient * base * (1.0 - metallic) * (1.0 - fresnel);
        lit += reflected * fresnel * (1.0 - 0.5 * rough);
        lit += in.emissive;
        lit = neutral(lit * frame.params.z);
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
    fn an_untextured_shader_binds_no_texture() {
        let (plain, textured) = (shader_source(false), shader_source(true));
        // In the plain shader every line naming a map is a comment.
        for line in plain
            .lines()
            .filter(|l| l.contains("_texture") || l.contains("@group(1)"))
        {
            assert!(line.trim_start().starts_with("//"), "{line}");
        }
        assert!(textured.contains("@group(1) @binding(2) var normal_texture"));
        assert!(!textured.contains("//T"));
    }

    #[test]
    fn a_frame_shades_with_the_four_point_lights_nearest_the_target() {
        let world = World::new();
        world.set_camera(Camera::looking_at([0., 0., 9.], [0., 0., 0.]));
        // Along x at these distances from the target; the second 1 is
        // added after the first.
        for (x, intensity) in [(5., 1.), (1., 1.), (3., 1.), (2., 1.), (4., 1.), (1., 2.)] {
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
            .map(|[position, color]| (position[0], color[3]))
            .collect();
        assert_eq!(shaded, [(1., 1.), (1., 2.), (2., 1.), (3., 1.)]);
        assert_eq!(uniforms.params[0], 4.);
    }
}
