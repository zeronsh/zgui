//! `sticky_top` as scripts write it: on an element it holds in its scroll
//! view; on a `ui.list` it stays the inset of the list's own sticky rows.
mod common;

use common::Harness;

/// Where `id` shows, after a frame's flush has placed what sticks.
fn y(h: &mut Harness, id: &str) -> f32 {
    h.frames(2);
    h.ui.scene.borrow_mut().flush();
    let node = h.find(id).unwrap();
    h.ui.scene.borrow().bounds(node).y
}

#[test]
fn sticky_top_holds_an_element_and_follows_a_state() {
    let mut h = Harness::inline(
        r##"local offset = ui.state("offset", 0)
        local inset = ui.state("inset", 4)
        ui.command { id = "down", run = function() offset:set(50) end }
        ui.command { id = "more", run = function() inset:set(10) end }
        return ui.scroll { w = 100, h = 100, offset = offset,
            ui.column {
                ui.div { w = 100, h = 30 },
                ui.div { id = "head", w = 100, h = 20, sticky_top = inset },
                ui.div { w = 100, h = 400 },
            },
        }"##,
    );
    assert_eq!(y(&mut h, "head"), 30.);
    assert!(h.host.run_command("down"));
    assert_eq!(y(&mut h, "head"), 4.);
    assert!(h.host.run_command("more"));
    assert_eq!(y(&mut h, "head"), 10.);
    assert_eq!(h.error(), None);
}

/// Sections from `ui.each`, each with its header: the shape headers take.
#[test]
fn each_section_holds_its_header_in_turn() {
    let mut h = Harness::inline(
        r##"local offset = ui.state("offset", 0)
        local names = ui.state("names", { "a", "b" })
        ui.command { id = "down", run = function() offset:set(50) end }
        ui.command { id = "on", run = function() offset:set(190) end }
        return ui.scroll { w = 100, h = 100, offset = offset,
            ui.each(names, function(name)
                return ui.column { h = 200,
                    ui.div { id = name, w = 100, h = 20, sticky_top = 0 },
                }
            end),
        }"##,
    );
    assert_eq!(y(&mut h, "a"), 0.);
    assert!(h.host.run_command("down"));
    assert_eq!(y(&mut h, "a"), 0.);
    assert_eq!(y(&mut h, "b"), 150.);
    assert!(h.host.run_command("on"));
    assert_eq!(y(&mut h, "a"), -10., "the next section pushes it out");
    assert_eq!(y(&mut h, "b"), 10.);
    assert_eq!(h.error(), None);
}

#[test]
fn on_a_list_it_is_the_inset_of_the_rows_not_a_place() {
    let mut h = Harness::inline(
        r##"local offset = ui.state("offset", 0)
        ui.command { id = "down", run = function() offset:set(50) end }
        return ui.scroll { w = 100, h = 100, offset = offset,
            ui.list { id = "list", w = 100, h = 60, sticky_top = 4,
                heights = { 20, 20, 20, 20, 20, 20 }, sticky = { 1 },
                row = function(i) return ui.div { h = 20 } end },
            ui.div { w = 100, h = 400 },
        }"##,
    );
    h.frames(2);
    assert!(h.host.run_command("down"));
    assert_eq!(
        y(&mut h, "list"),
        -50.,
        "the list scrolls away as any child"
    );
    assert_eq!(h.error(), None);
}
