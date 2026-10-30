//! `zgx test`: an app's own Luau tests (`*.test.luau`), each against the
//! app mounted headless and driven by devtools' targets.
use std::{path::PathBuf, process::Command};

const APP: &str = r#"
local count = ui.state("count", 0)
local name = ui.state("name", "")
local saved = ui.state("saved", false)
ui.command { id = "reset", title = "Reset", run = function() count:set(0) end }
return ui.column {
    ui.text { id = "label", "Count: {count}" },
    ui.button { id = "inc", w = 60, h = 24, "Add", on_click = function() count:add(1) end },
    ui.input { id = "name", label = "Name", value = name },
    ui.text { id = "echo", "Hello, {name}" },
    ui.button { id = "save", w = 60, h = 24, "Save", on_click = function()
        ui.after(50, function() saved:set(true) end)
    end },
    ui.show(saved, ui.text { id = "done", "Saved" }),
}
"#;

fn zgx() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zgx"))
}
/// An app with `APP` and the test files `tests`, in a directory of its own.
fn app(name: &str, tests: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-test-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(dir.join("app.luau"), APP).unwrap();
    for (file, code) in tests {
        std::fs::write(dir.join("tests").join(file), code).unwrap();
    }
    dir
}
fn run(dir: &PathBuf, args: &[&str]) -> (Option<i32>, String) {
    let out = zgx().arg("test").arg(dir).args(args).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

#[test]
fn passing_tests_drive_the_app_by_devtools_targets() {
    let dir = app(
        "pass",
        &[(
            "app.test.luau",
            r##"
test("adding counts", function()
    local app = mount()
    expect(app:text("#label")).to_be("Count: 0")
    app:click('role:button "Add"')
    app:click("#inc")
    expect(app:text("#label")).to_be("Count: 2")
    expect(app:state("count")).to_be(2)
end)

test("each test mounts a fresh app", function()
    expect(mount():state("count")).to_be(0)
end)

test("typing, commands, state and waiting", function()
    local app = mount()
    app:type("#name", "Ada")
    expect(app:text("#echo")).to_contain("Ada")
    app:set_state("count", 5)
    expect(app:text("#label")).to_be("Count: 5")
    app:command("reset")
    expect(app:text("#label")).to_be("Count: 0")
    expect(app:has('"Saved"')).to_be(false)
    app:click("#save")
    expect(app:wait('"Saved"', { timeout_ms = 2000 })).to_be(true)
    expect(app:count("role:button")).to_be(2)
    expect(app:error()).to_be_nil()
    expect({ 1, { a = 2 } }).to_equal({ 1, { a = 2 } })
    expect("abc").never.to_contain("d")
end)
"##,
        )],
    );
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("ok   adding counts"), "{text}");
    assert!(text.contains("ok   each test mounts a fresh app"), "{text}");
    assert!(text.contains("3 passed, 0 failed"), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failure_names_its_file_and_line() {
    let dir = app(
        "fail",
        &[(
            "app.test.luau",
            "test(\"wrong count\", function()\n\
             \tlocal app = mount()\n\
             \texpect(app:text(\"#label\")).to_be(\"Count: 1\")\n\
             end)\n\
             test(\"no such node\", function()\n\
             \tmount():click(\"#missing\")\n\
             end)\n\
             test(\"passes\", function() end)\n",
        )],
    );
    let file = dir.join("tests/app.test.luau");
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains(&format!(
            "{}:3: expected \"Count: 0\" to be \"Count: 1\"",
            file.display()
        )),
        "{text}"
    );
    assert!(text.contains("FAIL no such node"), "{text}");
    assert!(
        text.contains(&format!("{}:6: ", file.display())) && text.contains("#missing"),
        "a target that matches nothing fails at the test's line: {text}"
    );
    assert!(text.contains("1 passed, 2 failed"), "{text}");
    assert!(
        !text.contains("stack traceback"),
        "the runner's frames are left out: {text}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_filter_runs_the_tests_whose_names_hold_it() {
    let dir = app(
        "filter",
        &[(
            "app.test.luau",
            "test(\"saving\", function() end)\ntest(\"loading\", function() error(\"ran\") end)\n",
        )],
    );
    let (code, text) = run(&dir, &["sav"]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_file_that_fails_to_load_or_an_app_that_fails_to_mount_fails() {
    let dir = app(
        "broken",
        &[
            ("a.test.luau", "test(\"x\", function() end\n"),
            (
                "b.test.luau",
                "test(\"mounts\", function() mount(\"missing.luau\") end)\n",
            ),
        ],
    );
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("FAIL (file)"), "{text}");
    assert!(
        text.contains("FAIL mounts") && text.contains("mount:"),
        "{text}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_directory_without_tests_or_an_app_is_an_error() {
    let dir = app("none", &[]);
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(2), "{text}");
    assert!(text.contains("no tests under"), "{text}");
    let (code, text) = run(&dir.join("tests"), &[]);
    assert_eq!(code, Some(2), "{text}");
    assert!(text.contains("holds no app.luau"), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// With luau-lsp: a test file knows `test`, `mount` and `expect` and the
/// app's methods; the app's own script does not.
#[test]
fn check_gives_the_test_globals_to_test_files_alone() {
    if zgx_script::check::find_luau_lsp().is_none() {
        eprintln!("skipped: luau-lsp is not installed");
        return;
    }
    let dir = app(
        "check",
        &[(
            "app.test.luau",
            "test(\"t\", function()\n\tlocal app = mount()\n\tapp:click(\"#inc\")\n\
             \texpect(app:text(\"#label\")).never.to_be(\"\")\n\tapp:clik(\"#inc\")\nend)\n",
        )],
    );
    std::fs::write(dir.join("helper.luau"), "mount()\nreturn nil\n").unwrap();
    let out = zgx().arg("check").arg(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let test_file = dir.join("tests/app.test.luau");
    assert!(
        stdout.contains(&format!("{}:5:", test_file.display())),
        "a misspelled method is a type error: {stdout}"
    );
    assert!(
        !stdout.contains(&format!("{}:2:", test_file.display())),
        "mount is known: {stdout}"
    );
    assert!(
        stdout.contains("helper.luau:1:") && stdout.contains("'mount'"),
        "an app's script doesn't see the test globals: {stdout}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
