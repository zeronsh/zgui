//! The layout model as scripts see it: borders inside a box's size,
//! overlays that stack and align their children, scroll views and inputs
//! that fill and fit their parents, and `relative` containing blocks.
mod common;

use common::Harness;
use zgui::scene::Rect;

impl Harness {
    fn rect(&mut self, id: &str) -> Rect {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        self.ui.scene.borrow().bounds(node)
    }
}

#[test]
fn borders_take_room_inside_a_box() {
    let mut h = Harness::inline(
        r##"return ui.column { items = "start",
            ui.column { id = "card", w = 100, h = 50, p = 4, border = 1, border_color = "#fff",
                ui.div { id = "fill", w_full = true, h_full = true },
            },
            ui.row { id = "pill", px = 6, border = 2, border_color = "#fff",
                ui.div { id = "dot", w = 10, h = 10 },
            },
            ui.column { id = "dock", border_sides = { 1, 1, 0, 1 }, border_color = "#fff",
                ui.div { id = "tile", w = 10, h = 10 },
            },
            ui.column { id = "ring", border = 2, border_over = true, border_color = "#fff",
                ui.div { id = "photo", w = 10, h = 10 },
            },
        }"##,
    );
    assert_eq!(h.rect("card"), Rect::new(0., 0., 100., 50.));
    assert_eq!(h.rect("fill"), Rect::new(5., 5., 90., 40.));
    assert_eq!(h.rect("pill"), Rect::new(0., 50., 26., 14.));
    assert_eq!(h.rect("dot"), Rect::new(8., 52., 10., 10.));
    assert_eq!(h.rect("dock"), Rect::new(0., 64., 12., 11.));
    assert_eq!(h.rect("tile"), Rect::new(1., 65., 10., 10.));
    // An inset stroke paints over its content and takes no room.
    assert_eq!(h.rect("photo"), Rect::new(0., 75., 10., 10.));
}

#[test]
fn overlays_stretch_and_align_their_children() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.overlay { id = "tile", w = 20, h = 20, items = "center", justify = "center",
                ui.svg { id = "icon", code = "<svg xmlns='http://www.w3.org/2000/svg'/>", w = 12, h = 12 },
            },
            ui.overlay { id = "card", w = 100, h = 60,
                ui.div { id = "fill", bg = "#f00" },
                ui.div { id = "corner", w = 10, h = 10, self = "end" },
            },
            ui.overlay { id = "bar", w = 100, h = 30, items = "end", justify = "stretch",
                ui.div { id = "side", w = 10 },
            },
            ui.row { w = 300, h = 80, items = "stretch",
                ui.div { w = 100 },
                ui.overlay { id = "preview", grow = 1,
                    ui.column { id = "page", grow = 1, bg = "#123" },
                },
            },
        }"##,
    );
    // Centred both ways in the tile.
    assert_eq!(h.rect("icon"), Rect::new(4., 4., 12., 12.));
    // Unsized: fills; sized: its own size, `self` across.
    assert_eq!(h.rect("fill"), Rect::new(0., 20., 100., 60.));
    assert_eq!(h.rect("corner"), Rect::new(90., 20., 10., 10.));
    assert_eq!(h.rect("side"), Rect::new(90., 80., 10., 30.));
    // The preview pane shows its content.
    assert_eq!(h.rect("preview"), Rect::new(100., 110., 200., 80.));
    assert_eq!(h.rect("page"), Rect::new(100., 110., 200., 80.));
}

