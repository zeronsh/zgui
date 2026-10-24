//! The cost of `ui.each`'s reorder animation, headless: reversing 1,000 rows
//! of three heights, with and without `animate`. Times the change (keys,
//! the rows' old places, the reorder), the frame whose layout applies it
//! (the rows' new places and the moves' start), the frames that move the
//! rows, and checks nothing runs once they settle.
//! Run: cargo run --release -p zgui-script --example script_layout_bench
#[path = "support/bench_limits.rs"]
mod bench_limits;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgui::{
    compose::{TaskRunner, prelude::*},
    frame::{Frame, FrameClock},
    task::LocalExecutor,
    widgets::Ui,
};
use zgui_script::{ScriptHost, Value};

const N: usize = 1000;
const REORDERS: u32 = 20;

struct Costs {
    change: Duration,
    layout: Duration,
    frame: Duration,
    frames: u32,
    layout_nodes: usize,
    composited: usize,
}

fn run(options: &str) -> Costs {
    let mut ui = Ui::new(1200., 900.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let clock = FrameClock::new();
    let script = format!(
        r##"local rows = ui.shared("rows", nil)
        return ui.scroll {{ w = 400, h = 900, ui.column {{
            ui.each(rows, function(i)
                return ui.div {{ w = 200, h = 10 + i % 3 * 5, bg = "#58a6ff" }}
            end, {{ key = function(i) return i end, {options} }}),
        }} }}"##
    );
    let host = ScriptHost::inline(&ui.runtime, "bench", script).limits(bench_limits::limits());
    let order: Vec<Value> = (0..N).map(|i| Value::Num(i as f64)).collect();
    let rows = host.shared("rows", Value::List(order.clone().into()));
    host.reload().unwrap();
    ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        provide(clock.clone(), host.view()),
    ));
    let settle = || while executor.borrow_mut().tick() > 0 {};
    settle();
    ui.prepare_frame();
    settle();
    ui.scene.borrow_mut().flush();
    let interval = Duration::from_secs_f64(1. / 120.);
    let start = Instant::now();
    let mut index = 0u64;
    let mut frame = |ui: &Ui| {
        index += 1;
        clock.deliver(Frame {
            index,
            time: start + interval * index as u32,
            interval,
        });
        settle();
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        // Tasks the frame woke (the engine, when moves start) run, as a
        // window's executor runs them.
        settle();
        report
    };
    let mut costs = Costs {
        change: Duration::ZERO,
        layout: Duration::ZERO,
        frame: Duration::ZERO,
        frames: 0,
        layout_nodes: 0,
        composited: 0,
    };
    for n in 0..REORDERS {
        let mut list = order.clone();
        if n % 2 == 0 {
            list.reverse();
        }
        let began = Instant::now();
        rows.set(Value::List(list.into()));
        settle();
        costs.change += began.elapsed();
        let began = Instant::now();
        frame(&ui);
        costs.layout += began.elapsed();
        while clock.wants_frame() {
            let began = Instant::now();
            let report = frame(&ui);
            costs.frame += began.elapsed();
            costs.frames += 1;
            costs.layout_nodes += report.layout_nodes + report.paint_nodes;
            costs.composited += report.composite_nodes;
        }
        // Settled: nothing asks for a frame and a frame finds nothing to do.
        assert!(!clock.wants_frame());
        assert!(frame(&ui).is_idle(), "idle after the rows settle");
    }
    costs
}

fn main() {
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!("{N} rows reversed, {REORDERS} times, mean:");
    for (name, options) in [("without animate", ""), ("with animate", "animate = true")] {
        let c = run(options);
        let per = |d: Duration| ms(d) / REORDERS as f64;
        let frame = if c.frames > 0 {
            format!(
                "{:.3} ms over {} frames ({} nodes composited a frame, {} laid out or painted)",
                ms(c.frame) / c.frames as f64,
                c.frames / REORDERS,
                c.composited / c.frames as usize,
                c.layout_nodes
            )
        } else {
            "none".into()
        };
        println!(
            "  {name:16}: change {:.2} ms, its layout frame {:.2} ms, moving frames {frame}",
            per(c.change),
            per(c.layout),
        );
    }
}
