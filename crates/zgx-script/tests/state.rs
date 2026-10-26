//! State updates (`set_key`, `patch`, `update`, `insert`), read-only state
//! values, elements in state and trackable refs, `ui.persist`, and the
//! duration warnings.
mod common;

use common::Harness;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::reactive::Runtime;
use zgx_script::{ScriptHost, Value};

/// A command's handler, run as a menu would.
fn run(h: &mut Harness, id: &str) {
    assert!(h.host.run_command(id), "no command `{id}`");
    h.settle();
}

#[test]
fn a_set_key_path_is_read_whole_and_a_wrong_step_is_named() {
    let mut h = Harness::inline(
        r#"local rows = ui.state("rows", { { name = "a", tags = { "x", "y" } } })
        local said = ui.state("said", "")
        ui.command { id = "write", run = function()
            rows:set_key({ 1, "tags", 2 }, "z")
            rows:set_key({ 1.0, "name" }, "b")
            local ok, err = pcall(function() rows:set_key({ 1, true }, 1) end)
            local ok2, err2 = pcall(function() rows:set_key({ 0, "name" }, 1) end)
            local row = rows:peek()[1]
            said:set(`{row.name} {row.tags[2]} {ok} {tostring(err):match("not boolean") ~= nil} {ok2} {tostring(err2):match("not 0") ~= nil}`)
        end }
        return ui.text { id = "said", "{said}" }"#,
    );
    run(&mut h, "write");
    assert_eq!(h.error(), None);
    assert_eq!(h.text("said"), "b z false true false true");
}

