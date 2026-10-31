//! A Luau app's plugins from its `zgx.json` alone (`app_config`): the app
//! exposes shared state, an action it defines (`ui.actions`) and a target
//! to filter; a plugin is allowed them; `zgx test` mounts it as `zgx run`
//! would. A grant beyond what is exposed, or one that doesn't exist, is an
//! error as it loads, naming the plugin and the grant.
use std::{path::PathBuf, process::Command};

const APP: &str = r##"
local tasks = ui.shared("tasks", {
    { id = 1, title = "Write", done = false },
    { id = 2, title = "Ship", done = true },
})
ui.actions {
    complete = function(id)
        local list = table.clone(tasks:peek())
        for i, task in list do
            if task.id == id then
                list[i] = { id = task.id, title = task.title, done = true }
            end
        end
        tasks:set(list)
    end,
}
return ui.column {
    ui.text { id = "title", "Tasks" },
    ui.each(tasks, function(task)
        return ui.text { id = `task-{task.id}`, task.title }
    end, { id = "tasks", key = function(task) return task.id end }),
}
"##;

const PLUGIN: &str = r##"
local tasks = ui.shared("tasks", {})
ui.contribute {
    target = "#title",
    after = ui.button { id = "finish", w = 60, h = 24, "Finish", on_click = function()
        ui.invoke("complete", tasks:peek()[1].id)
    end },
}
ui.filter {
    target = "#tasks",
    items = function(items)
        local open = {}
        for _, task in items do
            if not task.done then
                table.insert(open, task)
            end
        end
        return open
    end,
}
"##;

const TEST: &str = r##"
test("the plugin adds, filters and calls the app's action", function()
    local app = mount()
    expect(app:error()).to_be_nil()
    expect(app:has("#finish")).to_be(true)
    expect(app:has("#task-1")).to_be(true)
    expect(app:has("#task-2")).to_be(false)
    app:click("#finish")
    expect(app:state("tasks")[1].done).to_be(true)
    expect(app:has("#task-1")).to_be(false)
end)
"##;

const CONFIG: &str = r##"{
    "expose": {
        "shared": { "tasks": "read" },
        "actions": ["complete"],
        "hooks": { "#tasks": "override" }
    },
    "plugins": {
        "focus": { "path": "plugins/focus", "allow": ["shared:tasks", "action:complete", "override:#tasks"] }
    }
}"##;

fn app(name: &str, config: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-app-plugins-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plugins/focus")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(dir.join("app.luau"), APP).unwrap();
    std::fs::write(dir.join("plugins/focus/init.luau"), PLUGIN).unwrap();
    std::fs::write(dir.join("tests/app.test.luau"), TEST).unwrap();
    std::fs::write(dir.join("zgx.json"), config).unwrap();
    dir
}

#[test]
fn a_luau_apps_plugins_come_from_its_zgx_json() {
    let dir = app("granted", CONFIG);
    let out = Command::new(env!("CARGO_BIN_EXE_zgx"))
        .arg("test")
        .arg(&dir)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_grant_beyond_what_the_app_exposes_is_an_error_naming_it() {
    for (config, said) in [
        (
            CONFIG.replace(r#""actions": ["complete"],"#, ""),
            "plugin `focus` is allowed `action:complete`, which the app doesn't expose \
             (add \"complete\" to expose.actions)",
        ),
        (
            CONFIG.replace(r##""hooks": { "#tasks": "override" }"##, r#""hooks": {}"#),
            "plugin `focus` is allowed `override:#tasks`, which the app doesn't expose",
        ),
        (
            CONFIG.replace("\"shared:tasks\"", "\"shared:tasks\", \"camera\""),
            "plugin `focus`: unknown grant `camera`",
        ),
        (
            CONFIG.replace("plugins/focus", "plugins/missing"),
            "plugin `focus`:",
        ),
        (
            CONFIG.replace(r#""tasks": "read""#, r#""tasks": "all""#),
            "`expose.shared.tasks` is \"all\": \"read\" or \"write\"",
        ),
    ] {
        let dir = app("refused", &config);
        let error = zgx_script::app_config::read(&dir).unwrap_err();
        assert!(error.contains(said), "{error}\n(wanted: {said})");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// A plugin not allowed an exposed action can't call it.
#[test]
fn an_action_not_granted_is_refused() {
    let config = CONFIG.replace("\"action:complete\", ", "");
    let dir = app("ungranted", &config);
    let mut app = zgx_script::testing::TestApp::new(|runtime| {
        zgx_script::app_config::read(&dir)
            .unwrap()
            .apply(zgx_script::ScriptHost::new(runtime, dir.join("app.luau")))
    });
    app.click("finish");
    app.frames(1);
    let error = app.error().unwrap_or_default();
    assert!(error.contains("complete"), "{error:?}");
    let tasks = app.host.states();
    let done = tasks
        .iter()
        .find(|(name, _)| name == "tasks")
        .map(|(_, value)| value.to_json());
    assert!(
        done.is_some_and(|json| json.contains("\"done\":false")),
        "the app's action didn't run"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The app's own script defines actions; a plugin can't.
#[test]
fn only_the_app_defines_actions() {
    let dir = app("plugin-defines", CONFIG);
    std::fs::write(
        dir.join("plugins/focus/init.luau"),
        "ui.actions { complete = function() end }\n",
    )
    .unwrap();
    let app = zgx_script::testing::TestApp::new(|runtime| {
        zgx_script::app_config::read(&dir)
            .unwrap()
            .apply(zgx_script::ScriptHost::new(runtime, dir.join("app.luau")))
    });
    let error = app.error().unwrap_or_default();
    assert!(
        error.contains("only the app's script defines actions"),
        "{error:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
