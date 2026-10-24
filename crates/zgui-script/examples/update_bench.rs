//! What an update costs a script against the same interface in Rust,
//! headless: state the host publishes or a click changes, through effects
//! and the script's functions to the scene's nodes. Each scenario runs as a
//! Luau script and as idiomatic Rust; instructions retired (macOS) and
//! allocations per update, after a warm-up.
//! Run: cargo run --release -p zgui-script --example update_bench
//! (`TICKS` sets the updates, 600 by default; `ONLY=name` runs one scenario.)
#[path = "support/bench_limits.rs"]
mod bench_limits;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
use zgui::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    scene::Color,
    widgets::Ui,
};
use zgui_script::{ScriptHost, Value};

/// Instructions this thread has retired (macOS; 0 elsewhere).
fn instructions() -> u64 {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn thread_selfcounts(kind: i32, buffer: *mut u64, size: usize) -> i32;
        }
        let mut counts = [0u64; 2];
        // SAFETY: THSC_CPI (1) fills two u64s: instructions, cycles.
        unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) };
        counts[0]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

/// Counts the allocations made (`realloc` counts as one).
struct Counting;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
// SAFETY: every call goes to `System` unchanged (its contract the caller's);
// the counter is an atomic, which allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

const ROWS: usize = 200;
const CHANGED: usize = 10;

/// What changes an interface each update: the `Ui`, the update's number,
/// and the node named `button` if there is one.
type Step = Box<dyn FnMut(&mut Ui, usize, Option<zgui::scene::NodeId>)>;

/// One update's step, the frame after it, its cost.
struct Scenario {
    name: &'static str,
    /// The script, its `ROWS` set.
    script: &'static str,
    /// The same interface in Rust, and what changes it each update, for
    /// that many updates (what they publish made beforehand: the engine's
    /// cost is measured, not the app's model).
    rust: fn(&Ui, usize) -> (View, Step),
    /// What changes the script's interface each update (the host's side).
    luau: fn(&ScriptHost, usize) -> Step,
}

fn label(row: usize, tick: usize) -> String {
    format!("Row {row} at {tick}")
}
fn rows_value(tick: usize) -> Value {
    let rows: Vec<Value> = (0..ROWS)
        .map(|i| {
            // Each update, `CHANGED` rows (moving through the list) change.
            let at = if (i + tick).is_multiple_of(ROWS / CHANGED) {
                tick
            } else {
                0
            };
            Value::Map(std::rc::Rc::from([
                (std::rc::Rc::from("id"), Value::Num(i as f64)),
                (std::rc::Rc::from("label"), Value::Str(label(i, at).into())),
            ]))
        })
        .collect();
    Value::List(std::rc::Rc::from(rows))
}
/// Rows of a label each, ten of whose labels change each update.
fn rows_rust(ui: &Ui, ticks: usize) -> (View, Step) {
    let labels: Vec<_> = (0..ROWS).map(|i| ui.signal(label(i, 0))).collect();
    let published: Vec<Vec<String>> = (0..=ticks)
        .map(|tick| {
            (0..ROWS)
                .map(|i| {
                    let at = if (i + tick).is_multiple_of(ROWS / CHANGED) {
                        tick
                    } else {
                        0
                    };
                    label(i, at)
                })
                .collect()
        })
        .collect();
    let view = column().children(labels.iter().map(|label| {
        let label = label.clone();
        row()
            .gap(8.)
            .px(6.)
            .child(text_signal(move || label.get()).text_size(13.))
    }));
    (
        view,
        Box::new(move |ui, tick, _| {
            ui.runtime.batch(|| {
                for (signal, next) in labels.iter().zip(&published[tick]) {
                    if signal.with_untracked(|now| now != next) {
                        signal.set(next.clone());
                    }
                }
            })
        }),
    )
}
/// The rows each update publishes, made beforehand, published in turn.
fn rows_published(host: &ScriptHost, ticks: usize) -> Step {
    let rows = host.shared("rows", Value::List(std::rc::Rc::from([])));
    let published: Vec<Value> = (0..=ticks).map(rows_value).collect();
    Box::new(move |_, tick, _| {
        rows.set(published[tick].clone());
    })
}
fn click(ui: &mut Ui, node: zgui::scene::NodeId) {
    let bounds = ui.scene.borrow().bounds(node);
    let (x, y) = (bounds.x + 4., bounds.y + 4.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "template",
        script: r##"
ui.shared("count", 0)
return ui.text { "Count: {count}" }
"##,
        rust: |ui, _| {
            let count = ui.signal(0_usize);
            let view = text_signal({
                let count = count.clone();
                move || format!("Count: {}", count.get())
            });
            (
                view,
                Box::new(move |_, tick, _| {
                    count.set(tick);
                }),
            )
        },
        luau: |host, _| {
            let count = host.shared("count", 0.);
            Box::new(move |_, tick, _| {
                count.set(Value::Num(tick as f64));
            })
        },
    },
    Scenario {
        name: "function-prop",
        script: r##"
local selected = ui.shared("selected", 0)
local boxes = {}
for i = 1, ROWS do
    boxes[i] = ui.div {
        w = 20, h = 20,
        bg = function() return if selected:get() == i then "#ff8800" else "#223344" end,
    }
end
return ui.column { boxes }
"##,
        rust: |ui, _| {
            let selected = ui.signal(0_usize);
            let boxes = (1..=ROWS).map(|i| {
                let selected = selected.clone();
                div().size(20., 20.).reactive_style(move || {
                    Styles::new().bg(if selected.get() == i {
                        Color(0xff, 0x88, 0, 255)
                    } else {
                        Color(0x22, 0x33, 0x44, 255)
                    })
                })
            });
            let view = column().children(boxes);
            (
                view,
                Box::new(move |_, tick, _| {
                    selected.set(tick % ROWS + 1);
                }),
            )
        },
        luau: |host, _| {
            let selected = host.shared("selected", 0.);
            Box::new(move |_, tick, _| {
                selected.set(Value::Num((tick % ROWS + 1) as f64));
            })
        },
    },
    Scenario {
        name: "rows",
        script: r##"
local rows = ui.shared("rows", {})
return ui.column {
    ui.each(rows, function(item)
        return ui.row { gap = 8, px = 6, ui.text { item.label, text_size = 13 } }
    end, { key = function(item) return item.id end }),
}
"##,
        rust: rows_rust,
        luau: rows_published,
    },
    Scenario {
        name: "rows-in-place",
        script: r##"
local rows = ui.shared("rows", {})
return ui.column {
    ui.each(rows, function(item, i, row)
        return ui.row { gap = 8, px = 6, ui.text { function() return row:get().label end, text_size = 13 } }
    end, { key = function(item) return item.id end }),
}
"##,
        rust: rows_rust,
        luau: rows_published,
    },
    Scenario {
        name: "handler",
        script: r##"
local count = ui.state("count", 0)
return ui.column {
    ui.button { id = "button", w = 80, h = 30,
        on_click = function() count:set(count:get() + 1) end,
        ui.text { "Clicks" },
    },
    ui.text { "{count}" },
}
"##,
        rust: |ui, _| {
            let count = ui.signal(0_usize);
            let view = column()
                .child(
                    button()
                        .id("button")
                        .size(80., 30.)
                        .child(text("Clicks"))
                        .on_click({
                            let count = count.clone();
                            move || {
                                count.update(|n| *n += 1);
                            }
                        }),
                )
                .child(text_signal(move || count.get().to_string()));
            (
                view,
                Box::new(|ui, _, button| click(ui, button.expect("the button"))),
            )
        },
        luau: |_, _| Box::new(|ui, _, button| click(ui, button.expect("the button"))),
    },
];

