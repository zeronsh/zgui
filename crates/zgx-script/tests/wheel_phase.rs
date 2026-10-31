//! Luau's `on_wheel` as it bubbles: a wrapper's handler hears a wheel once,
//! after the scroll views inside it have had it, and `true` back keeps it
//! from the scroll views around it.
mod common;
use common::Harness;
use zgx::input::InputEvent;

const APP: &str = r#"
local outer = ui.state("outer", 0)
local inner = ui.state("inner", 0)
local heard = ui.state("heard", 0)
local function lines(prefix, n)
    local list = {}
    for i = 1, n do
        table.insert(list, ui.text { h = 20, `{prefix} {i}` })
    end
    return list
end
local inner_content = lines("inner", 30)
local outer_content = lines("outer", 30)
table.insert(outer_content, 1, ui.column {
    id = "wrapper",
    h = 100,
    on_wheel = function()
        heard:set(heard:peek() + 1)
        return true
    end,
    ui.scroll { id = "inner", h = 100, offset = inner, ui.column(inner_content) },
})
return ui.column {
    ui.text { id = "state", function()
        return `{math.floor(outer:get())} {math.floor(inner:get())} {heard:get()}`
    end },
    ui.scroll { id = "outer", h = 200, w = 200, offset = outer, ui.column(outer_content) },
}
"#;

fn wheel(h: &mut Harness, dy: f32) {
    // Over the inner scroll view: the outer's first child.
    h.ui.dispatch(InputEvent::Scroll {
        x: 50.,
        y: 70.,
        delta_x: 0.,
        delta_y: dy,
    });
    h.frames(2);
}

#[test]
fn the_inner_view_scrolls_and_the_wrapper_keeps_the_rest_from_the_outer() {
    let mut h = Harness::inline(APP);
    h.frames(2);
    assert_eq!(h.text("state"), "0 0 0");
    // The inner view takes it: it scrolls, and the wrapper hears nothing.
    wheel(&mut h, 40.);
    assert_eq!(h.text("state"), "0 40 0");
    // At the inner's end (600 of content in 100: 500), what is left goes
    // up to the wrapper, which takes it: the outer stays put, and the
    // handler ran once for the one wheel.
    wheel(&mut h, 1000.);
    let state = h.text("state");
    assert!(state.starts_with("0 500 "), "{state}");
    wheel(&mut h, 40.);
    let state = h.text("state");
    assert!(state.starts_with("0 500 "), "the outer stays put: {state}");
    assert_eq!(h.error(), None);
}

const TARGET: &str = r#"
local heard = ui.state("heard", 0)
local outer = ui.state("outer", 0)
local rows = {}
for i = 1, 30 do
    table.insert(rows, ui.text { h = 20, `row {i}` })
end
table.insert(rows, 1, ui.column {
    id = "wrapper",
    h = 60,
    on_wheel = function()
        heard:set(heard:peek() + 1)
        return true
    end,
    ui.text { "over the wrapper itself" },
})
return ui.column {
    ui.text { id = "state", function() return `{math.floor(outer:get())} {heard:get()}` end },
    ui.scroll { id = "outer", h = 200, w = 200, offset = outer, ui.column(rows) },
}
"#;

#[test]
fn a_wrapper_hears_a_wheel_on_itself_once_and_keeps_it() {
    let mut h = Harness::inline(TARGET);
    h.frames(2);
    h.ui.dispatch(InputEvent::Scroll {
        x: 50.,
        y: 50.,
        delta_x: 0.,
        delta_y: 40.,
    });
    h.frames(2);
    assert_eq!(h.text("state"), "0 1", "once, and the outer stays put");
}

/// A trackpad gesture as the desktop reports one: its phases around its
/// motion, then momentum's.
fn trackpad(h: &mut Harness, x: f32, y: f32, steps: &[f32], momentum: &[f32]) {
    use zgx::input::ScrollGesture;
    let scroll = |h: &mut Harness, dy: f32| {
        h.ui.dispatch(InputEvent::Scroll {
            x,
            y,
            delta_x: 0.,
            delta_y: dy,
        });
        h.frames(1);
    };
    h.ui.dispatch(InputEvent::ScrollGesture(ScrollGesture::Began));
    for dy in steps {
        scroll(h, *dy);
    }
    h.ui.dispatch(InputEvent::ScrollGesture(ScrollGesture::Ended));
    h.ui.dispatch(InputEvent::ScrollGesture(ScrollGesture::MomentumBegan));
    for dy in momentum {
        scroll(h, *dy);
    }
    h.ui.dispatch(InputEvent::ScrollGesture(ScrollGesture::MomentumEnded));
    h.frames(2);
}

