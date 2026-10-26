//! Layout animations: `ui.each` rows that move when the list changes,
//! entering and leaving rows, `layout = "position"` and `layout_id`. Times
//! are 60 Hz frames; positions are where rows show (layout plus translation).
mod common;

use common::Harness;

const MS_PER_FRAME: f64 = 1000. / 60.;

impl Harness {
    fn run_ms(&mut self, ms: f64) {
        for _ in 0..(ms / MS_PER_FRAME).round() as usize {
            self.frame();
        }
    }
    /// Where `id` shows: its box, translated, scaled.
    fn shown(&mut self, id: &str) -> zgx::scene::Rect {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        self.ui.scene.borrow().transformed_bounds(node)
    }
    fn y(&mut self, id: &str) -> f32 {
        self.shown(id).y
    }
    fn x(&mut self, id: &str) -> f32 {
        self.shown(id).x
    }
    /// A change, then the frame whose layout applies it (the moves start
    /// there) and the frame that renders their start.
    fn change(&mut self, button: &str) {
        self.click(button);
        self.frame();
        self.frame();
    }
}
fn close(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.6, "{what}: {a} != {b}");
}

/// Rows of heights `h` in `container` (with `props`), reversed by `#flip`.
fn list(rows: &str, options: &str, container: &str, props: &str) -> String {
    format!(
        r##"local rows = ui.state("rows", {{ {rows} }})
        return ui.column {{
            ui.{container} {{ {props}
                ui.each(rows, function(r)
                    return ui.div {{ id = r.id, w = r.w or 40, h = r.h, shrink = 0 }}
                end, {{ key = function(r) return r.id end, {options} }}),
            }},
            ui.button {{ id = "flip", w = 4, h = 4, on_click = function()
                local out = {{}}
                for i, r in rows:get() do table.insert(out, 1, r) end
                rows:set(out)
            end }},
            ui.button {{ id = "drop_b", w = 4, h = 4, on_click = function()
                local out = {{}}
                for _, r in rows:get() do if r.id ~= "b" then table.insert(out, r) end end
                rows:set(out)
            end }},
            ui.button {{ id = "add_d", w = 4, h = 4, on_click = function()
                local out = {{ {{ id = "d", h = 20 }} }}
                for _, r in rows:get() do table.insert(out, r) end
                rows:set(out)
            end }},
        }}"##
    )
}
const LINEAR: &str = r#"animate = { duration = 100, ease = "linear" }"#;
const ABC: &str = r#"{ id = "a", h = 20 }, { id = "b", h = 20 }, { id = "c", h = 20 }"#;

#[test]
fn reordered_rows_slide_from_where_they_were() {
    let mut h = Harness::inline(&list(ABC, LINEAR, "column", ""));
    assert_eq!((h.y("a"), h.y("b"), h.y("c")), (0., 20., 40.));
    h.change("flip");
    // The first frame after the change shows every row where it was.
    close(h.y("a"), 0., "a starts where it was");
    close(h.y("c"), 40., "c starts where it was");
    close(h.y("b"), 20., "b did not move");
    // The frames that move the rows lay nothing out and paint nothing:
    // they composite.
    h.ui.scene.borrow_mut().flush();
    for _ in 0..3 {
        h.frame();
        let report = h.ui.scene.borrow_mut().flush();
        assert_eq!(
            (report.layout_nodes, report.paint_nodes),
            (0, 0),
            "a moving frame only composites"
        );
        assert!(report.composite_nodes > 0, "it moves");
    }
    close(h.y("a"), 20., "a half way");
    close(h.y("c"), 20., "c half way");
    h.run_ms(60.);
    assert_eq!((h.y("a"), h.y("b"), h.y("c")), (40., 20., 0.));
    assert!(!h.clock.wants_frame(), "settled rows ask for no frames");
    // Settled, the translation is gone: layout alone places them.
    let node = h.find("a").unwrap();
    assert_eq!(h.ui.scene.borrow().transform(node).x, 0.);
    assert_eq!(h.ui.scene.borrow().transform(node).y, 0.);
}

