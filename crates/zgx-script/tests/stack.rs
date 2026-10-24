//! A deep interface mounts, lays out and tears down on a 1 MB stack, the
//! size of the main thread's on Windows, in a debug build too: mounting,
//! lowering and layout recurse once per element, and a debug build's
//! frames are large.
mod common;

use common::Harness;

/// Levels of four nested elements each (a pane, a row, a scroll view and
/// its content) with a button beside: 128 elements deep, as deep as apps
/// and browsers nest.
const LEVELS: usize = 32;

#[test]
fn a_tree_128_elements_deep_runs_on_a_one_megabyte_stack() {
    let script = format!(
        r##"
local node = ui.text {{ "leaf", text_size = 12, color = "#8899aa" }}
for i = 1, {LEVELS} do
    node = ui.column {{ p = 4, gap = 2, bg = "#101010", rounded = 4,
        ui.row {{ px = 2, items = "center",
            ui.button {{ px = 6, on_click = function() end, ui.text "go" }},
            ui.scroll {{ h = 400, ui.div {{ w_full = true, node }} }},
        }},
    }}
end
return node
"##
    );
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let h = Harness::inline(&script);
            h.frames(2);
        })
        .expect("a thread")
        .join()
        .expect("the tree mounts, lays out and goes");
}
