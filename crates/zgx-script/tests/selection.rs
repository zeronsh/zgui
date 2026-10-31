//! A script styles display text selections as CSS's `::selection` does.
mod common;
use common::Harness;
use zgx::scene::{Color, NodeKind};

#[test]
fn selection_props_and_theme_tokens_style_a_nodes_highlight() {
    let mut h = Harness::inline(
        r##"local tint = ui.state("tint", "#ff0000")
        return ui.column {
            ui.column {
                selectable = true,
                selection_color = "#0000ff80",
                selected_text_color = function() return tint:get() end,
                selection_radius = 5,
                ui.text { "Props", id = "props" },
            },
            ui.column {
                selectable = true,
                theme = { ["selection.text"] = "#00ff00", ["selection.radius"] = 3 },
                ui.text { "Themed", id = "themed" },
            },
        }"##,
    );
    h.settle();
    h.ui.prepare_frame();
    let text_in = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        if matches!(scene.kind(node), NodeKind::Text { .. }) {
            node
        } else {
            scene.children(node)[0]
        }
    };
    let props = text_in(&mut h, "props");
    assert!(h.ui.select_all_text(props));
    let highlight = h.ui.scene.borrow().text_highlight(props).unwrap().clone();
    assert_eq!(highlight.color, Color(0, 0, 255, 128));
    assert_eq!(highlight.text_color, Some(Color(255, 0, 0, 255)));
    assert_eq!(highlight.radius, 5.);
    let themed = text_in(&mut h, "themed");
    assert!(h.ui.select_all_text(themed));
    let highlight = h.ui.scene.borrow().text_highlight(themed).unwrap().clone();
    assert_eq!(highlight.text_color, Some(Color(0, 255, 0, 255)));
    assert_eq!(highlight.radius, 3.);
}

/// `selection_radius` is the subtree's `$selection.radius`: it follows a
/// state, and keeps a `theme` given beside it in either order.
#[test]
fn a_selection_radius_follows_state_beside_a_theme() {
    let mut h = Harness::inline(
        r##"local round = ui.state("round", 2)
        return ui.column {
            ui.column {
                selectable = true,
                selection_radius = round,
                theme = { ["selection.text"] = "#00ff00" },
                ui.text { "First", id = "first" },
            },
            ui.column {
                theme = { ["selection.text"] = "#00ff00" },
                selectable = true,
                selection_radius = 7,
                ui.text { "Second", id = "second" },
            },
        }"##,
    );
    h.settle();
    h.ui.prepare_frame();
    let look = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let text = {
            let scene = h.ui.scene.borrow();
            if matches!(scene.kind(node), NodeKind::Text { .. }) {
                node
            } else {
                scene.children(node)[0]
            }
        };
        assert!(h.ui.select_all_text(text));
        h.ui.prepare_frame();
        let highlight = h.ui.scene.borrow().text_highlight(text).unwrap().clone();
        (highlight.radius, highlight.text_color)
    };
    let green = Some(Color(0, 255, 0, 255));
    assert_eq!(look(&mut h, "first"), (2., green));
    assert_eq!(look(&mut h, "second"), (7., green));
    h.host
        .set_state("round", zgx_script::Value::Num(5.))
        .unwrap();
    h.settle();
    assert_eq!(look(&mut h, "first"), (5., green), "follows the state");
    assert_eq!(h.error(), None);
}
