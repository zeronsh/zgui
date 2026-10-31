//! Grid areas, named lines and dense flow as scripts write them.
mod common;

use common::Harness;
use zgx::scene::Rect;
use zgx_script::{ScriptHost, testing::TestApp};

fn rect(h: &mut Harness, id: &str) -> Rect {
    h.frames(1);
    let node = h.find(id).unwrap();
    h.ui.scene.borrow().bounds(node)
}

#[test]
fn areas_and_lines_place_items() {
    let mut h = Harness::inline(
        r##"return ui.div { grid = true, w = 300, h = 200,
            cols = "[left] 100 [right] 1fr", rows = "20 1fr",
            areas = { "head head", "side main" },
            ui.div { id = "main", area = "main" },
            ui.div { id = "head", area = "head" },
            ui.div { id = "side", grid_column = "left / right", grid_row = 2 },
        }"##,
    );
    assert_eq!(rect(&mut h, "head"), Rect::new(0., 0., 300., 20.));
    assert_eq!(rect(&mut h, "main"), Rect::new(100., 20., 200., 180.));
    assert_eq!(rect(&mut h, "side"), Rect::new(0., 20., 100., 180.));
    assert_eq!(h.error(), None);
}

#[test]
fn dense_flow_fills_a_hole() {
    let mut h = Harness::inline(
        r##"return ui.div { grid = true, w = 300, cols = 3, flow = "dense",
            ui.div { h = 10, col_span = 2 },
            ui.div { h = 10, col_span = 2 },
            ui.div { id = "c", h = 10 },
        }"##,
    );
    let c = rect(&mut h, "c");
    assert_eq!((c.x, c.y), (200., 0.));
}

#[test]
fn what_css_rejects_stops_the_script_saying_why() {
    for (code, says) in [
        (
            r#"return ui.div { grid = true, areas = { "a b", "b a" } }"#,
            "isn't a rectangle",
        ),
        (
            r#"return ui.div { grid_column = "span 1 / span 2" }"#,
            "both ends can't span",
        ),
        (r#"return ui.div { flow = "sideways" }"#, "a grid's flow"),
    ] {
        let error = TestApp::try_new(|runtime| ScriptHost::inline(runtime, "app", code))
            .err()
            .expect("an error");
        assert!(error.contains(says), "{error}");
    }
}
