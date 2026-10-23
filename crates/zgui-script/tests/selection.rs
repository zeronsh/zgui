//! A script styles display text selections as CSS's `::selection` does.
mod common;
use common::Harness;
use zgui::scene::{Color, NodeKind};

#[test]
fn selection_props_and_theme_tokens_style_a_nodes_highlight() {
    let mut h = Harness::inline(
        r##"local tint = ui.state("tint", "#ff0000")
        return ui.column {
            ui.column {
                selectable = true,
                selection_color = "#0000ff80",
                selected_text_color = function() return tint:get() end,
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
    let themed = text_in(&mut h, "themed");
    assert!(h.ui.select_all_text(themed));
    let highlight = h.ui.scene.borrow().text_highlight(themed).unwrap().clone();
    assert_eq!(highlight.text_color, Some(Color(0, 255, 0, 255)));
    assert_eq!(highlight.radius, 3.);
}
