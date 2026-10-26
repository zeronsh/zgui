//! Windows: a GPU context made on a thread of its own before any window
//! exists (as `Application::run` starts one at launch, beside the first
//! window) draws and presents to a window made after it: the adapter it
//! found without a window is one that window can present from.
#![cfg(windows)]
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::platform::windows::{EventLoopBuilderExtWindows, WindowAttributesExtWindows};
use winit::window::{Window, WindowId};
use zgx::scene::{Color, Rect, Scene};
use zgx_gpu::{GpuContext, GpuRenderer, PendingGpuContext};

struct App {
    pending: Option<PendingGpuContext>,
    presented: Option<Result<(), String>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title("zgx early context test")
            .with_inner_size(winit::dpi::PhysicalSize::new(200, 120))
            .with_no_redirection_bitmap(true);
        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        let context: GpuContext = match self.pending.take().unwrap().join() {
            Ok(context) => context,
            Err(error) => {
                eprintln!("no GPU here: {error}");
                event_loop.exit();
                return;
            }
        };
        self.presented = Some((|| {
            let mut renderer =
                GpuRenderer::for_window_with_transparency(window, Some(&context), false)
                    .map_err(|e| e.to_string())?;
            renderer.set_background(Color(30, 30, 30, 255));
            let scene = Scene::new(200., 120.);
            renderer
                .render(&scene, &[Rect::new(0., 0., 200., 120.)])
                .map_err(|e| e.to_string())?;
            renderer.present().map_err(|e| e.to_string())
        })());
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

#[test]
fn a_context_made_before_any_window_presents_to_one_made_after() {
    // Started before the event loop, as the application starts it.
    let pending = GpuContext::prepare_in_background();
    let mut event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut app = App {
        pending: Some(pending),
        presented: None,
    };
    event_loop.run_app_on_demand(&mut app).unwrap();
    if let Some(presented) = app.presented {
        presented.expect("the early context draws and presents to the window");
    }
}
