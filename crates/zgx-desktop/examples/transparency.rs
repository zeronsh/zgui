//! Native compositor alpha probe. Empty client pixels remain transparent.
//!
//! `--composition` (macOS) checks, in the process, what the window server
//! composites over the desktop: the window and every layer under its frame
//! are not opaque and fill nothing, and the frame its layer shows holds the
//! panels' premultiplied colours over alpha zero, through colour changes, a
//! hidden panel and a resize. No screen is captured. Prints `PASS` or a
//! `FAIL` line for each failed check, then exits.
use std::time::Duration;
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let composition = std::env::args().any(|arg| arg == "--composition");
    Application::new()
        .window(WindowOptions {
            title: "zgx transparency".into(),
            width: 320.,
            height: 240.,
            transparent: true,
            ..Default::default()
        })
        .run(move |cx| {
            let blue = cx.ui.signal(false);
            let visible = cx.ui.signal(true);
            let close = cx.window.clone();
            let root = overlay()
                .w_full()
                .h_full()
                .focusable(true)
                .on_event({
                    let blue = blue.clone();
                    let visible = visible.clone();
                    move |event| {
                        if event.phase == EventPhase::Bubble {
                            return;
                        }
                        if let InputEvent::KeyDown {
                            key, repeat: false, ..
                        } = &event.event
                        {
                            match key {
                                Key::Character(key) if key.eq_ignore_ascii_case("c") => {
                                    blue.set(!blue.get());
                                    println!("COLOR blue={}", blue.get());
                                }
                                Key::Character(key) if key.eq_ignore_ascii_case("v") => {
                                    visible.set(!visible.get());
                                    println!("VISIBLE {}", visible.get());
                                }
                                Key::Escape => close.close(),
                                _ => return,
                            }
                            event.prevent_default();
                        }
                    }
                })
                // A: [20,140) x [20,100), alpha 128. C switches red/blue;
                // V removes its pixels while retaining the same layout node.
                .child(
                    div()
                        .id("alpha")
                        .absolute()
                        .ml(20.)
                        .mt(20.)
                        .size(120., 80.)
                        .reactive_style({
                            let blue = blue.clone();
                            let visible = visible.clone();
                            move || {
                                Styles::new()
                                    .bg(if blue.get() {
                                        rgba(0x2060e080)
                                    } else {
                                        rgba(0xe0402080)
                                    })
                                    .opacity(if visible.get() { 1. } else { 0. })
                            }
                        }),
                )
                // B overlaps A only on [100,140) x [60,100), alpha64.
                .child(
                    div()
                        .id("overlap")
                        .absolute()
                        .ml(100.)
                        .mt(60.)
                        .size(120., 80.)
                        .bg(rgba(0x40e08040)),
                )
                .child(
                    button()
                        .id("opaque")
                        .absolute()
                        .ml(20.)
                        .mt(160.)
                        .size(120., 40.)
                        .bg(rgb(0xeeeeee))
                        .text_color(rgb(0x102030))
                        .child(text("Close"))
                        .on_click({
                            let window = cx.window.clone();
                            move || window.close()
                        }),
                );
            let view = cx.render(root);
            cx.ui.input.focus(&cx.ui.scene, Some(view.node()));
            let (shown_blue, shown_visible) = (blue.clone(), visible.clone());
            cx.on_closed(move || {
                let (blue, visible) = (shown_blue, shown_visible);
                println!("TRANSPARENCY blue={} visible={}", blue.get(), visible.get())
            });
            if composition {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    let failures = check_composition(&window, &blue, &visible).await;
                    for failure in &failures {
                        println!("FAIL {failure}");
                    }
                    if failures.is_empty() {
                        println!("PASS");
                    }
                    window.close();
                });
            } else if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(30)).await;
                    window.close();
                });
            }
        })
}

/// Premultiplied RGBA of `0xRRGGBBAA`.
fn premultiplied(color: u32) -> [f32; 4] {
    let [r, g, b, a] = color.to_be_bytes().map(f32::from);
    [r * a / 255., g * a / 255., b * a / 255., a]
}
/// `top` over `under`, premultiplied.
fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let keep = 1. - top[3] / 255.;
    [0, 1, 2, 3].map(|i| top[i] + under[i] * keep)
}

/// The window's composition once it has drawn what changed: read after a
/// few frames' time.
async fn composition(window: &zgx_desktop::WindowHandle) -> zgx_desktop::WindowComposition {
    zgx::timer::sleep(Duration::from_millis(400)).await;
    let reply = window.debug_composition();
    loop {
        if let Ok(composition) = reply.try_recv() {
            return composition;
        }
        zgx::timer::sleep(Duration::from_millis(16)).await;
    }
}