/// Instructions and allocations per update.
fn run(scenario: &Scenario, luau: bool, ticks: usize) -> (f64, f64) {
    let mut ui = Ui::new(1200., 900.);
    let mut host = None;
    let step = if luau {
        let code = scenario.script.replace("ROWS", &ROWS.to_string());
        let script = ScriptHost::inline(&ui.runtime, "bench", code).limits(bench_limits::limits());
        let step = (scenario.luau)(&script, ticks);
        script.reload().expect("the bench script loads");
        let mounted = ui.mount(script.view());
        host = Some(script);
        (step, mounted)
    } else {
        let (view, step) = (scenario.rust)(&ui, ticks);
        let mounted = ui.mount(view);
        (step, mounted)
    };
    let (mut step, mounted) = step;
    let frame = |ui: &Ui| {
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
    };
    frame(&ui);
    let button = mounted.find("button");
    // What reading the counter costs, taken off each update.
    let overhead = (0..16)
        .map(|_| {
            let start = instructions();
            instructions() - start
        })
        .min()
        .unwrap_or(0);
    let warmup = 60.min(ticks / 2);
    let (mut instructions_spent, mut allocations) = (0, 0);
    for tick in 1..=ticks {
        let (start, allocated) = (instructions(), ALLOCATIONS.load(Relaxed));
        step(&mut ui, tick, button);
        frame(&ui);
        if tick > warmup {
            instructions_spent += (instructions() - start).saturating_sub(overhead);
            allocations += ALLOCATIONS.load(Relaxed) - allocated;
        }
    }
    drop(host);
    let measured = (ticks - warmup) as f64;
    (
        instructions_spent as f64 / measured,
        allocations as f64 / measured,
    )
}

fn main() {
    let ticks: usize = std::env::var("TICKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let only = std::env::var("ONLY").ok();
    println!("per update: instructions (k) and allocations, Luau against Rust");
    for scenario in SCENARIOS {
        if only.as_deref().is_some_and(|only| only != scenario.name) {
            continue;
        }
        let (luau, luau_allocs) = run(scenario, true, ticks);
        let (rust, rust_allocs) = run(scenario, false, ticks);
        println!(
            "  {:14} luau {:8.1}k {:6.1} | rust {:8.1}k {:6.1} | luau/rust {:5.2}x",
            scenario.name,
            luau / 1e3,
            luau_allocs,
            rust / 1e3,
            rust_allocs,
            luau / rust.max(1.)
        );
    }
}
