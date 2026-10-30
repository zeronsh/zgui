//! `zgx new`: every template, with and without `--i18n`, writes the files
//! it promises, and its app loads, lints clean, passes its own tests and,
//! with luau-lsp installed, type checks. (zgx-run's `tests/templates.rs`
//! audits them.)
use std::{path::PathBuf, process::Command};
use zgx_script::scaffold::TEMPLATES;

fn zgx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zgx"))
}
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-new-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn run(command: &mut Command) -> (Option<i32>, String) {
    let out = command.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}
/// The files under `dir`, relative and sorted.
fn listing(dir: &std::path::Path) -> Vec<String> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn every_template_runs_lints_clean_and_passes_its_tests() {
    let root = scratch("all");
    for (template, _) in TEMPLATES {
        for i18n in [false, true] {
            let name = format!("{template}-{}", if i18n { "i18n" } else { "plain" });
            let app = root.join(&name);
            let mut new = zgx();
            new.arg("new").arg(&app).args(["--template", template]);
            if i18n {
                new.arg("--i18n");
            }
            let (code, text) = run(&mut new);
            assert_eq!(code, Some(0), "{name}: {text}");
            assert!(
                text.contains("zgx run ."),
                "{name} says what's next: {text}"
            );

            let files = listing(&app);
            for promised in [
                "zgx.json",
                "app.luau",
                "theme.luau",
                ".luaurc",
                ".gitignore",
                "README.md",
                ".github/workflows/zgx.yml",
                "tests/app.test.luau",
                ".zgx/types.d.luau",
            ] {
                assert!(
                    files.iter().any(|f| f == promised),
                    "{name} lacks {promised}: {files:?}"
                );
            }
            assert!(
                files.iter().any(|f| f.starts_with("components/")),
                "{name}: {files:?}"
            );
            assert_eq!(
                files.iter().any(|f| f == "locales/en.json"),
                i18n,
                "{name}: locales only with --i18n: {files:?}"
            );
            let everything: String = files
                .iter()
                .map(|f| std::fs::read_to_string(app.join(f)).unwrap())
                .collect();
            for placeholder in ["{{name}}", "{{title}}", "{{template}}", "{{version}}"] {
                assert!(
                    !everything.contains(placeholder),
                    "{name}: {placeholder} left unfilled"
                );
            }
            let title = zgx_script::scaffold::title(&name);
            let config = std::fs::read_to_string(app.join("zgx.json")).unwrap();
            assert!(
                config.contains(&format!("\"title\": \"{title}\"")),
                "{config}"
            );

            let (code, text) = run(zgx().arg("lint").arg(&app));
            assert_eq!(code, Some(0), "{name} lints clean: {text}");
            let (code, text) = run(zgx().arg("test").arg(&app));
            assert_eq!(code, Some(0), "{name} passes its tests: {text}");
            assert!(text.contains(" 0 failed"), "{name}: {text}");
            if zgx_script::check::find_luau_lsp().is_some() {
                let (code, text) = run(zgx().arg("check").arg(&app));
                assert_eq!(code, Some(0), "{name} type checks: {text}");
                assert!(text.contains("0 errors, 0 warnings"), "{name}: {text}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_template_loads_without_errors_or_warnings() {
    for (template, _) in TEMPLATES {
        let root = scratch(&format!("load-{template}"));
        let app = root.join("app");
        zgx_script::scaffold::create(&app, template, false).unwrap();
        let mut test = zgx_script::testing::TestApp::new(|runtime| {
            zgx_script::ScriptHost::new(runtime, app.join("app.luau"))
        });
        test.frames(3);
        assert_eq!(test.error(), None, "{template}");
        assert!(!test.tree().is_empty(), "{template}");
        let warnings = test.host.warnings();
        assert!(warnings.is_empty(), "{template} warns: {warnings:?}");
        drop(test);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn it_refuses_a_bad_name_an_unknown_template_and_a_full_directory() {
    let root = scratch("refuse");
    let (code, text) = run(zgx().arg("new").arg(root.join("9lives")));
    assert_eq!(code, Some(2), "{text}");
    assert!(text.contains("can't name an app"), "{text}");
    let (code, text) = run(zgx()
        .arg("new")
        .arg(root.join("app"))
        .args(["--template", "blog"]));
    assert_eq!(code, Some(2), "{text}");
    assert!(
        text.contains("no template `blog`") && text.contains("basic"),
        "{text}"
    );
    std::fs::create_dir_all(root.join("full")).unwrap();
    std::fs::write(root.join("full/notes.txt"), "mine").unwrap();
    let (code, text) = run(zgx().arg("new").arg(root.join("full")));
    assert_eq!(code, Some(2), "{text}");
    assert!(text.contains("isn't empty"), "{text}");
    assert_eq!(
        std::fs::read_to_string(root.join("full/notes.txt")).unwrap(),
        "mine",
        "nothing is overwritten"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_name_reads_as_a_title() {
    assert_eq!(zgx_script::scaffold::title("my-notes"), "My Notes");
    assert_eq!(zgx_script::scaffold::title("todo_app2"), "Todo App2");
}
