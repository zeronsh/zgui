mod common;

use common::Harness;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    input::{InputEvent, PointerButton},
    reactive::Runtime,
};
use zgx_script::{Limits, ScriptHost, Value};

const COUNTER: &str = r#"
local count = ui.state("count", 0)
return ui.column {
    p = 24, gap = 12,
    ui.text { "Count: {count}", id = "label" },
    ui.button { id = "inc", on_click = ui.add(count), w = 120, h = 40, "Increment" },
    ui.button { id = "double", w = 120, h = 40, "Double",
        on_click = function() count:update(function(n) return n * 2 end) end },
}
"#;

#[test]
fn templates_bind_identifiers_and_leave_other_braces_alone() {
    let mut h = Harness::inline(
        r#"local n = ui.state("n", 7)
        return ui.column {
            ui.text { "{n} of {{n}} in { braces } and {1} and {", id = "t" },
        }"#,
    );
    assert_eq!(h.text("t"), "7 of {n} in { braces } and {1} and {");
    let error = ScriptHost::inline(&Runtime::new(), "app", "return ui.text '{missing}'")
        .reload()
        .unwrap_err();
    assert!(error.contains("unknown state `missing`"), "{error}");
}

#[test]
fn templates_read_a_states_fields_after_a_dot() {
    let mut h = Harness::inline(
        r#"local user = ui.state("user", { name = "Ada", tags = { "math", "engines" } })
        ui.command { id = "rename", run = function() user:set_key("name", "Grace") end }
        return ui.column {
            ui.text { "{user.name} likes {user.tags.2}{user.missing}", id = "t" },
        }"#,
    );
    assert_eq!(h.text("t"), "Ada likes engines");
    assert!(h.host.run_command("rename"));
    h.settle();
    assert_eq!(h.text("t"), "Grace likes engines");
    // A Luau path is not a key: said so when the text is built.
    let error = ScriptHost::inline(
        &Runtime::new(),
        "app",
        r#"local state = { status = ui.state("status", "") }
        return ui.text { "{state.status}" }"#,
    )
    .reload()
    .unwrap_err();
    assert!(error.contains("unknown state `state`"), "{error}");
    assert!(
        error.contains("`{state.status}` is state `state`'s field `status`"),
        "{error}"
    );
}

#[test]
fn template_text_and_handlers_update_rust_state() {
    let mut h = Harness::inline(COUNTER);
    assert_eq!(h.text("label"), "Count: 0");
    h.click("inc");
    h.click("inc");
    assert_eq!(h.text("label"), "Count: 2");
    h.click("double");
    assert_eq!(h.text("label"), "Count: 4");
}

fn temp_script(name: &str, code: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-script-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.luau");
    std::fs::write(&path, code).unwrap();
    path
}
/// Rewrites a script with a later modification time than the last load saw.
fn rewrite(path: &std::path::Path, code: &str) {
    std::fs::write(path, code).unwrap();
    let later = std::time::SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(later)
        .unwrap();
}

