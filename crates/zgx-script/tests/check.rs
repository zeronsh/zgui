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
/// `from` copied to `to`, its generated `.zgx` left out; `to`.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> PathBuf {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (path, name) = (entry.path(), entry.file_name());
        if name == ".zgx" {
            continue;
        }
        match path.is_dir() {
            true => {
                copy_dir(&path, &to.join(&name));
            }
            false => {
                std::fs::copy(&path, to.join(&name)).unwrap();
            }
        }
    }
    to.to_path_buf()
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

/// Unknown members of the globals are errors: nothing in their declarations
/// may turn them to `never` (a function intersected with a table, as
/// `ui.window` was), which would quiet every lookup on them.
#[test]
fn an_unknown_member_of_ui_or_anime_or_ui_window_fails() {
    if !installed() {
        return;
    }
    for (name, code) in [
        ("ui", "print(ui.untrack)\nreturn ui.text { \"hi\" }\n"),
        ("anime", "print(anime.nope)\nreturn ui.text { \"hi\" }\n"),
        (
            "window",
            "print(ui.window.nope)\nreturn ui.text { \"hi\" }\n",
        ),
    ] {
        let dir = scripts(&format!("unknown-{name}"), &[("app.luau", code)]);
        let out = zgx().arg("check").arg(&dir).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!out.status.success(), "{name}: {text}");
        assert!(text.contains("not found"), "{name}: {text}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// `ui.window` still takes its options, and reads its fields.
#[test]
fn ui_window_is_called_and_read() {
    if !installed() {
        return;
    }
    let code = "local open = ui.state(\"open\", false)\n\
        ui.window { id = \"settings\", title = \"Settings\", width = 300, open = open, ui.text { \"hi\" } }\n\
        local tall = ui.window.size:get().height > 400\n\
        return ui.text { tostring(tall) }\n";
    let dir = scripts("window-api", &[("app.luau", code)]);
    let out = zgx().arg("check").arg(&dir).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
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
    // Copies: `zgx check` writes each directory's `.zgx/types.d.luau`, and a
    // test writes nothing into the repository (the terminal's has none
    // tracked, and the staleness test below reads the tracked ones).
    let copies = scripts("examples", &[]);
    let examples = [
        copy_dir(&manifest.join("examples"), &copies.join("examples")),
        copy_dir(
            &manifest.join("../zgx-desktop/examples/terminal"),
            &copies.join("terminal"),
        ),
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
    std::fs::remove_dir_all(copies).unwrap();
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

#[test]
fn a_reload_writes_the_types_of_the_whole_app() {
    // A module this run hasn't required still declares the app's tokens: the
    // file is what `zgx types` writes, not only what was loaded.
    let dir = scripts(
        "reload-types",
        &[
            ("app.luau", "return ui.text { \"hi\" }\n"),
            (
                "panes.luau",
                "ui.theme.tokens { panes = { edge = \"#fff\" } }\nreturn {}\n",
            ),
        ],
    );
    let types = zgx_script::app_types::types_file(&dir);
    std::fs::create_dir_all(types.parent().unwrap()).unwrap();
    std::fs::write(&types, "").unwrap();
    let runtime = zgx::reactive::Runtime::new();
    let host = zgx_script::ScriptHost::new(&runtime, dir.join("app.luau"));
    host.reload().unwrap();
    let written = std::fs::read_to_string(&types).unwrap();
    assert!(written.contains("edge: string"), "{written}");
    zgx_script::app_types::write_types(&dir).unwrap();
    assert_eq!(std::fs::read_to_string(&types).unwrap(), written);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_examples_types_files_are_what_zgx_types_writes() {
    // Committed for editors; a change to zgx's own definitions regenerates
    // them (`zgx types <dir>`), or every run of an example rewrites them.
    // The tracked ones only: an untracked one (an editor's, a host's) is
    // no one's to keep current.
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let Ok(listed) = Command::new("git")
        .args(["ls-files", "-z", "--", "*/.zgx/types.d.luau"])
        .current_dir(&crates)
        .output()
    else {
        eprintln!("skipped: no git to list the tracked types files");
        return;
    };
    let files: Vec<PathBuf> = String::from_utf8_lossy(&listed.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(|name| crates.join(name))
        .collect();
    if !listed.status.success() || files.is_empty() {
        eprintln!("skipped: not in the repository");
        return;
    }
    for file in files {
        let dir = file.parent().and_then(|zgx| zgx.parent()).unwrap();
        let sources: Vec<_> = zgx_script::check::scripts(dir)
            .into_iter()
            .map(|script| {
                let source = std::fs::read_to_string(&script).unwrap();
                (script, source)
            })
            .collect();
        assert!(
            std::fs::read_to_string(&file).unwrap() == zgx_script::app_types::definitions(&sources),
            "{} is stale: run `zgx types {}`",
            file.display(),
            dir.display()
        );
    }
}

#[test]
fn a_misspelled_route_or_param_is_a_type_error() {
    if !installed() {
        return;
    }
    let router =
        "local router = ui.router { routes = { list = \"/\", thread = \"/thread/:id\" } }\n";
    let dir = scripts(
        "routes",
        &[
            (
                "app.luau",
                &format!(
                    "{router}router.navigate(\"thread\", {{ id = 7 }})\nrouter.replace(\"list\")\n\
                     local _shown = router.is(\"thread\")\n\
                     return router.view({{ list = ui.text {{ \"All\" }}, thread = function(params)\n\
                     \treturn ui.text {{ params.id }}\nend }})\n"
                ),
            ),
            (
                "name.luau",
                &format!("{router}router.navigate(\"thraed\", {{ id = 7 }})\nreturn {{}}\n"),
            ),
            (
                "param.luau",
                &format!("{router}router.navigate(\"thread\", {{ idd = 7 }})\nreturn {{}}\n"),
            ),
        ],
    );
    let out = zgx().arg("check").arg(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(!stdout.contains("app.luau"), "{stdout}");
    assert!(stdout.contains("name.luau:2:"), "{stdout}");
    assert!(stdout.contains("param.luau:2:"), "{stdout}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// A module requiring `@app/…`, required by an app.luau that doesn't: the
/// alias reaches it too (luau-lsp checks a module with each script that
/// requires it).
#[test]
fn a_component_requiring_at_app_checks_when_the_app_does_not() {
    if !installed() {
        return;
    }
    let dir = scripts(
        "at-app",
        &[
            (
                "app.luau",
                "local Card = require(\"./card\")\nreturn Card()\n",
            ),
            ("theme.luau", "return { card = \"$surface.raised\" }\n"),
            (
                "card.luau",
                "local theme = require(\"@app/theme\")\n\
                 return function()\n\treturn ui.div { bg = theme.card }\nend\n",
            ),
        ],
    );
    let out = zgx().arg("check").arg(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    std::fs::remove_dir_all(dir).unwrap();
}
