//! The GPU view comparison's other side (scripts/run_macos_comparison.py
//! `--scene viewport`, against viewport_workload): the same field of cubes
//! (support/cubes.rs) drawn straight into a 960 by 720 window's surface with
//! winit and wgpu alone, as a minimal wgpu app does. The device is made as
//! zgx makes its own (low power), the surface presents in FIFO (vsync) order.
//! `ZGX_MODE=animate` draws every refresh; `idle` draws once and waits. Runs
//! `ZGX_SECONDS`, then reports the frames drawn after the first.
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use zgx_desktop::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};
use zgx_workload::Workload;

#[path = "support/cubes.rs"]
mod cubes;

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

struct App {
    animate: bool,
    seconds: f64,
    start: Instant,
    drawn: Rc<Cell<u64>>,
    cubes: cubes::Cubes,
    gpu: Option<Gpu>,
    done: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("wgpu GPU view")
                        .with_inner_size(winit::dpi::LogicalSize::new(960., 720.)),
                )
                .expect("a window"),
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone()).expect("a surface");
        let adapter = block(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("an adapter");
        let (device, queue) = block(adapter.request_device(&Default::default())).expect("a device");
        let size = window.inner_size();
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        self.gpu = Some(Gpu {
            window,
            surface,
            device,
            queue,
            config,
        });
        self.start = Instant::now();
        self.gpu.as_ref().unwrap().window.request_redraw();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(gpu) = &mut self.gpu else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gpu.config.width = size.width.max(1);
                gpu.config.height = size.height.max(1);
                gpu.surface.configure(&gpu.device, &gpu.config);
                gpu.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let texture = match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                    _ => {
                        gpu.surface.configure(&gpu.device, &gpu.config);
                        gpu.window.request_redraw();
                        return;
                    }
                };
                let target = texture.texture.create_view(&Default::default());
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                let scale = gpu.window.scale_factor() as f32;
                self.cubes.render(&mut ViewportFrame::new(
                    &gpu.device,
                    &gpu.queue,
                    &mut encoder,
                    &target,
                    gpu.config.format,
                    (gpu.config.width, gpu.config.height),
                    scale,
                ));
                gpu.queue.submit([encoder.finish()]);
                gpu.window.pre_present_notify();
                texture.present();
                if self.animate {
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let end = self.start + Duration::from_secs_f64(self.seconds);
        if self.done {
            return;
        }
        if self.seconds > 0. && Instant::now() >= end {
            self.done = true;
            let n = self.drawn.get().saturating_sub(1);
            eprintln!(
                "animation_frames={n} fps={:.1}",
                n as f64 / self.start.elapsed().as_secs_f64()
            );
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(if self.seconds > 0. {
            ControlFlow::WaitUntil(end)
        } else {
            ControlFlow::Wait
        });
    }
}

fn block<F: std::future::Future>(future: F) -> F::Output {
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

fn main() {
    let work = Workload::from_env();
    // Read and reported as every side reads the suite's font, though nothing
    // here draws text.
    let _ = zgx_workload::font_file();
    let drawn = Rc::new(Cell::new(0_u64));
    let mut app = App {
        animate: std::env::var("ZGX_MODE").as_deref() != Ok("idle"),
        seconds: work.seconds,
        start: Instant::now(),
        drawn: drawn.clone(),
        cubes: cubes::Cubes::new(drawn),
        gpu: None,
        done: false,
    };
    EventLoop::new()
        .expect("an event loop")
        .run_app(&mut app)
        .expect("the event loop");
}
