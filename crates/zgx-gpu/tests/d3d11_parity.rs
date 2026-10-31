//! Direct3D 11 (`ZGX_D3D11=1`, the `d3d11` feature) draws what Direct3D 12
//! does: cards whose bars and values change each frame (kept frames
//! patched), text through the glyph atlas, rounded borders, a list scrolled
//! inside a rounded clip (scroll copies, clip regions), and heavy's
//! continuous geometry (bars and a progress bar with fractional edges),
//! isolated layers (translucent, nested, and turning inside), and images
//! (a picture with straight alpha and a gradient's ramp), and canvas
//! polylines (a moving sparkline, a translucent stroke over itself), and
//! a truncated text's blurred shadow reaching past its box, and a counter
//! filled with a gradient (source-in and behind blends), at
//! 1×, 1.5× and 2×, byte for byte: both compile the same HLSL with FXC
//! (Direct3D 12 pinned to it, docs/windows.md). `ZGX_D3D11` is read once a
//! process, so the test runs itself again with it and compares that run's
//! frames with its own.
#[cfg(windows)]
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, widgets::Ui};
#[cfg(windows)]
use zgx_gpu::{GpuRenderer, text::FontData};

#[cfg(windows)]
const GEIST: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");
#[cfg(windows)]
const SIZE: (f32, f32) = (400., 300.);
#[cfg(windows)]
const SCALES: [f32; 3] = [1., 1.5, 2.];
#[cfg(windows)]
const FRAMES: u32 = 40;
/// Set in the run drawing with Direct3D 11.
#[cfg(windows)]
const CHILD: &str = "ZGX_D3D11_PARITY_CHILD";

