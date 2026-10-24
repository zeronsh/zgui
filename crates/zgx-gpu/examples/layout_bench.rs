//! Text layout and drawing cost of a label-heavy screen: first frames of new
//! text, then a window resize dragged across widths. Offscreen at 2x, like a
//! Retina window; times are CPU (layout, then encoding the render).
//! Run: cargo run --release -p zgx-gpu --example layout_bench
use std::time::{Duration, Instant};
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};
use zgx_gpu::GpuRenderer;

const WORDS: &[&str] = &[
    "process",
    "memory",
    "renderer",
    "streams",
    "tokens",
    "while",
    "layout",
    "measures",
    "paragraphs",
    "at",
    "every",
    "width",
    "and",
    "the",
    "glyphs",
    "stay",
    "retained",
];
const HEIGHT: f32 = 900.;

thread_local! {
    /// Varies every text, so each first frame is cold.
    static SALT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn sentence(seed: usize, words: usize) -> String {
    let salt = SALT.with(|salt| salt.get());
    let seed = seed + salt * 101;
    // Salted words are new to the shaper's caches too.
    (0..words)
        .map(|i| {
            let word = WORDS[(seed * 7 + i * 3) % WORDS.len()];
            if salt > 0 && i % 2 == 0 {
                format!("{word}{salt}")
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn screen() -> View {
    // Monitor-like rows of short labels.
    let labels = column()
        .w_percent(34.)
        .gap(2.)
        .children((0..50).map(|row_| {
            row()
                .gap(8.)
                .text_size(11.)
                .children((0..4).map(move |col| {
                    text(match col {
                        0 => format!(
                            "{} {row_}",
                            WORDS[(row_ + SALT.with(|salt| salt.get())) % WORDS.len()]
                        ),
                        1 => format!("{:.1}%", (row_ * 37 % 1000) as f32 / 10.),
                        2 => format!("{} MB", row_ * 13 % 900),
                        _ => format!(
                            "pid {}",
                            1000 + row_ * 17 + SALT.with(|salt| salt.get()) * 7919
                        ),
                    })
                }))
        }));
    // Wrapped plain paragraphs, as in descriptions and messages.
    let plain = column().w_percent(33.).gap(6.).children((0..24).map(|i| {
        text(sentence(i, 18 + i % 11))
            .text_wrap(true)
            .text_size(12.)
    }));
    // Rich paragraphs, as markdown renders.
    let rich = column()
        .w_percent(33.)
        .gap(6.)
        .children((0..24).map(|i| -> View {
            rich_text()
                .child(text_span(sentence(i + 50, 8)))
                .child(text_span(sentence(i + 90, 3)).font_weight(600))
                .child(text_span(sentence(i + 130, 9 + i % 7)))
                .w_full()
                .text_wrap(true)
                .text_size(12.)
                .into()
        }));
    row()
        .w_full()
        .h(HEIGHT)
        .gap(8.)
        .bg(Color(20, 22, 28, 255))
        .text_color(Color(230, 230, 235, 255))
        .child(labels)
        .child(plain)
        .child(rich)
}

fn frame(ui: &mut Ui, gpu: &mut GpuRenderer, layout: &mut Duration, render: &mut Duration) {
    let started = Instant::now();
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    let laid = Instant::now();
    gpu.render(&ui.scene.borrow(), &damage).unwrap();
    *layout += laid - started;
    *render += laid.elapsed();
}

fn main() {
    const FIRST: usize = 100;
    const RESIZES: usize = 400;
    let mut gpu = GpuRenderer::new(2800, 1800).expect("GPU");
    gpu.set_scale_factor(2.);
    // First frames of text new to every cache (fonts are loaded by then):
    // mounting, which lays out too, then the frame.
    let (mut layout, mut render) = (Duration::ZERO, Duration::ZERO);
    let mut ui = Ui::new(1400., HEIGHT);
    for salt in 1..=FIRST {
        SALT.with(|s| s.set(salt));
        ui = Ui::new(1400., HEIGHT);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let started = Instant::now();
        ui.mount(screen());
        layout += started.elapsed();
        frame(&mut ui, &mut gpu, &mut layout, &mut render);
    }
    let per = |d: Duration, n: usize| d.as_secs_f64() * 1e3 / n as f64;
    println!(
        "first frame   mount+layout {:.3} ms  render {:.3} ms  (mean of {FIRST})",
        per(layout, FIRST),
        per(render, FIRST)
    );
    // A window resize dragged back and forth. The target keeps its size:
    // this measures text and layout, not reallocating GPU textures.
    let (mut layout, mut render) = (Duration::ZERO, Duration::ZERO);
    for step in 1..=RESIZES {
        let width = 1400. - (step % 20) as f32 * 25.;
        ui.scene.borrow_mut().resize(width, HEIGHT);
        frame(&mut ui, &mut gpu, &mut layout, &mut render);
    }
    println!(
        "resize step   layout       {:.3} ms  render {:.3} ms  (mean of {RESIZES})",
        per(layout, RESIZES),
        per(render, RESIZES)
    );
}
