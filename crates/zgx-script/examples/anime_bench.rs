//! Per-frame CPU cost of animating many elements, headless: the anime
//! engine (Luau API, Rust engine) against the same motion written directly
//! with zgx signals and reactive styles.
//! Run: cargo run --release -p zgx-script --example anime_bench
//! (`ONLY=luau|rust|direct` runs one engine, `FRAMES` sets the frame count.)
#[path = "support/bench_limits.rs"]
mod bench_limits;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    compose::{TaskRunner, prelude::*},
    frame::{Frame, FrameClock},
    task::LocalExecutor,
    widgets::Ui,
};
use zgx_script::ScriptHost;

const N: usize = 1000;
fn frames() -> u32 {
    std::env::var("FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600)
}

/// Mounts `view`, runs `FRAMES` 120 Hz frames and returns the mean cost of
/// one frame: tasks (the engine or the Rust driver) plus the scene's flush.
fn run(
    view: impl FnOnce(&mut Ui) -> View,
    drive: impl Fn(&Ui, f32) + 'static,
) -> (Duration, Duration, Duration) {
    let mut ui = Ui::new(1200., 900.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let clock = FrameClock::new();
    // For the script, building includes evaluating it.
    let built = Instant::now();
    let root = view(&mut ui);
    ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        provide(clock.clone(), root),
    ));
    let settle = || while executor.borrow_mut().tick() > 0 {};
    settle();
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let mounted = built.elapsed();
    let interval = Duration::from_secs_f64(1. / 120.);
    let start = Instant::now();
    let (mut animate, mut scene) = (Duration::ZERO, Duration::ZERO);
    for index in 1..=frames() {
        let began = Instant::now();
        clock.deliver(Frame {
            index: index as u64,
            time: start + interval * index,
            interval,
        });
        drive(&ui, index as f32 / 120.);
        settle();
        let animated = Instant::now();
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        std::hint::black_box(report.damage.len());
        animate += animated - began;
        scene += animated.elapsed();
    }
    (animate / frames().max(1), scene / frames().max(1), mounted)
}

fn main() {
    let only = std::env::var("ONLY").ok();
    let want = |n: &str| only.as_deref().is_none_or(|o| o == n);
    // Built in a loop, as a script would.
    let script = format!(
        "anime.animate('.dot', {{ y = 300, opacity = 0.2, duration = 1000, delay = anime.stagger(2), loop = true, alternate = true, ease = 'inOutSine' }})\n\
         local dots = {{}}\n\
         for i = 0, {} do dots[i + 1] = ui.div {{ class = 'dot', w = 6, h = 6, bg = '#58a6ff', x = i % 40 * 20 }} end\n\
         return ui.overlay {{ w = 1200, h = 900, dots }}",
        N - 1
    );
    let luau = if !want("luau") {
        Default::default()
    } else {
        run(
            |ui| {
                let host =
                    ScriptHost::inline(&ui.runtime, "bench", script).limits(bench_limits::limits());
                let evaluated = Instant::now();
                host.reload().unwrap();
                eprintln!(
                    "script evaluation {:.2} ms",
                    evaluated.elapsed().as_secs_f64() * 1e3
                );
                host.view()
            },
            |_, _| {},
        )
    };

    // The same motion in Rust: a signal per dot, a reactive style reading it.
    let phases: Rc<RefCell<Vec<zgx::reactive::Signal<f32>>>> = Rc::default();
    let rust = if !want("rust") {
        Default::default()
    } else {
        run(
            {
                let phases = phases.clone();
                move |ui| {
                    let mut children = Vec::new();
                    for i in 0..N {
                        let t = ui.signal(0_f32);
                        phases.borrow_mut().push(t.clone());
                        let x = (i % 40 * 20) as f32;
                        children.push(div().size(6., 6.).bg(rgb(0x58a6ff)).reactive_style(
                            move || {
                                let p = t.get();
                                Styles::new().translate(x, 300. * p).opacity(1. - 0.8 * p)
                            },
                        ));
                    }
                    overlay().size(1200., 900.).children(children)
                }
            },
            move |ui, seconds| {
                ui.runtime.batch(|| {
                    for (i, t) in phases.borrow().iter().enumerate() {
                        let local = ((seconds * 1000. - i as f32 * 2.) / 1000.).max(0.);
                        let cycle = local % 2.;
                        let p = if cycle > 1. { 2. - cycle } else { cycle };
                        t.set(0.5 - (std::f32::consts::PI * p).cos() / 2.);
                    }
                });
            },
        )
    };
    // The floor: the same values written straight to the scene, no engine,
    // signals or styles.
    let nodes: Rc<RefCell<Vec<zgx::scene::NodeId>>> = Rc::default();
    let direct = if !want("direct") {
        Default::default()
    } else {
        run(
            {
                let nodes = nodes.clone();
                move |_| {
                    let children: Vec<View> = (0..N)
                        .map(|_| div().size(6., 6.).bg(rgb(0x58a6ff)))
                        .collect();
                    let nodes = nodes.clone();
                    overlay()
                        .size(1200., 900.)
                        .children(children)
                        .on_mount(move |handle| {
                            let scene = handle.scene().unwrap();
                            nodes
                                .borrow_mut()
                                .extend_from_slice(scene.borrow().children(handle.node()));
                        })
                }
            },
            move |ui, seconds| {
                let mut scene = ui.scene.borrow_mut();
                for (i, node) in nodes.borrow().iter().enumerate() {
                    let local = ((seconds * 1000. - i as f32 * 2.) / 1000.).max(0.);
                    let cycle = local % 2.;
                    let p = 0.5
                        - (std::f32::consts::PI * if cycle > 1. { 2. - cycle } else { cycle })
                            .cos()
                            / 2.;
                    scene.set_transform(
                        *node,
                        zgx::scene::Transform {
                            x: (i % 40 * 20) as f32,
                            y: 300. * p,
                        },
                    );
                    let mut effects = scene.effects(*node);
                    effects.opacity = 1. - 0.8 * p;
                    scene.set_effects(*node, effects);
                }
            },
        )
    };
    println!("{N} elements, y + opacity every frame, mean per frame:");
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    for (name, (animate, scene, mounted)) in [
        ("luau + anime engine", luau),
        ("rust signals", rust),
        ("direct scene (floor)", direct),
    ] {
        println!(
            "  {name:20}: {:>7.1} µs  (animate {:>6.1}, scene {:>6.1})  build+mount+layout {:>5.2} ms",
            us(animate + scene),
            us(animate),
            us(scene),
            mounted.as_secs_f64() * 1e3
        );
    }
}
