//! Pixel parity between renderer paths: the Windows suite's scenes (lab,
//! heavy, animate, vlist) drawn offscreen as their windows draw them, read
//! back every tick. `write <dir>` writes each scene's hashes, a line a tick,
//! and its frame every `FRAMES_EVERY` ticks; `compare <a> <b>` reports where
//! two such directories differ, and by how much. Made to compare Direct3D 11
//! (`ZGX_D3D11=1`, the `d3d11` feature) with Direct3D 12:
//!
//! ```powershell
//! cargo build --release -p zgx-desktop --features d3d11 --example render_parity
//! target\release\examples\render_parity.exe write out\d3d12
//! $env:ZGX_D3D11 = "1"; target\release\examples\render_parity.exe write out\d3d11
//! target\release\examples\render_parity.exe compare out\d3d12 out\d3d11
//! ```
//!
//! `TICKS` (240 a scene), `SCALE` (1), `FRAMES_EVERY` (30), `SCENES`
//! (all of them: `lab,heavy,animate,vlist-fling,vlist-jump`).
use std::path::{Path, PathBuf};
use zgx::{compose::prelude::*, widgets::Ui};
use zgx_gpu::GpuRenderer;
use zgx_workload::{Mode, Workload, heavy::Heavy, vlist};

#[path = "support/animation_view.rs"]
mod animation_view;
#[path = "support/headless_font.rs"]
mod headless_font;
#[path = "support/heavy_view.rs"]
mod heavy_view;
#[path = "workload_headless.rs"]
#[allow(dead_code)]
mod lab;
#[path = "support/vlist_view.rs"]
mod vlist_view;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const SCENES: &str = "lab,heavy,animate,vlist-fling,vlist-jump";

/// What moves a scene on a tick.
type Step = Box<dyn FnMut(&mut Ui, u64)>;

/// A scene mounted on `ui`, and what moves it.
fn scene(name: &str, ui: &mut Ui) -> Step {
    match name {
        "lab" => {
            let mut work = Workload::from_env();
            work.mode = Mode::Both;
            let stream = ui.signal(work.visible_text());
            let offset = ui.signal(work.scroll_offset);
            ui.mount(lab::app(stream.clone(), offset.clone()));
            Box::new(move |_, _| {
                work.tick();
                stream.set(work.visible_text());
                offset.set(work.scroll_offset);
            })
        }
        "heavy" => {
            let mut work = Heavy::from_env();
            work.mode = Mode::Both;
            let live = ui.signal(0_u64);
            let scroll = ui.signal(0_f32);
            let log = ui.signal(work.visible_log());
            ui.mount(heavy_view::dashboard(
                live.clone(),
                scroll.clone(),
                log.clone(),
            ));
            Box::new(move |ui, _| {
                work.tick();
                ui.runtime.batch(|| {
                    live.set(work.live);
                    log.set(work.visible_log());
                    scroll.set(work.scroll);
                });
            })
        }
        "animate" => {
            let clock = ui.signal(0_f32);
            ui.mount(animation_view::animation(clock.clone()));
            Box::new(move |_, tick| {
                clock.set(tick as f32 / 60.);
            })
        }
        "vlist-fling" | "vlist-jump" => {
            let mode = match name {
                "vlist-jump" => vlist::Mode::Jump,
                _ => vlist::Mode::Fling,
            };
            let heights = VariableHeights::new(&ui.runtime, vlist::ROWS, vlist_view::ESTIMATE);
            let offset = ui.signal(0_f32);
            let list = measured_virtual_list(
                offset.clone(),
                heights.clone(),
                2,
                |i| i,
                |_, index, _| vlist_view::chat_row(index),
            )
            .w_full()
            .h_full();
            ui.mount(vlist_view::frame(list));
            // As vlist_workload moves it.
            Box::new(move |_, tick| match vlist::jump(tick) {
                Some(row) if mode == vlist::Mode::Jump => {
                    offset.set(heights.row_offset(row));
                }
                _ => {
                    let max = (heights.content_height() - vlist::HEIGHT).max(0.);
                    let moved = offset.get() + vlist::delta(mode, tick);
                    offset.set(if moved > max { 0. } else { moved });
                }
            })
        }
        other => panic!("no scene `{other}` (of {SCENES})"),
    }
}

