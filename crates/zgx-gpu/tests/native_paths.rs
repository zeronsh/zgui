//! Each frame drawn three ways on the Mac, which must agree byte for byte:
//! by its damage through a window's own surfaces (Metal directly, where the
//! frame allows), by its damage offscreen (wgpu on Metal), and whole by a
//! renderer new to it. Scenes: a reordered, edited list in a rounded clip;
//! a list scrolled while its rows change; zoom, a turn about each axis in
//! perspective, nested blends and layer shaders; boxes moved alone (kept
//! frames patched); and a resize, at device scales 1 and 2.
//!
//! Its own binary without the test harness: layer presentation runs on the
//! main thread, which the harness keeps for itself.
#[cfg(target_os = "macos")]
use zgx::{
    compose::prelude::*,
    reactive::Signal,
    scene::{BlendMode, Color, Rect},
    widgets::Ui,
};
#[cfg(target_os = "macos")]
use zgx_gpu::GpuRenderer;

#[cfg(target_os = "macos")]
const SIZE: (f32, f32) = (360., 260.);

/// A scene's view of the tick it is at, and of a scroll offset.
#[cfg(target_os = "macos")]
type Scene = fn(Signal<u32>, Signal<f32>) -> View;

/// Twelve rows reordered and edited each tick, in a rounded clip.
#[cfg(target_os = "macos")]
fn reordered(tick: Signal<u32>, _: Signal<f32>) -> View {
    let keys = tick.clone();
    div().size(SIZE.0, SIZE.1).bg(Color(16, 20, 28, 255)).child(
        div()
            .absolute()
            .ml(10.5)
            .mt(8.)
            .size(220., 240.)
            .rounded(9.)
            .overflow_hidden()
            .bg(Color(24, 32, 44, 255))
            .child(keyed(
                move || {
                    let t = keys.get() as usize;
                    let mut order: Vec<u32> = (0..12).collect();
                    order.rotate_left(t % 12);
                    if t % 3 == 1 {
                        order.reverse();
                    }
                    order
                },
                move |row, _| {
                    let tick = tick.clone();
                    div()
                        .w(200.)
                        .h(19.5)
                        .reactive_style(move || {
                            let on = (tick.get() + row).is_multiple_of(4);
                            Styles::new().bg(if on {
                                Color(70, 110, 190, 255)
                            } else {
                                Color(34, 44, 60, 255)
                            })
                        })
                        .child(text(format!("row {row}")).text_size(12.).ml(6.).mt(2.))
                },
            )),
    )
}

/// A list scrolled by whole and fractional steps while its rows change,
/// in a rounded panel.
#[cfg(target_os = "macos")]
fn scrolled(tick: Signal<u32>, offset: Signal<f32>) -> View {
    let rows = virtual_list(
        offset,
        18.,
        2,
        || 400,
        |index| index,
        move |_, index, _| {
            let tick = tick.clone();
            div()
                .w(300.)
                .h(18.)
                .bg(if index.is_multiple_of(2) {
                    Color(25, 34, 48, 255)
                } else {
                    Color(29, 40, 56, 255)
                })
                .child(text_signal(move || {
                    // Every fifth row changes as it scrolls; the rest are
                    // copied.
                    let changing = if index.is_multiple_of(5) {
                        tick.get()
                    } else {
                        0
                    };
                    format!("{index}: {changing}")
                }))
        },
    )
    .w(300.)
    .h(200.);
    // As an app's sidebar: an opaque, square scroll view inside a rounded
    // panel whose corners stay put as it scrolls (#800).
    div().size(SIZE.0, SIZE.1).bg(Color(12, 14, 20, 255)).child(
        div()
            .absolute()
            .ml(20.)
            .mt(20.)
            .size(300., 200.)
            .rounded(10.)
            .overflow_hidden()
            .bg(Color(24, 30, 40, 255))
            .child(rows),
    )
}

