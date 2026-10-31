//! `display = "contents"` as scripts write it: on an element, and on
//! `ui.each`, whose rows are then the grid's own cells.
mod common;

use common::Harness;

fn x_y(h: &mut Harness, id: &str) -> (f32, f32) {
    h.frames(1);
    let node = h.find(id).unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    (b.x, b.y)
}

#[test]
fn each_rows_are_a_grids_cells() {
    let mut h = Harness::inline(
        r##"local items = ui.state("items", { "a", "b", "c", "d" })
        return ui.div { grid = true, cols = 3, w = 300,
            ui.div { id = "first", h = 20 },
            ui.each(items, function(item)
                return ui.div { id = item, h = 20 }
            end, { display = "contents" }),
        }"##,
    );
    assert_eq!(x_y(&mut h, "first"), (0., 0.));
    assert_eq!(x_y(&mut h, "a"), (100., 0.));
    assert_eq!(x_y(&mut h, "b"), (200., 0.));
    assert_eq!(x_y(&mut h, "c"), (0., 20.));
    assert_eq!(x_y(&mut h, "d"), (100., 20.));
    assert_eq!(h.error(), None);
}

/// By default `ui.each` is a box of its own, its rows laid out as its
/// parent lays out (a grid's rows in one cell).
#[test]
fn by_default_each_is_one_box() {
    let mut h = Harness::inline(
        r##"local items = ui.state("items", { "a", "b" })
        return ui.div { grid = true, cols = 3, w = 300,
            ui.div { id = "first", h = 20 },
            ui.each(items, function(item)
                return ui.div { id = item, h = 20 }
            end),
        }"##,
    );
    assert_eq!(x_y(&mut h, "a").0, 100.);
    assert_eq!(x_y(&mut h, "b").0, 100., "in the same cell");
    assert_eq!(h.error(), None);
}

#[test]
fn an_element_and_a_state_turn_it_on_and_off() {
    let mut h = Harness::inline(
        r##"local flat = ui.state("flat", true)
        ui.command { id = "box", run = function() flat:set(false) end }
        return ui.row { gap = 10,
            ui.div { id = "a", w = 30, h = 10 },
            ui.div { display = function() return if flat:get() then "contents" else "block" end,
                ui.div { id = "b", w = 40, h = 10 },
                ui.div { id = "c", w = 50, h = 10 },
            },
        }"##,
    );
    assert_eq!(x_y(&mut h, "c").0, 90., "items of the row");
    assert!(h.host.run_command("box"));
    h.frames(2);
    assert_eq!(
        x_y(&mut h, "c").0,
        40.,
        "in a column box of their own, at its start"
    );
    assert_eq!(h.error(), None);
}
