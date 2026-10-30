//! The machine beyond the window (`ui.clipboard`, `ui.open_files`,
//! `ui.save_file`, `ui.read_file`, `ui.write_file`) through a platform the
//! window provides: here a fake, as a desktop window provides the real one.
mod common;
use common::Harness;
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::platform::{FilePicker, Pending, Platform, PlatformService};
use zgx_script::ScriptHost;

struct Fake {
    clipboard: RefCell<String>,
    image: RefCell<Option<std::sync::Arc<zgx::image::ImageData>>>,
    /// What the open dialog picks, and where the save dialog saves.
    opens: PathBuf,
    saves: PathBuf,
}
impl Platform for Fake {
    fn clipboard_text(&self) -> Option<String> {
        Some(self.clipboard.borrow().clone())
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        *self.clipboard.borrow_mut() = text;
        true
    }
    fn clipboard_image(&self) -> Pending<Option<zgx::image::ImageData>> {
        // A copy sharing the pixels.
        let image = (self.image.borrow().as_ref())
            .map(|image| image.transformed(zgx::affine::Affine::IDENTITY));
        Box::pin(async move { image })
    }
    fn set_clipboard_image(&self, image: std::sync::Arc<zgx::image::ImageData>) -> Pending<bool> {
        *self.image.borrow_mut() = Some(image);
        Box::pin(async { true })
    }
    fn open_files(&self, _: FilePicker) -> Pending<Result<Vec<PathBuf>, String>> {
        let path = self.opens.clone();
        Box::pin(async move { Ok(vec![path]) })
    }
    fn save_file(&self, _: FilePicker) -> Pending<Result<Option<PathBuf>, String>> {
        let path = self.saves.clone();
        Box::pin(async move { Ok(Some(path)) })
    }
}

const APP: &str = r#"
local shown = ui.state("shown", "")
local kept = ui.state("kept", nil)
return ui.column {
    ui.text { id = "label", function() return shown:get() end },
    ui.button { id = "copy", w = 40, h = 20, "c", on_click = function()
        ui.clipboard.write("copied text")
        shown:set(ui.clipboard.read() or "")
    end },
    ui.button { id = "image", w = 40, h = 20, "i", on_click = function()
        ui.task(function()
            local before = ui.clipboard.read_image():await()
            local mask = ui.bitmap(3, 2, function(x, y) return x / 2 end)
            local written = ui.clipboard.write_image(mask):await()
            -- Kept in state, as a pasted image to show is.
            kept:set(ui.clipboard.read_image():await())
            shown:set(`{before == nil} {written} {kept:get().w}x{kept:get().h}`)
        end)
    end },
    ui.button { id = "open", w = 40, h = 20, "o", on_click = function()
        ui.task(function()
            local files = ui.open_files({ filters = { { "Text", { "txt" } } } }):await()
            shown:set(`{files[1].name}: {ui.read_file(files[1]):await()}`)
        end)
    end },
    ui.button { id = "save", w = 40, h = 20, "s", on_click = function()
        ui.task(function()
            local file = ui.save_file({ name = "out.txt" }):await()
            ui.write_file(file, "saved text"):await()
            shown:set(`wrote {file.name}`)
        end)
    end },
    ui.button { id = "sneak", w = 40, h = 20, "x", on_click = function()
        ui.read_file("/etc/hosts")
    end },
}
"#;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-io-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn app(dir: &std::path::Path, code: &'static str, plugin: Option<&'static str>) -> Harness {
    let fake = Rc::new(Fake {
        clipboard: RefCell::new(String::new()),
        image: RefCell::new(None),
        opens: dir.join("in.txt"),
        saves: dir.join("out.txt"),
    });
    Harness::wrapped(
        move |runtime| {
            let host = ScriptHost::inline(runtime, "app", code);
            match plugin {
                Some(code) => host.plugin("peek.luau", code),
                None => host,
            }
        },
        move |view| zgx::compose::provide(PlatformService(fake), view),
    )
}
fn wait_for(h: &mut Harness, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        h.settle();
        h.ui.prepare_frame();
        if h.text("label") == text {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!(
        "label reads {:?}, not {text:?} ({:?})",
        h.text("label"),
        h.host.error().get()
    );
}

#[test]
fn the_clipboard_takes_and_gives_text() {
    let dir = dir("clipboard");
    let mut h = app(&dir, APP, None);
    h.click("copy");
    wait_for(&mut h, "copied text");
}

#[test]
fn the_clipboard_takes_and_gives_images() {
    let dir = dir("clipboard-image");
    let mut h = app(&dir, APP, None);
    h.click("image");
    wait_for(&mut h, "true true 3x2");
}

#[test]
fn a_picked_file_reads_and_a_saved_one_is_written() {
    let dir = dir("files");
    std::fs::write(dir.join("in.txt"), "hello from disk").unwrap();
    let mut h = app(&dir, APP, None);
    h.click("open");
    wait_for(&mut h, "in.txt: hello from disk");
    h.click("save");
    wait_for(&mut h, "wrote out.txt");
    assert_eq!(
        std::fs::read_to_string(dir.join("out.txt")).unwrap(),
        "saved text"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_file_the_user_did_not_pick_is_out_of_reach() {
    let dir = dir("sneak");
    let mut h = app(&dir, APP, None);
    h.click("sneak");
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("only files the user picked"), "{error}");
}

#[test]
fn a_plugin_needs_its_hosts_grant() {
    let dir = dir("plugin");
    let h = app(
        &dir,
        r#"return ui.text { "app", id = "label" }"#,
        Some(r#"ui.clipboard.read()"#),
    );
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("Capability::Clipboard"), "{error}");
    let h = app(
        &dir,
        r#"return ui.text { "app", id = "label" }"#,
        Some(r#"ui.clipboard.read_image()"#),
    );
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("Capability::Clipboard"), "{error}");
}

#[test]
fn without_a_platform_it_says_so() {
    let mut h = Harness::new(|runtime| ScriptHost::inline(runtime, "app", APP));
    h.click("copy");
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("need a desktop window"), "{error}");
}

/// `ui.args` is what the app was launched with; `ui.on_open` is handed the
/// files and URLs the system opens with it, the files the script's to read
/// as a dialog's are, and a launch's waiting until the script takes them.
#[test]
fn launch_arguments_and_files_the_system_opens_reach_the_script() {
    let file = std::env::temp_dir().join(format!("zgx-on-open-{}.txt", std::process::id()));
    std::fs::write(&file, "opened by the system").unwrap();
    let (opened, given) = (file.clone(), file.clone());
    let mut h = Harness::with_desktop(move |runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"local args = ui.state("args", table.concat(ui.args, " "))
            local got = ui.state("got", "")
            ui.on_open(function(e)
                local urls = table.concat(e.urls, " ")
                for _, file in e.files do
                    ui.task(function()
                        got:set(`{file.name}: {ui.read_file(file):await()} {urls}`)
                    end)
                end
                if #e.files == 0 then got:set(urls) end
            end)
            return ui.column { ui.text { "{args}", id = "args" }, ui.text { "{got}", id = "got" } }"#,
        )
        .args(["--verbose", "notes.txt"]);
        // Asked before the script loads (a launch's file): it waits.
        host.open(vec![opened], vec!["zgx://launch".into()]);
        host
    });
    assert_eq!(h.text("args"), "--verbose notes.txt");
    let name = given.file_name().unwrap().to_string_lossy().into_owned();
    assert!(h.wait_until(|h| h.text("got").starts_with(&name)));
    assert_eq!(
        h.text("got"),
        format!("{name}: opened by the system zgx://launch")
    );
    h.host.open(vec![], vec!["https://example.org/a".into()]);
    h.settle();
    assert_eq!(h.text("got"), "https://example.org/a");
    let _ = std::fs::remove_file(&file);
}

