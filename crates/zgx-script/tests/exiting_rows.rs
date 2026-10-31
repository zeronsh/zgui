//! What leaves takes no clicks: a row removed from its list plays its exit
//! where it was, but a second click there reaches what's under it, never a
//! handler of data already gone.
mod common;

use common::Harness;
use zgx::input::Key;

const STACK: &str = r##"local items = ui.state("items", { 1, 2, 3 })
local log = ui.state("log", "")
local function remove(item)
    log:set(log:get() .. item .. " ")
    items:update(function(list)
        local kept = {}
        for _, i in list do if i ~= item then table.insert(kept, i) end end
        return kept
    end)
end
return ui.column { w = 200, h = 300,
    ui.overlay { id = "stack", w = 200, h = 40,
        ui.each(items, function(item)
            return ui.div { id = "row_" .. item, absolute = true, left = 0, top = 0,
                w = 200, h = 40, bg = "#336699", on_click = function() remove(item) end }
        end, {
            key = function(item) return item end,
            animate = true,
            exit = { opacity = 0, duration = 400, ease = "linear" },
        }),
    },
    ui.text { id = "log", w = 1, h = 1, "{log}" },
}"##;

#[test]
fn a_second_click_on_a_leaving_row_reaches_the_one_under_it() {
    let mut h = Harness::inline(STACK);
    h.frames(2);
    // The last row paints on top.
    h.click("stack");
    h.frames(3);
    assert_eq!(h.text("log"), "3 ");
    // Mid-exit: row 3 still shows over row 2.
    h.click("stack");
    h.frames(3);
    assert_eq!(h.text("log"), "3 2 ", "never 3 twice");
    h.frames(40);
    assert_eq!(h.find("row_3"), None, "gone once its exit plays");
    assert_eq!(h.error(), None);
}

/// What a presence lets go of, replaced by something smaller, takes no
/// clicks while it leaves: the box under it does.
#[test]
fn a_presence_leaving_takes_no_clicks() {
    let mut h = Harness::inline(
        r##"local shown = ui.state("shown", "a")
        local log = ui.state("log", "")
        return ui.column { w = 200, h = 300,
            ui.overlay { id = "spot", w = 200, h = 40,
                ui.div { w = 200, h = 40, on_click = function() log:set(log:get() .. "under ") end },
                ui.presence(shown, function(name)
                    return ui.div { id = "p_" .. name, w = if name == "a" then 200 else 20, h = 40,
                        bg = "#336699",
                        on_click = function()
                            log:set(log:get() .. name .. " ")
                            shown:set("b")
                        end }
                end, { exit = { opacity = 0, duration = 400, ease = "linear" } }),
            },
            ui.text { id = "log", w = 1, h = 1, "{log}" },
        }"##,
    );
    h.frames(2);
    h.click("spot");
    h.frames(3);
    assert_eq!(h.text("log"), "a ");
    h.click("spot");
    h.frames(3);
    assert_eq!(h.text("log"), "a under ", "never `a` twice");
    assert_eq!(h.error(), None);
}

/// Buttons in a list leaving with an exit: clicking one removes it.
const BUTTONS: &str = r##"local items = ui.state("items", { 1, 2, 3 })
local log = ui.state("log", "")
return ui.column { w = 200, h = 300,
    ui.each(items, function(item)
        return ui.button { id = "row_" .. item, w = 200, h = 30, on_click = function()
            log:set(log:get() .. item .. " ")
            items:update(function(list)
                local kept = {}
                for _, i in list do if i ~= item then table.insert(kept, i) end end
                return kept
            end)
        end, "Remove " .. item }
    end, {
        key = function(item) return item end,
        exit = { opacity = 0, duration = 400, ease = "linear" },
    }),
    ui.text { id = "log", w = 1, h = 1, "{log}" },
}"##;

impl Harness {
    fn focused_is(&mut self, id: &str) -> bool {
        let node = self.find(id);
        node.is_some() && self.ui.input.focused() == node
    }
}

/// Focus moves to the next row as its row leaves, and Enter acts on that
/// one, never on the row whose data is gone.
#[test]
fn enter_after_a_removal_acts_on_the_next_row() {
    let mut h = Harness::inline(BUTTONS);
    h.frames(2);
    h.click("row_2");
    h.frames(2);
    assert_eq!(h.text("log"), "2 ");
    assert!(h.focused_is("row_3"), "focus moved to the next row");
    h.key(Key::Enter);
    h.frames(2);
    assert_eq!(h.text("log"), "2 3 ");
    assert_eq!(h.error(), None);
}

/// The last row leaving gives focus to the one before it; Space acts there.
#[test]
fn space_after_removing_the_last_row_acts_on_the_one_before() {
    let mut h = Harness::inline(BUTTONS);
    h.frames(2);
    h.click("row_3");
    h.frames(2);
    assert!(h.focused_is("row_2"), "focus moved to the row before");
    h.key(Key::Space);
    h.frames(2);
    assert_eq!(h.text("log"), "3 2 ");
    assert_eq!(h.error(), None);
}

/// The only row leaving leaves nothing focused, and no key reaches it.
#[test]
fn keys_never_reach_a_row_leaving() {
    let mut h = Harness::inline(&BUTTONS.replace("{ 1, 2, 3 }", "{ 1 }"));
    h.frames(2);
    h.click("row_1");
    h.frames(2);
    assert!(!h.focused_is("row_1"));
    h.key(Key::Enter);
    h.key(Key::Space);
    h.frames(2);
    assert_eq!(h.text("log"), "1 ");
    assert_eq!(h.error(), None);
}