#[test]
fn a_trackpad_gesture_scrolls_the_inner_view_and_never_the_outer() {
    let mut h = Harness::inline(APP);
    h.ui.set_elastic_scrolling(true);
    h.frames(2);
    // Past the inner's end and on with momentum, over the inner view.
    trackpad(&mut h, 50., 70., &[60.; 12], &[40., 30., 20., 10.]);
    assert_eq!(
        h.text("state"),
        "0 500 0",
        "the inner at its end, the outer still"
    );
    // A new gesture at the inner's end: the wrapper takes what is left.
    std::thread::sleep(std::time::Duration::from_millis(320));
    trackpad(&mut h, 50., 70., &[30., 30.], &[20.]);
    let state = h.text("state");
    assert!(state.starts_with("0 500 "), "the outer stays put: {state}");
    assert_eq!(h.error(), None);
}

/// A transcript (a list) inside a page (a scroll view), contained or not.
fn page(contain: bool) -> String {
    format!(
        r#"
local page = ui.state("page", 0)
local list = ui.state("list", 0)
local rows = {{}}
for i = 1, 30 do
    table.insert(rows, ui.text {{ h = 20, `page row {{i}}` }})
end
table.insert(rows, 1, ui.list {{
    id = "transcript",
    h = 100,
    count = 30,
    height = 20,
    offset = list,
    {}
    row = function(i) return ui.text {{ `message {{i}}` }} end,
}})
return ui.column {{
    ui.text {{ id = "state", function() return `{{math.floor(page:get())}} {{math.floor(list:get())}}` end }},
    ui.scroll {{ id = "page", h = 200, w = 200, offset = page, ui.column(rows) }},
}}
"#,
        if contain {
            r#"overscroll = "contain","#
        } else {
            ""
        }
    )
}

#[test]
fn a_contained_list_keeps_a_gesture_at_its_edge_momentum_and_all() {
    let mut h = Harness::inline(&page(true));
    h.ui.set_elastic_scrolling(true);
    h.frames(2);
    // Mid-range: the list scrolls, the page stays.
    trackpad(&mut h, 50., 70., &[40.], &[]);
    assert_eq!(h.text("state"), "0 40");
    // On past the list's end (600 in 100: 500), momentum included: the
    // page never moves.
    trackpad(&mut h, 50., 70., &[200.; 4], &[100., 60., 30.]);
    assert_eq!(h.text("state"), "0 500");
    // A new gesture starting at its end: still the list's.
    std::thread::sleep(std::time::Duration::from_millis(320));
    trackpad(&mut h, 50., 70., &[50., 50.], &[30.]);
    assert_eq!(h.text("state"), "0 500", "contained: the page stays put");
    // Up again from the end: the list.
    std::thread::sleep(std::time::Duration::from_millis(320));
    trackpad(&mut h, 50., 70., &[-60.], &[]);
    assert_eq!(h.text("state"), "0 440");
    assert_eq!(h.error(), None);
}

#[test]
fn an_uncontained_list_hands_a_new_gesture_at_its_edge_to_the_page() {
    let mut h = Harness::inline(&page(false));
    h.frames(2);
    trackpad(&mut h, 50., 70., &[600.], &[]);
    assert_eq!(h.text("state"), "0 500");
    std::thread::sleep(std::time::Duration::from_millis(320));
    trackpad(&mut h, 50., 70., &[50.], &[]);
    assert_eq!(h.text("state"), "50 500", "auto: the page takes it");
}

#[test]
fn a_contained_scroll_view_or_textarea_keeps_a_gesture_at_its_edge() {
    let app = r#"
local page = ui.state("page", 0)
local inner = ui.state("inner", 0)
local text = ui.state("text", "")
local lines = {}
for i = 1, 30 do
    table.insert(lines, ui.text { h = 20, `line {i}` })
end
local rows = {}
for i = 1, 30 do
    table.insert(rows, ui.text { h = 20, `page row {i}` })
end
table.insert(rows, 1, ui.scroll { id = "inner", h = 100, offset = inner, overscroll = "contain", ui.column(lines) })
table.insert(rows, 2, ui.textarea { id = "notes", h = 60, value = text, overscroll = "contain" })
return ui.column {
    ui.text { id = "state", function() return `{math.floor(page:get())} {math.floor(inner:get())}` end },
    ui.scroll { id = "page", h = 300, w = 200, offset = page, ui.column(rows) },
}
"#;
    let mut h = Harness::inline(app);
    h.frames(2);
    // The inner scroll view, to its end and on: the page stays.
    trackpad(&mut h, 50., 70., &[600.], &[]);
    std::thread::sleep(std::time::Duration::from_millis(320));
    trackpad(&mut h, 50., 70., &[50.], &[20.]);
    assert_eq!(h.text("state"), "0 500");
    // The textarea, empty (nothing to scroll): contained, the page stays.
    std::thread::sleep(std::time::Duration::from_millis(320));
    trackpad(&mut h, 50., 150., &[50.], &[20.]);
    assert_eq!(h.text("state"), "0 500");
    assert_eq!(h.error(), None);
}
