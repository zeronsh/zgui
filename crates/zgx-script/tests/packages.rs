//! Packages (`ScriptHost::package`, zgx.json's `"packages"`): module folders
//! outside the app's directory it may require, as `@name/…` or through its
//! `.luaurc` aliases, and nothing else outside it.
mod common;
use common::Harness;
use std::path::PathBuf;
use zgx_script::{Plugin, ScriptHost};

/// A directory of files, made fresh for one test.
fn files(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-packages-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (path, code) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, code).unwrap();
    }
    dir
}

/// A shared kit: a button that requires a sibling, and a file reaching out
/// of the kit.
fn kit(name: &str) -> PathBuf {
    files(
        &format!("{name}-kit"),
        &[
            (
                "button.luau",
                r#"local theme = require("./theme")
return function(label) return ui.text { id = "button", label .. " " .. theme.accent } end"#,
            ),
            ("theme.luau", r##"return { accent = "#8b7cf6" }"##),
            ("escape.luau", r#"return require("../secret")"#),
        ],
    )
}

/// An app beside the kit (`../<name>-kit`), with `app.luau` as given.
fn app(name: &str, code: &str) -> PathBuf {
    let luaurc = format!(
        r#"{{ "aliases": {{ "shared": "../zgx-packages-{}-{name}-kit" }} }}"#,
        std::process::id()
    );
    files(
        &format!("{name}-app"),
        &[("app.luau", code), (".luaurc", &luaurc)],
    )
    .join("app.luau")
}

#[test]
fn an_app_requires_a_package_by_name_and_through_its_luaurc() {
    let kit = kit("named");
    let app = app(
        "named",
        r#"local button = require("@kit/button")
assert(require("@shared/theme") == require("@kit/theme"), "one module, by either name")
return button("Save")"#,
    );
    let mut h = Harness::new(move |runtime| ScriptHost::new(runtime, &app).package("kit", &kit));
    assert_eq!(h.text("button"), "Save #8b7cf6");
}

/// Without the package, a `.luaurc` alias out of the app's directory stays
/// outside it.
#[test]
fn an_alias_outside_the_app_needs_a_package() {
    kit("unlisted");
    let app = app("unlisted", r#"return require("@shared/button")("Save")"#);
    let runtime = zgx::reactive::Runtime::new();
    let error = ScriptHost::new(&runtime, &app).reload().unwrap_err();
    assert!(
        error.contains("directory or below, or a package"),
        "{error}"
    );
}

/// A package is its folder: a module in it reaches no further, a link out
/// of it included.
#[test]
fn a_package_reaches_nothing_outside_its_folder() {
    let kit = kit("escape");
    let escape = app(
        "escape",
        r#"return ui.text { tostring(require("@kit/escape")) }"#,
    );
    let runtime = zgx::reactive::Runtime::new();
    let host = ScriptHost::new(&runtime, &escape).package("kit", &kit);
    let error = host.reload().unwrap_err();
    assert!(
        error.contains("directory or below, or a package"),
        "{error}"
    );
    #[cfg(unix)]
    {
        let outside = files("escape-outside", &[("secret.luau", "return 1")]);
        std::os::unix::fs::symlink(outside.join("secret.luau"), kit.join("link.luau")).unwrap();
        let app = app(
            "escape",
            r#"return ui.text { tostring(require("@kit/link")) }"#,
        );
        let host = ScriptHost::new(&runtime, &app).package("kit", &kit);
        let error = host.reload().unwrap_err();
        assert!(
            error.contains("directory or below, or a package"),
            "{error}"
        );
    }
}

/// Packages are the app's: a plugin requires none.
#[test]
fn plugins_do_not_reach_the_apps_packages() {
    let kit = kit("plugin");
    let app = app("plugin", r#"return ui.text { id = "t", "Hi" }"#);
    let plugin = files(
        "plugin-plugin",
        &[("init.luau", r#"local button = require("@kit/button")"#)],
    );
    let mut h = Harness::new(move |runtime| {
        let host = ScriptHost::new(runtime, &app).package("kit", &kit);
        host.set_plugins(vec![Plugin {
            name: "reach".into(),
            entry: plugin.join("init.luau"),
            grants: Vec::new(),
        }]);
        host
    });
    h.ui.prepare_frame();
    let error = h.error().unwrap_or_default();
    assert!(error.contains("nor a package of that name"), "{error}");
    assert_eq!(h.text("t"), "Hi");
}

/// A package takes none of zgx's aliases, and agrees with a `.luaurc` alias
/// of its name.
#[test]
fn a_package_name_is_its_own() {
    let kit = kit("names");
    let elsewhere = files("names-elsewhere", &[("button.luau", "return 1")]);
    let app = app("names", r#"return ui.text { "Hi" }"#);
    let runtime = zgx::reactive::Runtime::new();
    let error = ScriptHost::new(&runtime, &app)
        .package("app", &kit)
        .reload()
        .unwrap_err();
    assert!(error.contains("`@app` is zgx's own alias"), "{error}");
    let error = ScriptHost::new(&runtime, &app)
        .package("shared", &elsewhere)
        .reload()
        .unwrap_err();
    assert!(error.contains(".luaurc's alias `shared`"), "{error}");
    ScriptHost::new(&runtime, &app)
        .package("shared", &kit)
        .reload()
        .expect("the alias and the package name one folder");
}

/// A package's theme files load as its modules do: `@name/…`, or a path
/// into it, and not without the package.
#[test]
fn a_package_ships_its_themes() {
    let kit = kit("themes");
    std::fs::write(
        kit.join("theme.json"),
        r##"{ "name": "kit", "type": "dark", "extends": "zgx-dark", "tokens": { "accent": "#ff0000" } }"##,
    )
    .unwrap();
    let code = format!(
        r#"local by_name = ui.theme.load("@kit/theme.json")
local by_path = ui.theme.load("../zgx-packages-{}-themes-kit/theme.json")
return ui.text {{ id = "t", by_name .. " " .. by_path }}"#,
        std::process::id()
    );
    let themes = app("themes", &code);
    let path = themes.clone();
    let mut h = Harness::new(move |runtime| ScriptHost::new(runtime, &path).package("kit", &kit));
    assert_eq!(h.text("t"), "kit kit");
    let runtime = zgx::reactive::Runtime::new();
    let error = ScriptHost::new(&runtime, &themes).reload().unwrap_err();
    assert!(error.contains("no package `kit`"), "{error}");
}
