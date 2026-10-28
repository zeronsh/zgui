//! viewport_workload with its plane left out of phase (`ZGX_PLANE_CLOCK=off`): the comparison's
//! cells for how a GPU view's plane keeps its drawables in phase
//! (scripts/run_macos_comparison.py `--scene viewport`).
#[path = "support/viewport_app.rs"]
mod app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: before any thread starts; the host reads it once, later.
    unsafe { std::env::set_var("ZGX_PLANE_CLOCK", "off") };
    app::run(false)
}
