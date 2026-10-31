//! `z` as scripts write it: over later siblings, following a state.
mod common;

use common::Harness;

#[test]
fn z_raises_a_node_over_its_later_sibling_and_follows_a_state() {
    let mut h = Harness::inline(
        r##"local raised = ui.state("raised", 1)
        local clicks = ui.state("clicks", "")
        ui.command { id = "lower", run = function() raised:set(0) end }
        return ui.overlay { w = 100, h = 100,
            ui.div { id = "a", w = 100, h = 100, z = raised,
                on_click = function() clicks:set("a") end },
            ui.div { id = "b", w = 100, h = 100,
                on_click = function() clicks:set("b") end },
            ui.text { id = "said", w = 1, h = 1, "{clicks}" },
        }"##,
    );
    h.frames(1);
    h.click("a");
    assert_eq!(h.text("said"), "a");
    assert!(h.host.run_command("lower"));
    h.frames(1);
    h.click("a");
    assert_eq!(h.text("said"), "b", "lowered, the later sibling takes it");
    assert_eq!(h.error(), None);
}
