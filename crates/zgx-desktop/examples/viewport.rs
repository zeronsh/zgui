//! A GPU view: a lit cube drawn with wgpu by the app, in a box with rounded
//! corners among ordinary UI. It turns while "Spin" is on (the view animates:
//! a frame each refresh); drag it to turn it yourself (a frame per move). Off
//! and untouched, the window does no work.
//! Run: cargo run --release -p zgx-desktop --example viewport
//!
//! `--smoke-test` draws, spins for a moment, stops, then prints the frames the
//! view drew and quits.
use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, input::InputEvent, scene::Color};
use zgx_desktop::{
    Application, WindowOptions,
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx GPU view".into(),
            width: 560.,
            height: 460.,
            ..Default::default()
        })
        .run(move |cx| {
            let yaw = Rc::new(Cell::new(0.6_f32));
            let pitch = Rc::new(Cell::new(0.4_f32));
            let drawn = Rc::new(Cell::new(0_u32));
            let view = zgx_desktop::viewport::view(Cube {
                yaw: yaw.clone(),
                pitch: pitch.clone(),
                drawn: drawn.clone(),
                last: None,
                gpu: None,
            });
            let spinning = cx.ui.signal(false);
            let dragging = Rc::new(Cell::new(None::<(f32, f32)>));
            let toggle = {
                let (view, spinning) = (view.clone(), spinning.clone());
                move || {
                    spinning.set(!spinning.get());
                    view.animate(spinning.get());
                }
            };
            let label = spinning.clone();
            cx.render(
                column()
                    .w_full()
                    .p(24.)
                    .gap(16.)
                    .bg(rgb(0x0f141b))
                    .text_color(rgb(0xe0eaf5))
                    .child(
                        row()
                            .gap(12.)
                            .items_center()
                            .child(text("A cube drawn with wgpu").text_size(20.))
                            .child(
                                button()
                                    .px(12.)
                                    .py(6.)
                                    .rounded(6.)
                                    .bg(rgb(0x243447))
                                    .child(text_signal(move || {
                                        if label.get() { "Stop" } else { "Spin" }.to_string()
                                    }))
                                    .on_click(toggle.clone()),
                            ),
                    )
                    .child(
                        gpu_view("Cube", &view)
                            .w_full()
                            .h(340.)
                            .rounded(16.)
                            .overflow_hidden()
                            .bg(Color(20, 26, 34, 255))
                            .on_event({
                                let (view, yaw, pitch) = (view.clone(), yaw.clone(), pitch.clone());
                                move |event| match event.event {
                                    InputEvent::PointerDown { x, y, .. } => {
                                        dragging.set(Some((x, y)));
                                    }
                                    InputEvent::PointerMove { x, y } => {
                                        let Some((from_x, from_y)) = dragging.get() else {
                                            return;
                                        };
                                        dragging.set(Some((x, y)));
                                        yaw.set(yaw.get() + (x - from_x) * 0.01);
                                        pitch.set(
                                            (pitch.get() + (y - from_y) * 0.01).clamp(-1.4, 1.4),
                                        );
                                        view.invalidate();
                                    }
                                    InputEvent::PointerUp { .. } | InputEvent::PointerCancel => {
                                        dragging.set(None)
                                    }
                                    _ => {}
                                }
                            }),
                    ),
            );
            if smoke {
                let (window, frames) = (cx.window.clone(), cx.frames.clone());
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    println!("VIEWPORT still={}", drawn.get());
                    toggle();
                    for _ in 0..30 {
                        frames.next().await;
                    }
                    toggle();
                    let spun = drawn.get();
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    println!("VIEWPORT spun={spun} idle={}", drawn.get() - spun);
                    window.close();
                });
            }
        })
}

/// The cube's device resources, made with the first frame.
struct Gpu {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// Its multisampled colour and depth, for the frame's size.
    targets: Option<((u32, u32), wgpu::TextureView, wgpu::TextureView)>,
}

struct Cube {
    yaw: Rc<Cell<f32>>,
    pitch: Rc<Cell<f32>>,
    drawn: Rc<Cell<u32>>,
    /// When it last drew, spinning: it turns by the time since.
    last: Option<std::time::Instant>,
    gpu: Option<Gpu>,
}
const SAMPLES: u32 = 4;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

