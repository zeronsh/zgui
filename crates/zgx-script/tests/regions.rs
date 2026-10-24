//! Control-flow regions (`show`, `switch`, `presence`) and `layout` boxes
//! are invisible to layout: what they hold sizes and places itself as if it
//! stood in their place, at once.
mod common;

use common::Harness;
use zgx::scene::Rect;

impl Harness {
    fn rect(&mut self, id: &str) -> Rect {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        self.ui.scene.borrow().bounds(node)
    }
    fn run_frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }
}

/// The editor body under `show` fills the window's height (its `grow`
/// reaches the column it stands in), and what replaces it sizes itself.
#[test]
fn an_editor_body_under_show_fills_the_window() {
    let mut h = Harness::inline(
        r##"local editing = ui.state("editing", true)
        return ui.column { w_full = true, h_full = true, items = "stretch",
            ui.row { id = "toolbar", h = 40 },
            ui.show(editing,
                ui.column { id = "body", grow = 1, items = "stretch",
                    ui.textarea { id = "area", label = "Body", value = ui.state("text", ""), h_full = true } },
                ui.text { "No note", id = "none" }),
            ui.row { id = "status", h = 20 },
            ui.button { id = "toggle", w = 10, h = 10, absolute = true, on_click = function() editing:set(not editing:get()) end },
        }"##,
    );
    assert_eq!(h.rect("body"), Rect::new(0., 40., 400., 240.));
    assert_eq!(
        h.rect("area").height,
        240.,
        "its percentage height resolves"
    );
    assert_eq!(h.rect("status").y, 280.);
    h.click("toggle");
    let none = h.rect("none");
    assert!(none.y == 40. && none.height < 40., "{none:?}");
    assert_eq!(h.rect("status").y, 40. + none.height);
    h.click("toggle");
    assert_eq!(h.rect("body"), Rect::new(0., 40., 400., 240.));
}

/// What `show` and `switch` hold takes percentages, `self` and `absolute`
/// insets against the real parent, and neither grows in a row.
#[test]
fn show_and_switch_lay_out_as_their_content() {
    let mut h = Harness::inline(
        r##"local tab = ui.state("tab", "a")
        return ui.overlay { w = 400, h = 300,
            ui.row { w_full = true, h = 100, items = "start",
                ui.show(true, ui.div { id = "half", w = "50%", h = 10 }),
                ui.switch(tab, {
                    a = ui.div { id = "a", w = 30, h = 30, self = "end" },
                    b = ui.div { id = "b", grow = 1, h = 20 },
                }),
                ui.div { id = "after", w = 10, h = 10 },
            },
            ui.show(true, ui.column { id = "sheet", absolute = true, top = 10, left = 20, right = 0, bottom = 0 }),
            ui.button { id = "go", w = 1, h = 1, absolute = true, on_click = ui.set(tab, "b") },
        }"##,
    );
    assert_eq!(h.rect("half"), Rect::new(0., 0., 200., 10.));
    assert_eq!(
        h.rect("a"),
        Rect::new(200., 70., 30., 30.),
        "`self` reaches the row"
    );
    assert_eq!(h.rect("after").x, 230., "nothing grows that did not ask to");
    assert_eq!(
        h.rect("sheet"),
        Rect::new(20., 10., 380., 290.),
        "insets against the overlay"
    );
    h.click("go");
    assert_eq!(
        h.rect("b"),
        Rect::new(200., 0., 190., 20.),
        "the new case's `grow`"
    );
    assert_eq!(h.rect("after").x, 390.);
}