#[test]
fn hot_reload_keeps_state_and_a_failed_reload_keeps_the_interface() {
    let path = temp_script("reload", COUNTER);
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, &path));
    h.click("inc");
    h.click("inc");
    h.click("inc");
    assert!(!h.host.reload_if_changed(), "nothing changed yet");

    rewrite(
        &path,
        &COUNTER.replace("Count: {count}", "Clicks so far: {count}"),
    );
    assert!(h.host.reload_if_changed());
    h.settle();
    assert_eq!(
        h.text("label"),
        "Clicks so far: 3",
        "state survives the reload"
    );
    assert_eq!(h.host.error().get(), None);

    rewrite(
        &path,
        "return ui.column { ui.text { 'oops', nonsense = 1 } }",
    );
    assert!(h.host.reload_if_changed());
    let error = h.host.error().get().expect("the failed load is reported");
    assert!(error.contains("unknown property `nonsense`"), "{error}");
    assert!(
        error.starts_with("app.luau: "),
        "names the file, not its path: {error}"
    );
    assert!(
        error.contains("app.luau:1"),
        "keeps the script's own frames: {error}"
    );
    assert!(!error.contains("[C]"), "drops Rust frames: {error}");
    assert_eq!(
        h.text("label"),
        "Clicks so far: 3",
        "the previous interface stays mounted"
    );
    h.click("inc");
    assert_eq!(h.text("label"), "Clicks so far: 4", "and keeps working");

    rewrite(&path, COUNTER);
    assert!(h.host.reload_if_changed());
    assert_eq!(h.host.error().get(), None, "a good load clears the error");
    assert_eq!(h.text("label"), "Count: 4");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn runaway_scripts_hit_their_time_budget() {
    let runtime = Runtime::new();
    let limits = Limits {
        load: Duration::from_millis(30),
        ..Limits::default()
    };
    // A bare loop, one in a coroutine, and one calling the host: each is
    // stopped once its budget runs out, not long after.
    for code in [
        "while true do end",
        "coroutine.wrap(function() while true do end end)()",
        "local n = ui.state('n', 0) while true do n:set(n:peek() + 1) end",
        "while true do pcall(function() while true do end end) end",
    ] {
        let host = ScriptHost::inline(&runtime, "spin", code).limits(limits);
        let started = Instant::now();
        let error = host.reload().unwrap_err();
        assert!(error.contains("time budget"), "{code}: {error}");
        let spent = started.elapsed();
        assert!(
            spent < Duration::from_millis(500),
            "{code}: stopped after {spent:?}"
        );
    }

    // A handler that spins is stopped too, and the interface keeps running.
    let mut h = Harness::inline(
        r#"local n = ui.state("n", 0)
        return ui.column {
            ui.button { id = "spin", w = 80, h = 30, on_click = function() n:set(1); while true do end end },
            ui.text { "{n}", id = "n" },
        }"#,
    );
    h.click("spin");
    let error = h.host.error().get().unwrap();
    assert!(error.contains("time budget"), "{error}");
    assert_eq!(h.text("n"), "1", "writes before the error stand");
}

#[test]
fn a_script_short_of_its_budget_is_never_stopped() {
    // Budgets start from a CPU reading up to 1 ms old, so a script is
    // stopped up to 1 ms short of its budget at most: one running 5 ms
    // short finishes, every time.
    let runtime = Runtime::new();
    let limits = Limits {
        load: Duration::from_millis(30),
        ..Limits::default()
    };
    for _ in 0..10 {
        let host = ScriptHost::inline(
            &runtime,
            "busy",
            "local start = os.clock() while os.clock() - start < 0.025 do end return ui.text { 'done' }",
        )
        .limits(limits);
        host.reload().expect("25 ms of a 30 ms budget");
    }
}

#[test]
fn what_a_selection_lays_out_again_is_not_the_scripts_to_pay_for() {
    // Selecting a long document to its end shows its last lines, coloured
    // by a highlighter that has to run over every line before them: the
    // editor's work, as typing there would be, not the handler's (the code
    // editor's Replace All selected all of 100,000 lines and was stopped).
    let limits = Limits {
        call: Duration::from_millis(20),
        ..Limits::default()
    };
    let mut h = Harness::new(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r##"local text = ui.state("text", "")
            local el = ui.ref()
            -- 20,000 lines the editor has not coloured yet, made on load.
            local documents = {}
            for _, name in { "bytes", "chars", "all" } do
                local lines = table.create(20000)
                for i = 1, 20000 do
                    lines[i] = `local {name}_{i} = {i}`
                end
                documents[name] = table.concat(lines, "\n")
            end
            local function open(name)
                text:set(documents[name])
            end
            -- Script that runs on, where a budget passed is found.
            local function after()
                for _ = 1, 1000 do end
            end
            -- A token a character, as a tokenizer colours code.
            local spans = {}
            for i = 1, 20 do
                spans[i] = { i, i, if i % 2 == 0 then "#ff0000" else "#00ff00" }
            end
            return ui.column {
                ui.button { id = "bytes", w = 80, h = 20, on_click = function()
                    open("bytes")
                    el:get():set_selection_bytes(0, #text:peek())
                    after()
                end },
                ui.button { id = "chars", w = 80, h = 20, on_click = function()
                    open("chars")
                    el:get():set_selection(0, #text:peek())
                    after()
                end },
                ui.button { id = "all", w = 80, h = 20, on_click = function()
                    open("all")
                    el:get():select_all()
                    after()
                end },
                ui.textarea { ref = el, value = text, w = 300, h = 200, line_numbers = true,
                    highlight = function(line, state)
                        return spans, state
                    end },
            }"##,
        )
        .limits(limits)
    });
    for button in ["bytes", "chars", "all"] {
        h.click(button);
        assert_eq!(h.host.error().get(), None, "{button}");
    }
}

