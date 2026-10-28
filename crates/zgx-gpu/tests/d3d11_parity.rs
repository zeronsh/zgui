//! Direct3D 11 (`ZGX_D3D11=1`, the `d3d11` feature) draws what Direct3D 12
//! does: cards whose bars and values change each frame (kept frames
//! patched), text through the glyph atlas, rounded borders, a list scrolled
//! inside a rounded clip (scroll copies, clip regions), and heavy's
//! continuous geometry (bars and a progress bar with fractional edges), at
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
    let (spark, progress) = (live.clone(), live.clone());
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
    div()
        .w(SIZE.0)
        .h(SIZE.1)
        .bg(Color(16, 20, 28, 255))
        .text_color(Color(229, 237, 247, 255))
        .children((0..3).map(card))
        .child(list)
        .child(continuous)
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
