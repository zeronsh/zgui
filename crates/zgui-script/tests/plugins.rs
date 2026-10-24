//! Plugins an app manages (`set_plugins`): loaded from the disk with what
//! the app granted them, kept from restricted actions and shared state, and
//! given each slot's context (one panel component, an instance per pane).
mod common;
use common::Harness;
use std::path::PathBuf;
use zgui_script::{Capability, Plugin, ScriptHost, Value};

fn plugin(name: &str, code: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgui-plugins-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("init.luau");
    std::fs::write(&entry, code).unwrap();
    entry
}
fn reads(name: &str) -> Capability {
    Capability::Named(name.into())
}

const APP: &str = r#"
return ui.column {
    ui.slot("panel:notes", { id = "left", context = { pane = "p1" } }),
    ui.slot("panel:notes", { id = "right", context = { pane = "p2" } }),
    ui.slot("bar", { id = "bar" }),
}
"#;

#[test]
fn a_panel_component_is_an_instance_per_slot_with_its_context() {
    let entry = plugin(
        "notes",
        r#"
local Notes = ui.component(function(props)
    return ui.text { id = `note-{props.slot.pane}`, `notes for {props.slot.pane}` }
end)
ui.contribute("panel:notes", Notes {})
"#,
    );
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.set_plugins(vec![Plugin {
            name: "notes".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    assert_eq!(h.text("note-p1"), "notes for p1");
    assert_eq!(h.text("note-p2"), "notes for p2");
}

#[test]
fn plugins_are_set_at_runtime_and_load_on_reload() {
    let mut h = Harness::inline(APP);
    assert!(h.find("hello").is_none());
    let entry = plugin(
        "late",
        r#"ui.contribute("bar", ui.text { "hello", id = "hello" })"#,
    );
    h.host.set_plugins(vec![Plugin {
        name: "late".into(),
        entry,
        grants: vec![],
    }]);
    h.host.reload().unwrap();
    h.settle();
    assert_eq!(h.text("hello"), "hello");
    // Taken away again.
    h.host.set_plugins(vec![]);
    h.host.reload().unwrap();
    h.settle();
    assert!(h.find("hello").is_none());
}

#[test]
fn restricted_actions_and_shared_state_need_the_grant() {
    let entry = plugin(
        "peek",
        r#"
local seen = ui.shared("sessions", {})
ui.invoke("send_text", "ls")
ui.contribute("bar", ui.text { "peeked", id = "peeked" })
"#,
    );
    let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let make = |grants: Vec<Capability>| {
        let (entry, sent) = (entry.clone(), sent.clone());
        move |runtime: &zgui::reactive::Runtime| {
            let host = ScriptHost::inline(runtime, "app", APP);
            host.action("send_text", move |text| {
                if let Value::Str(text) = text {
                    sent.borrow_mut().push(text.to_string());
                }
            });
            host.shared("sessions", Value::Nil);
            host.expose_action("send_text", Some(reads("terminals.write")));
            host.expose_shared("sessions", Some(reads("terminals.read")));
            host.set_plugins(vec![Plugin {
                name: "peek".into(),
                entry,
                grants,
            }]);
            host
        }
    };
    // Without the grants: left out, with why.
    let mut h = Harness::new(make(vec![]));
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("ui.shared `sessions`"), "{error}");
    assert!(h.find("peeked").is_none());
    // With only one of them: the action is still out of reach.
    let h = Harness::new(make(vec![reads("terminals.read")]));
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("ui.invoke `send_text`"), "{error}");
    // With both.
    let mut h = Harness::new(make(vec![
        reads("terminals.read"),
        reads("terminals.write"),
    ]));
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.text("peeked"), "peeked");
    assert_eq!(*sent.borrow(), vec!["ls".to_string()]);
}

#[test]
fn the_app_itself_is_not_restricted() {
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"local s = ui.shared("sessions", "x") return ui.text { function() return s:get() end, id = "s" }"#,
        );
        host.shared("sessions", "all of them");
        host.expose_shared("sessions", Some(reads("terminals.read")));
        host
    });
    assert_eq!(h.text("s"), "all of them");
}