#[test]
fn nodes_a_script_keeps_mount_again_after_collections() {
    // Nodes made once and kept in variables are shown, hidden and shown
    // again, with garbage collected while they are away: a node lives as
    // long as the script or the interface holds it.
    let mut h = Harness::inline(
        r#"local open = ui.state("open", true)
        local kept = ui.text { "kept", id = "kept" }
        local rows = {}
        for i = 1, 3 do rows[i] = ui.text { "row " .. i, id = "row" .. i } end
        return ui.column {
            ui.show(function() return open:get() end, kept),
            ui.show(function() return open:get() end, ui.column { rows }),
            ui.button { id = "toggle", w = 4, h = 4, on_click = function()
                for _ = 1, 5000 do local _ = { 1, 2, 3 } end
                open:set(not open:get())
            end },
        }"#,
    );
    for _ in 0..3 {
        assert_eq!(h.text("kept"), "kept");
        assert_eq!(h.text("row3"), "row 3");
        h.click("toggle");
        assert!(h.find("kept").is_none(), "hidden");
        h.click("toggle");
    }
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn garbage_from_builtins_is_collected_before_the_memory_limit() {
    // `table.clone` allocates without giving the collector a turn, and
    // setting an equal value allocates nothing in Lua: before, this filled
    // the VM with garbage until the limit failed the script (40 KB a turn,
    // 10 MB here).
    let limits = Limits {
        memory: 4 << 20,
        load: Duration::from_secs(20),
        ..Limits::default()
    };
    let runtime = Runtime::new();
    let churn = ScriptHost::inline(
        &runtime,
        "churn",
        r#"local big = {}
        for i = 1, 1000 do big["k" .. i] = i end
        local map = ui.state("map", big)
        for i = 1, 250 do map:set(table.clone(map:peek())) end
        return ui.div {}"#,
    )
    .limits(limits);
    churn.reload().expect("garbage is collected");
    // What a script keeps still meets the limit.
    let keep = ScriptHost::inline(
        &runtime,
        "keep",
        r#"local big = {}
        for i = 1, 1000 do big["k" .. i] = i end
        local kept = {}
        for i = 1, 1000 do kept[i] = table.clone(big) end
        return ui.div {}"#,
    )
    .limits(limits);
    let error = keep.reload().unwrap_err();
    assert!(error.contains("memory"), "{error}");
    assert!(
        error.contains("Limits::memory"),
        "the error names the limit to raise: {error}"
    );
}

#[test]
fn copies_of_state_meet_the_memory_limit() {
    // A state's records are built for the script through Luau's C API, in
    // one protected call: reaching the limit there is still the script's
    // memory error.
    let runtime = Runtime::new();
    let limits = Limits {
        memory: 4 << 20,
        load: Duration::from_secs(20),
        ..Limits::default()
    };
    let host = ScriptHost::inline(
        &runtime,
        "copies",
        r#"local rows = {}
        for i = 1, 2000 do rows[i] = { name = "row " .. i, n = i } end
        local s = ui.state("rows", rows)
        local kept = {}
        for i = 1, 1000 do
            s:update(function(list) kept[i] = list; return list end)
        end
        return ui.div {}"#,
    )
    .limits(limits);
    let error = host.reload().unwrap_err();
    assert!(error.contains("memory"), "{error}");
}

#[test]
fn a_script_keeping_nodes_meets_the_memory_limit() {
    // Nodes are made with the limit set aside for their one allocation
    // (vendor/mlua/ZGX_PATCH.md): a script that keeps making them still
    // fails at the limit, not past it.
    let runtime = Runtime::new();
    let limits = Limits {
        memory: 4 << 20,
        load: Duration::from_secs(20),
        ..Limits::default()
    };
    let host = ScriptHost::inline(
        &runtime,
        "nodes",
        r#"local kept = {}
        for i = 1, 1000000 do kept[i] = ui.div { w = 1 } end
        return ui.div {}"#,
    )
    .limits(limits);
    let error = host.reload().unwrap_err();
    assert!(error.contains("memory"), "{error}");
}

#[test]
fn scripts_are_sandboxed() {
    let runtime = Runtime::new();
    for code in [
        "ui.text = nil return ui.div {}",
        "string.len = nil return ui.div {}",
    ] {
        let error = ScriptHost::inline(&runtime, "app", code)
            .reload()
            .unwrap_err();
        assert!(error.contains("readonly"), "{code}: {error}");
    }
}

#[test]
fn tweens_run_in_rust_and_stop_requesting_frames_at_rest() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", false)
        return ui.column {
            ui.button { id = "toggle", w = 80, h = 30, on_click = ui.toggle(open) },
            ui.div { id = "panel", w = 50, h = 50, opacity = ui.tween(open, { duration = 100, from = 0.2, to = 1 }) },
        }"#,
    );
    assert!((h.opacity("panel") - 0.2).abs() < 1e-6, "starts at rest");
    assert!(!h.clock.wants_frame());

    h.click("toggle");
    assert!(h.clock.wants_frame(), "a change starts the animation");
    let mut seen = Vec::new();
    for _ in 0..10 {
        h.frame();
        seen.push(h.opacity("panel"));
    }
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert!(seen[1] > 0.2 && seen[1] < 1., "{seen:?}");
    assert_eq!(*seen.last().unwrap(), 1.);
    assert!(
        !h.clock.wants_frame(),
        "settled animations request no frames"
    );
}

