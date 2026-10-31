//! `zgx test`: an app's own tests, written in Luau. Each `*.test.luau`
//! file runs in a VM of its own, with three globals:
//!
//! - `test(name, body)` adds a test; the file's tests run in its order;
//! - `mount(path?)` mounts the app (its directory's `app.luau`, or `path`
//!   from that directory) headless, as [`TestApp`] does, and drives it with
//!   devtools' targets (`#id`, `"label"`, `role:button`, `#list row 3`,
//!   `a >> b`: [`crate::driver`]);
//! - `expect(actual)` checks a value (`to_be`, `to_equal`, `to_contain`,
//!   `to_be_truthy`, `to_be_nil`, and each negated under `never`).
//!
//! Their types are `types/zgx-test.d.luau`, which `zgx check` gives test
//! files alone. A headless app has no fetcher (`ui.fetch` fails as offline
//! does) and no storage (`ui.persist` keeps its state in memory).
use crate::{
    ScriptHost, Value,
    testing::{FakeDesktop, TestApp},
};
use mlua::{Function, Lua, Table, UserData, UserDataMethods};
use std::{
    cell::{OnceCell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zgx::{reactive::Signal, scene::Rect};

/// The definitions of a test file's globals, for luau-lsp.
pub const DEFINITIONS: &str = include_str!("../types/zgx-test.d.luau");

/// One test's result.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Its file.
    pub file: PathBuf,
    /// Its name, or `(file)` for an error loading the file itself.
    pub name: String,
    /// Why it failed (the error, at its line), or None when it passed.
    pub failure: Option<String>,
    /// How long it took.
    pub duration: Duration,
}

/// Every test's result, in the order they ran.
#[derive(Debug, Default)]
pub struct Report {
    /// Each test's, in order.
    pub outcomes: Vec<Outcome>,
}
impl Report {
    /// How many failed.
    pub fn failed(&self) -> usize {
        self.outcomes.iter().filter(|o| o.failure.is_some()).count()
    }
}

/// Whether `path` is a test file: its name ends in `.test.luau`.
pub fn is_test(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".test.luau"))
}

/// The test files under the app's directory `dir`, sorted.
pub fn test_files(dir: &Path) -> Vec<PathBuf> {
    crate::check::scripts(dir)
        .into_iter()
        .filter(|file| is_test(file))
        .collect()
}

/// Runs the tests of the app at `dir` (its `app.luau`) whose names contain
/// `filter`, telling `each` of each as it ends.
pub fn run(
    dir: &Path,
    filter: Option<&str>,
    mut each: impl FnMut(&Outcome),
) -> Result<Report, String> {
    if !dir.join("app.luau").is_file() {
        return Err(format!("{} holds no app.luau", dir.display()));
    }
    let files = test_files(dir);
    if files.is_empty() {
        return Err(format!(
            "no tests under {} (files named *.test.luau)",
            dir.display()
        ));
    }
    let mut report = Report::default();
    for file in files {
        let mut record = |name: String, failure: Option<String>, duration| {
            let outcome = Outcome {
                file: file.clone(),
                name,
                failure,
                duration,
            };
            each(&outcome);
            report.outcomes.push(outcome);
        };
        let start = Instant::now();
        let lua = Lua::new();
        let tests = match load(&lua, dir, &file) {
            Ok(tests) => tests,
            Err(error) => {
                record(
                    "(file)".into(),
                    Some(message(&error, &file)),
                    start.elapsed(),
                );
                continue;
            }
        };
        for (name, body) in tests {
            if filter.is_some_and(|filter| !name.contains(filter)) {
                continue;
            }
            let start = Instant::now();
            let failure = body
                .call::<()>(())
                .err()
                .map(|error| message(&error, &file));
            // Its apps go with it: the next test mounts fresh ones.
            let _ = lua.gc_collect();
            let _ = lua.gc_collect();
            record(name, failure, start.elapsed());
        }
    }
    Ok(report)
}

