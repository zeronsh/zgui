//! `zgx audit` (`zgx-run --audit`): every Luau example audits clean,
//! explored past its first screen, and a broken app's findings name their
//! element, the line that built it and the state that opened it, with the
//! exit status CI reads.
use std::path::{Path, PathBuf};
use std::process::Command;
use zgx_run::audit::audit_app;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zgx-script/examples")
        .join(name)
}

#[test]
fn the_examples_audit_clean_through_what_they_open() {
    for name in [
        "catalog",
        "dashboard",
        "mail",
        "chat",
        "team",
        "sketch",
        "code_editor",
        "notes",
        "popups",
        "selectable",
        "composer_states",
    ] {
        let audit = audit_app(&example(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(audit.findings.is_empty(), "{name}: {}", audit.to_json());
        assert!(audit.errors.is_empty(), "{name}: {}", audit.to_json());
    }
    // The catalogue's commands, its buttons pressed (its menu, popover,
    // dialog, toasts) and more are each a state of their own.
    let catalog = audit_app(&example("catalog")).unwrap();
    for state in [
        "start",
        "command:palette",
        "press:#view-menu",
        "press:#filters",
        "press:#open-dialog",
    ] {
        assert!(
            catalog.states.iter().any(|s| s == state),
            "{state}: {:?}",
            catalog.states
        );
    }
}

const BROKEN: &str = r#"local open = ui.state("open", false)
return ui.column {
	p = 16,
	gap = 8,
	ui.button { id = "unnamed", on_click = function() end },
	ui.div { id = "file", focusable = true, ui.text { "Report.pdf" } },
	ui.button { id = "more", "More", on_click = ui.toggle(open) },
	ui.modal { open = open, ui.button { "Close", on_click = ui.set(open, false) } },
}
"#;

fn broken() -> tempfile_dir::Dir {
    let dir = tempfile_dir::Dir::new("zgx-audit");
    std::fs::write(dir.path().join("app.luau"), BROKEN).unwrap();
    dir
}

#[test]
fn a_broken_apps_findings_name_element_line_and_state() {
    let dir = broken();
    let audit = audit_app(dir.path()).unwrap();
    let found: Vec<_> = audit
        .findings
        .iter()
        .map(|f| {
            (
                f.rule,
                f.target.as_str(),
                f.source.as_deref(),
                f.state.as_str(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("no-name", "#unnamed", Some("app.luau:5"), "start"),
            ("no-role", "#file", Some("app.luau:6"), "start"),
            // Only once the button opened it.
            ("no-name", found[2].1, Some("app.luau:8"), "press:#more"),
        ],
        "{}",
        audit.to_json()
    );
    assert_eq!(audit.findings[2].role, "Dialog");
}

#[test]
fn the_command_prints_json_and_fails_on_findings() {
    let dir = broken();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_zgx-run"))
            .arg("--audit")
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&[dir.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["findings"].as_array().unwrap().len(), 3);
    assert_eq!(json["findings"][0]["source"], "app.luau:5");
    let human = run(&["--human", dir.path().to_str().unwrap()]);
    assert_eq!(human.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&human.stdout).contains("3 findings in 3 states"));
    let clean = run(&[example("mail").to_str().unwrap()]);
    assert_eq!(
        clean.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&clean.stdout)
    );
    let missing = run(&["/nonexistent/app"]);
    assert_eq!(missing.status.code(), Some(2));
}

/// A route only the router reaches: its findings are that route's, a
/// route's `:params` given `1`.
#[test]
fn routes_are_visited() {
    let dir = tempfile_dir::Dir::new("zgx-audit-routes");
    std::fs::write(
        dir.path().join("app.luau"),
        r#"local router = ui.router { routes = { home = "/", thread = "/thread/:id" } }
return router.view {
	home = function()
		return ui.text { "Inbox" }
	end,
	thread = function(params)
		return ui.button { id = "reply-" .. params.id, on_click = function() end }
	end,
}
"#,
    )
    .unwrap();
    let audit = audit_app(dir.path()).unwrap();
    assert!(
        audit.states.iter().any(|s| s == "route:/thread/1"),
        "{:?}",
        audit.states
    );
    let found: Vec<_> = audit
        .findings
        .iter()
        .map(|f| (f.rule, f.target.as_str(), f.state.as_str()))
        .collect();
    assert_eq!(
        found,
        [("no-name", "#reply-1", "route:/thread/1")],
        "{}",
        audit.to_json()
    );
}

/// A directory removed with the test.
mod tempfile_dir {
    pub struct Dir(std::path::PathBuf);
    impl Dir {
        pub fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
        pub fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
