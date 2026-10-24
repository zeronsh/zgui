//! What one `set_key` on a long list state costs a mail-like script,
//! headless: 4,000 messages, a mailbox filter and unread counts computed
//! over them, a list of the filtered rows; each tick marks one message read
//! or unread. Counts instructions retired per tick (macOS) and times them.
//! Run: cargo run --release -p zgui-script --example list_state_bench
//! (`N` sets the message count, `TICKS` the ticks.)
#[path = "support/bench_limits.rs"]
mod bench_limits;
use std::time::Instant;
use zgui::widgets::Ui;
use zgui_script::ScriptHost;

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
        unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut usage) };
        usage.fields[29]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

const SCRIPT: &str = r##"
local N = ...
local rows = {}
local boxes = { "inbox", "archive", "sent", "spam" }
for i = 1, N do
    rows[i] = {
        id = i,
        mailbox = boxes[(i % 4) + 1],
        unread = i % 3 == 0,
        starred = false,
        sender = "Sender " .. i,
        subject = "Subject line " .. i,
        preview = "A preview of message " .. i,
    }
end
local messages = ui.state("messages", rows)
local mailbox = ui.state("mailbox", "inbox")
local shown = ui.computed(function()
    local out = {}
    local box = mailbox:get()
    for _, m in messages:get() do
        if m.mailbox == box then
            table.insert(out, m)
        end
    end
    return out
end)
local unread = ui.computed(function()
    local counts = {}
    for _, m in messages:get() do
        if m.unread then
            counts[m.mailbox] = (counts[m.mailbox] or 0) + 1
        end
    end
    return counts
end)
local tick = 0
ui.command { id = "tick", run = function()
    tick += 1
    local i = (tick * 7) % N + 1
    messages:set_key({ i, "unread" }, not messages:peek()[i].unread)
end }
return ui.column { w = 800, h = 600,
    ui.text { function() return "Unread: " .. tostring(unread:get().inbox or 0) end },
    ui.list { h = 560, height = 28, count = function() return #shown:get() end,
        row = function(i)
            return ui.row { h = 28,
                ui.text { function() local m = shown:get()[i]; return if m then m.subject else "" end },
                ui.text { function() local m = shown:get()[i]; return if m and m.unread then "*" else "" end },
            }
        end,
    },
}
"##;

fn main() {
    let n: usize = std::env::var("N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000);
    let ticks: usize = std::env::var("TICKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let mut ui = Ui::new(800., 600.);
    let code = SCRIPT.replace("local N = ...", &format!("local N = {n}"));
    let host = ScriptHost::inline(&ui.runtime, "bench", code).limits(bench_limits::limits());
    host.reload().expect("the bench script loads");
    ui.mount(host.view());
    ui.prepare_frame();
    for _ in 0..10 {
        host.run_command("tick");
        ui.prepare_frame();
    }
    let (start, at) = (instructions(), Instant::now());
    for _ in 0..ticks {
        host.run_command("tick");
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
    }
    let elapsed = at.elapsed();
    println!(
        "{n} rows: {:.2} M instructions, {:.3} ms per tick",
        (instructions() - start) as f64 / 1e6 / ticks as f64,
        elapsed.as_secs_f64() * 1e3 / ticks as f64
    );
    if let Some(error) = host.error().get() {
        println!("script error: {error}");
    }
}
