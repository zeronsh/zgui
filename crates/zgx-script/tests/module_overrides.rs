//! A plugin replacing one of the app's modules (`ui.override`): only the
//! modules the app requires as `@app/…`, with `Capability::Override` (a trust
//! grant: the replacement runs as the app's own code), reaching the
//! original as `@app-original/…` from the replacement alone.
mod common;
use common::Harness;
use std::path::{Path, PathBuf};
use zgx_script::{Capability, Plugin, ScriptHost};

/// A directory of files, made fresh for one test.
fn files(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-overrides-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, code) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, code).unwrap();
    }
    dir
}

const CARD: &str = r#"return function(title) return ui.text { id = "card", title } end"#;

/// The app: a card from `@app/card`, and the same module by a relative path.
fn app(name: &str) -> PathBuf {
    files(
        &format!("{name}-app"),
        &[
            (
                "main.luau",
                r#"
local card = require("@app/card")
local private = require("./card")
return ui.column { card("Inbox"), ui.row { id = "private", private("Sent") } }
"#,
            ),
            ("card.luau", CARD),
        ],
    )
    .join("main.luau")
}

/// A plugin replacing `@app/card` with one that wraps the original.
fn plugin(name: &str, plugin_code: &str) -> PathBuf {
    files(
        &format!("{name}-plugin"),
        &[
            ("init.luau", plugin_code),
            (
                "card.luau",
                r#"
local original = require("@app-original/card")
return function(title)
    return ui.row { id = "wrapped", original(title), ui.text { id = "badge", "New" } }
end
"#,
            ),
        ],
    )
    .join("init.luau")
}

fn host(app: &Path, entry: PathBuf, grants: Vec<Capability>) -> Harness {
    let app = app.to_path_buf();
    Harness::new(move |runtime| {
        let host = ScriptHost::new(runtime, &app);
        host.set_plugins(vec![Plugin {
            name: "badges".into(),
            entry,
            grants,
        }]);
        host
    })
}

const OVERRIDE: &str = r#"ui.override { module = "@app/card", with = "./card" }"#;

#[test]
fn a_plugin_replaces_an_app_module_it_is_trusted_with() {
    let app = app("trusted");
    let mut h = host(
        &app,
        plugin("trusted", OVERRIDE),
        vec![Capability::Override("@app/card".into())],
    );
    h.ui.prepare_frame();
    assert_eq!(h.error(), None);
    // The replacement wraps the original, required as `@app-original/card`.
    assert!(h.find("wrapped").is_some());
    assert_eq!(h.text("badge"), "New");
    assert_eq!(h.text("card"), "Inbox");
    // `Override("*")` trusts the plugin with every module.
    let mut any = host(
        &app,
        plugin("any", OVERRIDE),
        vec![Capability::Override("*".into())],
    );
    any.ui.prepare_frame();
    assert!(any.find("wrapped").is_some());
}

#[test]
fn without_the_grant_the_app_keeps_its_module() {
    let mut h = host(&app("ungranted"), plugin("ungranted", OVERRIDE), Vec::new());
    h.ui.prepare_frame();
    let error = h.error().unwrap_or_default();
    assert!(
        error.contains("needs Override(\"@app/card\")") && error.contains("trust grant"),
        "{error}"
    );
    assert!(h.find("wrapped").is_none());
    assert_eq!(h.text("card"), "Inbox");
}

/// A relative require is the app's own: no plugin replaces it.
#[test]
fn a_relative_require_stays_the_apps() {
    let mut h = host(
        &app("relative"),
        plugin("relative", OVERRIDE),
        vec![Capability::Override("*".into())],
    );
    h.ui.prepare_frame();
    let private = h.find("private").unwrap();
    let scene = h.ui.scene.borrow();
    let mut texts = Vec::new();
    let mut pending = vec![private];
    while let Some(id) = pending.pop() {
        pending.extend(scene.children(id).iter().copied());
        if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(id) {
            texts.push(text.to_string());
        }
    }
    assert_eq!(texts, ["Sent"], "the original card alone, unwrapped");
}

/// `@app-original/…` is the replacement's alone: not the app's, not a
/// plugin's.
#[test]
fn the_original_is_reached_from_its_replacement_alone() {
    let app = files(
        "original-app",
        &[
            (
                "main.luau",
                r#"return require("@app-original/card")("Inbox")"#,
            ),
            ("card.luau", CARD),
        ],
    )
    .join("main.luau");
    let runtime = zgx::reactive::Runtime::new();
    let host = ScriptHost::new(&runtime, &app);
    let error = host.reload().unwrap_err();
    assert!(
        error.contains("for the module that overrides it alone"),
        "{error}"
    );
    // A plugin requiring the app's modules is refused.
    let h = host_with(
        "plugin-requires",
        r#"local card = require("@app/card")"#,
        vec![Capability::Override("*".into())],
    );
    h.ui.prepare_frame();
    let error = h.error().unwrap_or_default();
    assert!(
        error.contains("a plugin replaces with ui.override"),
        "{error}"
    );
}

fn host_with(name: &str, plugin_code: &str, grants: Vec<Capability>) -> Harness {
    host(&app(name), plugin(name, plugin_code), grants)
}

/// `app` and `app-original` are zgx's aliases: a `.luaurc` defining one is
/// told to rename it, with the file named.
#[test]
fn a_luaurc_cannot_take_the_apps_alias() {
    let app = files(
        "reserved-app",
        &[
            ("main.luau", r#"return ui.text { "Hi" }"#),
            (".luaurc", r#"{ "aliases": { "app": "./lib" } }"#),
        ],
    )
    .join("main.luau");
    let runtime = zgx::reactive::Runtime::new();
    let error = ScriptHost::new(&runtime, &app).reload().unwrap_err();
    assert!(
        error.contains(".luaurc") && error.contains("`app` is zgx's own alias"),
        "{error}"
    );
}

/// A replacement is one of the plugin's own files; `module` is one of the
/// app's `@app/…` modules.
#[test]
fn an_override_names_an_app_module_and_a_plugin_file() {
    for (code, says) in [
        (
            r#"ui.override { module = "./card", with = "./card" }"#,
            "only the app's `@app/…` modules",
        ),
        (
            r#"ui.override { module = "@app/card", with = "../../elsewhere" }"#,
            "one of the plugin's own files",
        ),
    ] {
        let h = host_with("names", code, vec![Capability::Override("*".into())]);
        h.ui.prepare_frame();
        let error = h.error().unwrap_or_default();
        assert!(error.contains(says), "{code}: {error}");
    }
}
