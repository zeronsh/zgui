//! Drawing a `World`: its view's painter. A frame is one pass with depth and
//! 4x multisampling: each geometry's meshes in one instanced draw, opaque
//! ones first, then the translucent back to front. Pipelines and buffers are
//! made with the first frame and kept; a geometry's buffers are uploaded
//! once and dropped a while after the last frame drawing it.
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
    /// Point lights in use, whether to encode sRGB itself, exposure.
    params: [f32; 4],
}

struct Buffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    used: u64,
}
struct Gpu {
    format: wgpu::TextureFormat,
    opaque: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
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
        frame.queue.write_buffer(
            &gpu.uniforms,
            0,
            bytemuck::bytes_of(&frame_uniforms(&world, view_projection, encode)),
        );

        // Meshes by geometry, translucent ones after, back to front.
        let mut drawn = world.drawn();
        let depth = |m: &Mat4| {
            let p = view.project(Vec3::from([
                m.columns[3][0],
                m.columns[3][1],
                m.columns[3][2],
            ]));
            -p.z
        };
        let translucent = |d: &crate::world::Drawn| d.material.color[3] < 1.;
        drawn.sort_by(|a, b| {
            translucent(a).cmp(&translucent(b)).then_with(|| {
                if translucent(a) {
                    depth(&b.matrix).total_cmp(&depth(&a.matrix))
                } else {
                    a.geometry.id().cmp(&b.geometry.id())
                }
            })
        });
        let instances: Vec<Instance> = drawn.iter().map(instance).collect();
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
        }
        let now = gpu.frame;
        gpu.geometries
            .retain(|_, buffers| now - buffers.used <= KEEP_FRAMES);

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
        // Runs of one geometry, of one pass (opaque, then translucent).
        let mut start = 0;
        while start < drawn.len() {
            let (geometry, glass) = (drawn[start].geometry.id(), translucent(&drawn[start]));
            let end = start
                + drawn[start..]
                    .iter()
                    .take_while(|d| d.geometry.id() == geometry && translucent(d) == glass)
                    .count();
            let buffers = &gpu.geometries[&geometry];
            pass.set_pipeline(if glass { &gpu.translucent } else { &gpu.opaque });
            pass.set_vertex_buffer(0, buffers.vertices.slice(..));
            pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..buffers.count, 0, start as u32..end as u32);
            start = end;
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
    };
    let mut points = Vec::new();
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
            Light::Point { .. } => points.push(*light),
        }
    }
    // The nearest the camera's target, past the four a frame shades with.
    let target = world.camera.target;
    points.sort_by(|a, b| {
        let distance = |l: &Light| match l {
            Light::Point { position, .. } => (*position - target).length(),
            _ => 0.,
        };
        distance(a).total_cmp(&distance(b))
    });
    for (slot, light) in uniforms.points.iter_mut().zip(&points) {
        if let Light::Point {
            position,
            color,
            intensity,
            range,
        } = *light
        {
            *slot = [
                [position.x, position.y, position.z, range.max(1e-3)],
                [color[0], color[1], color[2], intensity],
            ];
        }
    }
    uniforms.params[0] = points.len().min(POINT_LIGHTS) as f32;
    uniforms
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D frame"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D frame"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attributes =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let instance_attributes = wgpu::vertex_attr_array![
            3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4,
            7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4
        ];
        let pipeline = |translucent: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if translucent {
                    "3D translucent"
                } else {
                    "3D opaque"
                }),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
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
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: translucent
                            .then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
        Self {
            format,
            opaque: pipeline(false),
            translucent: pipeline(true),
            uniforms,
            bind,
            instances: None,
            geometries: HashMap::new(),
            targets: None,
            frame: 0,
        }
    }
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
}
@group(0) @binding(0) var<uniform> frame: Frame;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) emissive: vec3<f32>,
    @location(4) material: vec3<f32>,
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
    return Out(frame.view_projection * world, world.xyz, n, color, emissive.xyz, vec3(n0.w, n1.w, n2.w));
}

const PI: f32 = 3.14159265;

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

@fragment fn fragment(in: Out, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let base = in.color.rgb;
    var lit: vec3<f32>;
    if in.material.z > 0.5 {
        lit = base;
    } else {
        var n = normalize(in.normal);
        if !front {
            n = -n;
        }
        let v = normalize(frame.camera.xyz - in.world);
        let metallic = in.material.x;
        let roughness = in.material.y;
        lit = shade(base, metallic, roughness, n, v, -frame.sun_direction.xyz, frame.sun_color.rgb);
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
    let alpha = clamp(in.color.a, 0.0, 1.0);
    return vec4(lit * alpha, alpha);
}
"#;