/// `presence` with sizing of its own is a box its content fills; without,
/// its content lays out in its place. What leaves stays where it was, as
/// large as it was, out of the flow.
#[test]
fn presence_applies_its_sizing_and_its_content_fills_it() {
    let mut h = Harness::inline(
        r##"local tab = ui.state("tab", "a")
        return ui.row { w_full = true, h_full = true, items = "start",
            ui.presence(tab, function(v)
                return ui.column { id = "pane-" .. v, min_w = "10%", ui.text { v } }
            end, { w = 120, h_full = true, exit = { opacity = 0, duration = 100 } }),
            ui.div { id = "rest", grow = 1, h = 10 },
            ui.presence(tab, function(v) return ui.div { id = "tag-" .. v, w = 30, h = 8 } end),
            ui.button { id = "go", w = 1, h = 1, absolute = true, on_click = ui.set(tab, "b") },
        }"##,
    );
    assert_eq!(h.rect("pane-a"), Rect::new(0., 0., 120., 300.));
    assert_eq!(h.rect("rest"), Rect::new(120., 0., 250., 10.));
    assert_eq!(
        h.rect("tag-a"),
        Rect::new(370., 0., 30., 8.),
        "as if in its place"
    );
    h.click("go");
    h.frame();
    assert_eq!(
        h.rect("pane-b"),
        Rect::new(0., 0., 120., 300.),
        "entering fills it"
    );
    assert_eq!(
        h.rect("pane-a"),
        Rect::new(0., 0., 120., 300.),
        "leaving keeps its place"
    );
    assert_eq!(h.rect("rest"), Rect::new(120., 0., 250., 10.));
    h.run_frames(10);
    assert!(h.find("pane-a").is_none(), "gone after its exit");
    assert_eq!(h.rect("pane-b"), Rect::new(0., 0., 120., 300.));
}

/// `mode = "wait"`: while the old content leaves, nothing takes its place
/// and it stays where it was.
#[test]
fn waiting_presence_keeps_what_leaves_in_place() {
    let mut h = Harness::inline(
        r##"local tab = ui.state("tab", "a")
        return ui.column { items = "start",
            ui.div { h = 50, w = 10 },
            ui.presence(tab, function(v) return ui.div { id = v, w = 40, h = 30 } end,
                { mode = "wait", exit = { opacity = 0, duration = 100 } }),
            ui.div { id = "below", w = 10, h = 10 },
            ui.button { id = "go", w = 1, h = 1, absolute = true, on_click = ui.set(tab, "b") },
        }"##,
    );
    assert_eq!(h.rect("a"), Rect::new(0., 50., 40., 30.));
    assert_eq!(h.rect("below").y, 80.);
    h.click("go");
    h.frame();
    assert_eq!(
        h.rect("a"),
        Rect::new(0., 50., 40., 30.),
        "leaving, in place"
    );
    assert_eq!(h.rect("below").y, 50., "out of the flow");
    h.run_frames(12);
    assert!(h.find("a").is_none());
    assert_eq!(h.rect("b"), Rect::new(0., 50., 40., 30.));
    assert_eq!(h.rect("below").y, 80.);
}

/// A `layout = "width"` pane (the notes app's preview) is at its full
/// size at once, the scroll view inside it too; only the space it takes
/// eases open.
#[test]
fn a_width_layout_box_lays_its_content_out_at_the_target_size() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", false)
        local rows = {}
        for i = 1, 40 do rows[i] = ui.text { "row " .. i } end
        return ui.row { w_full = true, h_full = true, items = "stretch",
            ui.column { id = "main", grow = 1 },
            ui.column { id = "pane", layout = "width", layout_ease = "linear", layout_duration = 100,
                w = function() return open:get() and 160 or 0 end,
                h_full = true, shrink = 0, overflow = "hidden", items = "stretch",
                ui.scroll { id = "scroll", grow = 1, w_full = true,
                    ui.column { id = "list", w_full = true, rows } } },
            ui.button { id = "go", w = 1, h = 1, absolute = true, on_click = function() open:set(not open:get()) end },
        }"##,
    );
    h.run_frames(30);
    assert_eq!(h.rect("main").width, 400.);
    h.click("go");
    h.ui.prepare_frame();
    h.settle();
    let main = h.rect("main");
    assert_eq!(main.width, 400., "no jump on the frame it opens");
    let scroll = h.rect("scroll");
    assert_eq!(
        (scroll.width, scroll.height),
        (160., 300.),
        "the scroll view at once"
    );
    assert_eq!(h.rect("list").width, 160.);
    h.frame();
    h.run_frames(3);
    let mid = h.rect("main").width;
    assert!(mid < 390. && mid > 250., "easing open: {mid}");
    let scroll = h.rect("scroll");
    assert_eq!(
        (scroll.width, scroll.height),
        (160., 300.),
        "still full size"
    );
    h.run_frames(8);
    assert_eq!(h.rect("main").width, 240.);
    assert_eq!(h.rect("pane"), Rect::new(240., 0., 160., 300.));
    assert!(!h.clock.wants_frame());
}