#[cfg(windows)]
fn view(live: Signal<u32>, offset: Signal<f32>) -> View {
    let (spark, progress, turn, wave) = (live.clone(), live.clone(), live.clone(), live.clone());
    let counted = live.clone();
    let card = move |index: u32| {
        let (value, bars) = (live.clone(), live.clone());
        div()
            .absolute()
            .ml(8. + 128. * index as f32)
            .mt(8.)
            .size(120., 110.)
            .rounded(6.)
            .border(1.)
            .border_color(Color(40, 50, 64, 255))
            .bg(Color(22, 28, 38, 255))
            .child(
                text_signal(move || format!("{:.1} ms", 3.7 * (value.get() + index) as f32))
                    .text_size(15.)
                    .absolute()
                    .ml(8.)
                    .mt(6.)
                    .size(100., 20.),
            )
            .children((0..12).map(move |bar| {
                let bars = bars.clone();
                div()
                    .absolute()
                    .ml(8. + 8.5 * bar as f32)
                    .w(5.)
                    .rounded(2.)
                    .bg(Color(94, 177, 255, 255))
                    .reactive_style(move || {
                        let height = 2. + ((bars.get() + bar + index) % 9) as f32 * 7.25;
                        Styles::new().mt(102. - height).h(height)
                    })
            }))
    };
    let list = virtual_list(
        offset,
        22.,
        2,
        || 500,
        |index| index,
        |_, index, _| {
            div()
                .w(376.)
                .h(22.)
                .bg(if index % 2 == 0 {
                    Color(25, 34, 48, 255)
                } else {
                    Color(29, 40, 56, 255)
                })
                .child(
                    text(format!("row {index}"))
                        .text_size(13.)
                        .ml(6.)
                        .mt(3.)
                        .size(200., 16.),
                )
        },
    )
    .absolute()
    .ml(12.)
    .mt(130.)
    .w(376.)
    .h(150.)
    .rounded(10.)
    .overflow_hidden();
    // As heavy's sparklines and progress bars: edges between pixels that move
    // each frame, inside a rounded card.
    let continuous = div()
        .absolute()
        .ml(12.)
        .mt(286.)
        .size(376., 12.)
        .rounded(3.)
        .bg(Color(30, 38, 52, 255))
        .overflow_hidden()
        .children((0..40).map(move |bar| {
            let spark = spark.clone();
            div()
                .absolute()
                .ml(2.3 + 9.1 * bar as f32)
                .w(6.3)
                .bg(Color(126, 231, 135, 255))
                .reactive_style(move || {
                    let height = 1.7 + ((spark.get() * 3 + bar * 7) % 23) as f32 * 0.37;
                    Styles::new().mt(12. - height).h(height)
                })
        }))
        .child(
            div()
                .absolute()
                .mt(0.)
                .h(3.4)
                .bg(Color(255, 196, 64, 255))
                .reactive_style(move || {
                    Styles::new().w(376. * ((progress.get() % 37) as f32 + 0.43) / 37.)
                }),
        );
    // Layers: a translucent isolated subtree over the third card, a nested
    // one inside it, and a square turning in that, each frame.
    let layered = div()
        .absolute()
        .ml(250.)
        .mt(40.)
        .size(110., 90.)
        .isolated(true)
        .opacity(0.7)
        .rounded(8.)
        .bg(Color(120, 72, 200, 255))
        .child(
            text("layer")
                .text_size(13.)
                .absolute()
                .ml(8.)
                .mt(4.)
                .size(80., 16.),
        )
        .child(
            div()
                .absolute()
                .ml(30.)
                .mt(26.)
                .size(56., 56.)
                .isolated(true)
                .opacity(0.8)
                .rounded(28.)
                .bg(Color(255, 140, 90, 255))
                .child(
                    div()
                        .absolute()
                        .ml(14.)
                        .mt(14.)
                        .size(28., 28.)
                        .bg(Color(240, 240, 250, 255))
                        .reactive_style(move || Styles::new().rotate(turn.get() as f32 * 0.13)),
                ),
        );
    // Images: a picture with straight alpha, scaled between pixels, and a
    // gradient (its ramp an image too), over the second card.
    let pixels: Vec<u8> = (0..16 * 12)
        .flat_map(|i: u32| {
            let (x, y) = (i % 16, i / 16);
            [
                (x * 16) as u8,
                (y * 21) as u8,
                180,
                if (x + y) % 3 == 0 { 96 } else { 255 },
            ]
        })
        .collect();
    let picture = std::sync::Arc::new(zgx::image::ImageData::new(16, 12, pixels).unwrap());
    let pictured = div()
        .absolute()
        .ml(140.)
        .mt(36.)
        .size(112., 64.)
        .child(image("picture", picture).absolute().size(53.5, 40.25))
        .child(
            div()
                .absolute()
                .ml(56.)
                .size(56., 64.)
                .rounded(6.)
                .bg_gradient(
                    0.7,
                    vec![
                        zgx::canvas::GradientStop {
                            offset: 0.,
                            color: Color(255, 90, 120, 255),
                        },
                        zgx::canvas::GradientStop {
                            offset: 1.,
                            color: Color(40, 200, 255, 160),
                        },
                    ],
                ),
        );
    // Polylines: a sparkline moving each frame, mitred, and a translucent
    // round stroke over itself, on a canvas over the first card.
    let lines = canvas(move |_| {
        use zgx::canvas::{Canvas, Path, Stroke};
        let line = |points: &[(f32, f32)]| {
            let mut path = Path::builder();
            path.move_to(points[0].0, points[0].1);
            for &(x, y) in &points[1..] {
                path.line_to(x, y);
            }
            path.build().unwrap()
        };
        let phase = wave.get() as f32 * 0.31;
        let spark: Vec<(f32, f32)> = (0..40)
            .map(|k| {
                let t = k as f32;
                (2.3 + t * 2.4, 14. + 7. * (t * 0.7 + phase).sin())
            })
            .collect();
        let mut canvas = Canvas::new();
        canvas.stroke(
            line(&spark),
            Color(63, 185, 80, 255),
            Stroke {
                width: 1.5,
                ..Default::default()
            },
        );
        canvas.stroke(
            line(&[(4., 40.), (30., 22.), (52., 44.), (90., 26.)]),
            Color(255, 255, 255, 140),
            Stroke {
                width: 5.,
                ..Default::default()
            },
        );
        canvas
    })
    .absolute()
    .ml(12.)
    .mt(36.)
    .size(100., 50.);
    // A truncated text whose blurred shadow reaches past its box: its
    // glyphs clipped to the box, its shadow not.
    let shadowed = text("HHHH HHHH HHHH HHHH")
        .truncate()
        .size(130., 20.)
        .text_size(14.)
        .text_color(Color(229, 237, 247, 255))
        .text_shadow(0., 12., 3., Color(230, 60, 60, 255))
        .absolute()
        .ml(260.)
        .mt(250.);
    // A counter filled with a gradient, with a shadow: its own layer's
    // glyphs, the gradient source-in and the shadow behind them.
    let filled = text_signal(move || format!("{} frames", counted.get()))
        .size(110., 24.)
        .text_size(18.)
        .text_shadow(1., 2., 0., Color(20, 20, 20, 255))
        .text_gradient(zgx::decoration::Gradient::Linear {
            angle: 0.3,
            stops: vec![
                zgx::canvas::GradientStop {
                    offset: 0.,
                    color: Color(255, 120, 40, 255),
                },
                zgx::canvas::GradientStop {
                    offset: 1.,
                    color: Color(80, 140, 255, 255),
                },
            ]
            .into(),
        })
        .absolute()
        .ml(140.)
        .mt(270.);
    div()
        .w(SIZE.0)
        .h(SIZE.1)
        .bg(Color(16, 20, 28, 255))
        .text_color(Color(229, 237, 247, 255))
        .children((0..3).map(card))
        .child(list)
        .child(continuous)
        .child(layered)
        .child(pictured)
        .child(lines)
        .child(shadowed)
        .child(filled)
}

