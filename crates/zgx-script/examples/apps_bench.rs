//! The example apps' common interactions (`support/apps.rs`), each to its
//! frame drawn on a GPU offscreen with its text shaped. `RENDER=0` draws
//! nothing (text still shaped as a renderer's). The gate runs the same
//! interactions undrawn and small, inside `list_state_bench`.
//! Run: ZGX_PROFILE=1 cargo run --release -p zgx-script --example apps_bench
#[path = "support/apps.rs"]
mod apps;
#[path = "support/bench_limits.rs"]
mod bench_limits;

use zgx::scene::{Rect, Scene};
use zgx_gpu::{GpuRenderer, TextServices};

/// A GPU drawing every frame offscreen at twice the points.
struct Gpu(GpuRenderer);
impl apps::Drawing for Gpu {
    fn draw(&mut self, scene: &Scene, damage: &[Rect]) {
        self.0.render(scene, damage).expect("a frame drawn");
    }
    fn finish(&mut self) {
        self.0.readback().expect("the GPU caught up");
    }
}
/// Text shaped as a renderer's, nothing drawn.
struct Shaped(#[allow(dead_code)] TextServices);
impl apps::Drawing for Shaped {
    fn draw(&mut self, _: &Scene, _: &[Rect]) {}
}

fn main() {
    let render = std::env::var("RENDER").map_or(true, |v| v != "0");
    apps::run(&|scene, (w, h)| {
        if render {
            let scale = 2.;
            let mut gpu = GpuRenderer::new((w * scale) as u32, (h * scale) as u32).expect("a GPU");
            gpu.set_scale_factor(scale);
            gpu.install_text(scene);
            Some(Box::new(Gpu(gpu)) as Box<dyn apps::Drawing>)
        } else {
            let text = TextServices::new();
            text.install(scene);
            Some(Box::new(Shaped(text)))
        }
    });
}
