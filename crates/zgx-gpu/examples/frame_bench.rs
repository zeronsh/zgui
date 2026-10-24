//! Per-frame CPU cost of animating a few nodes inside a larger static scene,
//! rendered offscreen on the GPU. It separates reactive updates, scene flush
//! (layout and damage) and render encoding, so work that scales with the whole
//! scene rather than with the damage is visible.
//!
//! Run: cargo run --release -p zgx-gpu --example frame_bench [frames] [static-rows]
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, widgets::Ui};
use zgx_gpu::GpuRenderer;

const WIDTH: f32 = 1180.;
const HEIGHT: f32 = 780.;

struct Animated {
    dots: Vec<Signal<f32>>,
    shimmer: Vec<Signal<u8>>,
    cells: Vec<Signal<f32>>,
    fold: Signal<f32>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let frames: usize = args.next().map_or(600, |v| v.parse().expect("frame count"));
    let rows: usize = args
        .next()
        .map_or(60, |v| v.parse().expect("static row count"));
    for (name, mode) in [
        ("paint-only (opacity + text colour)", Mode::Paint),
        ("layout inside fixed slots", Mode::Cells),
        ("layout up to the root (fold grows)", Mode::Fold),
    ] {
        // An optional third argument runs only scenarios whose name contains it.
        if std::env::args()
            .nth(3)
            .is_some_and(|only| !name.contains(only.as_str()))
        {
            continue;
        }
        run(name, frames, rows, mode)?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Paint,
    Cells,
    Fold,
}

fn run(
    name: &str,
    frames: usize,
    rows: usize,
    mode: Mode,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = Ui::new(WIDTH, HEIGHT);
    let mut renderer = GpuRenderer::new((WIDTH * 2.) as u32, (HEIGHT * 2.) as u32)?;
    renderer.set_scale_factor(2.);
    renderer.set_background(Color(18, 22, 30, 255));
    install_text(&mut ui, &renderer);
    let (view, animated) = scene(&ui, rows);
    ui.mount(view);
    // Warm caches (layout, glyph atlas) before timing.
    for frame in 0..30 {
        step(&animated, frame, mode);
        frame_once(&ui, &mut renderer)?;
    }
    let nodes = ui.scene.borrow().paint_items().count();
    let (mut update, mut flush, mut render) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let mut damaged = 0.;
    let (mut draws, mut regions, mut instances) = (0, 0, 0);
    for frame in 0..frames {
        let t0 = Instant::now();
        step(&animated, 30 + frame, mode);
        let t1 = Instant::now();
        let report = ui.scene.borrow_mut().flush();
        let t2 = Instant::now();
        let scene = ui.scene.borrow();
        let area: f32 = report.damage.iter().map(|r| r.width * r.height).sum();
        damaged += area / (WIDTH * HEIGHT);
        let stats = black_box(renderer.render(&scene, &report.damage)?);
        draws += stats.draw_calls;
        instances += stats.instances;
        regions += report.damage.len();
        let t3 = Instant::now();
        update += t1 - t0;
        flush += t2 - t1;
        render += t3 - t2;
    }
    let per = |d: Duration| d.as_secs_f64() * 1e6 / frames as f64;
    println!(
        "{name:<36} {nodes:>5} nodes  damage {:>4.1}% in {:>4.1} rects  {:>5.1} draws {:>6.1} quads  update {:>5.1}  flush {:>6.1}  render {:>6.1}  total {:>6.1} µs/frame",
        damaged / frames as f32 * 100.,
        regions as f64 / frames as f64,
        draws as f64 / frames as f64,
        instances as f64 / frames as f64,
        per(update),
        per(flush),
        per(render),
        per(update + flush + render),
    );
    Ok(())
}

fn frame_once(ui: &Ui, renderer: &mut GpuRenderer) -> Result<(), Box<dyn std::error::Error>> {
    let report = ui.scene.borrow_mut().flush();
    renderer.render(&ui.scene.borrow(), &report.damage)?;
    Ok(())
}

fn step(animated: &Animated, frame: usize, mode: Mode) {
    let t = frame as f32 / 120.;
    if mode == Mode::Fold {
        animated.fold.set(84. * (0.5 - 0.5 * (t * 3.).cos()));
    }
    if mode == Mode::Paint {
        for (i, dot) in animated.dots.iter().enumerate() {
            dot.set(0.1 + 0.9 * ((t * 1.3 + i as f32 * 0.11).fract()));
        }
        for (i, ch) in animated.shimmer.iter().enumerate() {
            let centre = (t / 3.4).fract() * 1.8 - 0.4;
            let glow = (1. - ((i as f32 / 22.) - centre).abs() / 0.36).clamp(0., 1.);
            ch.set((90. + 165. * glow) as u8);
        }
    }
    if mode == Mode::Cells {
        for (i, cell) in animated.cells.iter().enumerate() {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * (t - i as f32 * 0.15) / 2.4).cos();
            cell.set(12.6 + 1.4 * w);
        }
    }
}