#[test]
fn rows_of_any_height_slide_to_their_places() {
    let rows = r#"{ id = "a", h = 10 }, { id = "b", h = 30 }, { id = "c", h = 25 }"#;
    let mut h = Harness::inline(&list(rows, LINEAR, "column", ""));
    assert_eq!((h.y("a"), h.y("b"), h.y("c")), (0., 10., 40.));
    h.change("flip");
    close(h.y("a"), 0., "a");
    close(h.y("b"), 10., "b");
    close(h.y("c"), 40., "c");
    h.run_ms(50.);
    // Each half way to its own new place: c 0, b 25, a 55.
    close(h.y("a"), 27.5, "a half way");
    close(h.y("b"), 17.5, "b half way");
    close(h.y("c"), 20., "c half way");
    h.run_ms(60.);
    assert_eq!((h.y("a"), h.y("b"), h.y("c")), (55., 25., 0.));
}

#[test]
fn rows_wrapped_in_a_row_and_a_grid_slide_too() {
    let rows = r#"{ id = "a", h = 10, w = 30 }, { id = "b", h = 10, w = 50 }, { id = "c", h = 10, w = 20 }"#;
    let mut h = Harness::inline(&list(rows, LINEAR, "row", ""));
    assert_eq!((h.x("a"), h.x("b"), h.x("c")), (0., 30., 80.));
    h.change("flip");
    close(h.x("a"), 0., "a starts where it was");
    close(h.x("c"), 80., "c starts where it was");
    h.run_ms(120.);
    assert_eq!((h.x("c"), h.x("b"), h.x("a")), (0., 20., 70.));

    // A grid of the rows themselves: the list's own grid props.
    let rows =
        r#"{ id = "a", h = 10 }, { id = "b", h = 10 }, { id = "c", h = 10 }, { id = "e", h = 10 }"#;
    let grid = format!("{LINEAR}, grid = true, cols = 2, w = 80");
    let mut h = Harness::inline(&list(rows, &grid, "column", ""));
    let at = |h: &mut Harness, id: &str| (h.x(id), h.y(id));
    assert_eq!(at(&mut h, "a"), (0., 0.));
    assert_eq!(at(&mut h, "e"), (40., 10.));
    h.change("flip");
    close(at(&mut h, "a").0, 0., "a starts where it was");
    close(at(&mut h, "e").1, 10., "e starts where it was");
    h.run_ms(50.);
    let (x, y) = at(&mut h, "a");
    close(x, 20., "a half way across");
    close(y, 5., "a half way down");
    h.run_ms(60.);
    assert_eq!(at(&mut h, "a"), (40., 10.));
    assert_eq!(at(&mut h, "e"), (0., 0.));
}

#[test]
fn a_reorder_mid_move_continues_from_where_rows_show() {
    let mut h = Harness::inline(&list(ABC, LINEAR, "column", ""));
    h.change("flip");
    h.run_ms(33.);
    let a = h.y("a");
    assert!(a > 5. && a < 35., "a is moving: {a}");
    // Back again, mid-flight: on no frame does a row jump. The first move
    // goes 40 points in 100 ms, a frame's worth of it is 6.7.
    let mut path = vec![(h.y("a"), h.y("c"))];
    h.click("flip");
    for _ in 0..8 {
        h.frame();
        path.push((h.y("a"), h.y("c")));
    }
    for step in path.windows(2) {
        let (da, dc) = (step[1].0 - step[0].0, step[1].1 - step[0].1);
        assert!(da.abs() < 6.8 && dc.abs() < 6.8, "a jump: {path:?}");
    }
    assert!(path[8].0 < path[1].0, "a turned back: {path:?}");
    h.run_ms(60.);
    assert_eq!((h.y("a"), h.y("b"), h.y("c")), (0., 20., 40.));
    assert!(!h.clock.wants_frame());
}

