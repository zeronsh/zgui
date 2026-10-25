//! Host calls with results (`ui.call`): a promise read by `:after`, by its
//! reactive fields, and by `:await()` in a `ui.task`; rejections handled or
//! reported.
mod common;
use common::Harness;
use std::time::{Duration, Instant};
use zgx_script::{ScriptHost, Value};

const APP: &str = r#"
local result = ui.state("result", "")
-- Called while the script loads: the interface follows its fields.
local first = ui.call("times_ten", 1)
local function show(text) result:set(text) end
return ui.column {
    ui.text { id = "loading", function()
        if first.done then return `first {first.value}` end
        return "loading"
    end },
    ui.text { id = "label", function() return result:get() end },
    ui.button { id = "after", w = 40, h = 20, "a", on_click = function()
        ui.call("times_ten", 2):after(function(v) show(`after {v}`) end, function(e) show(`after error {e}`) end)
    end },
    ui.button { id = "rejected", w = 40, h = 20, "r", on_click = function()
        ui.call("times_ten", "fail"):after(function(v) show(`after {v}`) end, function(e) show(`after error {e}`) end)
    end },
    ui.button { id = "task", w = 40, h = 20, "t", on_click = function()
        ui.task(function()
            local v = ui.call("times_ten", 3):await()
            local ok, e = pcall(function() return ui.call("times_ten", "fail"):await() end)
            show(`task {v}, then {if ok then "?" else e}`)
        end)
    end },
    ui.button { id = "sync", w = 40, h = 20, "s", on_click = function()
        ui.call("plain"):after(function(v) show(`plain {v}`) end)
    end },
    ui.button { id = "unhandled", w = 40, h = 20, "u", on_click = function()
        ui.call("times_ten", "fail")
    end },
    ui.button { id = "outside", w = 40, h = 20, "o", on_click = function()
        ui.call("times_ten", 4):await()
    end },
}
"#;

fn app() -> Harness {
    Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        // A host call that takes a moment: ten times a number, or fails.
        host.async_action("times_ten", |arg| async move {
            zgx::timer::sleep(Duration::from_millis(5)).await;
            match arg {
                Value::Num(n) => Ok(Value::Num(n * 10.)),
                _ => Err("nope".into()),
            }
        });
        host.action("plain", |_| {});
        host
    })
}
/// Settle until `id` reads `text` (the host calls take a few ms).
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
fn an_interface_follows_a_promise_it_reads() {
    let mut h = app();
    assert_eq!(h.text("loading"), "loading");
    wait_for(&mut h, "loading", "first 10");
}

#[test]
fn after_hears_a_value_or_an_error() {
    let mut h = app();
    h.click("after");
    wait_for(&mut h, "label", "after 20");
    h.click("rejected");
    wait_for(&mut h, "label", "after error nope");
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn a_task_awaits_values_and_catches_rejections() {
    let mut h = app();
    h.click("task");
    wait_for(&mut h, "label", "task 30, then nope");
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn a_plain_action_settles_at_once() {
    let mut h = app();
    h.click("sync");
    h.settle();
    assert_eq!(h.text("label"), "plain nil");
}

#[test]
fn a_rejection_nothing_handles_is_reported() {
    let mut h = app();
    h.click("unhandled");
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.host.error().get().is_none() && Instant::now() < deadline {
        h.settle();
        std::thread::sleep(Duration::from_millis(2));
    }
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("ui.call(\"times_ten\"): nope"), "{error}");
}

#[test]
fn await_outside_a_task_says_where_it_belongs() {
    let mut h = app();
    h.click("outside");
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("await runs inside ui.task"), "{error}");
}

/// What a host call costs: `ui.call` to an action that answers at once,
/// its promise settled and an `:after` callback run, start to finish, for
/// batches of 1,000 to 8,000 calls (the cost per call should not grow).
/// `cargo test --release -p zgx-script --test state calls:: -- --ignored --nocapture`
#[test]
#[ignore]
fn call_cost() {
    for n in [1_000usize, 2_000, 4_000, 8_000] {
        let mut h = Harness::new(|runtime| {
            let host = ScriptHost::inline(
                runtime,
                "app",
                r#"
local count = ui.state("count", 0)
local total = ui.shared("n", 0)
return ui.button { id = "go", w = 40, h = 20, "go", on_click = function()
    local n, want = 0, total:get()
    for i = 1, want do
        ui.call("echo", i):after(function() n += 1; if n == want then count:set(n) end end)
    end
end, ui.text { id = "count", "{count}" } }
"#,
            );
            host.shared("n", n as f64);
            host.async_action("echo", |arg| async move { Ok(arg) });
            host
        });
        let mut runs: Vec<f64> = (0..5)
            .map(|_| {
                let start = Instant::now();
                h.click("go");
                h.settle();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(h.text("count"), n.to_string(), "{:?}", h.host.error().get());
                elapsed
            })
            .collect();
        runs.sort_by(f64::total_cmp);
        println!(
            "{n} calls: {:.2} µs per call (ui.call + settle + after, median of 5)",
            runs[2] / n as f64 * 1e6
        );
    }
}