#[test]
fn set_key_patch_update_and_insert_change_state() {
    let mut h = Harness::inline(
        r#"local doc = ui.state("doc", { tabs = { { id = "a", title = "A" }, { id = "b", title = "B" } }, active = "a" })
        local list = ui.state("list", { 1, 2, 3 })
        local runs = ui.state("runs", 0)
        ui.effect(function() doc:get() runs:set(runs:peek() + 1) end)
        local function cmd(id, run) ui.command { id = id, run = run } end
        cmd("title", function() doc:set_key({ "tabs", 2, "title" }, "Bee") end)
        cmd("same", function() doc:set_key({ "tabs", 2, "title" }, "Bee") end)
        cmd("key", function() doc:set_key("active", "b") end)
        cmd("drop", function() doc:set_key("active", nil) end)
        cmd("patch", function() doc:patch({ active = "c", extra = true }) end)
        cmd("mutate", function() doc:update(function(t) t.tabs[1].title = "Ay" end) end)
        cmd("replace", function() list:update(function(t) return { #t } end) end)
        cmd("insert", function() list:insert(1, 0) list:insert(3, 9) end)
        return ui.column {
            ui.text { id = "doc", function()
                local d = doc:get()
                return `{d.tabs[1].title} {d.tabs[2].title} {d.active or "-"} {d.extra and "x" or ""}`
            end },
            ui.text { id = "list", function() return table.concat(list:get(), ",") end },
            ui.text { id = "runs", "{runs}" },
        }"#,
    );
    assert_eq!(h.text("doc"), "A B a ");
    assert_eq!(h.text("runs"), "1");
    run(&mut h, "title");
    assert_eq!(h.text("doc"), "A Bee a ");
    assert_eq!(h.text("runs"), "2");
    run(&mut h, "same");
    assert_eq!(h.text("runs"), "2", "an equal write notifies nobody");
    run(&mut h, "key");
    assert_eq!(h.text("doc"), "A Bee b ");
    run(&mut h, "drop");
    assert_eq!(h.text("doc"), "A Bee - ", "nil removes the key");
    run(&mut h, "patch");
    assert_eq!(h.text("doc"), "A Bee c x");
    run(&mut h, "mutate");
    assert_eq!(
        h.text("doc"),
        "Ay Bee c x",
        "update commits the mutated copy"
    );
    run(&mut h, "insert");
    assert_eq!(h.text("list"), "0,1,9,2,3");
    run(&mut h, "replace");
    assert_eq!(h.text("list"), "5", "a returned value replaces the state");
}

#[test]
fn updates_share_what_they_do_not_touch() {
    let h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"local doc = ui.shared("doc", nil)
            ui.command { id = "edit", run = function() doc:set_key({ "tabs", 2, "title" }, "x") end }
            ui.command { id = "reset", run = function() doc:set(doc:peek()) end }
            return ui.div {}"#,
        );
        let tab = |id: &str| {
            Value::Map(
                [
                    ("id".into(), Value::from(id)),
                    ("title".into(), Value::from(id)),
                ]
                .into(),
            )
        };
        host.shared(
            "doc",
            Value::Map(
                [(
                    "tabs".into(),
                    Value::List(vec![tab("a"), tab("b"), tab("c")].into()),
                )]
                .into(),
            ),
        );
        host
    });
    let doc = h.host.shared("doc", Value::Nil);
    let tabs = |v: &Value| v.field("tabs").unwrap().list().to_vec();
    let before = tabs(&doc.with_untracked(Clone::clone));
    assert!(h.host.run_command("edit"));
    let after = tabs(&doc.with_untracked(Clone::clone));
    let same = |a: &Value, b: &Value| match (a, b) {
        (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
        _ => false,
    };
    assert!(same(&before[0], &after[0]) && same(&before[2], &after[2]));
    assert!(!same(&before[1], &after[1]));
    assert_eq!(after[1].field("title"), Some(&Value::from("x")));
    // Writing back what was read keeps the very same value.
    let value = doc.with_untracked(Clone::clone);
    assert!(h.host.run_command("reset"));
    let Value::Map(kept) = doc.with_untracked(Clone::clone) else {
        panic!("a map")
    };
    let Value::Map(value) = value else {
        panic!("a map")
    };
    assert!(Rc::ptr_eq(&kept, &value));
}

#[test]
fn republished_values_keep_what_did_not_change_and_its_tables() {
    // A model Rust publishes whole each time (a transcript as a turn
    // streams): the rows that did not change stay the same values, and
    // scripts get the same tables for them, converted once.
    let mut h = Harness::new(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r#"local doc = ui.shared("doc", nil)
            local picked = ui.shared("picked", nil)
            local seen = ui.state("seen", "")
            local prev = nil
            ui.effect(function()
                local d = doc:get()
                if d == nil then return end
                local row
                for _, r in d.rows do
                    if r.id == "b" then row = r end
                end
                local word = if prev == nil then "new" elseif rawequal(prev, row) then "same" else "changed"
                seen:set(seen:peek() .. word .. " ")
                prev = row
                -- A row read is its value again, written back.
                picked:set(row)
            end)
            return ui.text { id = "seen", "{seen}" }"#,
        )
    });
    let row = |id: &str, text: &str| {
        // Large enough to be converted once and kept (16 values deep).
        let words: Vec<Value> = (0..16)
            .map(|i| Value::from(format!("{text}{i}").as_str()))
            .collect();
        Value::Map(
            [
                ("id".into(), Value::from(id)),
                ("words".into(), Value::List(words.into())),
            ]
            .into(),
        )
    };
    let doc_of = |rows: Vec<Value>| Value::Map([("rows".into(), Value::List(rows.into()))].into());
    let doc = h.host.shared("doc", Value::Nil);
    let picked = h.host.shared("picked", Value::Nil);
    let rows_of = |v: &Value| v.field("rows").unwrap().list().to_vec();
    let ptr = |v: &Value| match v {
        Value::Map(m) => Rc::as_ptr(m) as *const u8 as usize,
        _ => 0,
    };
    doc.set(doc_of(vec![row("a", "x"), row("b", "y")]));
    h.settle();
    let first = rows_of(&doc.with_untracked(Clone::clone));
    // All new values, equal to the old: nothing changes at all.
    assert!(!doc.set(doc_of(vec![row("a", "x"), row("b", "y")])));
    // A row inserted above: `b` is matched by its id and kept.
    doc.set(doc_of(vec![row("z", "w"), row("a", "x"), row("b", "y")]));
    h.settle();
    let second = rows_of(&doc.with_untracked(Clone::clone));
    assert_eq!(ptr(&second[1]), ptr(&first[0]), "`a` kept");
    assert_eq!(ptr(&second[2]), ptr(&first[1]), "`b` kept");
    assert_eq!(
        ptr(&picked.with_untracked(Clone::clone)),
        ptr(&first[1]),
        "the table read is `b` again"
    );
    doc.set(doc_of(vec![
        row("z", "w"),
        row("a", "x"),
        row("b", "changed"),
    ]));
    h.settle();
    assert_eq!(h.text("seen"), "new same changed ");
}

