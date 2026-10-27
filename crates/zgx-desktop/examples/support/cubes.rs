//! The windowed GPU view comparison's scene (viewport_workload, wgpu_viewport):
//! a field of lit cubes turning with time, drawn with depth and 4x
//! multisampling in one instanced draw. Both apps draw it through
//! `Viewport::render`, so they do the same GPU work for a frame.
use std::{cell::Cell, rc::Rc, time::Instant};
use zgx_desktop::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

/// Cubes in the field: a 25 by 16 by 5 grid.
pub const CUBES: u32 = 2000;
const SAMPLES: u32 = 4;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Cubes {
    /// Frames drawn so far.
    pub drawn: Rc<Cell<u64>>,
    start: Instant,
    gpu: Option<Gpu>,
}
struct Gpu {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    targets: Option<((u32, u32), wgpu::TextureView, wgpu::TextureView)>,
}
impl Cubes {
    pub fn new(drawn: Rc<Cell<u64>>) -> Self {
        Self {
            drawn,
            start: Instant::now(),
            gpu: None,
        }
    }
}
impl Viewport for Cubes {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        self.drawn.set(self.drawn.get() + 1);
        // Linear shading, written encoded: through the sRGB view where there
        // is one (zgx's frames), else into the target as it is (an sRGB
        // surface).
        let (target, format) = match frame.target_srgb {
            Some(srgb) => (srgb, frame.format.add_srgb_suffix()),
            None => (frame.target, frame.format),
        };
        if self.gpu.as_ref().is_none_or(|gpu| gpu.format != format) {
            self.gpu = Some(Gpu::new(frame.device, format));
        }
        let gpu = self.gpu.as_mut().unwrap();
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
                texture(format, "cubes colour"),
                texture(DEPTH, "cubes depth"),
            ));
        }
        let time = frame
            .time
            .saturating_duration_since(self.start)
            .as_secs_f32();
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let uniforms = [time, aspect, 0., 0.];
        frame.queue.write_buffer(&gpu.uniforms, 0, bytes(&uniforms));
        let (_, colour, depth) = gpu.targets.as_ref().unwrap();
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cubes"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: colour,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.025,
                            b: 0.035,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &gpu.bind, &[]);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..));
        pass.draw(0..36, 0..CUBES);
    }
    fn device_changed(&mut self) {
        self.gpu = None;
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        use wgpu::util::DeviceExt;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cubes"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube"),
            contents: bytes(&cube()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cubes uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cubes"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cubes"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cubes"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("cubes"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
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
        });
        Self {
            format,
            pipeline,
            vertices,
            uniforms,
            bind,
            targets: None,
        }
    }
}

/// Each instance a cube of the grid, turning about its own axis by time;
/// the camera looks down the field from above its near edge.
const SHADER: &str = r#"
struct U { time: f32, aspect: f32 }
@group(0) @binding(0) var<uniform> u: U;
struct Out { @builtin(position) p: vec4<f32>, @location(0) n: vec3<f32>, @location(1) c: vec3<f32> }
fn turn(a: f32, b: f32) -> mat3x3<f32> {
    let sa = sin(a);
    let ca = cos(a);
    let sb = sin(b);
    let cb = cos(b);
    return mat3x3(vec3(ca, 0.0, -sa), vec3(0.0, 1.0, 0.0), vec3(sa, 0.0, ca))
         * mat3x3(vec3(1.0, 0.0, 0.0), vec3(0.0, cb, sb), vec3(0.0, -sb, cb));
}
@vertex fn vertex(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @builtin(instance_index) i: u32) -> Out {
    let x = f32(i % 25u) - 12.0;
    let z = f32((i / 25u) % 16u);
    let y = f32(i / 400u) - 2.0;
    let r = turn(u.time * (0.6 + 0.02 * x), u.time * 0.4 + z * 0.3);
    let world = r * (p * 0.32) + vec3(x * 0.9, y * 0.9, -z * 0.9 - 4.0);
    // Look down 0.35 radians, then a perspective of 0.9 radians.
    let s = sin(0.35);
    let c = cos(0.35);
    let eye = vec3(world.x, c * (world.y - 3.0) + s * world.z, -s * (world.y - 3.0) + c * world.z);
    let f = 1.0 / tan(0.45);
    let near = 0.1;
    let far = 60.0;
    let clip = vec4(eye.x * f / u.aspect, eye.y * f, (far / (near - far)) * eye.z + near * far / (near - far), -eye.z);
    let hue = vec3(0.35 + 0.5 * fract(f32(i) * 0.618), 0.55, 0.9 - 0.4 * fract(f32(i) * 0.37));
    return Out(clip, r * n, hue);
}
@fragment fn fragment(in: Out) -> @location(0) vec4<f32> {
    let light = normalize(vec3(-0.4, 0.8, 0.5));
    let lit = 0.12 + 0.88 * max(dot(normalize(in.n), light), 0.0);
    return vec4(in.c * lit, 1.0);
}
"#;

/// Positions and normals, two triangles a face.
fn cube() -> Vec<f32> {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1., 0., 0.], [0., 1., 0.], [0., 0., 1.]),
        ([-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]),
        ([0., 1., 0.], [0., 0., 1.], [1., 0., 0.]),
        ([0., -1., 0.], [1., 0., 0.], [0., 0., 1.]),
        ([0., 0., 1.], [1., 0., 0.], [0., 1., 0.]),
        ([0., 0., -1.], [0., 1., 0.], [1., 0., 0.]),
    ];
    let mut out = Vec::with_capacity(36 * 6);
    for (n, u, v) in faces {
        let corner = |a: f32, b: f32| [0, 1, 2].map(|i| n[i] + u[i] * a + v[i] * b);
        for (a, b) in [
            (-1., -1.),
            (1., -1.),
            (1., 1.),
            (-1., -1.),
            (1., 1.),
            (-1., 1.),
        ] {
            out.extend(corner(a, b));
            out.extend(n);
        }
    }
    out
}

fn bytes(values: &[f32]) -> &[u8] {
    // SAFETY: f32s have no padding, and u8's alignment is 1.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