#[test]
fn rows_enter_and_leave_with_presence_parameters() {
    let options = format!(
        r#"{LINEAR},
        enter = {{ opacity = {{ 0, 1 }}, duration = 100, ease = "linear" }},
        exit = {{ opacity = 0, duration = 100, ease = "linear" }}"#
    );
    let mut h = Harness::inline(&list(ABC, &options, "column", ""));
    close(h.opacity("a"), 1., "the first rows do not enter");
    h.click("drop_b");
    h.frame();
    // b leaves the flow where it was, fading; c closes up from where it was.
    assert!(h.find("b").is_some(), "b stays while it leaves");
    close(h.y("b"), 20., "b holds its place");
    close(h.y("c"), 40., "c starts where it was");
    h.run_ms(50.);
    close(h.opacity("b"), 0.5, "b half faded");
    close(h.y("c"), 30., "c half way up");
    close(h.y("b"), 20., "b stays put");
    h.run_ms(70.);
    assert!(h.find("b").is_none(), "b unmounts after its exit");
    assert_eq!((h.y("a"), h.y("c")), (0., 20.));

    h.click("add_d");
    h.frame();
    // d enters at the top, fading in; the others slide down to make room.
    close(h.y("d"), 0., "d enters in its place");
    assert!(h.opacity("d") < 0.05, "d starts hidden");
    close(h.y("a"), 0., "a starts where it was");
    h.run_ms(50.);
    close(h.opacity("d"), 0.5, "d half in");
    close(h.y("a"), 10., "a half way down");
    h.run_ms(70.);
    close(h.opacity("d"), 1., "d in");
    assert_eq!((h.y("d"), h.y("a"), h.y("c")), (0., 20., 40.));
    assert!(!h.clock.wants_frame());
}

#[test]
fn without_animate_rows_jump_and_nothing_runs() {
    let mut h = Harness::inline(&list(ABC, "", "column", ""));
    h.click("flip");
    h.frame();
    assert_eq!((h.y("a"), h.y("b"), h.y("c")), (40., 20., 0.));
    assert!(!h.clock.wants_frame(), "no animation, no frames");
    let node = h.find("a").unwrap();
    assert_eq!(h.ui.scene.borrow().transform(node).y, 0.);
}

#[test]
fn layout_position_slides_a_node_layout_moves() {
    let mut h = Harness::inline(
        r##"local wide = ui.state("wide", false)
        return ui.column {
            ui.row {
                ui.div { w = function() return if wide:get() then 50 else 10 end, h = 10, shrink = 0 },
                ui.div { id = "p", layout = "position", layout_duration = 100, layout_ease = "linear",
                    w = 10, h = 10, bg = "#ffffff", hover = { bg = "#000000" } },
            },
            ui.button { id = "go", w = 4, h = 4, on_click = function() wide:set(not wide:get()) end },
        }"##,
    );
    assert_eq!(h.x("p"), 10.);
    h.change("go");
    close(h.x("p"), 10., "starts where it was");
    h.run_ms(50.);
    close(h.x("p"), 30., "half way");
    h.run_ms(60.);
    assert_eq!(h.x("p"), 50.);
    assert!(!h.clock.wants_frame());
    // A layout pass that does not move it (its own size, the window)
    // starts nothing.
    h.ui.scene.borrow_mut().resize(420., 320.);
    h.frame();
    assert!(!h.clock.wants_frame());
}

#[test]
fn a_layout_id_hands_its_box_to_the_node_that_takes_it() {
    let mut h = Harness::inline(
        r##"local at = ui.state("at", "a")
        local function slot(name, w)
            return ui.div { w = 100, h = 20, shrink = 0,
                ui.show(function() return at:get() == name end,
                    ui.div { id = "hl_" .. name, layout_id = "selection", layout_duration = 100,
                        layout_ease = "linear", w = w, h = 20, bg = "#ffffff" }),
            }
        end
        return ui.column {
            ui.row { slot("a", 20), slot("b", 40) },
            ui.button { id = "a", w = 4, h = 4, on_click = function() at:set("a") end },
            ui.button { id = "b", w = 4, h = 4, on_click = function() at:set("b") end },
            ui.button { id = "none", w = 4, h = 4, on_click = function() at:set("") end },
        }"##,
    );
    let r = h.shown("hl_a");
    assert_eq!((r.x, r.width), (0., 20.));
    h.change("b");
    assert!(h.find("hl_a").is_none(), "the old one has gone");
    let r = h.shown("hl_b");
    close(r.x, 0., "the new one starts at the old one's place");
    close(r.width, 20., "and at its size");
    h.run_ms(50.);
    let r = h.shown("hl_b");
    close(r.x, 50., "half way");
    close(r.width, 30., "half grown");
    h.run_ms(60.);
    let r = h.shown("hl_b");
    assert_eq!((r.x, r.width), (100., 40.));
    assert!(!h.clock.wants_frame());

    // Back, mid-flight: from where it shows, with no jump (a frame of the
    // move is 16.7 points across, 3.3 in width).
    h.change("a");
    h.run_ms(50.);
    let mut path = vec![h.shown("hl_a")];
    h.click("b");
    for _ in 0..8 {
        h.frame();
        path.push(h.shown("hl_b"));
    }
    for step in path.windows(2) {
        let (dx, dw) = (step[1].x - step[0].x, step[1].width - step[0].width);
        assert!(dx.abs() < 16.8 && dw.abs() < 3.4, "a jump: {path:?}");
    }
    h.run_ms(120.);
    let r = h.shown("hl_b");
    assert_eq!((r.x, r.width), (100., 40.));

    // Mounting after the frame the old one left in starts in place.
    h.change("none");
    assert!(h.find("hl_b").is_none());
    h.change("a");
    let r = h.shown("hl_a");
    assert_eq!((r.x, r.width), (0., 20.));
    assert!(!h.clock.wants_frame());
}

