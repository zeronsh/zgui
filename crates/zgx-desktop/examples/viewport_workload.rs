//! The GPU view comparison's zgx side (scripts/run_macos_comparison.py
//! `--scene viewport`, against wgpu_viewport and viewport_composite): its
//! opaque view fills the window, so it shows on a layer of its own (see
//! support/viewport_app.rs; `ZGX_VIEW_COMPOSITE=1` keeps it in the window's
//! frame instead).
#[path = "support/viewport_app.rs"]
mod app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run(false)
}
