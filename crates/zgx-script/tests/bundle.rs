//! Apps run from a bundle (`ScriptHost::bundled`): scripts as bytecode,
//! modules, aliases and theme files found in it, nothing from the disk, and
//! what ships keeps errors off the interface.
mod common;
use common::Harness;
use std::{cell::RefCell, rc::Rc};
use zgx_script::{Bundle, BundleFile, ScriptHost, build};

/// A bundle of `(path, text)` files, scripts compiled as a build would.
fn bundle(name: &'static str, files: &[(&'static str, &str)]) -> &'static Bundle {
    let mut files: Vec<BundleFile> = files
        .iter()
        .map(|(path, text)| BundleFile {
            path,
            bytes: Box::leak(
                if path.ends_with(".luau") {
                    build::compile(text.as_bytes()).unwrap()
                } else {
                    text.as_bytes().to_vec()
                }
                .into_boxed_slice(),
            ),
        })
        .collect();
    files.sort_by_key(|file| file.path);
    Box::leak(Box::new(Bundle {
        name,
        root: Box::leak(format!("/zgx-bundle/{name}").into_boxed_str()),
        files: Box::leak(files.into_boxed_slice()),
    }))
}

#[test]
fn a_bundled_app_finds_its_modules_aliases_and_themes_in_the_bundle() {
    let ui = bundle(
        "bundled-app",
        &[
            (
                "app.luau",
                r#"local greet = require("./lib/greet")
local shared = require("@shared/name")
ui.theme.default(ui.theme.load("theme.json"))
return ui.column { ui.text { greet(shared), id = "label" } }"#,
            ),
            (
                "lib/greet.luau",
                "return function(name) return `Hello, {name}` end",
            ),
            ("common/name.luau", r#"return "bundle""#),
            (".luaurc", r#"{ "aliases": { "shared": "./common" } }"#),
            (
                "theme.json",
                r##"{ "name": "bundled", "type": "dark", "extends": "zgx-dark", "tokens": { "accent": "#ff0000" } }"##,
            ),
        ],
    );
    let mut h = Harness::new(|runtime| ScriptHost::bundled(runtime, ui, "app.luau"));
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.text("label"), "Hello, bundle");
    assert!(
        zgx::theme::Themes::of(&h.ui.runtime)
            .names()
            .iter()
            .any(|name| name == "bundled"),
        "the theme file came from the bundle"
    );
    // A bundle never changes: watching it reloads nothing (it waits for
    // plugins on the disk, at no cost).
    let watching = h.host.watch(std::time::Duration::from_millis(1));
    h.executor.borrow_mut().spawn(watching);
    let calls = h.host.script_calls();
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(3));
        h.settle();
    }
    assert_eq!(h.host.script_calls(), calls, "nothing reloaded");
    assert_eq!(h.text("label"), "Hello, bundle");
}

#[test]
fn a_module_outside_the_bundle_is_not_read_from_the_disk() {
    let ui = bundle(
        "sealed-app",
        &[(
            "app.luau",
            r#"return ui.text { require("./missing"), id = "label" }"#,
        )],
    );
    let runtime = zgx::reactive::Runtime::new();
    let error = ScriptHost::bundled(&runtime, ui, "app.luau")
        .reload()
        .unwrap_err();
    assert!(
        error.contains("missing.luau: not in the app's bundle"),
        "{error}"
    );
}

#[test]
fn what_ships_keeps_errors_off_the_interface_and_tells_the_app() {
    let ui = bundle(
        "shipped-app",
        &[(
            "app.luau",
            r#"return ui.button { id = "fail", w = 80, h = 30, on_click = function() error("boom") end, "Go" }"#,
        )],
    );
    let heard = Rc::new(RefCell::new(Vec::<String>::new()));
    let mut h =
        Harness::new(|runtime| ScriptHost::bundled(runtime, ui, "app.luau").error_banner(false));
    {
        let heard = heard.clone();
        h.host
            .on_error(move |message| heard.borrow_mut().push(message.to_owned()));
    }
    h.click("fail");
    h.settle();
    assert!(
        heard.borrow().iter().any(|m| m.contains("boom")),
        "{:?}",
        heard.borrow()
    );
    assert!(
        h.find("script-error-banner").is_none(),
        "no banner over what ships"
    );
    assert!(h.find("fail").is_some(), "the interface keeps running");
}

/// What bytecode saves a launch: preparing every script of the terminal
/// example from source (compile and load) and from bytecode (load).
/// `cargo test --release -p zgx-script --test bundle -- --ignored --nocapture`
#[test]
#[ignore]
fn bytecode_load_time() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../zgx-desktop/examples/terminal");
    let mut scripts = Vec::new();
    let mut stack = vec![dir];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "luau") {
                let source = std::fs::read(&path).unwrap();
                let bytecode = build::compile(&source).unwrap();
                scripts.push((source, bytecode));
            }
        }
    }
    let lua = mlua::Lua::new();
    let time = |bytecode: bool| {
        let mut runs: Vec<f64> = (0..200)
            .map(|_| {
                let start = std::time::Instant::now();
                for (source, compiled) in &scripts {
                    let chunk = if bytecode { &compiled[..] } else { &source[..] };
                    lua.load(chunk).into_function().unwrap();
                }
                start.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        runs.sort_by(f64::total_cmp);
        runs[runs.len() / 2]
    };
    let (source, bytecode) = (time(false), time(true));
    println!(
        "{} scripts: from source {source:.3} ms, from bytecode {bytecode:.3} ms ({:.1}x)",
        scripts.len(),
        source / bytecode
    );
}