#[test]
fn springs_settle_on_their_target() {
    let mut h = Harness::inline(
        r#"local x = ui.state("x", 0)
        return ui.column {
            ui.button { id = "go", w = 80, h = 30, on_click = ui.set(x, 100) },
            ui.div { id = "dot", w = 10, h = 10, opacity = ui.spring(x, { from = 0, to = 1 }) },
        }"#,
    );
    h.click("go");
    let mut frames = 0;
    while h.clock.wants_frame() {
        h.frame();
        frames += 1;
        assert!(frames < 600, "spring never settled");
    }
    assert!(frames > 5);
    // Opacity clamps at 1; the spring's scalar settled at 100.
    assert_eq!(h.opacity("dot"), 1.);
}

#[test]
fn plugins_contribute_to_slots_and_read_shared_state() {
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"return ui.column { ui.text { "app", id = "app" }, ui.slot("toolbar", { id = "toolbar" }) }"#,
        )
        .plugin(
            "greeter",
            r#"ui.contribute("toolbar", ui.text { "Hello, {user}", id = "greeting" })"#,
        );
        host.shared("user", "Ada");
        // Plugins reach the shared state the app exposes to them.
        host.expose_shared("user", None);
        host
    });
    assert_eq!(h.text("greeting"), "Hello, Ada");
    h.host.shared("user", Value::Nil).set("Grace".into());
    assert_eq!(
        h.text("greeting"),
        "Hello, Grace",
        "Rust writes reach plugin templates"
    );
}

#[test]
fn host_actions_and_lists() {
    let mut h = Harness::inline(
        r#"local items = ui.state("items", {})
        local draft = ui.state("draft", "")
        return ui.column {
            ui.button { id = "add", w = 80, h = 30, on_click = function()
                items:push("item " .. (items:len() + 1))
            end },
            ui.button { id = "save", w = 80, h = 30, on_click = ui.action("save", "now") },
            ui.text { function() return items:len() .. " items" end, id = "count" },
            ui.each(items, function(item, i) return ui.text { item, id = "row" .. i } end, { id = "list" }),
        }"#,
    );
    let saved = Rc::new(RefCell::new(Vec::new()));
    {
        let saved = saved.clone();
        h.host
            .action("save", move |arg| saved.borrow_mut().push(arg));
    }
    assert_eq!(h.text("count"), "0 items");
    h.click("add");
    h.click("add");
    assert_eq!(h.text("count"), "2 items");
    assert_eq!(h.text("row2"), "item 2");
    h.click("save");
    assert_eq!(*saved.borrow(), [Value::from("now")]);
}