/// `layout = "height"` on a node that grows: its `grow` and percentages
/// reach its parent as if the box were not there.
#[test]
fn a_layout_box_keeps_its_nodes_flex() {
    let mut h = Harness::inline(
        r##"return ui.column { w_full = true, h_full = true, items = "stretch",
            ui.div { h = 20 },
            ui.column { id = "grows", layout = "height", grow = 1, mx = 10, items = "stretch",
                ui.div { id = "fill", h = "50%" } },
            ui.div { id = "last", h = 20 },
        }"##,
    );
    assert_eq!(h.rect("grows"), Rect::new(10., 20., 380., 260.));
    assert_eq!(h.rect("fill").height, 130.);
    assert_eq!(h.rect("last").y, 280.);
}

/// A layout box clipping (`layout_clip`) cuts its content to the space it
/// takes while it eases.
#[test]
fn a_clipping_layout_box_clips_to_the_space() {
    let mut h = Harness::inline(
        r##"local tall = ui.state("tall", false)
        return ui.column {
            ui.column { id = "box", layout = "height", layout_clip = true, layout_ease = "linear", layout_duration = 100,
                ui.div { w = 50, h = function() return tall:get() and 100 or 20 end } },
            ui.button { id = "go", w = 1, h = 1, absolute = true, on_click = ui.set(tall, true) },
        }"##,
    );
    h.click("go");
    h.frame();
    h.run_frames(3);
    let node = h.find("box").unwrap();
    let scene = h.ui.scene.borrow();
    let wrapper = scene.parent(node).unwrap();
    let inset = scene.clip_inset(wrapper).expect("a clip while easing");
    let bottom = match inset.bottom {
        zgx::layout::Length::Px(v) => v,
        other => panic!("{other:?}"),
    };
    assert!(
        bottom > 5. && bottom < 75.,
        "the unshown part is cut: {bottom}"
    );
}

#[test]
fn a_presence_box_showing_nothing_lets_the_pointer_through() {
    use zgx::input::{InputEvent, PointerButton};
    let mut h = Harness::inline(
        r#"
local open = ui.state("open", false)
local dropped = ui.state("dropped", "")
return ui.row {
    w_full = true,
    h_full = true,
    items = "stretch",
    -- Window keys: the root listens, under the box.
    on_key = function() return false end,
    ui.column {
        grow = 1,
        items = "stretch",
        ui.div { id = "source", h = 40, on_drag = function() return "card" end },
        ui.div {
            id = "target",
            grow = 1,
            drop_when = function() return true end,
            on_drop = function(payload) dropped:set(payload) end,
        },
        ui.text { "{dropped}", id = "dropped" },
    },
    -- A sheet over everything, put away.
    ui.presence(function() return if open:get() then "sheet" else nil end, function()
        return ui.div { w_full = true, h_full = true }
    end, { absolute = true, top = 0, left = 0, right = 0, bottom = 0 }),
}
"#,
    );
    let (from, to) = (h.bounds("source"), h.bounds("target"));
    let (x, y) = (from.x + from.width / 2., from.y + from.height / 2.);
    let (tx, ty) = (to.x + to.width / 2., to.y + to.height / 2.);
    h.ui.dispatch(InputEvent::PointerMove { x, y });
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    for step in 1..=8 {
        let t = step as f32 / 8.;
        h.ui.dispatch(InputEvent::PointerMove {
            x: x + (tx - x) * t,
            y: y + (ty - y) * t,
        });
    }
    h.ui.dispatch(InputEvent::PointerUp {
        x: tx,
        y: ty,
        button: PointerButton::Primary,
    });
    h.settle();
    assert_eq!(h.text("dropped"), "card", "the empty box is not in the way");
}