/// A fetcher answering from a table, recording what it was asked.
fn fetching(host: &ScriptHost) -> std::rc::Rc<std::cell::RefCell<Vec<zgx_script::Request>>> {
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = asked.clone();
    host.fetcher(move |request: zgx_script::Request| {
        log.borrow_mut().push(request.clone());
        async move {
            zgx::timer::sleep(Duration::from_millis(2)).await;
            match request.url.as_str() {
                "https://api.test/user" => Ok(zgx_script::Response {
                    status: 200,
                    headers: vec![("Content-Type".into(), "application/json".into())],
                    body: br#"{"name":"Ada","id":7}"#.to_vec(),
                }),
                "https://api.test/missing" => Ok(zgx_script::Response {
                    status: 404,
                    headers: vec![],
                    body: b"not here".to_vec(),
                }),
                _ => Err("offline".into()),
            }
        }
    });
    asked
}

const FETCH_APP: &str = r#"
local shown = ui.state("shown", "")
return ui.column {
    ui.text { id = "label", function() return shown:get() end },
    ui.button { id = "get", w = 40, h = 20, "g", on_click = function()
        ui.task(function()
            local user = ui.fetch("https://api.test/user"):await()
            local missing = ui.fetch("https://api.test/missing"):await()
            shown:set(`{user.status} {user.ok} {user.json.name} {user.headers["content-type"]} / {missing.status} {missing.ok} {missing.body}`)
        end)
    end },
    ui.button { id = "post", w = 40, h = 20, "p", on_click = function()
        ui.fetch("https://api.test/elsewhere", { method = "post", headers = { authorization = "x" }, body = { a = 1 } })
            :after(nil, function(e) shown:set(`post {e}`) end)
    end },
}
"#;

#[test]
fn fetch_goes_through_the_hosts_transport() {
    let mut asked = None;
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", FETCH_APP);
        asked = Some(fetching(&host));
        host
    });
    let asked = asked.unwrap();
    h.click("get");
    wait_for(
        &mut h,
        "label",
        "200 true Ada application/json / 404 false not here",
    );
    h.click("post");
    wait_for(&mut h, "label", "post offline");
    let post = asked.borrow().last().cloned().unwrap();
    assert_eq!(post.method, "POST");
    assert_eq!(post.body.as_deref(), Some(&br#"{"a":1}"#[..]));
    assert!(post.headers.contains(&("authorization".into(), "x".into())));
    assert!(
        post.headers
            .contains(&("content-type".into(), "application/json".into()))
    );
}

#[test]
fn without_a_fetcher_fetch_says_so() {
    let mut h = Harness::new(|runtime| ScriptHost::inline(runtime, "app", FETCH_APP));
    h.click("post");
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("the host has no fetcher"), "{error}");
}

const PLUGIN_APP: &str =
    r#"return ui.column { ui.text { "app", id = "app" }, ui.slot("bar", { id = "bar" }) }"#;
const FETCHING_PLUGIN: &str = r#"
local seen = ui.state("seen", "")
ui.fetch("https://api.test/user"):after(function(r) seen:set(r.json.name) end)
ui.contribute("bar", ui.text { id = "plugin", function() return seen:get() end })
"#;

#[test]
fn a_plugin_fetches_only_if_granted() {
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", PLUGIN_APP)
            .plugin("fetcher.luau", FETCHING_PLUGIN)
            .grant("fetcher.luau", zgx_script::Capability::Fetch);
        fetching(&host);
        host
    });
    wait_for(&mut h, "plugin", "Ada");

    let mut h = Harness::new(|runtime| {
        let host =
            ScriptHost::inline(runtime, "app", PLUGIN_APP).plugin("fetcher.luau", FETCHING_PLUGIN);
        fetching(&host);
        host
    });
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("only if the host grants it"), "{error}");
    // Left out, not the app.
    assert_eq!(h.text("app"), "app");
    assert!(h.find("plugin").is_none());
}