#[test]
fn the_hot_reload_example_loads() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hot_reload");
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::new(runtime, dir.join("app.luau")).plugins(dir.join("plugins"));
        host.shared("window", "820×560");
        // What hot_reload.rs exposes to its plugins.
        host.expose_shared("window", None);
        host.expose_shared("count", None);
        host.expose_action("quit", None);
        host
    });
    assert_eq!(h.host.error().get(), None);
    assert!(h.find("missing").is_none());
    assert!(
        h.ui.scene.borrow().len() > 40,
        "the app and both plugins mounted"
    );
}

#[test]
fn errors_show_in_a_banner_inside_the_window() {
    let mut h = Harness::inline(COUNTER);
    h.click("inc");
    let error = h.host.error();
    error.set(Some("app: something broke".into()));
    let node = h.find("script-error").expect("banner text mounted");
    let scene = h.ui.scene.borrow();
    let b = scene.bounds(node);
    eprintln!("banner text bounds {b:?}");
    assert!(b.width > 100. && b.height > 5., "{b:?}");
    assert!(b.y > 0. && b.y + b.height <= 300., "{b:?}");
}

#[test]
fn scroll_offsets_bind_to_state_and_report_clamping() {
    let mut h = Harness::inline(
        r#"local offset = ui.state("offset", 0)
        local rows = {}
        for i = 1, 40 do rows[i] = ui.div { h = 20, w = 100 } end
        return ui.column {
            ui.scroll { offset = offset, h = 100, w = 200, id = "list", ui.column { rows } },
            ui.button { id = "end", w = 80, h = 30, on_click = ui.set(offset, 1e9) },
            ui.text { "{offset}", id = "at" },
        }"#,
    );
    assert_eq!(h.text("at"), "0");
    h.click("end");
    h.ui.prepare_frame();
    let at: f64 = h.text("at").parse().unwrap();
    assert!(
        (at - 700.).abs() < 1.,
        "clamped to content (800) minus viewport (100): {at}"
    );
}

#[test]
fn on_press_runs_on_pointer_down_before_release() {
    let mut h = Harness::inline(
        r#"local pressed = ui.state("pressed", 0)
        return ui.column {
            ui.div { id = "strip", w = 200, h = 40, fade = { 8, 8 }, on_press = ui.add(pressed) },
            ui.text { "{pressed}", id = "n" },
        }"#,
    );
    let node = h.find("strip").unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    let (x, y) = (b.x + 10., b.y + 10.);
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    assert_eq!(h.text("n"), "1", "before release");
    h.ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
    assert_eq!(h.text("n"), "1", "release does not run it again");
}