fn size(name: &str) -> (f32, f32) {
    match name {
        "heavy" => (zgx_workload::heavy::WIDTH, zgx_workload::heavy::HEIGHT),
        "animate" => (960., 720.),
        "lab" => (zgx_workload::WIDTH, zgx_workload::HEIGHT),
        _ => (vlist::WIDTH, vlist::HEIGHT),
    }
}

/// FNV-1a of a frame's bytes.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn write(dir: &Path) {
    let ticks: u64 = env("TICKS", 240);
    let scale: f32 = env("SCALE", 1.);
    let every: u64 = env("FRAMES_EVERY", 30);
    std::fs::create_dir_all(dir).expect("output directory");
    for name in std::env::var("SCENES")
        .as_deref()
        .unwrap_or(SCENES)
        .split(',')
    {
        let (width, height) = size(name);
        let mut ui = Ui::new(width, height);
        let mut gpu = GpuRenderer::new(
            (width * scale).round() as u32,
            (height * scale).round() as u32,
        )
        .expect("GPU");
        gpu.set_scale_factor(scale);
        headless_font::font().install(&mut gpu.text_system().borrow_mut());
        gpu.install_text(&mut ui.scene.borrow_mut());
        let mut step = scene(name, &mut ui);
        let mut hashes = String::new();
        for tick in 0..ticks {
            if tick > 0 {
                step(&mut ui, tick);
            }
            ui.prepare_frame();
            let report = ui.scene.borrow_mut().flush();
            gpu.render(&ui.scene.borrow(), &report.damage)
                .expect("render");
            let frame = gpu.readback().expect("readback");
            hashes.push_str(&format!("{:016x}\n", hash(&frame)));
            if tick % every == 0 || tick + 1 == ticks {
                std::fs::write(dir.join(format!("{name}-{tick}.rgba")), &frame).expect("frame");
            }
        }
        std::fs::write(dir.join(format!("{name}.hashes")), hashes).expect("hashes");
        println!(
            "{name}: {ticks} ticks at {scale}x on {:?}",
            gpu.adapter_info().backend
        );
    }
}

fn compare(a: &Path, b: &Path) -> bool {
    let mut same = true;
    for name in std::env::var("SCENES")
        .as_deref()
        .unwrap_or(SCENES)
        .split(',')
    {
        let read = |dir: &Path| std::fs::read_to_string(dir.join(format!("{name}.hashes")));
        let (Ok(left), Ok(right)) = (read(a), read(b)) else {
            println!("{name}: missing in one of them");
            same = false;
            continue;
        };
        let differ: Vec<usize> = left
            .lines()
            .zip(right.lines())
            .enumerate()
            .filter(|(_, (l, r))| l != r)
            .map(|(tick, _)| tick)
            .collect();
        if differ.is_empty() && left.lines().count() == right.lines().count() {
            println!(
                "{name}: {} ticks, every frame byte-identical",
                left.lines().count()
            );
            continue;
        }
        same = false;
        println!(
            "{name}: {} of {} ticks differ (first {:?})",
            differ.len(),
            left.lines().count(),
            &differ[..differ.len().min(8)]
        );
        // Kept frames: how many bytes differ, and by how much at most.
        for tick in &differ {
            let frame = |dir: &Path| std::fs::read(dir.join(format!("{name}-{tick}.rgba"))).ok();
            let (Some(l), Some(r)) = (frame(a), frame(b)) else {
                continue;
            };
            let (count, max) = l
                .iter()
                .zip(&r)
                .filter(|(x, y)| x != y)
                .fold((0, 0), |(count, max), (x, y)| {
                    (count + 1, max.max(x.abs_diff(*y)))
                });
            println!(
                "  tick {tick}: {count} of {} bytes differ, by at most {max}",
                l.len()
            );
        }
    }
    same
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match (args.get(1).map(String::as_str), args.get(2), args.get(3)) {
        (Some("write"), Some(dir), None) => write(&PathBuf::from(dir)),
        (Some("compare"), Some(a), Some(b)) => {
            if !compare(Path::new(a), Path::new(b)) {
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("render_parity write <dir> | compare <dir> <dir>");
            std::process::exit(2);
        }
    }
}