#[test]
fn a_broken_plugin_is_left_out_and_the_rest_loads() {
    let mut h = Harness::new(|runtime| {
        ScriptHost::inline(runtime, "app", PLUGIN_APP)
            .plugin("broken.luau", "error('plugin bug')")
            .plugin(
                "fine.luau",
                r#"ui.contribute("bar", ui.text { "fine", id = "fine" })"#,
            )
    });
    assert_eq!(h.text("app"), "app");
    assert_eq!(h.text("fine"), "fine");
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("plugin bug"), "{error}");
}

/// A 2×2 PNG.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 132,
    25, 96, 12, 0, 71, 202, 7, 249, 103, 89, 110, 183, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
    130,
];

#[test]
fn remote_images_and_svgs_load_once_through_the_fetcher() {
    use zgx::scene::NodeKind;
    let mut fetched = None;
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"return ui.column {
                ui.image { src = "https://img.test/avatar.png", id = "a", w = 20, h = 20 },
                ui.image { src = "https://img.test/avatar.png", id = "b", w = 20, h = 20 },
                ui.svg { src = "https://img.test/emoji.svg", id = "c", w = 20, h = 20 },
            }"#,
        );
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        fetched = Some(count.clone());
        host.fetcher(move |request: zgx_script::Request| {
            count.set(count.get() + 1);
            async move {
                zgx::timer::sleep(Duration::from_millis(2)).await;
                Ok(zgx_script::Response {
                    status: 200,
                    headers: vec![],
                    body: if request.url.ends_with(".png") {
                        PNG.to_vec()
                    } else {
                        br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#.to_vec()
                    },
                })
            }
        });
        host
    });
    let width = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            match scene.kind(n) {
                NodeKind::Image(image) => return Some(image.width()),
                NodeKind::Svg(_) => return Some(u32::MAX),
                _ => stack.extend(scene.children(n).iter().copied()),
            }
        }
        None
    };
    assert_eq!(width(&mut h, "a"), Some(1), "a placeholder while it comes");
    let deadline = Instant::now() + Duration::from_secs(5);
    while width(&mut h, "a") != Some(2) && Instant::now() < deadline {
        h.settle();
        h.ui.prepare_frame();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(width(&mut h, "a"), Some(2));
    assert_eq!(width(&mut h, "b"), Some(2));
    assert_eq!(
        fetched.unwrap().get(),
        2,
        "one fetch per URL: the image's and the SVG's"
    );
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn remote_images_show_their_fallback_until_they_come_and_if_they_fail() {
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"return ui.column {
                ui.image { src = "https://img.test/ok.png", id = "ok", w = 20, h = 20,
                    fallback = ui.text { "OK", id = "ok-initials" } },
                ui.image { src = "https://img.test/gone.png", id = "gone", w = 20, h = 20,
                    fallback = ui.text { "WL", id = "gone-initials" } },
            }"#,
        );
        host.fetcher(move |request: zgx_script::Request| async move {
            zgx::timer::sleep(Duration::from_millis(2)).await;
            if request.url.ends_with("gone.png") {
                return Ok(zgx_script::Response {
                    status: 404,
                    headers: vec![],
                    body: vec![],
                });
            }
            Ok(zgx_script::Response {
                status: 200,
                headers: vec![],
                body: PNG.to_vec(),
            })
        });
        host
    });
    // Both show their fallbacks while they load.
    assert!(h.find("ok-initials").is_some() && h.find("gone-initials").is_some());
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.find("ok-initials").is_some() && Instant::now() < deadline {
        h.settle();
        h.ui.prepare_frame();
        std::thread::sleep(Duration::from_millis(2));
    }
    for _ in 0..20 {
        h.settle();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.find("ok-initials").is_none(), "the picture once it came");
    assert!(
        h.find("gone-initials").is_some(),
        "the fallback when it failed"
    );
    assert_eq!(
        h.host.error().get(),
        None,
        "a failed picture is no script error"
    );
}