/// The file's tests, by name, once it has run.
fn load(lua: &Lua, dir: &Path, file: &Path) -> mlua::Result<Vec<(String, Function)>> {
    let globals = lua.globals();
    let tests = lua.create_table()?;
    let added = tests.clone();
    globals.set(
        "test",
        lua.create_function(move |lua, (name, body): (String, Function)| {
            let test = lua.create_table()?;
            test.set("name", name)?;
            test.set("body", body)?;
            added.push(test)
        })?,
    )?;
    let expect: Function = lua
        .load(include_str!("test_expect.luau"))
        .set_name("@zgx/expect")
        .call(())?;
    globals.set("expect", expect)?;
    let dir = dir.to_path_buf();
    let fixtures = dir.clone();
    globals.set(
        "mount",
        lua.create_function(move |_, path: Option<String>| {
            let path = dir.join(path.as_deref().unwrap_or("app.luau"));
            // Its packages, what it exposes to plugins and its plugins, as
            // `zgx run` gives them.
            let host = crate::app_config::read(&dir)
                .map_err(|error| mlua::Error::runtime(format!("mount: {error}")))?;
            let app = TestApp::try_with_desktop(|runtime| {
                let host = host.apply(ScriptHost::new(runtime, &path));
                host.0.persist.quiet();
                host
            })
            .map_err(|error| mlua::Error::runtime(format!("mount: {error}")))?;
            // The window's size, as `zgx run` opens it.
            let (width, height) = window_size(&dir);
            app.ui.scene.borrow_mut().resize(width, height);
            let viewport = app.ui.runtime.signal((width, height));
            app.host.window_size(viewport.clone());
            app.frames(1);
            Ok(Mounted {
                dir: fixtures.clone(),
                app: Rc::new(RefCell::new(app)),
                viewport,
                files: Files::default(),
            })
        })?,
    )?;
    let code = std::fs::read_to_string(file).map_err(mlua::Error::external)?;
    lua.load(code)
        .set_name(format!("@{}", file.display()))
        .exec()?;
    let mut out = Vec::new();
    for pair in tests.sequence_values::<Table>() {
        let pair = pair?;
        out.push((pair.get("name")?, pair.get("body")?));
    }
    Ok(out)
}

/// The window's size in points that the app's `zgx.json` sets, else 960 by
/// 640, as zgx-run opens it.
fn window_size(dir: &Path) -> (f32, f32) {
    let config = std::fs::read_to_string(dir.join("zgx.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let get = |key: &str, default: f32| {
        config
            .as_ref()
            .and_then(|config| config.get(key)?.as_f64())
            .map_or(default, |n| n as f32)
    };
    (get("width", 960.), get("height", 640.))
}

/// An error as a person reads it: the message at the test file's line,
/// without Lua's traceback through the runner. One the runner raised (no
/// node, a bad key) is placed at the test's line that called it.
fn message(error: &mlua::Error, file: &Path) -> String {
    match error {
        mlua::Error::CallbackError { cause, traceback } => {
            let cause = message(cause, file);
            let name = file.display().to_string();
            let at = traceback
                .lines()
                .map(str::trim)
                .find(|line| line.starts_with(&name))
                .and_then(|line| {
                    let line = line[name.len()..].strip_prefix(':')?;
                    let digits = line.split(':').next()?;
                    digits.parse::<u32>().ok()
                });
            match at {
                Some(line) if !cause.starts_with(&name) => format!("{name}:{line}: {cause}"),
                _ => cause,
            }
        }
        mlua::Error::RuntimeError(text) => untraced(text),
        other => untraced(&other.to_string()),
    }
}

/// `text` without the stack traceback Lua appends: the runner's frames,
/// not the test's.
fn untraced(text: &str) -> String {
    text.split("\nstack traceback:")
        .next()
        .unwrap_or(text)
        .trim_end()
        .to_owned()
}

/// An app `mount` made: [`TestApp`]'s acts and reads as methods.
struct Mounted {
    app: Rc<RefCell<TestApp>>,
    /// The app's directory, where a test's fixtures lie.
    dir: PathBuf,
    /// The window's size, `ui.window.size` following it.
    viewport: Signal<(f32, f32)>,
    files: Files,
}

/// The files a test's dialogs answer with: a directory of the mount's own,
/// made when first used and gone with it.
#[derive(Default)]
struct Files(OnceCell<PathBuf>);

impl Files {
    /// Where `name` lies: inside the directory, or an error.
    fn path(&self, name: &str) -> mlua::Result<PathBuf> {
        if !Path::new(name)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(mlua::Error::runtime(format!(
                "`{name}`: a test's files are named from its own directory"
            )));
        }
        static MOUNTS: AtomicUsize = AtomicUsize::new(0);
        let dir = self.0.get_or_init(|| {
            std::env::temp_dir().join(format!(
                "zgx-test-{}-{}",
                std::process::id(),
                MOUNTS.fetch_add(1, Ordering::Relaxed)
            ))
        });
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(mlua::Error::external)?;
        }
        Ok(path)
    }
}
/// `from` to `to`, a folder with all inside it.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        if let Some(dir) = self.0.get() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