#[test]
fn state_values_are_read_only_and_say_how_to_change_them() {
    let h = Harness::inline(
        r#"local doc = ui.state("doc", { a = 1 })
        ui.command { id = "poke", run = function() doc:get().a = 2 end }
        return ui.div {}"#,
    );
    assert!(h.host.run_command("poke"));
    let error = h.host.error().get().expect("modifying a state value fails");
    assert!(error.contains("readonly"), "{error}");
    assert!(error.contains("state:set_key"), "{error}");
}

#[test]
fn state_method_errors_are_clear() {
    for (code, expected) in [
        (
            r#"ui.state("n", 1):set_key("x", 1)"#,
            "only tables have keys",
        ),
        (r#"ui.state("l", { 1 }):set_key(4, 1)"#, "outside the list"),
        (
            r#"ui.state("l", { 1, 2 }):set_key(1, nil)"#,
            "state:remove(1)",
        ),
        (r#"ui.state("l", { 1 }):insert(3, 1)"#, "outside the list"),
        (r#"ui.state("m", { a = 1 }):push(1)"#, "not a list"),
        (r#"ui.state("m", {}):patch({ 1 })"#, "string keys"),
    ] {
        let error = ScriptHost::inline(
            &Runtime::new(),
            "app",
            format!("{code}\nreturn ui.div {{}}"),
        )
        .reload()
        .unwrap_err();
        assert!(error.contains(expected), "{code}: {error}");
    }
}

#[test]
fn refs_are_tracked_as_their_elements_mount_and_unmount() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", false)
        local r = ui.ref()
        local seen = ui.state("seen", "")
        ui.effect(function()
            local el = r:get()
            seen:set(seen:peek() .. (if el then el.id else "nil") .. ";")
        end)
        local count = ui.computed(function() return r.count end)
        return ui.column {
            ui.button { id = "toggle", w = 40, h = 20, on_click = ui.toggle(open) },
            ui.show(open, ui.div { id = "box", ref = r, w = 10, h = 10 }),
            ui.text { id = "seen", "{seen}" },
            ui.text { id = "count", function() return tostring(count:get()) end },
        }"#,
    );
    assert_eq!(h.text("seen"), "nil;");
    h.click("toggle");
    assert_eq!(h.text("seen"), "nil;box;", "mounting notifies");
    assert_eq!(h.text("count"), "1");
    h.click("toggle");
    assert_eq!(h.text("seen"), "nil;box;nil;", "unmounting notifies");
    assert_eq!(h.text("count"), "0");
}

#[test]
fn elements_can_be_kept_in_state() {
    let mut h = Harness::inline(
        r#"local Card = ui.component(function()
            local held = ui.use_state(nil)
            return ui.column {
                ui.div { id = "inner", w = 10, h = 10, on_mount = function(el) held:set(el) end },
                ui.text { id = "held", function()
                    local el = held:get()
                    return if el then `{el.id} {el.mounted}` else "none"
                end },
            }
        end)
        local first = ui.state("first", nil)
        return ui.column {
            Card {},
            ui.div { id = "outer", w = 5, h = 5, on_mount = function(el) first:set(el) end },
            ui.text { id = "first", function()
                local el = first:get()
                return if el then el.tag else "none"
            end },
        }"#,
    );
    assert_eq!(h.text("held"), "inner true");
    assert_eq!(h.text("first"), "div");
    // Elements are opaque to Rust and JSON.
    assert_eq!(
        Value::Map([("el".into(), Value::Nil)].into()).to_json(),
        r#"{"el":null}"#
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-persist-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
const COUNTER: &str = r#"local n = ui.persist("n", 0)
ui.command { id = "inc", run = function() n:add(1) end }
return ui.text { id = "n", "{n}" }"#;
fn persisted(dir: &Path, script: &str, code: &str) -> Harness {
    let dir = dir.to_owned();
    let (script, code) = (script.to_owned(), code.to_owned());
    Harness::new(move |runtime| ScriptHost::inline(runtime, script, code).storage(dir))
}
/// Waits for the writer thread to put `path` on disk.
fn wait_for(path: &Path) -> String {
    let start = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            return text;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{} never written",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn persisted_state_is_saved_and_restored() {
    let dir = temp_dir("restore");
    let file = dir.join("app/n.json");
    {
        let mut h = persisted(&dir, "app", COUNTER);
        assert_eq!(h.text("n"), "0");
        assert!(!file.exists(), "the initial value is not written");
        run(&mut h, "inc");
        run(&mut h, "inc");
        h.host.save();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "2");
        assert!(
            std::fs::read_dir(dir.join("app")).unwrap().count() == 1,
            "no temporary file is left"
        );
    }
    let mut h = persisted(&dir, "app", COUNTER);
    assert_eq!(h.text("n"), "2", "the next run starts from what was saved");
    // A file that is not JSON starts from the initial value, with a warning.
    std::fs::write(&file, "{ not json").unwrap();
    let mut h = persisted(&dir, "app", COUNTER);
    assert_eq!(h.text("n"), "0");
    assert!(
        h.host.warnings().iter().any(|w| w.contains("is not JSON")),
        "{:?}",
        h.host.warnings()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persisted_writes_are_debounced_off_the_main_thread() {
    let dir = temp_dir("debounce");
    let file = dir.join("app/n.json");
    let mut h = persisted(&dir, "app", COUNTER);
    for _ in 0..5 {
        run(&mut h, "inc");
    }
    assert!(
        !file.exists(),
        "nothing is written until the changes settle"
    );
    std::thread::sleep(Duration::from_millis(300));
    h.settle();
    assert_eq!(wait_for(&file), "5", "one write, of the last value");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persisted_state_survives_hot_reload_without_reading_again() {
    let dir = temp_dir("reload");
    let script_dir = dir.join("src");
    std::fs::create_dir_all(&script_dir).unwrap();
    let path = script_dir.join("app.luau");
    // A module's keys are its own, as with ui.state.
    std::fs::write(
        script_dir.join("panel.luau"),
        r#"return ui.persist("n", 100)"#,
    )
    .unwrap();
    let code = r#"local n = ui.persist("n", 0)
local m = require("./panel")
ui.command { id = "inc", run = function() n:add(1) m:add(1) end }
return ui.column { ui.text { id = "n", "{n}" }, ui.text { id = "m", function() return tostring(m:get()) end } }"#;
    std::fs::write(&path, code).unwrap();
    let storage = dir.join("state");
    let mut h = Harness::new({
        let (path, storage) = (path.clone(), storage.clone());
        move |runtime| ScriptHost::new(runtime, path).storage(storage)
    });
    run(&mut h, "inc");
    h.host.save();
    assert_eq!(
        std::fs::read_to_string(storage.join("app.luau/n.json")).unwrap(),
        "1"
    );
    assert_eq!(
        std::fs::read_to_string(storage.join("app.luau/panel.luau%3A%3An.json")).unwrap(),
        "101"
    );
    // Reloading keeps the live value; it does not read the file again.
    std::fs::write(storage.join("app.luau/n.json"), "50").unwrap();
    h.host.reload().unwrap();
    assert_eq!(h.text("n"), "1");
    assert_eq!(h.text("m"), "101");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn durations_are_milliseconds_and_seconds_warn_once() {
    let h = Harness::inline(
        r##"local open = ui.state("open", false)
        local function panel()
            return ui.div { w = 10, h = 10, opacity = ui.tween(open, { duration = 0.16 }) }
        end
        anime.animate("#x", { x = 10, duration = 120000 })
        anime.animate("#x", { x = 10, duration = 120000, loop = true })
        return ui.column { panel(), panel(), ui.div { layout = "height", layout_duration = 300 } }"##,
    );
    let warnings = h.host.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    let has = |site: &str, text: &str| {
        warnings
            .iter()
            .any(|w| w.contains(site) && w.contains(text))
    };
    assert!(
        has("app:3:", "`duration = 0.16`") && has("app:3:", "write 160"),
        "{warnings:?}"
    );
    assert!(has("app:5:", "over a minute"), "{warnings:?}");
    // A reload does not warn again.
    h.host.reload().unwrap();
    assert_eq!(h.host.warnings().len(), 2);
}

#[test]
fn a_computed_made_while_a_component_builds_has_its_value_at_once() {
    // Components and list rows build inside a flush, where an effect would
    // run only after them: their computed values were nil until then.
    let mut h = Harness::inline(
        r#"
local n = ui.state("n", 1)
local seen = ui.state("seen", "")
local function note(text: string)
	seen:set(seen:peek() .. text .. " ")
end
local Card = ui.component(function(props: { by: number })
	local c = ui.computed(function()
		return n:get() * props.by
	end)
	note(`build {c:peek()}`)
	return ui.text {
		function()
			return `card {c:get()}`
		end,
		id = "card",
		on_mount = function()
			note(`mount {c:peek()}`)
		end,
	}
end)
return ui.column {
	Card { by = 7 },
	ui.list {
		count = 1,
		height = 20,
		h = 40,
		row = function(i)
			local c = ui.computed(function()
				return n:get() + i
			end)
			note(`row {c:peek()}`)
			return ui.text { "x" }
		end,
	},
	ui.button { id = "more", on_click = ui.add(n), "+" },
	ui.text { "{seen}", id = "seen" },
}
"#,
    );
    h.frame();
    assert_eq!(h.text("seen"), "build 7 mount 7 row 2 ");
    h.click("more");
    h.frame();
    assert_eq!(h.text("card"), "card 14", "it still follows what it reads");
}

#[test]
fn a_computed_read_right_after_a_write_has_the_new_value() {
    // A click handler runs in the runtime's batch, where the computed's
    // effect would run only once the handler returns: a read in between,
    // inside `ui.batch` or after it, saw the old value (the mail app then
    // opened the message it had just archived).
    let mut h = Harness::inline(
        r#"
local n = ui.state("n", 1)
local doubled = ui.computed(function()
	return n:get() * 2
end)
local seen = ui.state("seen", "")
return ui.column {
	ui.button {
		id = "go",
		on_click = function()
			local inside = 0
			ui.batch(function()
				n:set(5)
				inside = doubled:peek()
			end)
			seen:set(`{inside} {doubled:get()}`)
		end,
		"go",
	},
	ui.text { "{seen}", id = "seen" },
}
"#,
    );
    h.frame();
    h.click("go");
    h.frame();
    assert_eq!(h.text("seen"), "10 10");
}

#[test]
fn timers_faster_than_a_frame_warn_once() {
    let h = Harness::inline(
        r#"local rate = 4
        ui.every(rate, function() return false end)
        ui.every(16, function() return false end)
        ui.after(1, function() end)
        return ui.column {}"#,
    );
    let warnings = h.host.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("app:2:") && warnings[0].contains("`ui.every(4)`"),
        "{warnings:?}"
    );
}

#[test]
fn a_long_each_suggests_a_virtual_list() {
    let h = Harness::inline(
        r#"local many, few = {}, {}
        for i = 1, 1500 do many[i] = i end
        for i = 1, 100 do few[i] = i end
        local long = ui.state("long", many)
        local short = ui.state("short", few)
        return ui.column {
            ui.each(long, function(i) return ui.div { h = 1 } end),
            ui.each(short, function(i) return ui.div { h = 1 } end),
        }"#,
    );
    let warnings = h.host.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("app:7:") && warnings[0].contains("over 1500 items"),
        "{warnings:?}"
    );
}

#[test]
fn timers_fire_on_their_clock_and_stop() {
    let mut h = Harness::inline(
        r#"local n = ui.state("n", 0)
        local after = ui.state("after", 0)
        local stop
        stop = ui.every(10, function()
            n:set(n:peek() + 1)
            if n:peek() >= 3 then stop() end
        end)
        ui.after(15, function() after:set(1) end)
        return ui.column { ui.text { "{n} {after}", id = "out" } }"#,
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(400);
    while std::time::Instant::now() < deadline {
        h.settle();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(h.text("out"), "3 1", "every stopped at 3, after fired once");
}

/// Items converted for scripts per `set_key` on one row of a list of `n`
/// rows that a filter computed and a list of its rows read.
fn converted_per_row_write(n: usize) -> u64 {
    let h = Harness::inline(&format!(
        r#"local rows = {{}}
        for i = 1, {n} do
            rows[i] = {{ id = i, box = if i % 2 == 0 then "inbox" else "sent", unread = false }}
        end
        local messages = ui.state("messages", rows)
        local shown = ui.computed(function()
            local out = {{}}
            for _, m in messages:get() do
                if m.box == "inbox" then table.insert(out, m) end
            end
            return out
        end)
        local tick = 0
        ui.command {{ id = "tick", run = function()
            tick += 1
            local i = (tick * 7) % {n} + 1
            messages:set_key({{ i, "unread" }}, not messages:peek()[i].unread)
        end }}
        return ui.list {{ h = 200, height = 20, count = function() return #shown:get() end,
            row = function(i)
                return ui.text {{ function() local m = shown:get()[i]; return if m and m.unread then "*" else "-" end }}
            end,
        }}"#
    ));
    h.frames(2);
    let before = zgx_script::values_converted();
    let writes = 20;
    for _ in 0..writes {
        assert!(h.host.run_command("tick"));
        h.frame();
    }
    (zgx_script::values_converted() - before) / writes
}

#[test]
fn a_row_written_in_a_long_list_converts_the_row_not_the_list() {
    // Each write: the list's table copied from the last one's with the new
    // row in it, the row's fields, and the filter's result kept as the
    // table its readers get.
    let (short, long) = (converted_per_row_write(400), converted_per_row_write(4000));
    assert_eq!(
        short, long,
        "a write converts as much with 4,000 rows as with 400"
    );
    assert!(short <= 8, "a write converts {short} items");
}

#[test]
fn a_table_nesting_thousands_deep_or_holding_itself_is_an_error_not_a_crash() {
    let mut h = Harness::inline(
        r#"local deep = {}
        for _ = 1, 100000 do deep = { deep } end
        local looped = {}
        looped[1] = looped
        local s = ui.state("s", 0)
        local function tried(value)
            local ok, e = pcall(function() s:set(value) end)
            return if ok then "set" else tostring(e)
        end
        local inner = {}
        for _ = 1, 100 do inner = { inner } end
        return ui.column {
            ui.text { id = "deep", tried(deep) },
            ui.text { id = "looped", tried(looped) },
            ui.text { id = "inner", tried(inner) },
        }"#,
    );
    for id in ["deep", "looped"] {
        assert!(
            h.text(id).contains("nests at most 128 tables deep"),
            "{}",
            h.text(id)
        );
    }
    assert_eq!(h.text("inner"), "set");
}

#[test]
fn a_small_table_read_again_is_the_same_until_written() {
    // Read twice (a row's key, then the row), a small value gives the same
    // read-only table; written, it gives the new one.
    let mut h = Harness::inline(
        r##"local item = ui.state("item", { id = 1, label = "a" })
        local seen = ui.state("seen", "")
        return ui.column {
            ui.text { id = "seen", "{seen}" },
            ui.button { id = "check", w = 10, h = 10, on_click = function()
                local a, b = item:get(), item:get()
                item:set({ id = 1, label = "b" })
                local c = item:get()
                seen:set(tostring(rawequal(a, b)) .. " " .. tostring(rawequal(a, c)) .. " " .. c.label .. " " .. a.label)
            end },
        }"##,
    );
    h.click("check");
    assert_eq!(h.text("seen"), "true false b a");
}

#[test]
fn a_path_picks_an_item_by_its_fields_and_append_adds_text() {
    let mut h = Harness::inline(
        r##"local chats = ui.state("chats", { c1 = { { id = "a", text = "Hi" }, { id = "b", text = "" } } })
        local said = ui.state("said", "")
        ui.command { id = "go", run = function()
            chats:set_key({ "c1", { id = "a" }, "text" }, "Hello")
            chats:append({ "c1", { id = "b" }, "text" }, "One")
            chats:append({ "c1", { id = "b" }, "text" }, " two")
            chats:append({ "c1", { id = "b" }, "note" }, "new")
            local ok, err = pcall(function() chats:set_key({ "c1", { id = "z" }, "text" }, "") end)
            local ok2, err2 = pcall(function() chats:append({ "c1", 1 }, "x") end)
            local c = chats:peek().c1
            said:set(`{c[1].text}|{c[2].text}|{c[2].note}|{ok}|{tostring(err):find("no item has") ~= nil}|{ok2}|{tostring(err2):find("not one") ~= nil}`)
        end }
        return ui.text { id = "said", "{said}" }"##,
    );
    assert!(h.host.run_command("go"));
    h.frames(1);
    assert_eq!(h.text("said"), "Hello|One two|new|false|true|false|true");
    assert_eq!(h.error(), None);
}