#[test]
fn a_failed_remote_image_is_fetched_again_when_shown_after_a_backoff() {
    let fetched = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = fetched.clone();
    let mut h = Harness::new(move |runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"local shown = ui.state("shown", true)
            return ui.column {
                ui.button { id = "toggle", on_click = function() shown:set(not shown:get()) end, "t" },
                ui.show(shown, ui.image { src = "https://img.test/flaky.png", w = 20, h = 20,
                    fallback = ui.text { "WL", id = "initials" } }),
            }"#,
        );
        host.fetcher(move |_| {
            count.set(count.get() + 1);
            let first = count.get() == 1;
            async move {
                zgx::timer::sleep(Duration::from_millis(2)).await;
                Ok(if first {
                    zgx_script::Response {
                        status: 503,
                        headers: vec![],
                        body: vec![],
                    }
                } else {
                    zgx_script::Response {
                        status: 200,
                        headers: vec![],
                        body: PNG.to_vec(),
                    }
                })
            }
        });
        host
    });
    let remount = |h: &mut Harness| {
        h.click("toggle");
        h.click("toggle");
        h.settle();
    };
    let wait = |h: &mut Harness, what: fn(&mut Harness) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !what(h) && Instant::now() < deadline {
            h.settle();
            h.ui.prepare_frame();
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    for _ in 0..30 {
        h.settle();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fetched.get(), 1);
    assert!(h.find("initials").is_some(), "the fallback when it failed");
    // Shown again within the backoff: not fetched again.
    remount(&mut h);
    assert_eq!(fetched.get(), 1, "no refetch within the backoff");
    std::thread::sleep(Duration::from_millis(5100));
    remount(&mut h);
    wait(&mut h, |h| h.find("initials").is_none());
    assert_eq!(fetched.get(), 2, "fetched again once the backoff passed");
    assert!(h.find("initials").is_none(), "the picture once it came");
}

const RESOURCE_APP: &str = r#"
local id = ui.state("id", 1)
local open = ui.state("open", true)
local user = ui.resource(function()
    local which = id:get()
    return ui.call("user", which):await()
end, { initial = "none" })
local Card = ui.component(function()
    local mine = ui.resource(function() return ui.call("user", 2):await() end)
    return ui.text { id = "card", function() return mine.status:get() end }
end)
return ui.column {
    ui.text { id = "shown", function()
        return `{user.status:get()} {user.loading:get()} {user.data:get()} {user.error:get() or "-"}`
    end },
    ui.button { id = "next", w = 40, h = 20, "n", on_click = function() id:add(1) end },
    ui.button { id = "reload", w = 40, h = 20, "r", on_click = user.reload },
    ui.button { id = "close", w = 40, h = 20, "c", on_click = function() open:set(false) end },
    ui.show(open, Card {}),
}
"#;

fn resources() -> Harness {
    Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", RESOURCE_APP);
        // The second user is slow to answer, the fourth is gone.
        host.async_action("user", |arg| async move {
            let n = arg.as_f64().unwrap_or(0.);
            let wait = if n == 2. { 40 } else { 2 };
            zgx::timer::sleep(Duration::from_millis(wait)).await;
            if n == 4. {
                return Err("gone".into());
            }
            Ok(Value::Str(format!("user {n}").into()))
        });
        host
    })
}

#[test]
fn a_resource_loads_fails_and_loads_again() {
    let mut h = resources();
    assert_eq!(h.text("shown"), "loading true none -");
    wait_for(&mut h, "shown", "ready false user 1 -");
    // Reloading keeps what it has, loading.
    h.click("reload");
    assert_eq!(h.text("shown"), "ready true user 1 -");
    wait_for(&mut h, "shown", "ready false user 1 -");
    h.click("next");
    h.click("next");
    h.click("next");
    wait_for(&mut h, "shown", "failed false user 1 gone");
    // Trying again shows it loading, then what it answered.
    h.click("reload");
    assert_eq!(h.text("shown"), "loading true user 1 gone");
    wait_for(&mut h, "shown", "failed false user 1 gone");
    assert_eq!(h.error(), None);
}

#[test]
fn a_resource_keyed_by_state_hears_only_its_last_load() {
    let mut h = resources();
    wait_for(&mut h, "shown", "ready false user 1 -");
    // The slow second user is asked for, then the third: the second's
    // answer, arriving last, is dropped.
    h.click("next");
    h.click("next");
    wait_for(&mut h, "shown", "ready false user 3 -");
    std::thread::sleep(Duration::from_millis(60));
    h.frames(3);
    assert_eq!(h.text("shown"), "ready false user 3 -");
    assert_eq!(h.error(), None);
}

#[test]
fn a_resource_whose_owner_has_gone_hears_nothing() {
    let mut h = resources();
    assert_eq!(h.text("card"), "loading");
    h.click("close");
    assert!(h.find("card").is_none());
    std::thread::sleep(Duration::from_millis(60));
    h.frames(3);
    let calls = h.calls();
    h.frames(10);
    assert_eq!(h.calls(), calls, "nothing runs while nothing changes");
    assert_eq!(h.error(), None);
}
