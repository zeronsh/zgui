//! The GPU view comparison's third cell (scripts/run_macos_comparison.py
//! `--scene viewport`): viewport_workload with its view composited into the
//! window's frame, as a view that isn't opaque, or isn't shown whole, is.
#[path = "support/viewport_app.rs"]
mod app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run(true)
}
