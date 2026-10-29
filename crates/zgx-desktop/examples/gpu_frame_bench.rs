//! What the heavy dashboard (see zgx_workload::heavy) costs the GPU a
//! frame, or with `ZGX_MODE=animate` the animation workload (see
//! zgx_workload::animation), offscreen at a Retina scale: each frame waited for, its Metal GPU
//! time read back (macOS), with the frame's damaged pixels, draws and
//! instances. A hash of the frame's pixels every 100 ticks tells whether a
//! change draws exactly the same thing.
//! Run: ZGX_MODE=both cargo run --release -p zgx-desktop --example gpu_frame_bench
//! (`TICKS` sets the frames, 600 by default; `SCALE` the device scale, 2.
//! `PACE=60` draws 60 frames a second, as a window does: the GPU then
//! clocks as it would, and scripts/macsample.swift's GPU megacycles a
//! second of this process, over 60, are its cycles a frame:
//! `PACE=60 TICKS=1000 gpu_frame_bench & /tmp/zgx-macsample $! 4 10 0.05`.
//! Unpaced GPU times swing with the GPU's clock, 2x for the same build.
//! About 0.17 Mcycles of a frame is the GPU waking for it at all: an empty
//! render pass costs that, whatever the target's size.)
//!
//! On macOS each frame is presented as a window presents it, through
//! surfaces of the renderer's own that a layer shows, each brought up to
//! date from the one shown before it is drawn (`GpuRenderer::debug_for_layer`):
//! a frame measured into one retained target skips that catch-up, and once
//! made a change look 6% cheaper than windows found it. `PRESENT=headless`
//! draws into one retained target, as elsewhere.
//!
//! `ZGX_MODE=fade`: a list scrolling under a fade (`fade_edges`, 48 px
//! top and bottom) of `CARDS` filled, rounded, clipping cards (20 by
//! default) with art over each, 1.37 px a frame; `FADE_BANDS=0` fades item
//! by item instead of each band as one. `ROWS=1` has rows of an icon and
//! a line of text on a transparent background in their place (nothing in
//! a band overlaps: faded item by item either way). It reports the main thread's
//! instructions a frame too (macOS): update, layout, damage and the
//! renderer's encoding, not the wait for the GPU.
use std::hash::{Hash, Hasher};
use zgx::widgets::Ui;
use zgx_gpu::GpuRenderer;
use zgx_workload::{Mode, heavy};

#[path = "support/animation_view.rs"]
mod animation_view;
#[path = "support/headless_font.rs"]
mod headless_font;
#[path = "support/heavy_view.rs"]
mod view;