#[test]
fn restyling_a_moving_row_keeps_it_where_it_shows() {
    let mut h = Harness::inline(
        r##"local rows = ui.state("rows", { "a", "b" })
        local tint = ui.state("tint", "#000000")
        return ui.column {
            ui.each(rows, function(r)
                return ui.div { id = r, w = 40, h = 20, bg = function() return tint:get() end }
            end, { key = function(r) return r end, animate = { duration = 100, ease = "linear" } }),
            ui.button { id = "flip", w = 4, h = 4, on_click = function() rows:set({ "b", "a" }) end },
            ui.button { id = "tint", w = 4, h = 4, on_click = function() tint:set("#ffffff") end },
        }"##,
    );
    h.change("flip");
    h.run_ms(50.);
    let a = h.y("a");
    close(a, 10., "half way");
    // The row's own style runs again (its colour changes) mid-move.
    h.click("tint");
    h.ui.prepare_frame();
    close(h.y("a"), a, "a restyle keeps the move's translation");
    h.run_ms(70.);
    assert_eq!((h.y("a"), h.y("b")), (20., 0.));
}

#[test]
fn a_rows_own_translation_composes_with_its_move() {
    let mut h = Harness::inline(
        r##"local rows = ui.state("rows", { "a", "b" })
        return ui.column {
            ui.each(rows, function(r)
                return ui.div { id = r, x = 5, w = 40, h = 20 }
            end, { key = function(r) return r end, animate = { duration = 100, ease = "linear" } }),
            ui.button { id = "flip", w = 4, h = 4, on_click = function() rows:set({ "b", "a" }) end },
            ui.button { id = "nudge", w = 4, h = 4, on_click = function()
                anime.animate("#a", { x = 12, duration = 50, ease = "linear" })
            end },
        }"##,
    );
    let translate = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let t = h.ui.scene.borrow().transform(node);
        (t.x, t.y)
    };
    assert_eq!(translate(&mut h, "a"), (5., 0.));
    h.change("flip");
    assert_eq!(
        translate(&mut h, "a"),
        (5., -20.),
        "held where it was, x kept"
    );
    h.run_ms(50.);
    close(h.y("a"), 10., "half way down");
    close(translate(&mut h, "a").0, 5., "its own x stays");
    // The script moves it across while it moves down: both show.
    h.click("nudge");
    h.run_ms(70.);
    assert_eq!(translate(&mut h, "a"), (12., 0.));
    assert_eq!(h.y("a"), 20.);
}

/// The tab strip in docs/scripting.md: a highlight in the active tab.
#[test]
fn a_tab_strips_highlight_slides_to_the_tab_that_takes_it() {
    let mut h = Harness::inline(
        r##"local tabs = ui.state("tabs", { { id = "a", name = "First" }, { id = "b", name = "Second tab" } })
        local active = ui.state("active", "a")
        return ui.column {
            ui.row {
                ui.each(tabs, function(tab)
                    return ui.row { id = "tab_" .. tab.id, px = 10, h = 28,
                        ui.presence(function() return if active:get() == tab.id then "on" else nil end, function()
                            return ui.div { id = "hl_" .. tab.id, layout_id = "selection", h_full = true,
                                rounded = 8, bg = "#ffffff14" }
                        end, { absolute = true, top = 0, left = 0, right = 0, bottom = 0 }),
                        ui.text { tab.name },
                    }
                end, {
                    key = function(tab) return tab.id end,
                    animate = { duration = 240, ease = "outQuint" },
                    exit = { opacity = 0, duration = 150 },
                }),
            },
            ui.button { id = "b", w = 4, h = 4, on_click = function() active:set("b") end },
        }"##,
    );
    let tab = h.shown("tab_a");
    let hl = h.shown("hl_a");
    assert_eq!(
        (hl.x, hl.y, hl.width, hl.height),
        (tab.x, tab.y, tab.width, tab.height)
    );
    h.change("b");
    let start = h.shown("hl_b");
    close(start.x, tab.x, "starts on the tab it left");
    close(start.width, tab.width, "at its width");
    // The highlight moves with the default spring.
    h.run_ms(700.);
    assert!(!h.clock.wants_frame());
    let (tab, hl) = (h.shown("tab_b"), h.shown("hl_b"));
    assert_eq!(
        (hl.x, hl.y, hl.width, hl.height),
        (tab.x, tab.y, tab.width, tab.height)
    );
}

