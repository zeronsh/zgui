//! What a renderer costs before its first frame, headless: the GPU device and
//! pipelines, the system font database, and shaping a line in four styles
//! (each new family, weight and style matches against every installed face).
//! Count instructions with `/usr/bin/time -l` (macOS).
//! Run: cargo run --release -p zgui-gpu --example startup_bench
use std::time::Instant;
use zgui::text_layout::{FontFamily, FontStyle};
use zgui_gpu::{GpuRenderer, text::ShapedText};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let renderer = GpuRenderer::new(1920, 1440)?;
    let created = start.elapsed();
    let fonts = renderer.text_system();
    let styles = [
        FontStyle::default(),
        FontStyle {
            weight: 700,
            ..FontStyle::default()
        },
        FontStyle {
            italic: true,
            ..FontStyle::default()
        },
        FontStyle {
            family: FontFamily::Monospace,
            ..FontStyle::default()
        },
    ];
    for style in &styles {
        let text = ShapedText::with_font(
            &mut fonts.borrow_mut(),
            "zgui performance lab",
            14.,
            None,
            style,
        );
        std::hint::black_box(text);
    }
    eprintln!(
        "renderer {created:?}, shaped {:?}",
        start.elapsed() - created
    );
    Ok(())
}
