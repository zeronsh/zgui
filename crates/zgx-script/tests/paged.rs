//! A list loaded a page at a time (`ui.paged`), a list asking for more as
//! its end comes in reach (`ui.list`'s `on_end`), and a state following
//! another once it has settled (`ui.debounced`).
mod common;
use common::Harness;
use std::time::{Duration, Instant};
use zgx_script::{ScriptHost, Value};

const APP: &str = r#"
local query = ui.state("query", "a")
local pages = ui.paged(function(cursor)
    -- The query is its key: a new one loads again from the start.
    local q = query:get()
    return ui.call("page", { cursor = cursor, q = q }):await()
end)
local typed = ui.state("typed", "")
local settled = ui.debounced(typed, 30)
return ui.column {
    ui.text { id = "pages", function()
        return `{#pages.items:get()} {pages.status:get()} {pages.done:get()} {pages.loading:get()}`
    end },
    ui.text { id = "first", function()
        local items = pages.items:get()
        return if #items > 0 then tostring(items[1]) else "-"
    end },
    ui.text { id = "error", function()
        local err = pages.error
        return if err then `{err.kind}: {err}` else "none"
    end },
    ui.button { id = "more", w = 40, h = 20, "m", on_click = pages.more },
    ui.button { id = "reload", w = 40, h = 20, "r", on_click = pages.reload },
    ui.button { id = "other", w = 40, h = 20, "o", on_click = function() query:set("b") end },
    ui.button { id = "fail", w = 40, h = 20, "f", on_click = function() query:set("fail") end },
    ui.button { id = "type", w = 40, h = 20, "t", on_click = function()
        typed:set("h")
        typed:set("he")
        typed:set("hey")
    end },
    ui.text { id = "settled", function() return settled:get() end },
}
"#;

const LIST_APP: &str = r#"
local pages = ui.paged(function(cursor)
    return ui.call("page", { cursor = cursor, q = "a" }):await()
end)
return ui.column {
    ui.text { id = "count", function() return `{#pages.items:get()} {pages.done:get()}` end },
    -- 100 points of 20-point rows: it asks for pages until its rows pass
    -- its end by 200 points, or there are no more.
    ui.list { id = "list", h = 100, w = 200, height = 20, data = pages.items,
        on_end = pages.more,
        row = function(item) return ui.text { tostring(item) } end },
}
"#;

/// Pages of numbers: 5, then 5, then 2 and no more; numbered from 100 for
/// the query "b"; a failure for "fail".
fn host(runtime: &zgx::reactive::Runtime, app: &str) -> ScriptHost {
    let host = ScriptHost::inline(runtime, "app", app);
    host.async_action("page", |arg| async move {
        zgx::timer::sleep(Duration::from_millis(2)).await;
        let field = |name: &str| arg.field(name).cloned().unwrap_or(Value::Nil);
        let base = match field("q") {
            Value::Str(q) if &*q == "fail" => return Err("the server is down".into()),
            Value::Str(q) if &*q == "b" => 100.,
            _ => 0.,
        };
        let page = match field("cursor") {
            Value::Num(n) => n as usize,
            _ => 1,
        };
        let (first, count) = match page {
            1 => (1, 5),
            2 => (6, 5),
            _ => (11, 2),
        };
        let items: Vec<Value> = (first..first + count)
            .map(|n| Value::Num(base + n as f64))
            .collect();
        let next = if page < 3 {
            Value::Num((page + 1) as f64)
        } else {
            Value::Nil
        };
        Ok(Value::record([
            ("items", Value::List(items.into())),
            ("next", next),
        ]))
    });
    host
}

/// Settle until `id` reads `text`.
fn wait_for(h: &mut Harness, id: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        h.settle();
        h.ui.prepare_frame();
        if h.text(id) == text {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("`{id}` reads {:?}, not {text:?}", h.text(id));
}

#[test]
fn pages_load_one_after_another_until_there_are_no_more() {
    let mut h = Harness::new(|runtime| host(runtime, APP));
    wait_for(&mut h, "pages", "5 ready false false");
    h.click("more");
    wait_for(&mut h, "pages", "10 ready false false");
    h.click("more");
    wait_for(&mut h, "pages", "12 ready true false");
    // No `next`: more asks nothing.
    h.click("more");
    h.frames(3);
    assert_eq!(h.text("pages"), "12 ready true false");
    assert_eq!(h.text("error"), "none");
}

#[test]
fn a_new_key_or_a_reload_loads_from_the_start() {
    let mut h = Harness::new(|runtime| host(runtime, APP));
    wait_for(&mut h, "pages", "5 ready false false");
    h.click("more");
    wait_for(&mut h, "pages", "10 ready false false");
    h.click("other");
    wait_for(&mut h, "first", "101");
    wait_for(&mut h, "pages", "5 ready false false");
    h.click("more");
    wait_for(&mut h, "pages", "10 ready false false");
    h.click("reload");
    wait_for(&mut h, "pages", "5 ready false false");
}

#[test]
fn a_failed_load_is_an_error_to_branch_on() {
    let mut h = Harness::new(|runtime| host(runtime, APP));
    wait_for(&mut h, "pages", "5 ready false false");
    h.click("fail");
    wait_for(&mut h, "error", "action: the server is down");
    assert!(h.text("pages").contains("failed"), "{}", h.text("pages"));
    assert_eq!(h.error(), None);
}

#[test]
fn a_list_asks_for_pages_until_it_fills_or_none_are_left() {
    let mut h = Harness::new(|runtime| host(runtime, LIST_APP));
    // 12 rows (240 points) never pass 100 + 200: it loads them all, then
    // stops, with no click.
    wait_for(&mut h, "count", "12 true");
    h.frames(5);
    assert_eq!(h.text("count"), "12 true");
}

#[test]
fn a_debounced_state_follows_once_it_settles() {
    let mut h = Harness::new(|runtime| host(runtime, APP));
    h.click("type");
    h.ui.prepare_frame();
    assert_eq!(h.text("settled"), "", "not yet: the wait is still on");
    wait_for(&mut h, "settled", "hey");
}