/// The notes example's selection: a highlight in the selected row, handed
/// from row to row by its `layout_id`.
#[test]
fn the_notes_selection_slides_from_row_to_row() {
    const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/notes/app.luau");
    let mut h = Harness::new(|runtime| zgx_script::ScriptHost::new(runtime, APP));
    h.run_ms(1500.);
    assert_eq!(h.host.error().get(), None);
    let pills = |h: &mut Harness| {
        h.ui.prepare_frame();
        let scene = h.ui.scene.borrow();
        let mut found = Vec::new();
        let mut stack = vec![h.handle.node()];
        while let Some(node) = stack.pop() {
            if let zgx::scene::NodeKind::Panel { quad, .. } | zgx::scene::NodeKind::Quad(quad) =
                scene.kind(node)
                && quad.fill == zgx::scene::Color(0xf5, 0xb7, 0x0a, 0x3d)
            {
                found.push(scene.transformed_bounds(node));
            }
            stack.extend(scene.children(node).iter().copied());
        }
        found
    };
    let first = pills(&mut h);
    assert_eq!(first.len(), 1, "one highlight: {first:?}");
    let (x, y) = (first[0].x + 40., first[0].y + first[0].height + 2. + 20.);
    h.ui.dispatch(zgx::input::InputEvent::PointerDown {
        x,
        y,
        button: zgx::input::PointerButton::Primary,
    });
    h.ui.dispatch(zgx::input::InputEvent::PointerUp {
        x,
        y,
        button: zgx::input::PointerButton::Primary,
    });
    h.settle();
    h.frame();
    let start = pills(&mut h);
    assert_eq!(start.len(), 1, "handed over, not doubled");
    assert!(
        (start[0].y - first[0].y).abs() < 1.,
        "starts on the row it left: {start:?} {first:?}"
    );
    h.run_ms(1500.);
    let end = pills(&mut h);
    assert_eq!(end.len(), 1);
    assert!(
        (end[0].y - (first[0].y + first[0].height + 2.)).abs() < 0.5,
        "lands on the next row: {end:?} {first:?}"
    );
    assert_eq!(h.host.error().get(), None);
    assert!(!h.clock.wants_frame(), "settles");
}

/// Rows moved by a change slide in the rows' order, and additive values
/// write in the order their slots were made, not in a hash map's: the same
/// change damages the same places in the same order every run (two runs
/// here, whose maps are seeded differently).
#[test]
fn a_reorder_damages_in_the_same_order_every_run() {
    let run = || {
        let h = Harness::inline(
            r##"local rows = ui.shared("rows", {})
            return ui.column { w = 400, h = 900, ui.each(rows, function(i)
                return ui.div { w = 200, h = 10 + i % 3 * 5, bg = "#58a6ff" }
            end, { key = function(i) return i end, animate = true }) }"##,
        );
        let order = |reversed: bool| {
            let mut rows: Vec<zgx_script::Value> =
                (0..30).map(|i| zgx_script::Value::Num(i as f64)).collect();
            if reversed {
                rows.reverse();
            }
            zgx_script::Value::List(rows.into())
        };
        h.host.shared("rows", order(false)).set(order(false));
        h.frames(2);
        h.ui.scene.borrow_mut().flush();
        h.host.shared("rows", order(false)).set(order(true));
        let mut frames = Vec::new();
        for _ in 0..30 {
            h.frame();
            let mut scene = h.ui.scene.borrow_mut();
            let damage = scene.flush().damage;
            let repaints = scene.repaints().1.to_vec();
            frames.push((format!("{damage:?}"), repaints));
        }
        frames
    };
    assert_eq!(run(), run());
}
