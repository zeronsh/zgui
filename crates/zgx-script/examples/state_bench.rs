//! What a state update costs a script, headless: one key of a 1,000-entry
//! map, one field deep in a list of tables, a push, and a write of an equal
//! value, by copying the table (the pattern before `set_key`) and with the
//! state methods.
//! Run: cargo run --release -p zgx-script --example state_bench
use std::time::{Duration, Instant};
use zgx::reactive::Runtime;
use zgx_script::{Limits, ScriptHost};

const N: usize = 2000;

const SCRIPT: &str = r#"
local N = ...
local big = {}
for i = 1, 1000 do
    big["k" .. i] = i
end
local map = ui.state("map", big)
local tabs = {}
for i = 1, 100 do
    table.insert(tabs, { id = "t" .. i, title = "Tab " .. i, root = { kind = "pane", id = "p" .. i } })
end
local doc = ui.state("doc", { tabs = tabs, active = "t1" })
local list = ui.state("list", {})

local function bench(id, run)
    ui.command { id = id, run = function()
        for i = 1, N do run(i) end
    end }
end
bench("map_copy", function(i)
    local m = table.clone(map:peek())
    m.k500 = i
    map:set(m)
end)
bench("nested_copy", function(i)
    local d = table.clone(doc:peek())
    local ts = table.clone(d.tabs)
    local t = table.clone(ts[50])
    t.title = "Tab " .. i
    ts[50] = t
    d.tabs = ts
    doc:set(d)
end)
bench("equal_copy", function(i)
    local m = table.clone(map:peek())
    map:set(m)
end)
bench("push", function(i)
    list:push(i)
end)
bench("push_short", function(i)
    if i % 10 == 0 then list:set({}) end
    list:push(i)
end)
bench("clear", function(i)
    if i == 1 then list:set({}) end
end)
if map.set_key then
    bench("map_set_key", function(i)
        map:set_key("k500", i)
    end)
    bench("nested_set_key", function(i)
        doc:set_key({ "tabs", 50, "title" }, "Tab " .. i)
    end)
    bench("equal_set_key", function(i)
        map:set_key("k500", 7)
    end)
    bench("map_update", function(i)
        map:update(function(m) m.k500 = i end)
    end)
    bench("map_patch", function(i)
        map:patch({ k500 = i, k501 = i })
    end)
end
return ui.div {}
"#;

fn main() {
    // `state_bench <id> <n>` runs one benchmark `n` times and nothing else,
    // for counting instructions (`/usr/bin/time -l` on macOS).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [id, n] = args.as_slice() {
        let n: usize = n.parse().expect("a count");
        let code = SCRIPT.replace("local N = ...", &format!("local N = {n}"));
        let runtime = Runtime::new();
        let host = ScriptHost::inline(&runtime, "bench", code).limits(Limits {
            load: Duration::from_secs(10),
            call: Duration::from_secs(600),
            ..Limits::default()
        });
        host.reload().expect("the bench script loads");
        if n > 0 {
            assert!(host.run_command(id), "no benchmark `{id}`");
        }
        return;
    }
    let runtime = Runtime::new();
    let code = SCRIPT.replace("local N = ...", &format!("local N = {N}"));
    let host = ScriptHost::inline(&runtime, "bench", code).limits(Limits {
        load: Duration::from_secs(10),
        call: Duration::from_secs(60),
        ..Limits::default()
    });
    host.reload().expect("the bench script loads");
    let ids: Vec<String> = host
        .commands()
        .with_untracked(|c| c.iter().map(|c| c.id.clone()).collect());
    for id in ids.iter().filter(|id| *id != "clear") {
        // Warm up, then take the best of five runs.
        host.run_command(id);
        let mut best = Duration::MAX;
        for _ in 0..5 {
            host.run_command("clear");
            let start = Instant::now();
            host.run_command(id);
            best = best.min(start.elapsed());
        }
        println!(
            "{id:16} {:8.2} µs per update",
            best.as_secs_f64() * 1e6 / N as f64
        );
    }
}