#[test]
fn scripts_require_modules_and_reload_when_they_change() {
    let dir = std::env::temp_dir().join(format!("zgx-require-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/theme.luau"), r#"return { label = "first" }"#).unwrap();
    std::fs::write(
        dir.join("app.luau"),
        r#"local theme = require("lib/theme")
        assert(require("lib/theme") == theme, "a module runs once")
        return ui.text { theme.label, id = "t" }"#,
    )
    .unwrap();
    let path = dir.join("app.luau");
    let mut h = common::Harness::new(|runtime| zgx_script::ScriptHost::new(runtime, path));
    assert_eq!(h.text("t"), "first");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(dir.join("lib/theme.luau"), r#"return { label = "second" }"#).unwrap();
    assert!(
        h.host.reload_if_changed(),
        "a required file's change reloads"
    );
    h.settle();
    assert_eq!(h.text("t"), "second");
    std::fs::write(
        dir.join("app.luau"),
        r#"return ui.text { require("../escape"), id = "t" }"#,
    )
    .unwrap();
    assert!(
        h.host.reload().is_err(),
        "modules stay inside the script's directory"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn shader_compile_errors_are_script_errors() {
    let runtime = zgx::reactive::Runtime::new();
    let host = ScriptHost::inline(
        &runtime,
        "app",
        "local s = ui.shader(\"fn fill(input: FillInput) -> vec4<f32> {\\n  let from = 1.0;\\n  return vec4(from);\\n}\")\nreturn ui.div { bg_shader = s }",
    );
    let error = host.reload().unwrap_err();
    assert!(
        error.contains("ui.shader") && error.contains("line 2"),
        "{error}"
    );
}

#[test]
fn modules_resolve_like_luau_and_keep_their_own_state() {
    let dir = std::env::temp_dir().join(format!("zgx-modules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ui/components")).unwrap();
    std::fs::create_dir_all(dir.join("ui/lib")).unwrap();
    let write = |path: &str, code: &str| std::fs::write(dir.join(path), code).unwrap();
    write(
        ".luaurc",
        r##"{ "aliases": { "components": "./ui/components" } }"##,
    );
    write(
        "ui/components/theme.luau",
        r##"return { accent = "#8b7cf6" }"##,
    );
    write(
        "ui/components/card.luau",
        r##"local theme = require("./theme")
        assert(require("@self/theme") == theme, "@self is the module's directory")
        local open = ui.state("open", "card")
        return function(label) return ui.text { label .. " " .. open:get() .. " " .. theme.accent, id = "card" } end"##,
    );
    write("ui/lib/helper.luau", r##"return "helper""##);
    write(
        "ui/lib/init.luau",
        r##"-- An init.luau is its directory as a module: `./` is beside the
        -- directory, `@self` inside it.
        assert(require("./components/theme").accent == "#8b7cf6", "./ from an init.luau")
        assert(require("@self/helper") == "helper", "@self from an init.luau")
        local open = ui.state("open", "lib") return { open = open }"##,
    );
    write(
        "app.luau",
        r##"local card = require("@components/card")
        local lib = require("./ui/lib")
        local open = ui.state("open", "app")
        return ui.column {
            card("hi"),
            ui.text { open:get() .. " " .. lib.open:get(), id = "keys" },
        }"##,
    );
    let path = dir.join("app.luau");
    let mut h = common::Harness::new(|runtime| zgx_script::ScriptHost::new(runtime, path));
    assert_eq!(
        h.text("card"),
        "hi card #8b7cf6",
        "a module's own state and a sibling module"
    );
    assert_eq!(h.text("keys"), "app lib", "each module's `open` is its own");
    write(
        "app.luau",
        r##"return ui.text { tostring(require("../outside")), id = "t" }"##,
    );
    let error = h.host.reload().unwrap_err();
    assert!(error.contains("directory or below"), "{error}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn text_templates_in_a_module_name_its_own_state() {
    let dir = std::env::temp_dir().join(format!("zgx-module-templates-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    let write = |path: &str, code: &str| std::fs::write(dir.join(path), code).unwrap();
    // The module's `count` is its own (`ui/counter.luau::count`); its
    // templates find it, before the script's state of the same name.
    write(
        "ui/counter.luau",
        r##"local count = ui.state("count", 7)
        local saved = ui.persist("saved", "kept")
        return function()
            return ui.column {
                ui.text { "Count: {count}", id = "module" },
                ui.text { "{saved}", id = "saved" },
                ui.button { id = "add", on_click = ui.add(count), "+" },
            }
        end"##,
    );
    write(
        "app.luau",
        r##"local count = ui.state("count", 1)
        local counter = require("./ui/counter")
        return ui.column { counter(), ui.text { "App: {count}", id = "app" } }"##,
    );
    let path = dir.join("app.luau");
    let mut h = common::Harness::new(|runtime| zgx_script::ScriptHost::new(runtime, path));
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.text("module"), "Count: 7");
    assert_eq!(h.text("saved"), "kept");
    assert_eq!(h.text("app"), "App: 1");
    h.click("add");
    assert_eq!(h.text("module"), "Count: 8", "the template follows it");
    assert_eq!(h.text("app"), "App: 1");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn editing_an_image_file_reloads_it() {
    let path = temp_script(
        "asset_reload",
        r#"return ui.column { ui.svg { id = "icon", src = "icon.svg", w = 20, h = 20 } }"#,
    );
    let icon = path.with_file_name("icon.svg");
    let square = |fill: &str| {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="{fill}"/></svg>"#
        )
    };
    rewrite(&icon, &square("red"));
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, &path));
    let svg = |h: &mut Harness| {
        let node = h.find("icon").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Svg(svg) = scene.kind(n) {
                return svg.clone();
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no svg under `icon`")
    };
    let red = svg(&mut h);
    assert!(!h.host.reload_if_changed(), "nothing changed yet");
    rewrite(&icon, &square("blue"));
    assert!(h.host.reload_if_changed(), "the edited image is seen");
    h.settle();
    let blue = svg(&mut h);
    assert!(
        !std::sync::Arc::ptr_eq(&red, &blue),
        "read again, not the cached copy"
    );
    assert!(!h.host.reload_if_changed(), "and seen once");
}