/// A card turned about each axis in perspective, and inside it a layer
/// shaded and blended over another blended layer (a holo card); beside it,
/// a shaded layer blended straight onto the window (so the window's frames
/// take wgpu's path), and a box zoomed.
#[cfg(target_os = "macos")]
fn holo(tick: Signal<u32>, _: Signal<f32>) -> View {
    holo_with(tick, true)
}
/// `holo` without the layer blended onto the window: its frames take
/// Metal directly, compositing the card's layer (its blends drawn inside).
#[cfg(target_os = "macos")]
fn holo_card(tick: Signal<u32>, _: Signal<f32>) -> View {
    holo_with(tick, false)
}
#[cfg(target_os = "macos")]
fn holo_with(tick: Signal<u32>, onto_window: bool) -> View {
    let shine = zgx::decoration::FillShader::new(
        "fn layer(input: LayerInput) -> vec4<f32> { let c = input.color; return vec4(c.g, c.b, c.r, c.a); }",
        None,
    );
    let (turn, zoom, glow) = (tick.clone(), tick.clone(), tick.clone());
    let tint = shine.clone();
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(40, 60, 90, 255))
        .child(
            div()
                .absolute()
                .ml(60.)
                .mt(30.)
                .size(220., 180.)
                .rounded(14.)
                .bg(Color(200, 160, 90, 255))
                .perspective(500.)
                .reactive_style(move || {
                    let t = turn.get() as f32;
                    Styles::new()
                        .rotate_x(0.05 * (t % 9.) - 0.2)
                        .rotate_y(0.07 * (t % 7.) - 0.2)
                })
                .child(
                    div()
                        .absolute()
                        .ml(20.)
                        .mt(20.)
                        .size(180., 140.)
                        .blend(BlendMode::Multiply)
                        .bg(Color(120, 200, 240, 255))
                        .child(
                            div()
                                .absolute()
                                .ml(30.)
                                .mt(30.)
                                .size(110., 70.)
                                .rounded(6.)
                                .bg(Color(250, 90, 120, 255))
                                .layer_shader(shine)
                                .blend(BlendMode::Screen)
                                .child(text("HOLO").text_size(20.).ml(10.).mt(20.)),
                        ),
                ),
        )
        .child(
            div()
                .absolute()
                .ml(10.)
                .mt(210.)
                .size(120., 40.)
                .bg(Color(90, 220, 140, 255))
                .reactive_style(move || Styles::new().zoom(1. + 0.05 * (zoom.get() % 6) as f32))
                .child(text("zoom").text_size(14.).ml(8.).mt(10.)),
        )
        .children(onto_window.then(move || {
            div()
                .absolute()
                .ml(290.)
                .mt(150.)
                .size(60., 90.)
                .rounded(10.)
                .layer_shader(tint)
                .blend(BlendMode::Overlay)
                .reactive_style(move || {
                    Styles::new().bg(Color(80 + (glow.get() * 20 % 160) as u8, 200, 120, 255))
                })
        }))
}

/// Boxes moved alone each tick (kept frames patched in place).
#[cfg(target_os = "macos")]
fn moved(tick: Signal<u32>, _: Signal<f32>) -> View {
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(18, 18, 26, 255))
        .children((0..24u32).map(move |cell| {
            let tick = tick.clone();
            div()
                .absolute()
                .size(26., 18.5)
                .rounded(if cell.is_multiple_of(3) { 4. } else { 0. })
                .bg(Color(60 + (cell * 7 % 150) as u8, 120, 200, 255))
                .reactive_style(move || {
                    let t = (tick.get() + cell) as f32;
                    Styles::new()
                        .ml(10. + (cell % 6) as f32 * 55. + (t * 3.5) % 20.)
                        .mt(10. + (cell / 6) as f32 * 60. + (t * 1.25) % 9.)
                })
        }))
}

/// A frame's pixels as RGBA, from BGRA.
#[cfg(target_os = "macos")]
fn rgba(mut bgra: Vec<u8>) -> Vec<u8> {
    for pixel in bgra.chunks_mut(4) {
        pixel.swap(0, 2);
    }
    bgra
}

/// Where two frames differ: how many pixels, their bounds, the largest
/// difference in a channel.
#[cfg(target_os = "macos")]
fn differ(a: &[u8], b: &[u8], width: usize) -> Option<String> {
    let (mut count, mut most) = (0, 0u8);
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for (index, (p, q)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        if p != q {
            count += 1;
            most = most.max(p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap());
            let (x, y) = (index % width, index / width);
            bounds = Some(bounds.map_or((x, y, x, y), |(x0, y0, x1, y1)| {
                (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
            }));
        }
    }
    (count > 0 || a.len() != b.len()).then(|| {
        format!(
            "{count} px differ (up to {most}) in {bounds:?}, sizes {} and {}",
            a.len(),
            b.len()
        )
    })
}

/// What a run's frames took through the window's surfaces: frames encoded
/// with Metal directly, scroll copies, frames patched as kept.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct Took {
    metal: u64,
    copies: usize,
    patched: usize,
}