/// What a tick changes in the scene.
type Advance = Box<dyn FnMut(&Ui, usize)>;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let ticks: usize = env("TICKS", 600);
    let pace: f64 = env("PACE", 0.);
    let started = std::time::Instant::now();
    let scale: f32 = env("SCALE", 2.);
    let animate = zgx::env::var("ZGX_MODE").as_deref() == Ok("animate");
    let fade = zgx::env::var("ZGX_MODE").as_deref() == Ok("fade");
    let (width, height) = if animate || fade {
        (960., 720.)
    } else {
        (heavy::WIDTH, heavy::HEIGHT)
    };
    let mut ui = Ui::new(width, height);
    let size = (
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    );
    // Through a layer's surfaces, as a window presents (macOS), unless asked
    // for one retained target.
    let layer = cfg!(target_os = "macos") && zgx::env::var("PRESENT").as_deref() != Ok("headless");
    #[cfg(target_os = "macos")]
    let mut gpu = if layer {
        GpuRenderer::debug_for_layer(objc2_quartz_core::CALayer::new(), size.0, size.1)
    } else {
        GpuRenderer::new(size.0, size.1)
    }
    .expect("GPU");
    #[cfg(not(target_os = "macos"))]
    let mut gpu = GpuRenderer::new(size.0, size.1).expect("GPU");
    gpu.set_scale_factor(scale);
    gpu.set_fade_bands(env("FADE_BANDS", 1) != 0);
    headless_font::font().install(&mut gpu.text_system().borrow_mut());
    gpu.install_text(&mut ui.scene.borrow_mut());
    // Each tick's change: the dashboard's data, or the animation's clock a
    // 120 Hz refresh on (as `animation_headless`).
    let (name, mut advance): (String, Advance) = if fade {
        faded_cards(&ui, env("CARDS", 20), env("ROWS", 0) != 0)
    } else if animate {
        let clock = ui.signal(0_f32);
        ui.mount(animation_view::animation(clock.clone()));
        let advance = move |_: &Ui, tick: usize| {
            clock.set(tick as f32 / 120.);
        };
        ("Animate".into(), Box::new(advance))
    } else {
        let mut work = heavy::Heavy::from_env();
        let mode = work.mode;
        let live = ui.signal(0_u64);
        let scroll = ui.signal(0_f32);
        let log = ui.signal(work.visible_log());
        ui.mount(view::dashboard(live.clone(), scroll.clone(), log.clone()));
        let advance = move |ui: &Ui, _: usize| {
            work.tick();
            ui.runtime.batch(|| {
                if matches!(mode, Mode::Stream | Mode::Both) {
                    live.set(work.live);
                    log.set(work.visible_log());
                }
                if matches!(mode, Mode::Scroll | Mode::Both) {
                    scroll.set(work.scroll);
                }
            });
        };
        (format!("{mode:?}"), Box::new(advance))
    };
    let mut times = Vec::new();
    let mut instructions = 0_u64;
    let (mut damaged, mut draws, mut instances, mut passes) = (0_u64, 0_usize, 0_usize, 0_usize);
    // The scene's own damage: its exact union (on a grid of logical
    // points) and how many rectangles.
    let (mut raw, mut rects) = (0_u64, 0_usize);
    let mut grid = vec![false; (width * height) as usize];
    let mut pixels = std::collections::hash_map::DefaultHasher::new();
    for tick in 0..ticks {
        if tick > 0 {
            advance(&ui, tick);
        }
        if pace > 0. {
            let due = started + std::time::Duration::from_secs_f64(tick as f64 / pace);
            std::thread::sleep(due.saturating_duration_since(std::time::Instant::now()));
        }
        let before = retired();
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        let stats = gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        if tick >= 60 {
            instructions += retired() - before;
        }
        if layer {
            // Shown: the frame waited for, as a window waits before its
            // layer takes the surface; every 100 ticks, what it shows.
            #[cfg(target_os = "macos")]
            {
                gpu.present().unwrap();
                if tick % 100 == 99 {
                    gpu.shown_pixels()
                        .expect("a surface shown")
                        .2
                        .hash(&mut pixels);
                }
            }
        } else if tick % 100 == 99 || pace == 0. {
            // Waits for the frame, and (every 100 ticks) is what it drew.
            // Paced, only then: a readback is GPU work of its own.
            let frame = gpu.readback().unwrap();
            if tick % 100 == 99 {
                frame.hash(&mut pixels);
            }
        }
        #[cfg(target_os = "macos")]
        let time = gpu.take_gpu_time();
        #[cfg(not(target_os = "macos"))]
        let time: Option<std::time::Duration> = None;
        if tick >= 60 {
            grid.fill(false);
            for d in &report.damage {
                let (x0, y0) = (d.x.max(0.) as usize, d.y.max(0.) as usize);
                let x1 = ((d.x + d.width).ceil() as usize).min(width as usize);
                let y1 = ((d.y + d.height).ceil() as usize).min(height as usize);
                for y in y0..y1 {
                    grid[y * width as usize + x0..y * width as usize + x1.max(x0)].fill(true);
                }
            }
            raw += grid.iter().filter(|c| **c).count() as u64 * (scale * scale) as u64;
            rects += report.damage.len();
            times.extend(time);
            damaged += stats.damaged_pixels;
            draws += stats.draw_calls;
            instances += stats.instances;
            passes += stats.render_passes;
        }
    }
    let measured = ticks.saturating_sub(60).max(1) as f64;
    times.sort();
    let at = |q: f64| {
        times
            .get(((times.len() as f64 - 1.) * q) as usize)
            .map_or(0., |t| t.as_secs_f64() * 1e3)
    };
    let mean = times.iter().map(|t| t.as_secs_f64()).sum::<f64>() / times.len().max(1) as f64 * 1e3;
    println!(
        "{name}{}: GPU {:.3} ms p10, {:.3} median, {:.3} p90, {mean:.3} mean ({} frames timed); damaged {:.2} Mpx, \
         {:.1} draws, {:.0} instances, {:.2} passes a frame; scene damage {:.2} Mpx in {:.1} rects; \
         {:.0} K instructions a frame; pixels {:016x}",
        if layer {
            " (through a layer)"
        } else {
            " (headless)"
        },
        at(0.1),
        at(0.5),
        at(0.9),
        times.len(),
        damaged as f64 / measured / 1e6,
        draws as f64 / measured,
        instances as f64 / measured,
        passes as f64 / measured,
        raw as f64 / measured / 1e6,
        rects as f64 / measured,
        instructions as f64 / measured / 1e3,
        pixels.finish(),
    );
}