impl Viewport for Cube {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        self.drawn.set(self.drawn.get() + 1);
        if let Some(last) = self.last.replace(frame.time) {
            let seconds = frame.time.saturating_duration_since(last).as_secs_f32();
            // A pause longer than a few refreshes was not spinning.
            if seconds < 0.1 {
                self.yaw.set(self.yaw.get() + seconds * 0.8);
            }
        }
        let format = frame
            .target_srgb
            .map_or(frame.format, |_| frame.format.add_srgb_suffix());
        let gpu = self
            .gpu
            .get_or_insert_with(|| Gpu::new(frame.device, format));
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
                texture(format, "cube colour"),
                texture(DEPTH, "cube depth"),
            ));
        }
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let model = mul(rotate_x(self.pitch.get()), rotate_y(self.yaw.get()));
        let projection = mul(perspective(0.8, aspect, 0.1, 20.), translate(0., 0., -4.));
        let mut uniforms = [0_f32; 32];
        uniforms[..16].copy_from_slice(&mul(projection, model));
        uniforms[16..].copy_from_slice(&model);
        frame
            .queue
            .write_buffer(&gpu.uniforms, 0, bytemuck_cast(&uniforms));
        let (_, colour, depth) = gpu.targets.as_ref().unwrap();
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cube"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: colour,
                    depth_slice: None,
                    resolve_target: Some(frame.target_srgb.unwrap_or(frame.target)),
                    ops: wgpu::Operations {
                        // Transparent: the box's own background shows around it.
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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
        pass.draw(0..36, 0..1);
    }
    fn device_changed(&mut self) {
        self.gpu = None;
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        use wgpu::util::DeviceExt;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck_cast(&cube_vertices()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube uniforms"),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cube"),
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
            label: Some("cube"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cube"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("cube"),
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
            pipeline,
            vertices,
            uniforms,
            bind,
            targets: None,
        }
    }
}

const SHADER: &str = r#"
struct Uniforms { clip: mat4x4<f32>, model: mat4x4<f32> }
@group(0) @binding(0) var<uniform> u: Uniforms;
struct Out { @builtin(position) position: vec4<f32>, @location(0) normal: vec3<f32> }
@vertex fn vertex(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> Out {
    return Out(u.clip * vec4(p, 1.0), (u.model * vec4(n, 0.0)).xyz);
}
@fragment fn fragment(in: Out) -> @location(0) vec4<f32> {
    let light = normalize(vec3(-0.4, 0.8, 0.6));
    let lit = 0.15 + 0.85 * max(dot(normalize(in.normal), light), 0.0);
    return vec4(vec3(0.25, 0.55, 1.0) * lit, 1.0);
}
"#;

/// Positions and normals, two triangles a face.
fn cube_vertices() -> Vec<f32> {
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
        let corner =
            |a: f32, b: f32| [0, 1, 2].map(|i| n[i] * 0.8 + u[i] * a * 0.8 + v[i] * b * 0.8);
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

type Mat = [f32; 16];
/// Column-major products, as WGSL reads them.
fn mul(a: Mat, b: Mat) -> Mat {
    let mut out = [0.; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}
fn translate(x: f32, y: f32, z: f32) -> Mat {
    [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, y, z, 1.]
}
fn rotate_x(a: f32) -> Mat {
    let (s, c) = a.sin_cos();
    [1., 0., 0., 0., 0., c, s, 0., 0., -s, c, 0., 0., 0., 0., 1.]
}
fn rotate_y(a: f32) -> Mat {
    let (s, c) = a.sin_cos();
    [c, 0., -s, 0., 0., 1., 0., 0., s, 0., c, 0., 0., 0., 0., 1.]
}
/// Depth 0 to 1, as wgpu clips.
fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Mat {
    let f = 1. / (fov * 0.5).tan();
    let range = far / (near - far);
    [
        f / aspect,
        0.,
        0.,
        0.,
        0.,
        f,
        0.,
        0.,
        0.,
        0.,
        range,
        -1.,
        0.,
        0.,
        near * range,
        0.,
    ]
}
fn bytemuck_cast(values: &[f32]) -> &[u8] {
    // SAFETY: f32s have no padding, and u8's alignment is 1.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
