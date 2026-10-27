//! A GPU view drawing every tick, offscreen, against the same painter
//! drawing with wgpu alone: what showing it in zgx costs over drawing it.
//! Run: MODE=zgx TICKS=600 cargo run --release -p zgx-desktop --example viewport_bench
//!
//! `MODE=ui` changes a box beside the view instead: the window's own frame
//! with the view's last one shown again, which `MODE=zgx` less this is.
//! `MODE=zgx` (the default) shows the painter in a GPU view beside a little
//! UI at a Retina scale and invalidates it every tick; `MODE=wgpu` draws the
//! same frames into a texture of its own device. Both report allocations per
//! tick after warm-up, and zgx also its painter's frames, its cached image
//! textures, the frames patched into its kept frame and whether the scene's
//! resources changed (a prune of the renderer's caches). Count instructions
//! with `/usr/bin/time -l` (macOS) or valgrind at two `TICKS`: the
//! difference over the ticks between them is the per-tick cost.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};
use zgx_desktop::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};
use zgx_gpu::GpuRenderer;

/// Counts the allocations made (`realloc` counts as one).
#[cfg_attr(feature = "mimalloc", allow(dead_code))]
struct Counting;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
// SAFETY: every call goes to `System` with the caller's own arguments;
// counting allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        // SAFETY: the caller's layout, as `alloc` requires.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` (every allocation here does).
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        // SAFETY: as `dealloc`, with the caller's new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
// With the `mimalloc` feature, that allocator stands instead (allocations
// then read 0).
#[cfg_attr(not(feature = "mimalloc"), global_allocator)]
#[cfg_attr(feature = "mimalloc", allow(dead_code))]
static GLOBAL: Counting = Counting;

const WIDTH: f32 = 560.;
const HEIGHT: f32 = 460.;
/// The view's box, in points.
const VIEW: (f32, f32) = (512., 340.);
const WARM: usize = 60;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let ticks: usize = env("TICKS", 600).max(WARM + 1);
    let scale: f32 = env("SCALE", 2.);
    match std::env::var("MODE").as_deref().unwrap_or("zgx") {
        "wgpu" => wgpu_alone(ticks, scale),
        "ui" => in_zgx(ticks, scale, false),
        _ => in_zgx(ticks, scale, true),
    }
}

/// `invalidate`: a frame of the view each tick; otherwise the box beside it
/// changes colour instead (the window's own frame, the view shown as it was).
fn in_zgx(ticks: usize, scale: f32, invalidate: bool) {
    let mut gpu = GpuRenderer::new((WIDTH * scale) as u32, (HEIGHT * scale) as u32).expect("GPU");
    gpu.set_scale_factor(scale);
    let drawn = Rc::new(Cell::new(0_u64));
    let view = zgx_desktop::viewport::view(Triangles::new(drawn.clone()));
    let mut ui = Ui::new(WIDTH, HEIGHT);
    let lit = ui.signal(false);
    let shade = lit.clone();
    ui.mount(
        column()
            .size(WIDTH, HEIGHT)
            .p(24.)
            .gap(16.)
            .bg(Color(15, 20, 27, 255))
            .child(div().h(40.).w(200.).rounded(8.).reactive_style(move || {
                Styles::new().bg(if shade.get() {
                    Color(52, 71, 96, 255)
                } else {
                    Color(36, 52, 71, 255)
                })
            }))
            .child(
                gpu_view("Triangles", &view)
                    .size(VIEW.0, VIEW.1)
                    .rounded(16.)
                    .overflow_hidden(),
            ),
    );
    let mut patched = 0;
    let (mut allocations, mut textures, mut revision) = (0, None, None);
    for tick in 0..ticks {
        let start = ALLOCATIONS.load(Relaxed);
        if invalidate {
            view.invalidate();
        } else {
            lit.set(tick % 2 == 0);
        }
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        let stats = gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        // A window presents every frame; readback only settles the GPU.
        if tick % 60 == 0 {
            gpu.readback().unwrap();
        }
        if tick == WARM {
            textures = Some(gpu.debug_cache_stats().image_textures);
            revision = Some(ui.scene.borrow().content_revision());
        }
        if tick >= WARM {
            allocations += ALLOCATIONS.load(Relaxed) - start;
            patched += usize::from(stats.patched_items > 0);
        }
    }
    let measured = (ticks - WARM) as f64;
    println!(
        "{}: {:.2} allocations/tick, painter frames {} of {ticks} ticks, \
         image textures {} -> {}, patched {patched} of {} ticks, resources {}",
        if invalidate { "zgx" } else { "zgx ui" },
        allocations as f64 / measured,
        drawn.get(),
        textures.unwrap(),
        gpu.debug_cache_stats().image_textures,
        ticks - WARM,
        if revision == Some(ui.scene.borrow().content_revision()) {
            "unchanged (no prune)"
        } else {
            "changed (pruned)"
        },
    );
}