impl Mounted {
    /// The fake desktop `mount` gave it.
    fn desktop(&self) -> Rc<FakeDesktop> {
        let desktop = self.app.borrow().desktop.clone();
        desktop.expect("mount gives every app a desktop")
    }
    /// The node `target` names, as devtools find it.
    fn node(app: &mut TestApp, target: &str) -> mlua::Result<zgx::scene::NodeId> {
        app.ui.prepare_frame();
        crate::driver::find(&app.ui, &app.handle, target).map_err(mlua::Error::runtime)
    }
    /// The same for an action: what of it a person can reach, inside its
    /// clips and uncovered, or why not.
    fn reach(app: &mut TestApp, target: &str) -> mlua::Result<(zgx::scene::NodeId, Rect)> {
        app.ui.prepare_frame();
        crate::driver::reach(&app.ui, &app.handle, target).map_err(mlua::Error::runtime)
    }
}

impl UserData for Mounted {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("click", |_, m, target: String| {
            let mut app = m.app.borrow_mut();
            let (_, bounds) = Self::reach(&mut app, &target)?;
            crate::driver::click_at(&mut app.ui, bounds);
            app.settle();
            Ok(())
        });
        methods.add_method("double_click", |_, m, target: String| {
            let mut app = m.app.borrow_mut();
            let (_, bounds) = Self::reach(&mut app, &target)?;
            crate::driver::double_click_at(&mut app.ui, bounds);
            app.settle();
            Ok(())
        });
        methods.add_method("type", |_, m, (target, text): (String, String)| {
            let mut app = m.app.borrow_mut();
            let (_, bounds) = Self::reach(&mut app, &target)?;
            crate::driver::click_at(&mut app.ui, bounds);
            app.ui.dispatch(zgx::input::InputEvent::Text(text));
            app.settle();
            Ok(())
        });
        // A field's whole text replaced, as a person selecting it all and
        // typing does (a form's `dirty` and validation follow).
        methods.add_method("fill", |_, m, (target, text): (String, String)| {
            let mut app = m.app.borrow_mut();
            let (_, bounds) = Self::reach(&mut app, &target)?;
            let (all, modifiers) =
                crate::driver::parse_keys("cmd+a").map_err(mlua::Error::runtime)?;
            crate::driver::click_at(&mut app.ui, bounds);
            crate::driver::press(&mut app.ui, all, modifiers);
            crate::driver::press(&mut app.ui, zgx::input::Key::Backspace, Default::default());
            app.ui.dispatch(zgx::input::InputEvent::Text(text));
            app.settle();
            Ok(())
        });
        methods.add_method("hover", |_, m, target: String| {
            let mut app = m.app.borrow_mut();
            let (_, b) = Self::reach(&mut app, &target)?;
            crate::driver::hover_at(&mut app.ui, crate::driver::middle(b));
            app.settle();
            Ok(())
        });
        methods.add_method("drag", |_, m, (from, to): (String, String)| {
            let mut app = m.app.borrow_mut();
            // What it is dragged from must be in reach; where it lands is
            // wherever its box lies (a drop under the preview, a list).
            let (_, from) = Self::reach(&mut app, &from)?;
            let to = Self::node(&mut app, &to)?;
            let to = app.ui.scene.borrow().bounds(to);
            crate::driver::drag(&mut app.ui, from, to);
            app.settle();
            Ok(())
        });
        methods.add_method("key", |_, m, keys: String| {
            let mut app = m.app.borrow_mut();
            let (key, modifiers) =
                crate::driver::parse_keys(&keys).map_err(mlua::Error::runtime)?;
            crate::driver::press(&mut app.ui, key, modifiers);
            app.settle();
            Ok(())
        });
        methods.add_method("scroll", |_, m, (target, dy): (String, f32)| {
            let mut app = m.app.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            crate::driver::scroll(&mut app.ui, node, dy);
            app.settle();
            Ok(())
        });
        methods.add_method("command", |_, m, id: String| {
            let app = m.app.borrow();
            if !app.host.run_command(&id) {
                return Err(mlua::Error::runtime(format!("no command `{id}`")));
            }
            app.settle();
            Ok(())
        });
        methods.add_method("text", |_, m, target: String| {
            let mut app = m.app.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            if let Some(editor) = app.ui.editor(node) {
                return Ok(editor.editor.borrow().text().into_owned());
            }
            let scene = app.ui.scene.borrow();
            Ok(crate::driver::first_text(&scene, node).unwrap_or_default())
        });
        // Where a field's selection is, in bytes from 0, as `string.find`
        // counts them: nil for anything else.
        methods.add_method("selection", |_, m, target: String| {
            let mut app = m.app.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            Ok(match app.ui.editor(node).map(|editor| editor.selection()) {
                Some((anchor, focus)) => (Some(anchor), Some(focus)),
                None => (None, None),
            })
        });
        methods.add_method("has", |_, m, target: String| {
            let app = m.app.borrow();
            app.ui.prepare_frame();
            let target = crate::driver::parse(&target).map_err(mlua::Error::runtime)?;
            let found = crate::driver::find_all(&app.ui, &app.handle, &target);
            Ok(found.is_ok_and(|found| !found.is_empty()))
        });
        methods.add_method("count", |_, m, target: String| {
            let app = m.app.borrow();
            app.ui.prepare_frame();
            let target = crate::driver::parse(&target).map_err(mlua::Error::runtime)?;
            let found = crate::driver::find_all(&app.ui, &app.handle, &target);
            Ok(found.map_or(0, |found| found.len()))
        });
        // The window's new size, laid out, as dragging its edge ends.
        methods.add_method("resize", |_, m, (width, height): (f32, f32)| {
            let app = m.app.borrow();
            app.ui.scene.borrow_mut().resize(width, height);
            m.viewport.set((width, height));
            app.frames(1);
            Ok(())
        });
        // What the desktop's dialogs answer next, from the mount's own files.
        methods.add_method("pick", |_, m, names: Vec<String>| {
            let files = names.iter().map(|name| m.files.path(name));
            m.desktop().pick(files.collect::<mlua::Result<Vec<_>>>()?);
            Ok(())
        });
        methods.add_method("pick_folder", |_, m, name: String| {
            m.desktop().pick_folder(m.files.path(&name)?);
            Ok(())
        });
        methods.add_method("save_to", |_, m, name: String| {
            m.desktop().save_to(m.files.path(&name)?);
            Ok(())
        });
        methods.add_method("write_file", |_, m, (name, text): (String, String)| {
            std::fs::write(m.files.path(&name)?, text).map_err(mlua::Error::external)
        });
        // A fixture of the app's directory (an image, a folder of them),
        // copied in as `name`, for the dialogs to answer with.
        methods.add_method("copy_file", |_, m, (from, name): (String, String)| {
            // Where links lead: a fixture is in the app's directory, wherever
            // the directory itself is reached from.
            let root = m.dir.canonicalize().unwrap_or_else(|_| m.dir.clone());
            let source =
                m.dir.join(&from).canonicalize().map_err(|error| {
                    mlua::Error::runtime(format!("copy_file `{from}`: {error}"))
                })?;
            if !source.starts_with(&root) {
                return Err(mlua::Error::runtime(format!(
                    "`{from}`: a fixture comes from the app's directory"
                )));
            }
            copy(&source, &m.files.path(&name)?)
                .map_err(|error| mlua::Error::runtime(format!("copy_file `{from}`: {error}")))
        });
        methods.add_method("read_file", |_, m, name: String| {
            std::fs::read_to_string(m.files.path(&name)?).map_err(mlua::Error::external)
        });
        methods.add_method("clipboard", |_, m, ()| Ok(m.desktop().clipboard()));
        methods.add_method("copy", |_, m, text: String| {
            m.desktop().copy(text);
            Ok(())
        });
        methods.add_method("tree", |_, m, target: Option<String>| {
            let mut app = m.app.borrow_mut();
            let Some(target) = target else {
                return Ok(app.tree());
            };
            let from = Self::node(&mut app, &target)?;
            let options = Default::default();
            let (lines, hidden) = crate::driver::tree(&app.ui, &app.handle, Some(from), &options);
            Ok(crate::driver::tree_text(&lines, hidden))
        });
        methods.add_method("frames", |_, m, n: Option<usize>| {
            m.app.borrow().frames(n.unwrap_or(1));
            Ok(())
        });
        // Until the target shows (holding `text`, if given), or is gone;
        // whether it did before the timeout (5 s). Frames are delivered
        // meanwhile, so animations and timers run.
        methods.add_method(
            "wait",
            |_, m, (target, options): (String, Option<Table>)| {
                let (text, gone, timeout): (Option<String>, bool, Option<f64>) = match &options {
                    Some(o) => (
                        o.get("text")?,
                        o.get::<Option<bool>>("gone")?.unwrap_or(false),
                        o.get("timeout_ms")?,
                    ),
                    None => (None, false, None),
                };
                let parsed = crate::driver::parse(&target).map_err(mlua::Error::runtime)?;
                let deadline =
                    Instant::now() + Duration::from_secs_f64(timeout.unwrap_or(5000.) / 1000.);
                let app = m.app.borrow();
                loop {
                    app.frame();
                    let found =
                        crate::driver::find_all(&app.ui, &app.handle, &parsed).unwrap_or_default();
                    let met = match (found.first(), gone) {
                        (None, true) => true,
                        (Some(_), true) | (None, false) => false,
                        (Some(node), false) => text.as_deref().is_none_or(|text| {
                            let scene = app.ui.scene.borrow();
                            crate::driver::first_text(&scene, *node)
                                .is_some_and(|t| t.contains(text))
                        }),
                    };
                    if met {
                        return Ok(true);
                    }
                    if Instant::now() >= deadline {
                        return Ok(false);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            },
        );
        methods.add_method("state", |_, m, name: String| {
            let states = m.app.borrow().host.states();
            let suffix = format!("::{name}");
            let found = states.iter().find(|(n, _)| *n == name).or_else(|| {
                let mut by_suffix = states.iter().filter(|(n, _)| n.ends_with(&suffix));
                match (by_suffix.next(), by_suffix.next()) {
                    (Some(one), None) => Some(one),
                    _ => None,
                }
            });
            match found {
                Some((_, value)) => Ok(value.clone()),
                None => Err(mlua::Error::runtime(format!("no state named `{name}`"))),
            }
        });
        methods.add_method("set_state", |_, m, (name, value): (String, Value)| {
            let app = m.app.borrow();
            app.host
                .set_state(&name, value)
                .map_err(mlua::Error::runtime)?;
            app.settle();
            Ok(())
        });
        methods.add_method("error", |_, m, ()| Ok(m.app.borrow().error()));
    }
}