#[test]
fn scroll_views_fill_a_stretching_parent() {
    let mut h = Harness::inline(
        r##"local words = string.rep("word ", 80)
        return ui.row {
            ui.column { id = "sidebar", w = 190, h = 280, shrink = 0, items = "stretch", border_sides = { 0, 1, 0, 0 },
                ui.scroll { id = "list", grow = 1,
                    ui.column { id = "content", px = 8,
                        ui.text { words, id = "words", wrap = true },
                    },
                },
            },
            ui.column { shrink = 0,
                ui.scroll { id = "plain", ui.text { "hi" } },
            },
        }"##,
    );
    // Across the sidebar's content box (inside its right border), down it.
    assert_eq!(h.rect("list"), Rect::new(0., 0., 189., 280.));
    assert_eq!(h.rect("content").width, 189.);
    assert!(h.rect("words").width <= 173.);
    // Unconstrained, its default size.
    assert_eq!(h.rect("plain").width, 320.);
    assert_eq!(h.rect("plain").height, 240.);
}

#[test]
fn inputs_fit_their_rows_whatever_their_text() {
    let mut h = Harness::inline(
        r##"local url = ui.state("url", string.rep("x", 300))
        return ui.column {
            ui.row { id = "bar", w = 200, h = 30, items = "center",
                ui.div { w = 50, h = 20, shrink = 0 },
                ui.input { id = "address", label = "Address", value = url, grow = 1, min_w = 0 },
            },
            ui.row { w = 200, h = 30,
                ui.div { w = 50, h = 20, shrink = 0 },
                ui.input { id = "narrow", label = "Narrow", value = url },
            },
            ui.column { w = 300, items = "stretch",
                ui.input { id = "wide", label = "Wide", value = url },
            },
        }"##,
    );
    assert_eq!(h.rect("address").width, 150.);
    assert_eq!(h.rect("narrow").width, 150.);
    assert_eq!(h.rect("wide").width, 300.);
}

#[test]
fn relative_marks_the_block_absolute_views_position_against() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", true)
        return ui.column {
            ui.div { h = 20 },
            ui.column { id = "panel", w = 200, h = 100, relative = true, border = 1, border_color = "#fff",
                ui.column { p = 10,
                    ui.div { id = "badge", absolute = true, right = 0, top = 0, w = 10, h = 10 },
                    ui.show(function() return open:get() end,
                        ui.div { id = "toast", absolute = true, left = 8, right = 8, bottom = 8, h = 20 }),
                },
            },
            ui.column { id = "plain", w = 200, h = 100,
                ui.column { id = "parent", w = 50, h = 40,
                    ui.div { id = "corner", absolute = true, right = 0, bottom = 0, w = 10, h = 10 },
                },
            },
        }"##,
    );
    // Against the panel's padding box (inside its 1 px border).
    assert_eq!(h.rect("badge"), Rect::new(189., 21., 10., 10.));
    assert_eq!(h.rect("toast"), Rect::new(9., 91., 182., 20.));
    // No relative ancestor: the parent, as before.
    assert_eq!(h.rect("corner"), Rect::new(40., 150., 10., 10.));
}

#[test]
fn snapped_boxes_keep_whole_point_edges_and_still_meet() {
    // Two panes split at an uneven ratio by percentages: their shared edge
    // falls between points, which would blur a 1-point border.
    let mut h = Harness::inline(
        r##"local function pane(id, left, right, snap)
            return ui.div { id = id, absolute = true, snap = snap, top = 0, bottom = 0,
                left = left, right = right, ml = 0.3 }
        end
        return ui.div { w = 401, h = 300, relative = true,
            pane("a", "0%", "45.9521%", true),
            pane("b", "54.0479%", "0%", true),
            pane("loose", "54.0479%", "0%", false) }"##,
    );
    h.ui.prepare_frame();
    let bounds = |h: &mut Harness, id: &str| {
        let n = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(n)
    };
    let (a, b, loose) = (
        bounds(&mut h, "a"),
        bounds(&mut h, "b"),
        bounds(&mut h, "loose"),
    );
    for r in [a, b] {
        for edge in [r.x, r.y, r.x + r.width, r.y + r.height] {
            assert_eq!(edge, edge.round(), "{r:?}");
        }
    }
    assert_eq!(a.x + a.width, b.x, "the shared edge rounds the same way");
    assert!(loose.x.fract() != 0., "without snap: {loose:?}");
}