fn wgpu_alone(ticks: usize, scale: f32) {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let adapter = pollster_block(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        ..Default::default()
    }))
    .expect("an adapter");
    let (device, queue) =
        pollster_block(adapter.request_device(&Default::default())).expect("a device");
    let size = (
        (VIEW.0 * scale).ceil() as u32,
        (VIEW.1 * scale).ceil() as u32,
    );
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frame"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    let drawn = Rc::new(Cell::new(0_u64));
    let mut painter = Triangles::new(drawn.clone());
    let mut allocations = 0;
    for tick in 0..ticks {
        let start = ALLOCATIONS.load(Relaxed);
        let mut encoder = device.create_command_encoder(&Default::default());
        painter.render(&mut frame(
            &device,
            &queue,
            &mut encoder,
            &target,
            size,
            scale,
        ));
        queue.submit([encoder.finish()]);
        if tick % 60 == 0 {
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
        if tick >= WARM {
            allocations += ALLOCATIONS.load(Relaxed) - start;
        }
    }
    println!(
        "wgpu: {:.2} allocations/tick, painter frames {}",
        allocations as f64 / (ticks - WARM) as f64,
        drawn.get(),
    );
}

/// A `ViewportFrame` for drawing outside zgx (its fields are zgx-gpu's to
/// add to, so made through its constructor for tests and benches).
fn frame<'a>(
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    encoder: &'a mut wgpu::CommandEncoder,
    target: &'a wgpu::TextureView,
    size: (u32, u32),
    scale: f32,
) -> ViewportFrame<'a> {
    ViewportFrame::new(
        device,
        queue,
        encoder,
        target,
        wgpu::TextureFormat::Bgra8Unorm,
        size,
        scale,
    )
}

fn pollster_block<F: std::future::Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}

/// A ring of triangles turning a little each frame: a pipeline, a uniform
/// written per frame, one instanced draw.
struct Triangles {
    drawn: Rc<Cell<u64>>,
    gpu: Option<(wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup)>,
}
const TRIANGLES: u32 = 2000;
impl Triangles {
    fn new(drawn: Rc<Cell<u64>>) -> Self {
        Self { drawn, gpu: None }
    }
}
impl Viewport for Triangles {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        let turn = self.drawn.get() as f32 * 0.01;
        self.drawn.set(self.drawn.get() + 1);
        let format = frame.format;
        let (pipeline, uniforms, bind) = self.gpu.get_or_insert_with(|| {
            let device = frame.device;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("triangles"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("turn"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
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
                label: None,
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                }],
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("triangles"),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: &[Some(&layout)],
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            (pipeline, uniforms, bind)
        });
        frame.queue.write_buffer(
            uniforms,
            0,
            &[turn.to_le_bytes(), [0; 4], [0; 4], [0; 4]].concat(),
        );
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("triangles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.1,
                            b: 0.13,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &*bind, &[]);
        pass.draw(0..3, 0..TRIANGLES);
    }
}
const SHADER: &str = r#"
@group(0) @binding(0) var<uniform> turn: vec4<f32>;
struct Out { @builtin(position) p: vec4<f32>, @location(0) c: vec3<f32> }
@vertex fn vertex(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> Out {
    let a = f32(i) * 0.00314159 * 2.0 + turn.x;
    let r = 0.2 + 0.7 * fract(f32(i) * 0.618);
    let corner = array(vec2(0.0, 0.02), vec2(-0.017, -0.01), vec2(0.017, -0.01))[v];
    let at = vec2(cos(a), sin(a)) * r + corner;
    return Out(vec4(at, 0.0, 1.0), vec3(fract(f32(i) * 0.37), 0.6, 1.0 - r));
}
@fragment fn fragment(in: Out) -> @location(0) vec4<f32> { return vec4(in.c, 1.0); }
"#;