/// A Motion-page-like tree: sidebar, header, static text rows, an event log,
/// and a band of animated loaders.
fn scene(ui: &Ui, rows: usize) -> (View, Animated) {
    let dots: Vec<_> = (0..15).map(|_| ui.signal(1_f32)).collect();
    let shimmer: Vec<_> = (0..23).map(|_| ui.signal(90_u8)).collect();
    let cells: Vec<_> = (0..5).map(|_| ui.signal(14_f32)).collect();
    let fold = ui.signal(0_f32);
    let card = || {
        column()
            .rounded(14.)
            .bg(rgba(0xffffff14))
            .border(1.)
            .border_color(rgba(0xffffff1f))
    };
    let loaders =
        row()
            .w_full()
            .gap(12.)
            .child(card().grow().h(150.).p(16.).child(text("MATRIX")).child(
                row().gap(6.).children(dots.iter().map(|dot| {
                    let dot = dot.clone();
                    div()
                        .size(10., 10.)
                        .rounded(5.)
                        .bg(rgb(0x6ea8ff))
                        .reactive_style(move || Styles::new().opacity(dot.get()))
                })),
            ))
            .child(
                card()
                    .grow()
                    .h(150.)
                    .p(16.)
                    .child(text("WAVE"))
                    .child(row().gap(4.).children(cells.iter().map(|cell| {
                        let cell = cell.clone();
                        column()
                            .size(16., 16.)
                            .items_center()
                            .justify_center()
                            .child(
                                div().rounded(4.).bg(rgb(0x9ad2ff)).reactive_style(move || {
                                    Styles::new().size(cell.get(), cell.get())
                                }),
                            )
                    }))),
            );
    let shimmer_row =
        card().w_full().p(20.).child(
            row().children("Reading workspace files".chars().zip(shimmer.iter()).map(
                |(ch, alpha)| {
                    let alpha = alpha.clone();
                    text(ch.to_string()).text_size(18.).reactive_style(move || {
                        Styles::new().text_color(Color(255, 255, 255, alpha.get()))
                    })
                },
            )),
        );
    let content = column()
        .grow()
        .min_w(0.)
        .p(32.)
        .gap(22.)
        .child(text("Motion").text_size(30.))
        .children((0..5).map(|i| {
            text(format!(
                "{}. A step describing what to check on this page.",
                i + 1
            ))
            .text_size(13.)
        }))
        .child(loaders)
        .child(shimmer_row)
        .child(card().w_full().p(20.).child(text("FOLD")).child({
            let fold = fold.clone();
            column()
                .w_full()
                .overflow_hidden()
                .reactive_style(move || Styles::new().h(fold.get()))
                .children((0..3).map(|i| text(format!("Folded line {i}")).h(24.)))
        }))
        .children((0..rows).map(|i| {
            card().w_full().p(12.).child(
                text(format!(
                    "Static row {i}: retained content that never changes"
                ))
                .text_size(13.),
            )
        }));
    let sidebar = column().w(220.).p(12.).gap(2.).children((0..8).map(|i| {
        row()
            .h(34.)
            .px(10.)
            .rounded(8.)
            .items_center()
            .child(text(format!("Section {i}")).text_size(13.))
    }));
    let log = column().w(300.).p(20.).gap(5.).children((0..40).map(|i| {
        text(format!(
            "{:>5.1}s  EVENT something happened {i}",
            i as f32 * 0.7
        ))
        .text_size(11.)
        .font_family("Menlo")
    }));
    let view = row()
        .w_full()
        .h_full()
        .items_stretch()
        .text_color(rgb(0xffffff))
        .child(sidebar)
        .child(
            row()
                .grow()
                .min_w(0.)
                .rounded(14.)
                .bg(rgba(0x12161ea6))
                .child(
                    scroll(ui.signal(0.))
                        .grow()
                        .min_w(0.)
                        .h_full()
                        .child(content),
                )
                .child(log),
        );
    (
        view,
        Animated {
            dots,
            shimmer,
            cells,
            fold,
        },
    )
}

fn install_text(ui: &mut Ui, renderer: &GpuRenderer) {
    let mut scene = ui.scene.borrow_mut();
    let fonts = renderer.text_system();
    scene.set_text_measurer(move |text: &str, size: f32, max: Option<f32>| {
        zgx_gpu::measure_text(&mut fonts.borrow_mut(), text, size, max)
    });
    let fonts = renderer.text_system();
    scene.set_font_text_shaper(
        move |text: &str,
              size: f32,
              width: Option<f32>,
              font: &zgx::text_layout::FontStyle|
              -> Box<dyn zgx::text_layout::TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
}