#[test]
fn plugins_reach_only_what_the_app_exposes() {
    let try_plugin = |code: &str| {
        let entry = plugin("nosy", code);
        let h = Harness::new(move |runtime| {
            let host = ScriptHost::inline(runtime, "app", APP);
            host.action("close_window", |_| panic!("a plugin closed the window"));
            host.shared("saved_layout", "secret");
            host.set_plugins(vec![Plugin {
                name: "nosy".into(),
                entry,
                grants: vec![],
            }]);
            host
        });
        h.host.error().get().unwrap_or_default()
    };
    let error = try_plugin(r#"ui.invoke("close_window")"#);
    assert!(error.contains("does not expose it to plugins"), "{error}");
    let error = try_plugin(r#"ui.shared("saved_layout", nil)"#);
    assert!(error.contains("does not expose it to plugins"), "{error}");
    // Nor by naming it in a template.
    let error = try_plugin(r#"ui.contribute("bar", ui.text { "{saved_layout}" })"#);
    assert!(error.contains("does not expose it to plugins"), "{error}");
    let error = try_plugin(r#"ui.term("p1")"#);
    assert!(error.contains("Capability::Terminals"), "{error}");
    let error = try_plugin(r#"ui.web({})"#);
    assert!(error.contains("Capability::Web"), "{error}");
}

#[test]
fn a_slot_from_a_plugin_shows_only_what_it_contributes() {
    let mine = plugin(
        "mine",
        r#"ui.contribute("panel:x", ui.text { "mine", id = "mine" })"#,
    );
    let other = plugin(
        "other",
        r#"ui.contribute("panel:x", ui.text { "other", id = "other" })"#,
    );
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"return ui.column { ui.slot("panel:x", { from = "mine" }) }"#,
        );
        host.set_plugins(vec![
            Plugin {
                name: "mine".into(),
                entry: mine,
                grants: vec![],
            },
            Plugin {
                name: "other".into(),
                entry: other,
                grants: vec![],
            },
        ]);
        host
    });
    assert_eq!(h.text("mine"), "mine");
    assert!(h.find("other").is_none(), "another plugin's is not shown");
}

#[test]
fn a_plugin_named_as_the_app_does_not_read_its_saved_state() {
    let entry = plugin(
        "app",
        r#"ui.contribute("bar", ui.text { ui.persist("token", "none"):peek(), id = "token" })"#,
    );
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"ui.persist("token", "s3cret") return ui.column { ui.slot("bar") }"#,
        );
        host.set_plugins(vec![Plugin {
            name: "app".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    assert_eq!(h.text("token"), "none");
}

#[test]
fn a_plugin_neither_replaces_nor_breaks_the_apps_commands() {
    let entry = plugin(
        "keys",
        r#"
ui.command { id = "copy", title = "Steal", menu = "Edit", key = "Cmd+C", run = function() end }
ui.command { id = "note", title = "New Note", menu = "Notes", key = "cmd+shift+n", run = function() end }
"#,
    );
    let h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"ui.command { id = "copy", title = "Copy", menu = "Edit", key = "cmd+c", run = function() end }
return ui.column {}"#,
        );
        host.set_plugins(vec![Plugin {
            name: "keys".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    assert_eq!(h.host.error().get(), None);
    let list = h.host.commands().get();
    let found: Vec<_> = list
        .iter()
        .map(|c| (c.id.as_str(), c.key.as_deref()))
        .collect();
    assert_eq!(
        found,
        [
            ("keys/copy", None),
            ("keys/note", Some("cmd+shift+n")),
            ("copy", Some("cmd+c"))
        ]
    );
}

#[test]
fn a_plugins_pages_are_its_own_not_the_apps() {
    let entry = plugin(
        "pages",
        r#"
local mine = ui.web("web:p1")
ui.contribute("bar", ui.text { function() return "seen: " .. mine.url end, id = "seen" })
"#,
    );
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"
local page = ui.web("web:p1")
page:load("https://mail.example.com/inbox")
return ui.column { ui.text { function() return page.url end, id = "app" }, ui.slot("bar") }
"#,
        );
        host.set_plugins(vec![Plugin {
            name: "pages".into(),
            entry,
            grants: vec![Capability::Web],
        }]);
        host
    });
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.text("app"), "https://mail.example.com/inbox");
    assert_eq!(
        h.text("seen"),
        "seen: ",
        "the plugin's web:p1 is another page"
    );
}

#[test]
fn a_plugin_of_bytecode_is_read_as_source_and_never_runs() {
    // Luau checks no bytecode it loads: crafted bytecode would corrupt the
    // VM, so a plugin's files are source, always.
    let bytecode = mlua::chunk::Compiler::new()
        .compile(r#"ui.contribute("bar", ui.text { "ran", id = "ran" })"#)
        .unwrap();
    let entry = plugin("bytecode", "");
    std::fs::write(&entry, &bytecode).unwrap();
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.set_plugins(vec![Plugin {
            name: "bytecode".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    assert!(h.find("ran").is_none(), "the bytecode ran");
}

#[test]
fn a_plugin_shows_remote_images_only_with_the_fetch_grant() {
    // An image's URL is a request: without the grant, a plugin could send
    // out what it read in one.
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    for (name, grants) in [("viewer", vec![]), ("fetcher", vec![Capability::Fetch])] {
        let entry = plugin(
            name,
            r#"ui.contribute("bar", ui.image { src = "https://images.test/secret?k=v", w = 10, h = 10 })"#,
        );
        let log = asked.clone();
        let h = Harness::new(|runtime| {
            let host = ScriptHost::inline(runtime, "app", APP);
            host.fetcher(move |request: zgui_script::Request| {
                log.borrow_mut().push(request.url.clone());
                async move { Err::<zgui_script::Response, String>("offline".into()) }
            });
            host.set_plugins(vec![Plugin {
                name: name.into(),
                entry,
                grants,
            }]);
            host
        });
        h.frames(2);
        let made = asked.borrow().len();
        match name {
            "viewer" => assert_eq!(made, 0, "a request without the grant"),
            _ => assert_eq!(made, 1, "the granted plugin's request"),
        }
    }
}

#[cfg(unix)]
#[test]
fn a_plugin_requires_nothing_outside_its_directory_through_a_link() {
    // A module that is a link to a file outside (a secret the plugin would
    // read back in a parse error) is outside, whatever its name says.
    let outside = std::env::temp_dir().join(format!("zgui-outside-{}.luau", std::process::id()));
    std::fs::write(&outside, r#"return "outside""#).unwrap();
    let entry = plugin(
        "linked",
        r#"local ok, got = pcall(require, "./secret")
ui.contribute("bar", ui.text { tostring(ok) .. " " .. tostring(got), id = "got" })"#,
    );
    let link = entry.parent().unwrap().join("secret.luau");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.set_plugins(vec![Plugin {
            name: "linked".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    let got = h.text("got");
    assert!(got.starts_with("false"), "{got}");
    assert!(got.contains("directory or below"), "{got}");
}

#[test]
fn a_plugins_images_come_from_its_directory() {
    let outside = std::env::temp_dir().join(format!("zgui-outside-{}.png", std::process::id()));
    std::fs::write(&outside, b"not really a png").unwrap();
    let entry = plugin(
        "pictures",
        &format!(
            r#"ui.contribute("bar", ui.image {{ src = "{}", w = 10, h = 10 }})"#,
            outside.display()
        ),
    );
    let h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.set_plugins(vec![Plugin {
            name: "pictures".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    h.frames(1);
    let error = h.error().unwrap_or_default();
    assert!(
        error.contains("a plugin's images come from its directory"),
        "{error}"
    );
}

#[test]
fn a_plugin_opens_windows_only_with_the_grant_and_never_reaches_the_apps_title_bar() {
    // A window of its own could pose as the app's or the system's, asking
    // for a password; the app's title bar area is the app's.
    for (name, grants, opens) in [
        ("posing", vec![], false),
        ("inspector", vec![Capability::Windows], true),
    ] {
        let entry = plugin(
            name,
            r#"
local ok = pcall(ui.window, {
    id = "w", open = ui.state("open", true), ui.text { "window" },
})
ui.contribute("bar", ui.text {
    tostring(ok) .. " " .. tostring(ui.window.titlebar_area == nil), id = "got",
})
"#,
        );
        let mut h = Harness::new(|runtime| {
            let host = ScriptHost::inline(runtime, "app", APP);
            host.set_plugins(vec![Plugin {
                name: name.into(),
                entry,
                grants,
            }]);
            host
        });
        assert_eq!(h.text("got"), format!("{opens} true"), "{name}");
    }
}

#[test]
fn a_plugins_state_counts_against_its_memory_limit() {
    // A state's value lives in Rust, outside the VM: without counting it,
    // a plugin making state after state held as much as it liked.
    let entry = plugin(
        "hoarder",
        r#"
local made, err = 0, ""
for i = 1, 1000 do
    local ok, e = pcall(function() ui.state("k" .. i, string.rep("x", 100000) .. i) end)
    if not ok then err = tostring(e) break end
    made = i
end
local pushed, perr = 0, ""
local list = ui.state("list", {})
for i = 1, 1000 do
    local ok, e = pcall(function() list:push(string.rep("y", 100000) .. i) end)
    if not ok then perr = tostring(e) break end
    pushed = i
end
-- One state written over and over holds one value.
local again = ui.state("again", 0)
local rewrites = 0
for i = 1, 200 do
    if pcall(function() again:set(string.rep("z", 10000) .. i) end) then
        rewrites = i
    end
end
ui.contribute("bar", ui.text { `{made} {err:find("memory") ~= nil} {pushed} {perr:find("memory") ~= nil} {rewrites}`, id = "got" })
"#,
    );
    let mut h = Harness::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP).limits(zgui_script::Limits {
            plugin_memory: 8 << 20,
            ..Default::default()
        });
        host.set_plugins(vec![Plugin {
            name: "hoarder".into(),
            entry,
            grants: vec![],
        }]);
        host
    });
    let got = h.text("got");
    let parts: Vec<&str> = got.split(' ').collect();
    let made: usize = parts[0].parse().unwrap();
    let pushed: usize = parts[2].parse().unwrap();
    // 8 MB is some 80 strings of 100 KB, less what the VM holds.
    assert!((20..90).contains(&made), "{got}");
    assert_eq!(parts[1], "true", "{got}");
    // The first loop's states still hold theirs: little room is left.
    assert!(pushed < 20, "{got}");
    assert_eq!(parts[3], "true", "{got}");
    assert_eq!(parts[4], "200", "{got}");
}
