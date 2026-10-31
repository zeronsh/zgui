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
fn a_test_drags_one_node_onto_another() {
    let dir = app(
        "drag",
        &[(
            "drag.test.luau",
            r##"
test("a card lands", function()
    local app = mount()
    app:drag("#card", "#bin")
    expect(app:state("landed")).to_be("card")
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("app.luau"),
        r##"local landed = ui.state("landed", "none")
return ui.column {
    w = 300, h = 200,
    ui.column { id = "card", w = 100, h = 40, on_drag = function() return "card" end },
    ui.column { id = "bin", w = 100, h = 40,
        on_drop = function(payload: string) landed:set(payload) end },
}
"##,
    )
    .unwrap();
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_test_hovers_and_says_nothing_of_having_no_storage() {
    let dir = app(
        "hover",
        &[(
            "hover.test.luau",
            r##"
test("the pointer comes and goes", function()
    local app = mount()
    app:hover("#a")
    expect(app:state("over")).to_be("a")
    app:hover("#b")
    expect(app:state("over")).to_be("b")
    app:set_state("saved", "changed")
    app:frames(2)
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("app.luau"),
        r##"local over = ui.state("over", "none")
local saved = ui.persist("saved", "first")
local function box(id: string)
    return ui.column { id = id, w = 100, h = 40, on_hover = function(inside: boolean)
        if inside then over:set(id) end
    end }
end
return ui.column { box("a"), box("b"), ui.text { saved } }
"##,
    )
    .unwrap();
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{text}");
    assert!(!text.contains("no storage"), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_test_answers_the_desktops_dialogs_and_resizes_the_window() {
    let dir = app(
        "desktop",
        &[(
            "desktop.test.luau",
            r##"
test("a fixture is copied in, a folder whole", function()
    local app = mount("desktop.luau")
    app:copy_file("fixtures", "lib")
    expect(app:read_file("lib/deep/note.txt")).to_be("fixture")
    local ok, message = pcall(function() app:copy_file("../zgx-outside-fixture.txt", "x") end)
    expect(ok).to_be(false)
    expect(tostring(message)).to_contain("app's directory")
end)

test("a file saved, picked and copied", function()
    local app = mount("desktop.luau")
    app:save_to("out/report.csv")
    app:click("#export")
    expect(app:wait("#saved")).to_be(true)
    expect(app:read_file("out/report.csv")).to_be("a,b")

    app:write_file("in.txt", "from disk")
    app:pick({ "in.txt" })
    app:click("#import")
    expect(app:wait("#opened", { text = "from disk" })).to_be(true)

    app:copy("pasted")
    app:click("#paste")
    expect(app:wait("#clip", { text = "pasted" })).to_be(true)
    app:click("#yank")
    expect(app:clipboard()).to_be("yanked")
end)

test("a dialog nobody answers is cancelled", function()
    local app = mount("desktop.luau")
    app:click("#export")
    app:frames(2)
    expect(app:has("#saved")).to_be(false)
    expect(app:error()).to_be_nil()
end)

test("a resize lays out and reaches ui.window.size", function()
    local app = mount("desktop.luau")
    expect(app:has('"wide"')).to_be(true)
    app:resize(500, 400)
    expect(app:has('"narrow"')).to_be(true)
    expect(app:text("#cols")).to_be("1")
    app:resize(1200, 400)
    expect(app:has('"wide"')).to_be(true)
    expect(app:text("#cols")).to_be("4")
end)

test("a file's name stays in the test's directory", function()
    local app = mount("desktop.luau")
    local ok, message = pcall(function() app:read_file("../secret") end)
    expect(ok).to_be(false)
    expect(tostring(message)).to_contain("own directory")
end)
"##,
        )],
    );
    std::fs::create_dir_all(dir.join("fixtures/deep")).unwrap();
    std::fs::write(
        dir.parent().unwrap().join("zgx-outside-fixture.txt"),
        "secret",
    )
    .unwrap();
    std::fs::write(dir.join("fixtures/deep/note.txt"), "fixture").unwrap();
    std::fs::write(
        dir.join("desktop.luau"),
        r#"
local saved = ui.state("saved", false)
local opened = ui.state("opened", "")
local clip = ui.state("clip", "")
local narrow = ui.computed(function() return ui.window.size:get().width < 760 end)
local box = ui.ref()
local cols = ui.computed(function() return math.floor((box.width or 0) / 300) end)
return ui.column {
    ui.button { id = "export", "Export", on_click = function()
        ui.task(function()
            local file = ui.save_file({ title = "Export" }):await()
            if file then
                ui.write_file(file, "a,b"):await()
                saved:set(true)
            end
        end)
    end },
    ui.button { id = "import", "Import", on_click = function()
        ui.task(function()
            local files = ui.open_files({ title = "Import" }):await()
            if files[1] then opened:set(ui.read_file(files[1]):await()) end
        end)
    end },
    ui.button { id = "paste", "Paste", on_click = function()
        clip:set(ui.clipboard.read() or "")
    end },
    ui.button { id = "yank", "Yank", on_click = function() ui.clipboard.write("yanked") end },
    ui.show(saved, ui.text { id = "saved", "Saved" }),
    ui.text { id = "opened", "{opened}" },
    ui.text { id = "clip", "{clip}" },
    ui.text { function() return if narrow:get() then "narrow" else "wide" end },
    ui.column { ref = box, ui.text { id = "cols", function() return tostring(cols:get()) end } },
}
"#,
    )
    .unwrap();
    let (code, output) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{output}");
}

#[test]
fn a_test_reads_a_fields_value_and_selection_and_part_of_a_text() {
    let dir = app(
        "reads",
        &[(
            "reads.test.luau",
            r##"
test("reads", function()
    local app = mount()
    app:type("#name", "Ada")
    expect(app:text("#name")).to_be("Ada")
    local anchor, focus = app:selection("#name")
    expect(anchor).to_be(3)
    expect(focus).to_be(3)
    expect(app:selection("#label")).to_be_nil()
    expect(app:text('~"Hello, A"')).to_be("Hello, Ada")
    expect(app:has('~"nothing like it"')).to_be(false)
    expect(app:tree("#echo")).to_contain("Hello, Ada")
    expect(app:tree("#echo")).never.to_contain("Count")
    app:double_click("#inc")
    expect(app:text("#label")).to_be("Count: 2")
end)
"##,
        )],
    );
    let (code, output) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{output}");
}

#[test]
fn a_folder_a_person_picks_is_the_only_one_a_script_reaches() {
    let dir = app(
        "folder",
        &[(
            "folder.test.luau",
            r##"
local function open(app): boolean
    app:write_file("vault/notes/a.md", "# A")
    app:write_file("vault/b.md", "# B")
    app:pick_folder("vault")
    app:click("#open")
    return app:wait("#name", { text = "vault" })
end

test("it lists, reads and writes inside what was picked", function()
    local app = mount("folder.luau")
    expect(open(app)).to_be(true)
    app:click("#list")
    expect(app:wait("#listed", { text = "b.md notes notes/a.md" })).to_be(true)
    app:click("#read")
    expect(app:wait("#read-out", { text = "# A" })).to_be(true)
    app:click("#write")
    expect(app:wait("#written", { text = "yes" })).to_be(true)
    expect(app:read_file("vault/new/c.md")).to_be("written")
end)

test("a cancelled dialog gives nothing, and the app goes on", function()
    local app = mount("folder.luau")
    app:click("#open")
    app:frames(3)
    expect(app:text("#name")).to_be("none")
    expect(app:error()).to_be_nil()
end)

test("paths out of it are refused, as an error to handle", function()
    local app = mount("folder.luau")
    expect(open(app)).to_be(true)
    app:click("#escape")
    expect(app:wait("#refused", { text = "denied" })).to_be(true)
end)

test("a handle survives in state and can't be made from a table", function()
    local app = mount("folder.luau")
    expect(open(app)).to_be(true)
    app:click("#forge")
    expect(app:wait("#forged", { text = "no" })).to_be(true)
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("folder.luau"),
        r##"
local folder = ui.state("folder", nil :: Folder?)
local name = ui.state("name", "none")
local listed = ui.state("listed", "")
local read_out = ui.state("read_out", "")
local written = ui.state("written", "")
local refused = ui.state("refused", "")
local forged = ui.state("forged", "")
return ui.column {
    ui.button { id = "open", "Open", on_click = function()
        ui.task(function()
            local picked = ui.open_folder():await()
            folder:set(picked)
            if picked then name:set(picked.name) end
        end)
    end },
    ui.button { id = "list", "List", on_click = function()
        ui.task(function()
            local paths = {}
            for _, entry in folder:peek():list("", { recursive = true }):await() do
                table.insert(paths, entry.path)
            end
            listed:set(table.concat(paths, " "))
        end)
    end },
    ui.button { id = "read", "Read", on_click = function()
        ui.task(function() read_out:set(folder:peek():read("notes/a.md"):await()) end)
    end },
    ui.button { id = "write", "Write", on_click = function()
        ui.task(function()
            folder:peek():write("new/c.md", "written"):await()
            written:set("yes")
        end)
    end },
    ui.button { id = "escape", "Escape", on_click = function()
        ui.task(function()
            local ok, failure = ui.try(function() return folder:peek():read("../outside"):await() end)
            refused:set(if ok then "read it" else failure.kind)
        end)
    end },
    ui.button { id = "forge", "Forge", on_click = function()
        local ok = pcall(function()
            return (folder:peek() :: any).list({ name = "x" }, "")
        end)
        forged:set(if ok then "yes" else "no")
    end },
    ui.text { id = "name", "{name}" },
    ui.text { id = "listed", "{listed}" },
    ui.text { id = "read-out", "{read_out}" },
    ui.text { id = "written", "{written}" },
    ui.text { id = "refused", "{refused}" },
    ui.text { id = "forged", "{forged}" },
}
"##,
    )
    .unwrap();
    let (code, output) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{output}");
}

#[test]
fn a_folders_files_can_be_read_shown_and_handed_on() {
    let dir = app(
        "folder-files",
        &[(
            "files.test.luau",
            r##"
test("a folder's file is a File the app may read and show", function()
    local app = mount("files.luau")
    app:write_file("lib/a.txt", "alpha")
    app:write_file("lib/deep/b.txt", "bravo")
    app:pick_folder("lib")
    app:click("#open")
    expect(app:wait("#listed", { text = "a.txt deep/b.txt" })).to_be(true)
    expect(app:wait("#read", { text = "alpha" })).to_be(true)
    app:click("#one")
    expect(app:wait("#one-out", { text = "bravo" })).to_be(true)
    app:click("#outside")
    expect(app:wait("#outside-out", { text = "denied" })).to_be(true)
    expect(app:error()).to_be_nil()
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("files.luau"),
        r##"
local folder = ui.state("folder", nil :: Folder?)
local listed = ui.state("listed", "")
local read = ui.state("read", "")
local one = ui.state("one", "")
local outside = ui.state("outside", "")
return ui.column {
    ui.button { id = "open", "Open", on_click = function()
        ui.task(function()
            local picked = ui.open_folder():await()
            if not picked then return end
            folder:set(picked)
            local paths, first = {}, nil
            for _, entry in picked:list("", { recursive = true, files = true }):await() do
                if entry.file then
                    table.insert(paths, entry.path)
                    first = first or entry.file
                end
            end
            listed:set(table.concat(paths, " "))
            -- A listed file reads as a dialog's does.
            read:set(ui.read_file(first):await())
        end)
    end },
    ui.button { id = "one", "One", on_click = function()
        ui.task(function()
            local file = folder:peek():file("deep/b.txt"):await()
            one:set(ui.read_file(file):await())
        end)
    end },
    ui.button { id = "outside", "Outside", on_click = function()
        ui.task(function()
            local ok, failure = ui.try(function() return folder:peek():file("../x"):await() end)
            outside:set(if ok then "reached" else failure.kind)
        end)
    end },
    ui.text { id = "listed", "{listed}" },
    ui.text { id = "read", "{read}" },
    ui.text { id = "one-out", "{one}" },
    ui.text { id = "outside-out", "{outside}" },
}
"##,
    )
    .unwrap();
    let (code, output) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{output}");
}

#[test]
fn an_action_says_what_hides_or_covers_its_target() {
    let dir = app(
        "reach",
        &[(
            "reach.test.luau",
            r##"
test("a clipped target, a covered one, and one in reach", function()
    local app = mount("reach.luau")
    local ok, message = pcall(function() app:click("#far") end)
    expect(ok).to_be(false)
    expect(tostring(message)).to_contain("outside")
    expect(tostring(message)).to_contain("in #list's clip")
    ok, message = pcall(function() app:click("#under") end)
    expect(ok).to_be(false)
    expect(tostring(message)).to_contain("covered")
    expect(tostring(message)).to_contain("#lid")
    app:click("#near")
    expect(app:text("#said")).to_be("near")
    app:scroll("#list", 400)
    app:click("#far")
    expect(app:text("#said")).to_be("far")
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("reach.luau"),
        r##"
local said = ui.state("said", "")
return ui.column {
    ui.scroll { id = "list", h = 100, w = 200,
        ui.button { id = "near", h = 40, "Near", on_click = function() said:set("near") end },
        ui.div { h = 300 },
        ui.button { id = "far", h = 40, "Far", on_click = function() said:set("far") end },
    },
    ui.overlay { w = 200, h = 40,
        ui.button { id = "under", "Under", on_click = function() said:set("under") end },
        ui.div { id = "lid", w = 200, h = 40, bg = "#000", on_click = function() end },
    },
    ui.text { id = "said", "{said}" },
}
"##,
    )
    .unwrap();
    let (code, output) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{output}");
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

/// Inside a popover, a menu and a submenu, a test reaches what shows, and a
/// click at a closed panel's button fails, saying what to open, rather than
/// landing on whatever is under it.
#[test]
fn a_test_reaches_into_popovers_and_menus_and_not_closed_ones() {
    let dir = app(
        "popups",
        &[(
            "popups.test.luau",
            r##"
test("open, then reach inside", function()
    local app = mount()
    local closed = pcall(function() app:click("#inside") end)
    expect(closed).to_be(false)
    app:click("#open")
    app:click("#inside")
    expect(app:state("log")).to_be("inside")
    app:click("#menu-open")
    app:click('role:menuitem "Share"')
    app:click("#copy")
    expect(app:state("log")).to_be("copy")
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("app.luau"),
        r##"local log = ui.state("log", "")
local pop = ui.state("pop", false)
local menu = ui.state("menu", false)
local share = ui.state("share", false)
return ui.row {
    ui.popover { open = pop, w = 160,
        anchor = ui.button { id = "open", on_click = ui.toggle(pop), "Open" },
        ui.column { ui.button { id = "inside", on_click = function() log:set("inside") end, "In" } } },
    ui.menu { open = menu, label = "More",
        anchor = ui.button { id = "menu-open", on_click = ui.toggle(menu), "Menu" },
        ui.submenu { open = share, label = "Share",
            ui.menu_item { id = "copy", on_click = function() log:set("copy") end, "Copy" } } },
}
"##,
    )
    .unwrap();
    let (code, text) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_test_acts_in_the_apps_other_windows() {
    let dir = app(
        "window",
        &[(
            "window.test.luau",
            r##"
test("a window opened, acted in and closed", function()
    local app = mount("windows.luau")
    expect(app:window("detail")).to_be_nil()
    app:click("#open")
    local detail = app:window("detail")
    expect(detail:text("#title")).to_be("Detail")
    -- One app: what the window does, the main window shows.
    detail:click("#add")
    expect(app:text("#count")).to_be("1")
    expect(detail:text("#count")).to_be("1")
    -- The same window while it is open, its content mounted once.
    expect(app:window("detail")).to_be(detail)
    detail:close()
    expect(app:state("detail")).to_be(false)
    expect(app:window("detail")).to_be_nil()
    local ok, message = pcall(function() detail:click("#add") end)
    expect(ok).to_be(false)
    expect(tostring(message)).to_contain("window `detail` is closed")
    expect(app:window("nowhere")).to_be_nil()
end)
"##,
        )],
    );
    std::fs::write(
        dir.join("windows.luau"),
        r##"
local detail = ui.state("detail", false)
local count = ui.state("count", 0)
ui.window {
    id = "detail",
    title = "Detail",
    width = 300,
    height = 200,
    open = detail,
    ui.column {
        ui.text { id = "title", "Detail" },
        ui.button { id = "add", "Add", on_click = ui.add(count, 1) },
        ui.text { id = "count", "{count}" },
    },
}
return ui.column {
    ui.button { id = "open", "Open", on_click = function() detail:set(true) end },
    ui.text { id = "count", "{count}" },
}
"##,
    )
    .unwrap();
    let (code, output) = run(&dir, &[]);
    assert_eq!(code, Some(0), "{output}");
    assert!(output.contains("1 passed"), "{output}");
}
