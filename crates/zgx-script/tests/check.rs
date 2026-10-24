//! `zgx check`: type checks scripts with luau-lsp before they run. These
//! tests run the binary when luau-lsp is installed (on the PATH or at
//! `LUAU_LSP`) and skip otherwise.
use std::{path::PathBuf, process::Command};

fn zgx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zgx"))
}
fn scripts(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-check-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, code) in files {
        std::fs::write(dir.join(file), code).unwrap();
    }
    dir
}
fn installed() -> bool {
    let found = zgx_script::check::find_luau_lsp().is_some();
    if !found {
        eprintln!("skipped: luau-lsp is not installed");
    }
    found
}

#[test]
fn a_local_function_used_before_its_definition_is_an_error() {
    if !installed() {
        return;
    }
    let dir = scripts(
        "early",
        &[(
            "app.luau",
            "local function render()\n\treturn ui.column { helper() }\nend\n\
             local function helper()\n\treturn ui.text { \"hi\" }\nend\nreturn render()\n",
        )],
    );
    let app = dir.join("app.luau");
    let out = zgx().arg("check").arg(&app).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "errors fail: {stdout}");
    let expected = format!(
        "{}:2:21: error[TypeError]: Unknown global 'helper'",
        app.display()
    );
    assert!(stdout.contains(&expected), "{stdout}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn clean_scripts_pass_and_misspelled_properties_fail() {
    if !installed() {
        return;
    }
    let dir = scripts(
        "dir",
        &[
            (
                "app.luau",
                "local count = ui.state(\"count\", 0)\nreturn ui.column {\n\tp = 12,\n\
                 \tui.text { \"Count: {count}\" },\n\tui.button { on_click = ui.add(count), \"+\" },\n}\n",
            ),
            ("typo.luau", "return ui.column { bgg = \"#fff\" }\n"),
        ],
    );
    // One clean file: no output, success. A template's state is used.
    let out = zgx()
        .arg("check")
        .arg(dir.join("app.luau"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert_eq!(stdout.trim(), "", "no findings");
    // The directory: the typo is found, as file:line:col.
    let out = zgx().arg("check").arg(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("typo.luau:1:"), "{stdout}");
    assert!(stdout.contains("bgg"), "{stdout}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn without_luau_lsp_it_says_how_to_install_it() {
    let dir = scripts("missing", &[("app.luau", "return ui.text { \"hi\" }\n")]);
    let out = zgx()
        .arg("check")
        .arg(dir.join("app.luau"))
        .env("PATH", "")
        .env("LUAU_LSP", dir.join("no-such-binary"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stderr}");
    assert!(stderr.contains("luau-lsp is not installed"), "{stderr}");
    assert!(
        stderr.contains("github.com/JohnnyMorganz/luau-lsp"),
        "{stderr}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn usage_errors_exit_2() {
    let out = zgx().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = zgx().arg("check").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn every_shipped_example_passes_in_strict_mode() {
    if !installed() {
        return;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples = [
        manifest.join("examples"),
        manifest.join("../zgx-desktop/examples/terminal"),
    ];
    // Strict where no `.luaurc` picks a mode; the terminal's says strict.
    let out = zgx().arg("check").args(&examples).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.trim().is_empty(), "no errors or warnings: {stdout}");
    // Every app and module was checked.
    let files: usize = stderr
        .split(" in ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_default();
    assert!(files >= 25, "{stderr}");
}

#[test]
fn a_debug_build_watching_checks_after_reloads() {
    if !installed() || !cfg!(debug_assertions) || zgx::env::var_os("ZGX_CHECK").is_some() {
        return;
    }
    let dir = scripts(
        "watch",
        &[(
            "app.luau",
            "local n: number = \"one\"\nreturn ui.text { tostring(n) }\n",
        )],
    );
    let runtime = zgx::reactive::Runtime::new();
    let host = zgx_script::ScriptHost::new(&runtime, dir.join("app.luau"));
    host.reload().unwrap();
    // Watching turns the checks on, starting with the load before it.
    let _watch = host.watch(std::time::Duration::from_secs(1));
    let started = std::time::Instant::now();
    while host.type_errors().get().is_none() && started.elapsed().as_secs() < 20 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        host.poll_check();
    }
    let shown = host.type_errors().get().expect("the check's errors");
    assert!(shown.starts_with("type check: 1 error"), "{shown}");
    assert!(shown.contains("app.luau:1:"), "{shown}");
    std::fs::remove_dir_all(dir).unwrap();
}
