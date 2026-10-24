//! `zgx lint` as tools run it: an exit status and, with `--format json`,
//! findings they can read.
use std::process::Command;

fn lint(source: &str, json: bool) -> (Option<i32>, String) {
    let dir = std::env::temp_dir().join(format!("zgx-lint-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(if json { "json.luau" } else { "text.luau" });
    std::fs::write(&file, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_zgx"));
    command.arg("lint");
    if json {
        command.args(["--format", "json"]);
    }
    let output = command.arg(&file).output().unwrap();
    (
        output.status.code(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

#[test]
fn findings_exit_one_and_print_as_json() {
    let bad = "local n = ui.state(\"n\", 0)\nreturn ui.text { \"n: \" .. n:get() }\n";
    let (status, out) = lint(bad, true);
    assert_eq!(status, Some(1));
    let report: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["files"], 1);
    let finding = &report["findings"][0];
    assert_eq!(finding["rule"], "untracked-get");
    assert_eq!(finding["line"], 2);
    assert!(finding["message"].as_str().unwrap().contains("n:get()"));

    let clean = "local n = ui.state(\"n\", 0)\nreturn ui.text { \"n: {n}\" }\n";
    let (status, out) = lint(clean, true);
    assert_eq!(status, Some(0));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["findings"],
        serde_json::json!([])
    );
    let (status, out) = lint(bad, false);
    assert_eq!(status, Some(1));
    assert!(out.contains("warning[untracked-get]"), "{out}");
}

#[test]
fn a_container_query_through_a_ref_is_clean() {
    // docs/scripting.md's responsive-layout recipe: a ref's size read in a
    // computed and in a text function is followed, not an untracked read.
    let recipe = r#"local root = ui.ref()
local wide = ui.computed(function()
	return (root.width or 1200) >= 1000
end)
return ui.row {
	ref = root,
	w_full = true,
	ui.show(function()
		return wide:get()
	end, ui.column { w = 220, shrink = 0, "Mailboxes" }),
	ui.text { function()
		return if (root.width or 0) > 500 then "wide" else "narrow"
	end },
}
"#;
    let (status, out) = lint(recipe, false);
    assert_eq!(status, Some(0), "{out}");
}
