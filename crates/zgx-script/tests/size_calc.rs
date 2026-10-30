//! Size math as scripts write it: `w = "calc(100% - 24px)"`.
mod common;

use common::Harness;
use zgx_script::{ScriptHost, testing::TestApp};

fn width(h: &mut Harness, id: &str) -> f32 {
    h.frames(2);
    let node = h.find(id).unwrap();
    h.ui.scene.borrow().bounds(node).width
}

#[test]
fn w_takes_size_math_and_follows_a_state() {
    let mut h = Harness::inline(
        r##"local gutter = ui.state("gutter", 24)
        ui.command { id = "wider", run = function() gutter:set(48) end }
        return ui.column { w = 300, h = 200,
            ui.div { id = "a", h = 10, w = "calc(100% - 24px)" },
            ui.div { id = "b", h = 10, w = function()
                return "calc(100% - " .. gutter:get() .. "px)"
            end },
            ui.div { id = "c", h = 10, w = 1000, max_w = "min(50%, 400px)" },
            ui.div { id = "d", h = 10, w = 10, min_w = "clamp(10px, 20%, 40px)" },
        }"##,
    );
    assert_eq!(width(&mut h, "a"), 276.);
    assert_eq!(width(&mut h, "b"), 276.);
    assert_eq!(width(&mut h, "c"), 150.);
    assert_eq!(width(&mut h, "d"), 40.);
    assert!(h.host.run_command("wider"));
    assert_eq!(width(&mut h, "b"), 252.);
    assert_eq!(h.error(), None);
}

#[test]
fn what_is_not_a_length_says_why() {
    for (code, says) in [
        (
            r#"return ui.div { w = "calc(10px * 20px)" }"#,
            "one must be a plain number",
        ),
        (r#"return ui.div { p = "calc(100% - 4px)" }"#, "size w, h"),
    ] {
        let error = TestApp::try_new(|runtime| ScriptHost::inline(runtime, "app", code))
            .err()
            .expect("an error");
        assert!(error.contains(says), "{error}");
    }
}