async fn check_composition(
    window: &zgx_desktop::WindowHandle,
    blue: &zgx::reactive::Signal<bool>,
    visible: &zgx::reactive::Signal<bool>,
) -> Vec<String> {
    let (red, blue_fill, green) = (
        premultiplied(0xe0402080),
        premultiplied(0x2060e080),
        premultiplied(0x40e08040),
    );
    let clear = [0.; 4];
    let mut failures = Vec::new();
    // (stage, change, width in points, alpha's fill or none)
    let stages: [(&str, u8, f64, Option<[f32; 4]>); 5] = [
        ("initial", 0, 320., Some(red)),
        ("blue", 1, 320., Some(blue_fill)),
        ("hidden", 2, 320., None),
        ("restored", 3, 320., Some(blue_fill)),
        ("resized", 4, 400., Some(blue_fill)),
    ];
    for (stage, change, width, alpha) in stages {
        match change {
            1 => _ = blue.set(true),
            2 => _ = visible.set(false),
            3 => _ = visible.set(true),
            4 => window.set_inner_size(400., 300.),
            _ => {}
        }
        let mut found = composition(window).await;
        // A resize lands within a few frames.
        for _ in 0..10 {
            if found.frame.as_ref().is_some_and(|(w, _, _)| {
                (*w as f64 / width).fract() == 0. && *w as f64 / width >= 1.
            }) {
                break;
            }
            found = composition(window).await;
        }
        let mut fail = |what: String| failures.push(format!("{stage}: {what}"));
        if found.window_opaque {
            fail("the native window is opaque".into());
        }
        if found.window_background_alpha != 0. {
            fail(format!(
                "the native window's background has alpha {}",
                found.window_background_alpha
            ));
        }
        // Visible layers (none of them or their ancestors hidden or
        // transparent) that show the frame, and those that would cover the
        // desktop under it: the frame's ancestors and the content's layers.
        let mut shown = Vec::new();
        let mut path: Vec<usize> = Vec::new();
        for (index, layer) in found.layers.iter().enumerate() {
            path.truncate(layer.depth);
            path.push(index);
            let seen = path.iter().all(|&i| {
                let layer = &found.layers[i];
                !layer.hidden && layer.opacity > 0.
            });
            if seen && layer.shows_frame {
                shown.push(path.clone());
            }
        }
        if shown.len() != 1 {
            fail(format!(
                "{} visible layers show the frame read, not one",
                shown.len()
            ));
        }
        let under: std::collections::BTreeSet<usize> = shown
            .iter()
            .flatten()
            .copied()
            .chain((0..found.layers.len()).filter(|&i| found.layers[i].in_content))
            .collect();
        for i in under {
            let layer = &found.layers[i];
            if layer.hidden || layer.opacity == 0. {
                continue;
            }
            if layer.opaque || layer.background_alpha != 0. {
                fail(format!(
                    "layer {i} ({}) at depth {}: opaque={} background alpha {}",
                    layer.class, layer.depth, layer.opaque, layer.background_alpha
                ));
            }
        }
        let Some((frame_width, frame_height, _)) = found.frame.as_ref() else {
            fail("no frame shown".into());
            continue;
        };
        let scale = *frame_width as f64 / width;
        if scale.fract() != 0. || scale < 1. {
            fail(format!(
                "frame {frame_width} pixels wide for {width} points"
            ));
            continue;
        }
        let height = *frame_height as f64 / scale;
        let tint = alpha.unwrap_or(clear);
        // (name, x, y in points, expected premultiplied RGBA)
        let mut samples = vec![
            ("empty top right", 260., 20., clear),
            ("empty bottom right", width - 10., height - 10., clear),
            ("alpha", 40., 40., tint),
            ("alpha right", 90., 30., tint),
            ("overlap", 120., 80., over(green, tint)),
            ("overlap only", 200., 120., green),
            ("between", 170., 40., clear),
        ];
        if width > 320. {
            samples.push(("resized area", 360., 260., clear));
        }
        for (name, x, y, expected) in samples {
            let Some(pixel) = found.pixel((x * scale) as u32, (y * scale) as u32) else {
                fail(format!("{name}: ({x}, {y}) outside the frame"));
                continue;
            };
            if (0..4).any(|i| (f32::from(pixel[i]) - expected[i]).abs() > 2.) {
                fail(format!(
                    "{name} at ({x}, {y}): {pixel:?}, expected {:?}",
                    expected.map(|v| v.round() as u8)
                ));
            }
        }
        // The opaque control: alpha 255 at its left edge.
        match found.pixel((30. * scale) as u32, (180. * scale) as u32) {
            Some(pixel) if pixel[3] == 255 => {}
            pixel => fail(format!("opaque control: {pixel:?}")),
        }
        println!(
            "STAGE {stage} {}x{} layers={} scale={scale}",
            frame_width,
            frame_height,
            found.layers.len()
        );
    }
    failures
}