/// The calling thread's retired instructions so far (0 where not counted).
fn retired() -> u64 {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            /// Kind 1 (THSC_CPI): instructions, then cycles.
            fn thread_selfcounts(kind: i32, buffer: *mut u64, size: usize) -> i32;
        }
        let mut counts = [0u64; 2];
        // SAFETY: Two u64s for THSC_CPI.
        if unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) } == 0 {
            return counts[0];
        }
    }
    0
}

/// `ZGX_MODE=fade`'s scene (see the top): its tick scrolls the list.
fn faded_cards(ui: &Ui, cards: usize, rows: bool) -> (String, Advance) {
    use zgx::scene::{Color, Layout, NodeKind, QuadStyle, Style, Transform};
    let mut scene = ui.scene.borrow_mut();
    let root = scene.root();
    scene.set_kind(root, NodeKind::Container(Layout::Overlay));
    let clip = scene.append(
        root,
        NodeKind::Container(Layout::Column),
        Style {
            width: Some(720.),
            height: Some(560.),
            clip: true,
            fade_edges: [48., 48.],
            ..Default::default()
        },
    );
    scene.set_transform(clip, Transform { x: 120., y: 80. });
    let list = scene.append(
        clip,
        NodeKind::Container(Layout::Column),
        Style {
            gap: 12.,
            ..Default::default()
        },
    );
    for i in (0..cards).filter(|_| rows) {
        let row = scene.append(
            list,
            NodeKind::Container(Layout::Row),
            Style {
                width: Some(720.),
                height: Some(96.),
                gap: 16.,
                ..Default::default()
            },
        );
        let shade = (i * 37 % 120) as u8;
        scene.append(
            row,
            NodeKind::Rect(Color(40 + shade / 2, 40, 80 + shade, 255)),
            Style {
                width: Some(48.),
                height: Some(48.),
                ..Default::default()
            },
        );
        scene.append(
            row,
            NodeKind::Text {
                text: format!("Row {i}: a line of text beside its icon").into(),
                color: Color(30, 30, 40, 255),
                font_size: 15.,
            },
            Style::default(),
        );
    }
    for i in (0..cards).filter(|_| !rows) {
        let card = scene.append(
            list,
            NodeKind::Panel {
                layout: Layout::Row,
                quad: QuadStyle {
                    fill: Color(240, 238, 232, 255),
                    radius: 12.,
                    ..Default::default()
                },
            },
            Style {
                width: Some(720.),
                height: Some(96.),
                clip: true,
                gap: 16.,
                ..Default::default()
            },
        );
        let shade = (i * 37 % 120) as u8;
        scene.append(
            card,
            NodeKind::Rect(Color(40 + shade / 2, 40, 80 + shade, 255)),
            Style {
                width: Some(200.),
                height: Some(96.),
                ..Default::default()
            },
        );
        scene.append(
            card,
            NodeKind::Rect(Color(90, 90, 110, 255)),
            Style {
                width: Some(360.),
                height: Some(14.),
                ..Default::default()
            },
        );
    }
    let extent = (cards as f32 * 108. - 560.).max(1.);
    let advance = move |ui: &Ui, tick: usize| {
        let y = (tick as f32 * 1.37) % extent;
        ui.scene
            .borrow_mut()
            .set_transform(list, Transform { x: 0., y: -y });
    };
    let what = if rows { "rows" } else { "cards" };
    (format!("Fade {cards} {what}"), Box::new(advance))
}
