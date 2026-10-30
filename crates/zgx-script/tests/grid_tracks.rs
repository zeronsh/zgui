//! Track lists as scripts write them: `cols` and `rows` take a number of
//! equal tracks or a list as CSS's `grid-template-columns` writes it.
mod common;

use common::Harness;
use zgx::scene::Rect;
use zgx_script::{ScriptHost, testing::TestApp};

impl Harness {
    fn tracks_rect(&mut self, id: &str) -> Rect {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        self.ui.scene.borrow().bounds(node)
    }
}
fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

#[test]
fn a_track_list_sizes_columns_and_rows() {
    let mut h = Harness::inline(
        r##"return ui.div { grid = true, w = 400, h = 200, cols = "100 1fr", rows = "50 1fr",
            ui.div { id = "a" }, ui.div { id = "b" },
            ui.div { id = "c" }, ui.div { id = "d" },
        }"##,
    );
    h.frames(1);
    let b = h.tracks_rect("b");
    near(b.x, 100.);
    near(b.width, 300.);
    let c = h.tracks_rect("c");
    near(c.y, 50.);
    near(c.height, 150.);
    assert_eq!(h.error(), None);
}

#[test]
fn a_number_is_still_equal_tracks() {
    let mut h = Harness::inline(
        r##"return ui.div { grid = true, w = 300, cols = 3,
            ui.div { id = "a", h = 10 }, ui.div { id = "b", h = 10 },
        }"##,
    );
    h.frames(1);
    near(h.tracks_rect("b").x, 100.);
}

#[test]
fn auto_fill_reflows_with_no_script() {
    let mut h = Harness::inline(
        r##"return ui.div { grid = true, w_full = true,
            cols = "repeat(auto-fill, minmax(100, 1fr))",
            ui.div { id = "a", h = 10 }, ui.div { id = "b", h = 10 },
            ui.div { id = "c", h = 10 }, ui.div { id = "d", h = 10 },
            ui.div { id = "e", h = 10 },
        }"##,
    );
    h.frames(1);
    // 400 wide (the harness's): four tracks of 100.
    near(h.tracks_rect("e").y, 10.);
    let calls = h.calls();
    h.ui.scene.borrow_mut().resize(600., 300.);
    h.frames(1);
    near(h.tracks_rect("e").y, 0.);
    // Six tracks of 100 fit, the sixth left empty: `auto-fill` keeps it.
    near(h.tracks_rect("e").x, 400.);
    assert_eq!(h.calls(), calls, "the reflow runs no script");
}

#[test]
fn a_state_or_function_follows() {
    let mut h = Harness::inline(
        r##"local side = ui.state("side", 100)
        ui.command { id = "wider", run = function() side:set(150) end }
        return ui.div { grid = true, w = 400,
            cols = function() return `{side:get()} 1fr` end,
            ui.div { id = "a", h = 10 }, ui.div { id = "b", h = 10 },
        }"##,
    );
    h.frames(1);
    near(h.tracks_rect("b").x, 100.);
    assert!(h.host.run_command("wider"));
    h.frames(1);
    near(h.tracks_rect("b").x, 150.);
    assert_eq!(h.error(), None);
}

#[test]
fn a_bad_literal_stops_the_script_saying_why() {
    let error = TestApp::try_new(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r#"return ui.div { grid = true, cols = "1fx 2" }"#,
        )
    })
    .err()
    .expect("it doesn't load");
    assert!(
        error.contains("`cols`: `1fx` is not a track size"),
        "{error}"
    );
}

#[test]
fn a_bad_computed_list_is_reported_not_dropped() {
    let h = Harness::inline(
        r##"local list = ui.state("list", "100 1fr")
        ui.command { id = "bad", run = function() list:set("repeat(auto-fill, 1fr)") end }
        return ui.div { grid = true, w = 400, cols = list,
            ui.div { id = "a", h = 10 }, ui.div { id = "b", h = 10 },
        }"##,
    );
    h.frames(1);
    assert_eq!(h.error(), None);
    assert!(h.host.run_command("bad"));
    h.frames(1);
    let error = h.error().unwrap_or_default();
    assert!(error.contains("needs a fixed size"), "{error:?}");
}
