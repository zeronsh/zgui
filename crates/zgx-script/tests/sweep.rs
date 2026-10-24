//! A sweeping mask (a text shimmer) is the engine's work alone: each step
//! moves the mask in the scene and repaints the masked node, with no
//! layout, no script and nothing else damaged. Budgets per step.
mod common;

use common::Harness;
use zgx::scene::Rect;

const SCRIPT: &str = r##"local band = ui.bitmap(160, 1, function(x)
    return math.max(0, 1 - (math.abs(x - 80) / 80) ^ 2)
end)
local off = ui.state("off", 0)
return ui.column { w = 400,
    ui.scroll { h = 100, offset = off,
        ui.text { "Working for 3s", id = "base", color = "#666666" },
        ui.column { id = "band",
            mask = { band, w = 160, h = 40, x = -160, y = -10,
                sweep = { x = 640, duration = 2000, frame_rate = 30 } },
            ui.text { "Working for 3s", color = "#ffffff" },
        },
        ui.div { h = 400 },
    },
    ui.button { id = "away", w = 10, h = 10, on_click = function() off:set(300) end },
}"##;

fn within(r: &Rect, outer: &Rect) -> bool {
    r.x >= outer.x.floor() - 1.
        && r.y >= outer.y.floor() - 1.
        && r.x + r.width <= (outer.x + outer.width).ceil() + 1.
        && r.y + r.height <= (outer.y + outer.height).ceil() + 1.
}

#[test]
fn a_sweeping_mask_costs_one_node_composite_per_step() {
    let mut h = Harness::inline(SCRIPT);
    let band = h.find("band").expect("the band");
    // Warm up: the sweep starts on its first frame.
    h.frames(4);
    let offset = |h: &Harness| h.ui.scene.borrow().mask(band).map(|m| m.offset.0);
    let converted = zgx_script::tables_converted();
    h.ui.scene.borrow_mut().flush();
    let bounds = h.ui.scene.borrow().visible_bounds(band).expect("shows");
    let (mut steps, mut laid_out) = (0, 0);
    let mut last = offset(&h);
    for _ in 0..30 {
        h.frame();
        let report = h.ui.scene.borrow_mut().flush();
        laid_out += report.layout_nodes;
        let now = offset(&h);
        if now != last {
            steps += 1;
            assert!(!report.damage.is_empty(), "a step repaints");
        }
        last = now;
        for rect in &report.damage {
            assert!(
                within(rect, &bounds),
                "{rect:?} is outside the band {bounds:?}"
            );
        }
    }
    // 30 frames at 60 Hz: half a second, 15 steps at 30 a second.
    assert!((14..=16).contains(&steps), "{steps} steps");
    assert_eq!(laid_out, 0, "no layout ran");
    assert_eq!(
        zgx_script::tables_converted(),
        converted,
        "no script data moved"
    );
    // Scrolled out of view, it steps 4 times a second at most.
    h.click("away");
    h.frames(4);
    let mut unseen = 0;
    let mut last = offset(&h);
    for _ in 0..60 {
        h.frame();
        let now = offset(&h);
        if now != last {
            unseen += 1;
        }
        last = now;
    }
    assert!(unseen <= 5, "{unseen} steps in a second unseen");
}