/// Each frame's pixels, RGBA, as this process draws them at `scale`.
#[cfg(windows)]
fn frames(scale: f32) -> Vec<Vec<u8>> {
    let mut gpu = GpuRenderer::new((SIZE.0 * scale) as u32, (SIZE.1 * scale) as u32).unwrap();
    gpu.set_scale_factor(scale);
    FontData::new(GEIST.to_vec())
        .unwrap()
        .install(&mut gpu.text_system().borrow_mut());
    let mut ui = Ui::new(SIZE.0, SIZE.1);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let (live, offset) = (ui.signal(0_u32), ui.signal(0_f32));
    ui.mount(view(live.clone(), offset.clone()));
    (0..FRAMES)
        .map(|frame| {
            live.set(frame);
            // Whole pixels at the scale (scroll copies), and some not.
            offset.set(frame as f32 * if frame % 4 == 3 { 7.3 } else { 6. });
            ui.prepare_frame();
            let report = ui.scene.borrow_mut().flush();
            gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
            gpu.readback().unwrap()
        })
        .collect()
}

#[cfg(windows)]
fn main() {
    use std::io::{Read, Write};
    if std::env::var_os(CHILD).is_some() {
        let mut out = std::io::stdout().lock();
        for scale in SCALES {
            for frame in frames(scale) {
                out.write_all(&frame).unwrap();
            }
        }
        return;
    }
    let ours: Vec<(f32, Vec<Vec<u8>>)> = SCALES
        .into_iter()
        .map(|scale| (scale, frames(scale)))
        .collect();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .env("ZGX_D3D11", "1")
        .env(CHILD, "1")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut theirs = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut theirs)
        .unwrap();
    assert!(
        child.wait().unwrap().success(),
        "the Direct3D 11 run failed"
    );
    let expected: usize = ours
        .iter()
        .flat_map(|(_, frames)| frames)
        .map(Vec::len)
        .sum();
    assert_eq!(
        theirs.len(),
        expected,
        "the Direct3D 11 run drew too few frames"
    );
    let mut rest = &theirs[..];
    for (scale, frames) in &ours {
        for (frame, ours) in frames.iter().enumerate() {
            let (theirs, after) = rest.split_at(ours.len());
            rest = after;
            let (mut differ, mut most) = (0, 0);
            for (a, b) in ours.iter().zip(theirs) {
                differ += usize::from(a != b);
                most = most.max(a.abs_diff(*b));
            }
            assert_eq!(
                differ, 0,
                "{scale}×, frame {frame}: {differ} bytes differ between Direct3D 12 and 11, by up to {most}"
            );
        }
        println!(
            "d3d11_parity {scale}×: {} frames byte-identical",
            frames.len()
        );
    }
}

#[cfg(not(windows))]
fn main() {}
