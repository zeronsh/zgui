//! A list's start coming in reach (`on_start`), whether it rests at its end
//! (`at_end`), and its rows in view staying put as an earlier page is added
//! above them.
mod common;
use common::Harness;

const APP: &str = r#"
-- Rows 101 to 140; each earlier page adds the ten before the first.
local rows = ui.state("rows", (function()
    local list = {}
    for n = 101, 140 do
        table.insert(list, { id = n })
    end
    return list
end)())
local offset = ui.state("offset", 400)
local at_end = ui.state("at_end", false)
local pages = ui.state("pages", 0)
return ui.column {
    ui.text { id = "state", function()
        return `{pages:get()} {at_end:get()} {#rows:get()}`
    end },
    ui.row {
        ui.button { id = "to_end", w = 40, h = 20, "e", on_click = function() offset:set(600) end },
        ui.button { id = "near_start", w = 40, h = 20, "s", on_click = function() offset:set(30) end },
    },
    ui.list {
        id = "list",
        h = 200,
        w = 200,
        height = 20,
        measured = true,
        data = rows,
        key = function(row) return row.id end,
        offset = offset,
        at_end = at_end,
        start_distance = 40,
        on_start = function()
            local list = rows:peek()
            local first = list[1].id
            if first <= 81 then
                return
            end
            pages:set(pages:peek() + 1)
            local earlier = {}
            for n = first - 10, first - 1 do
                table.insert(earlier, { id = n })
            end
            for _, row in list do
                table.insert(earlier, row)
            end
            rows:set(earlier)
        end,
        row = function(row)
            return ui.text { id = `row-{row.id}`, h = 20, tostring(row.id) }
        end,
    },
}
"#;

#[test]
fn an_earlier_page_comes_in_above_and_the_view_stays_on_its_rows() {
    let mut h = Harness::inline(APP);
    h.frames(3);
    // 40 rows of 20 points in 200: the end is at 600; 400 is short of it.
    assert_eq!(h.text("state"), "0 false 40");
    // Scrolled to the end: at its end.
    h.click("to_end");
    h.frames(2);
    assert_eq!(h.text("state"), "0 true 40");
    // Up to 30 points from the start: an earlier page comes in above. Row
    // 102 started 20 points down, so 10 points of it were above the view;
    // with ten rows of 20 points above it now, it is 10 points above still.
    h.click("near_start");
    h.frames(4);
    assert_eq!(h.text("state"), "1 false 50");
    let shown = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.scene.borrow().transformed_bounds(node)
    };
    let list = shown(&mut h, "list");
    let row = shown(&mut h, "row-102");
    assert_eq!(row.y - list.y, -10.);
    assert_eq!(h.error(), None);
}