/// `scene` run for `ticks` at `scale`, resized midway when `resize`: every
/// frame's three drawings compared, the differences found returned with
/// what the frames took.
#[cfg(target_os = "macos")]
fn run(name: &str, scene: Scene, scale: f32, ticks: u32, resize: bool) -> (Vec<String>, Took) {
    let mut took = Took::default();
    let mut size = SIZE;
    let pixels = |size: (f32, f32)| ((size.0 * scale) as u32, (size.1 * scale) as u32);
    let host = objc2_quartz_core::CALayer::new();
    let (w, h) = pixels(size);
    let mut layer = GpuRenderer::debug_for_layer(host.clone(), w, h).unwrap();
    layer.set_scale_factor(scale);
    let mut offscreen = GpuRenderer::new(w, h).unwrap();
    offscreen.set_scale_factor(scale);
    let mut ui = Ui::new(size.0, size.1);
    ui.scene.borrow_mut().set_pixel_scale(scale);
    layer.install_text(&mut ui.scene.borrow_mut());
    let tick = ui.signal(0);
    let offset = ui.signal(0.);
    ui.mount(scene(tick.clone(), offset.clone()));
    let mut found = Vec::new();
    for t in 0..ticks {
        tick.set(t);
        // Whole device pixels at 1x and 2x, and some not.
        offset.set(t as f32 * if t % 3 == 2 { 5.25 } else { 7. });
        if resize && t == ticks / 2 {
            size = (size.0 + 37.5, size.1 - 21.);
            let (w, h) = pixels(size);
            layer.resize(w, h);
            offscreen.resize(w, h);
            ui.scene.borrow_mut().resize(size.0, size.1);
        }
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        let scene = ui.scene.borrow();
        let stats = layer.render(&scene, &damage).unwrap();
        took.copies += stats.scroll_copies;
        took.patched += usize::from(stats.patched_items > 0);
        layer.present().unwrap();
        let shown = rgba(layer.shown_pixels().expect("a surface shown").2);
        offscreen.render(&scene, &damage).unwrap();
        let drawn = offscreen.readback().unwrap();
        let (w, h) = pixels(size);
        let mut fresh = GpuRenderer::new_with_context(w, h, &offscreen.context()).unwrap();
        fresh.set_scale_factor(scale);
        fresh
            .render(&scene, &[Rect::new(0., 0., size.0, size.1)])
            .unwrap();
        let whole = fresh.readback().unwrap();
        if let Some(diff) = differ(&drawn, &whole, w as usize) {
            found.push(format!(
                "{name} {scale}x tick {t}: offscreen by damage vs whole: {diff}"
            ));
        }
        if let Some(diff) = differ(&shown, &whole, w as usize) {
            found.push(format!(
                "{name} {scale}x tick {t}: shown by damage vs whole: {diff}"
            ));
        }
    }
    took.metal = layer.debug_metal_frames();
    (found, took)
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        let mut found = Vec::new();
        for scale in [1., 2.] {
            for (name, scene, resize) in [
                ("reordered", reordered as Scene, true),
                ("scrolled", scrolled as Scene, false),
                ("holo", holo as Scene, false),
                ("holo_card", holo_card as Scene, false),
                ("moved", moved as Scene, true),
            ] {
                let (differences, took) = run(name, scene, scale, 24, resize);
                println!(
                    "native_paths: {name} at {scale}x: {} frames differ ({} through Metal directly, {} scroll copies, {} patched)",
                    differences.len(),
                    took.metal,
                    took.copies,
                    took.patched
                );
                // Each scene takes the path it is here for.
                match name {
                    "scrolled" => assert!(took.copies > 0, "{name} at {scale}x: no scroll copied"),
                    "moved" => assert!(took.patched > 0, "{name} at {scale}x: no frame patched"),
                    _ => {}
                }
                // A layer blended onto the window takes wgpu's path whole.
                if name == "holo" {
                    assert_eq!(
                        took.metal, 0,
                        "{name} at {scale}x: a blend onto the window through Metal"
                    );
                } else {
                    assert!(
                        took.metal > 0,
                        "{name} at {scale}x: no frame through Metal directly"
                    );
                }
                found.extend(differences);
            }
        }
        for difference in found.iter().take(40) {
            eprintln!("{difference}");
        }
        assert!(found.is_empty(), "{} frames differ (above)", found.len());
    }
}
