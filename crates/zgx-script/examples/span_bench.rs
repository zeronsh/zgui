//! What rich text's spans cost a script, headless: `SCENE=spans-plain` builds
//! paragraphs of plain spans again each pass (`ui.span` read, lowered,
//! mounted, removed), `spans-follow` builds them once and restyles every run
//! through a state each pass (the options that follow), and `spans-md` builds
//! paragraphs shaped as markdown's are (bold, code chips, links). Those are
//! Luau spans in markdown's shape: `markdown_view` builds its runs in Rust and
//! does not go through `ui.span`, so that row does not cover it.
//! `REPS` passes over `N` paragraphs; the gate's rows are sized in
//! `scripts/perf/gate.py`. Prints instructions and setup's share on macOS.
//! Run: SCENE=spans-follow cargo run --release -p zgx-script --example span_bench
#[path = "support/bench_limits.rs"]
mod bench_limits;
use zgx::widgets::Ui;
use zgx_script::{ScriptHost, Value};

/// Instructions this process has retired (macOS; 0 elsewhere).
fn instructions() -> u64 {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Usage {
            uuid: [u8; 16],
            fields: [u64; 40],
        }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut Usage) -> i32;
        }
        let mut usage = Usage {
            uuid: [0; 16],
            fields: [0; 40],
        };
        // RUSAGE_INFO_V4: `ri_instructions` is the 30th field after the uuid.
        unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut usage) };
        usage.fields[29]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

const PLAIN: &str = r##"
local function paragraph(i: number)
    return ui.rich {
        wrap = true,
        "Row ",
        ui.span { "strong", bold = true },
        " beside ",
        ui.span { "coloured", color = "#88ccff" },
        " and ",
        ui.span { "italic", italic = true },
        " and ",
        ui.span { tostring(i), text_size = 12 },
    }
end
"##;

const MARKDOWN: &str = r##"
local function paragraph(i: number)
    return ui.rich {
        wrap = true,
        "Some prose with ",
        ui.span { "bold words", weight = 700 },
        ", a call to ",
        ui.span { "render_frame()", font = "mono", bg = "#ffffff14", padding = { 4, 0.5 }, radius = 3 },
        ", ",
        ui.span { "emphasis", italic = true },
        ", a ",
        ui.span { "link", color = "#5ea5ff", underline = true, href = "https://zgx.zeron.sh" },
        " and ",
        ui.span { "struck", strike = true },
        ` to line {i}.`,
    }
end
"##;

/// `paragraph(i)`'s `N`, built again whenever `gen` changes.
const REBUILT: &str = r##"
local gen = ui.state("gen", 0)
local generations = ui.computed(function()
    return { gen:get() }
end)
return ui.each(generations, function(_generation)
    local rows = {}
    for i = 1, N do
        rows[i] = paragraph(i)
    end
    return ui.column { w = 900, gap = 2, rows }
end, { key = function(generation) return generation end })
"##;

/// Paragraphs built once, every run's style following `on`.
const FOLLOWED: &str = r##"
local on = ui.state("on", false)
local function color()
    return if on:get() then "#ff8844" else "#44ff88"
end
local rows = {}
for i = 1, N do
    rows[i] = ui.rich {
        wrap = true,
        "Row ",
        ui.span { "strong", bold = on },
        " beside ",
        ui.span { "coloured", color = color },
        " and ",
        ui.span { "italic", italic = on, color = color },
        " and ",
        ui.span { tostring(i), underline = on },
    }
end
return ui.column { w = 900, gap = 2, rows }
"##;

fn main() {
    let number = |name: &str, default: usize| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let scene = std::env::var("SCENE").unwrap_or_else(|_| "spans-plain".into());
    let (paragraphs, reps) = (number("N", 40), number("REPS", 50));
    let follows = scene == "spans-follow";
    let code = format!(
        "local N = {paragraphs}\n{}",
        match scene.as_str() {
            "spans-plain" => format!("{PLAIN}{REBUILT}"),
            "spans-md" => format!("{MARKDOWN}{REBUILT}"),
            "spans-follow" => FOLLOWED.to_string(),
            other => panic!("no scene {other}"),
        }
    );
    let start = instructions();
    let mut ui = Ui::new(900., 700.);
    let host = ScriptHost::inline(&ui.runtime, "bench", code).limits(bench_limits::limits());
    host.reload().expect("the bench script loads");
    ui.mount(host.view());
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let built = instructions();
    for rep in 1..=reps {
        let change = if follows {
            ("on", Value::Bool(rep % 2 == 1))
        } else {
            ("gen", Value::Num(rep as f64))
        };
        host.set_state(change.0, change.1)
            .expect("the scene's state");
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
    }
    if let Some(error) = host.error().get() {
        panic!("{error}");
    }
    let (setup, passes) = (built - start, instructions() - built);
    if setup + passes > 0 {
        println!(
            "{scene}: {} passes over {paragraphs}: setup {:.1} M ({:.0}%), passes {:.1} M",
            reps,
            setup as f64 / 1e6,
            setup as f64 * 100. / (setup + passes) as f64,
            passes as f64 / 1e6
        );
    }
}