/// Files copied in a file manager reach `on_paste`, and read as what the
/// person picked in a dialog does; a plugin without the grants hears none.
#[test]
fn files_on_the_clipboard_reach_on_paste_and_can_be_read() {
    let file = std::env::temp_dir().join(format!("zgx-pasted-{}.txt", std::process::id()));
    std::fs::write(&file, "pasted file").unwrap();
    let dir = std::env::temp_dir().join(format!("zgx-paste-plugin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("plugin.luau");
    std::fs::write(
        &entry,
        r#"local heard = ui.state("heard", "not yet")
        ui.contribute("bar", ui.column {
            ui.textarea { id = "plugin", value = ui.state("text", ""), w = 100, h = 30,
                on_paste = function(e)
                    heard:set(`{#e.files} files`)
                    return true
                end },
            ui.text { id = "plugin-heard", "{heard}" },
        })"#,
    )
    .unwrap();
    let mut h = Harness::with_desktop(move |runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"local got = ui.state("got", "")
            return ui.column {
                ui.textarea { id = "box", value = ui.state("text", ""), w = 200, h = 60,
                    on_paste = function(e)
                        for _, file in e.files do
                            ui.task(function()
                                got:set(`{file.name}: {ui.read_file(file):await()}`)
                            end)
                        end
                        return #e.files > 0
                    end },
                ui.slot("bar"),
                ui.text { id = "got", "{got}" },
            }"#,
        );
        host.set_plugins(vec![zgx_script::Plugin {
            name: "paste".into(),
            entry: entry.clone(),
            grants: vec![zgx_script::Capability::Clipboard],
        }]);
        host
    });
    h.frames(1);
    let paste = |h: &mut Harness, id: &str, text: Option<&str>| {
        let node = h.find(id).unwrap();
        let editor = h.ui.editor(node).unwrap();
        h.ui.paste_into(&editor, text);
        h.settle();
        h.frames(2);
    };
    // No files: as ever, the text goes in.
    paste(&mut h, "box", Some("plain"));
    let name = h.find("box").unwrap();
    let typed =
        h.ui.editor(name)
            .unwrap()
            .editor
            .borrow()
            .text()
            .to_string();
    assert_eq!(typed, "plain");
    // Files, with their name as the text as Finder puts it: taken, and read.
    h.desktop.as_ref().unwrap().copy_files(vec![file.clone()]);
    let copied = file.file_name().unwrap().to_string_lossy().into_owned();
    paste(&mut h, "box", Some(&copied));
    assert!(
        h.wait_until(|h| !h.text("got").is_empty()),
        "{:?}",
        h.error()
    );
    assert_eq!(h.text("got"), format!("{copied}: pasted file"));
    let typed =
        h.ui.editor(name)
            .unwrap()
            .editor
            .borrow()
            .text()
            .to_string();
    assert_eq!(typed, "plain", "taken: its name doesn't go in");
    // A plugin granted the clipboard but not files hears none.
    paste(&mut h, "plugin", None);
    assert_eq!(h.text("plugin-heard"), "0 files");
    assert_eq!(h.error(), None);
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir_all(&dir);
}
