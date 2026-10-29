//! What a script is handed when something fails (`UiError`): the same
//! shape from `ui.try`, a promise's `error`, `:after`'s `on_error` and an
//! await, with a `kind` (and a status and body for a fetch asked to raise)
//! to branch on, and still text where text was expected.
mod common;
use common::Harness;
use std::time::{Duration, Instant};
use zgx_script::{ScriptHost, Value};

const APP: &str = r#"
local shown = ui.state("shown", "")
local kept = ui.state("kept", "")
local function show(text) shown:set(text) end
return ui.column {
    ui.text { id = "shown", function() return shown:get() end },
    ui.text { id = "kept", "{kept}" },
    -- A string raised: its text, still text everywhere text goes.
    ui.button { id = "raised", w = 40, h = 20, "r", on_click = function()
        local value, err = ui.try(function() error("no such user", 0) end)
        assert(value == nil)
        show(table.concat({
            err.kind, err.message, tostring(err), "saw: " .. err, err .. "!",
            tostring(err:find("such") ~= nil), err:upper(), `{err}`,
            tostring(err.status), tostring(err.value),
        }, "|"))
        kept:set(err)
    end },
    -- A table raised: kept as it was, its `message` read.
    ui.button { id = "table", w = 40, h = 20, "t", on_click = function()
        local raised = { code = 409, message = "taken" }
        local _, err = ui.try(function() error(raised) end)
        show(`{err.kind} {err.message} {err.value == raised} {err.value.code}`)
    end },
    -- Success is as before: the value and no error.
    ui.button { id = "fine", w = 40, h = 20, "f", on_click = function()
        local value, err = ui.try(function(a, b) return a + b end, 2, 3)
        show(`{value} {err == nil}`)
    end },
    -- A host action that rejects, awaited in a task.
    ui.button { id = "action", w = 40, h = 20, "a", on_click = function()
        ui.task(function()
            local _, err = ui.try(function() return ui.call("refuse"):await() end)
            show(`{err.kind}: {err}`)
        end)
    end },
    -- The network down, a 404 as a response, and a 404 asked to raise.
    ui.button { id = "fetch", w = 40, h = 20, "n", on_click = function()
        ui.task(function()
            local _, offline = ui.try(function() return ui.fetch("https://api.test/elsewhere"):await() end)
            local missing = ui.fetch("https://api.test/missing"):await()
            local _, err = ui.try(function()
                return ui.fetch("https://api.test/missing", { raise = true }):await()
            end)
            show(`{offline.kind} {offline} / {missing.ok} {missing.status} / {err.kind} {err.status} {err.body.reason} {err}`)
        end)
    end },
    -- A cancelled call: its `error` and `:after`'s on_error say so.
    ui.button { id = "cancel", w = 40, h = 20, "c", on_click = function()
        local p = ui.call("slow")
        p:after(nil, function(err) show(`after {err.kind}`) end)
        p:cancel()
        show(`{shown:get()}, field {p.error.kind} {p.error}`)
    end },
    -- An optimistic write refused: the state goes back, and the error says why.
    ui.button { id = "optimistic", w = 40, h = 20, "o", on_click = ui.async(function()
        local liked = ui.state("liked", false)
        local err = ui.optimistic(liked, true, function()
            return ui.call("refuse"):await()
        end)
        show(`{err.kind} {err} {liked:get()}`)
    end) },
    -- Uncaught in a `ui.async` handler: reported once, by its message.
    ui.button { id = "async", w = 40, h = 20, "h", on_click = ui.async(function()
        ui.call("refuse"):await()
    end) },
    -- Uncaught in a task: reported with its message.
    ui.button { id = "uncaught", w = 40, h = 20, "u", on_click = function()
        ui.task(function() ui.call("refuse"):await() end)
    end },
}
"#;

fn app() -> Harness {
    Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.async_action("refuse", |_| async move {
            zgx::timer::sleep(Duration::from_millis(2)).await;
            Err::<Value, String>("not allowed".into())
        });
        host.async_action("slow", |_| async move {
            zgx::timer::sleep(Duration::from_secs(60)).await;
            Ok(Value::Nil)
        });
        host.fetcher(|request: zgx_script::Request| async move {
            zgx::timer::sleep(Duration::from_millis(2)).await;
            match request.url.as_str() {
                "https://api.test/missing" => Ok(zgx_script::Response {
                    status: 404,
                    headers: vec![("Content-Type".into(), "application/json".into())],
                    body: br#"{"reason":"gone"}"#.to_vec(),
                }),
                _ => Err("the network is unreachable".into()),
            }
        });
        host
    })
}

/// Settle until `id` reads `text` (the host's work takes a few ms).
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
fn a_raised_string_is_an_error_that_is_still_text() {
    let mut h = app();
    h.click("raised");
    h.ui.prepare_frame();
    assert_eq!(
        h.text("shown"),
        "raised|no such user|no such user|saw: no such user|no such user!|true|NO SUCH USER|no such user|nil|nil"
    );
    assert_eq!(
        h.text("kept"),
        "no such user",
        "a state set to it holds its message"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn a_raised_table_is_kept_as_it_was() {
    let mut h = app();
    h.click("table");
    h.ui.prepare_frame();
    assert_eq!(h.text("shown"), "raised taken true 409");
}

#[test]
fn success_is_as_before() {
    let mut h = app();
    h.click("fine");
    h.ui.prepare_frame();
    assert_eq!(h.text("shown"), "5 true");
}

#[test]
fn each_failure_says_where_it_came_from() {
    let mut h = app();
    h.click("action");
    wait_for(&mut h, "shown", "action: not allowed");
    h.click("fetch");
    wait_for(
        &mut h,
        "shown",
        "network the network is unreachable / false 404 / http 404 gone HTTP 404",
    );
    h.click("cancel");
    h.ui.prepare_frame();
    assert_eq!(
        h.text("shown"),
        "after cancelled, field cancelled cancelled"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn an_error_nothing_caught_is_reported_by_its_message() {
    let mut h = app();
    h.click("uncaught");
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.error().is_none() && Instant::now() < deadline {
        h.settle();
        std::thread::sleep(Duration::from_millis(2));
    }
    let error = h.error().expect("reported");
    assert!(error.contains("not allowed"), "{error}");
    assert!(!error.contains("userdata"), "{error}");
}

#[test]
fn an_optimistic_write_refused_gives_back_the_error() {
    let mut h = app();
    h.click("optimistic");
    wait_for(&mut h, "shown", "action not allowed false");
}

#[test]
fn an_async_handlers_uncaught_error_is_reported_by_its_message() {
    let mut h = app();
    h.click("async");
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.error().is_none() && Instant::now() < deadline {
        h.settle();
        std::thread::sleep(Duration::from_millis(2));
    }
    let error = h.error().expect("reported");
    assert!(error.contains("not allowed"), "{error}");
    assert!(!error.contains("userdata"), "{error}");
}
