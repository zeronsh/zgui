//! Shared state a host exposes to plugins read only
//! (`ScriptHost::expose_shared_read`): a plugin reads and follows it, uses
//! it where a state is only read (a list's items), and every write (`set`,
//! `update`, `push`, a two-way binding) is an error naming the key. The
//! app's own scripts write it as before.
mod common;
use common::Harness;
use std::path::PathBuf;
use zgx_script::{Plugin, ScriptHost, Value};

fn plugin(name: &str, code: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-shared-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("init.luau");
    std::fs::write(&entry, code).unwrap();
    entry
}

const APP: &str = r##"
local tasks = ui.shared("tasks", {})
return ui.column {
    ui.slot("panel"),
    ui.button {
        id = "add",
        w = 60,
        h = 24,
        on_click = function() tasks:push("from the app") end,
        "Add",
    },
}
"##;

/// The app, with `tasks` exposed read only, and one plugin.
fn with(name: &str, code: &str) -> Harness {
    let entry = plugin(name, code);
    let name = name.to_string();
    Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.shared("tasks", Value::from_json(r#"["one", "two"]"#).unwrap());
        host.expose_shared_read("tasks", None);
        host.set_plugins(vec![Plugin {
            name,
            entry,
            grants: vec![],
        }]);
        host
    })
}

#[test]
fn a_plugin_reads_and_follows_read_only_state() {
    let mut h = with(
        "count",
        r##"
local tasks = ui.shared("tasks", {})
ui.contribute("panel", ui.text { id = "count", function() return `{tasks:len()} tasks` end })
ui.contribute("panel", ui.each(tasks, function(task) return ui.text { task } end))
"##,
    );
    assert_eq!(h.error(), None);
    assert_eq!(h.text("count"), "2 tasks");
    assert!(h.texts().contains(&"two".to_string()));
    h.click("add");
    assert_eq!(
        h.text("count"),
        "3 tasks",
        "the app writes it; the plugin follows"
    );
    assert!(h.texts().contains(&"from the app".to_string()));
}

#[test]
fn every_write_from_a_plugin_is_an_error_naming_the_key() {
    for write in [
        r#"tasks:set({})"#,
        r#"tasks:push("mine")"#,
        r#"tasks:update(function(list) return list end)"#,
        r#"tasks:remove(1)"#,
    ] {
        let mut h = with(
            "writer",
            &format!(
                r##"
local tasks = ui.shared("tasks", {{}})
ui.contribute("panel", ui.button {{ id = "write", w = 60, h = 24, on_click = function() {write} end, "Write" }})
"##
            ),
        );
        assert_eq!(h.error(), None);
        h.click("write");
        let error = h.error().unwrap_or_default();
        assert!(
            error.contains("`tasks`") && error.contains("read only"),
            "{write}: {error}"
        );
    }
}

#[test]
fn a_two_way_binding_is_a_write() {
    let h = with(
        "binder",
        r##"
local tasks = ui.shared("tasks", {})
ui.contribute("panel", ui.input { value = tasks })
"##,
    );
    let error = h.error().unwrap_or_default();
    assert!(
        error.contains("`tasks`") && error.contains("read only"),
        "{error}"
    );
}

#[test]
fn exposing_it_again_read_write_lets_plugins_write() {
    let entry = plugin(
        "writer2",
        r##"
local tasks = ui.shared("tasks", {})
ui.contribute("panel", ui.button { id = "write", w = 60, h = 24, on_click = function() tasks:push("mine") end, "Write" })
"##,
    );
    let mut h = Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.shared("tasks", Value::from_json(r#"[]"#).unwrap());
        host.expose_shared_read("tasks", None);
        host.expose_shared("tasks", None);
        host.set_plugins(vec![Plugin {
            name: "writer2".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    h.click("write");
    assert_eq!(h.error(), None);
}
