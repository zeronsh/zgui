//! `ui.each { reorder }`: rows dragged into a new order (above or below a
//! row by where they drop), moved with Alt+↑/↓ and heard as they move; it
//! works with animated rows, from their first visible frame. Also the test
//! helpers `TestApp::drag` and `TestApp::scroll`.
mod common;
use common::Harness;
use std::time::Duration;
use zgx::input::{InputEvent, Key, Modifiers, PointerButton};

fn app(animate: bool) -> Harness {
    let code = r##"
local folders = ui.state("folders", FIRST)
local moves = ui.state("moves", "")
return ui.column {
    ui.each(folders, function(name)
        return ui.row { id = `folder-{name}`, w = 160, h = 24, ui.text { name } }
    end, {
        key = function(name) return name end,
        reorder = function(from, to)
            moves:set(`{moves:peek()}{from}>{to};`)
            local list = table.clone(folders:peek())
            local name = table.remove(list, from)
            table.insert(list, to, name)
            folders:set(list)
        end,
        ANIMATE
    }),
    ui.text { id = "order", function() return table.concat(folders:get(), ",") end },
    ui.text { id = "moves", "{moves}" },
    ui.button { id = "fill", w = 40, h = 20, "f", on_click = function() folders:set({ "Inbox", "Work", "Personal" }) end },
}
"##
    .replace("FIRST", if animate { "{}" } else { r#"{ "Inbox", "Work", "Personal" }"# })
    .replace(
        "ANIMATE",
        if animate { "animate = true, enter = { opacity = { 0, 1 }, duration = 200 }," } else { "" },
    );
    let h = Harness::inline(&code);
    h.frames(2);
    h
}

#[test]
fn a_row_dropped_on_another_goes_above_or_below_it() {
    let mut h = app(false);
    assert_eq!(h.error(), None);
    h.drag("folder-Personal", "folder-Inbox");
    assert_eq!(h.text("order"), "Personal,Inbox,Work", "upper half: above");
    assert_eq!(h.text("moves"), "3>1;");
    // Released over a row's lower half: below it.
    let (a, b) = (h.bounds("folder-Personal"), h.bounds("folder-Work"));
    let (x, y) = (a.x + 10., a.y + 6.);
    let (tx, ty) = (b.x + 20., b.y + b.height * 0.8);
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    h.ui.dispatch(InputEvent::PointerMove {
        x: x + 16.,
        y: y + 8.,
    });
    h.ui.dispatch(InputEvent::PointerMove { x: tx, y: ty });
    h.ui.dispatch(InputEvent::PointerUp {
        x: tx,
        y: ty,
        button: PointerButton::Primary,
    });
    h.settle();
    assert_eq!(h.text("order"), "Inbox,Work,Personal", "lower half: below");
}

#[test]
fn alt_and_an_arrow_move_a_row_and_say_where() {
    let mut h = app(false);
    h.click("folder-Inbox");
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    h.key_with(Key::ArrowDown, alt);
    assert_eq!(h.text("order"), "Work,Inbox,Personal");
    let texts = h.texts();
    assert!(
        texts.contains(&"Moved Inbox to position 2 of 3".to_string()),
        "{texts:?}"
    );
    h.key_with(Key::ArrowUp, alt);
    assert_eq!(h.text("order"), "Inbox,Work,Personal");
    h.key_with(Key::ArrowUp, alt);
    assert_eq!(
        h.text("order"),
        "Inbox,Work,Personal",
        "at the top it stays"
    );
}

#[test]
fn animated_rows_reorder_from_their_first_visible_frame() {
    let mut h = app(true);
    h.click("fill");
    h.frames(1);
    // The first frame of the fade-in shows nothing and takes no pointer
    // (opacity 0, unlike CSS); a little later it is part way in.
    std::thread::sleep(Duration::from_millis(80));
    h.frames(1);
    // The row's wrapper (what `ui.each` animates) holds it.
    let n = h.find("folder-Inbox").unwrap();
    let opacity = {
        let scene = h.ui.scene.borrow();
        let wrapper = scene.parent(n).expect("the reorder wrapper");
        scene.effects(wrapper).opacity
    };
    assert!(opacity > 0. && opacity < 1., "mid-enter: {opacity}");
    h.drag("folder-Personal", "folder-Inbox");
    assert_eq!(h.text("order"), "Personal,Inbox,Work");
}

#[test]
#[should_panic(expected = "needs `key`")]
fn reorder_needs_a_key() {
    let _ = Harness::inline(
        r##"local l = ui.state("l", { 1, 2 })
        return ui.each(l, function(n) return ui.text { tostring(n) } end, { reorder = function() end })"##,
    );
}

#[test]
fn scroll_scrolls_the_viewport_under_a_node() {
    let mut h = Harness::inline(
        r##"local offset = ui.state("offset", 0)
        local rows = {}
        for i = 1, 50 do rows[i] = ui.text { `Row {i}`, h = 20 } end
        return ui.column {
            ui.scroll { id = "list", h = 100, w = 200, offset = offset, ui.column(rows) },
            ui.text { id = "at", function() return tostring(math.floor(offset:get())) end },
        }"##,
    );
    h.scroll("list", 60.);
    h.frames(2);
    assert!(h.text("at") != "0", "{}", h.text("at"));
}
