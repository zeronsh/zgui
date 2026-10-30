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
use crate::{ScriptHost, Value, testing::TestApp};
use mlua::{Function, Lua, Table, UserData, UserDataMethods};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

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
    globals.set(
        "mount",
        lua.create_function(move |_, path: Option<String>| {
            let path = dir.join(path.as_deref().unwrap_or("app.luau"));
            // Its packages, what it exposes to plugins and its plugins, as
            // `zgx run` gives them.
            let host = crate::app_config::read(&dir)
                .map_err(|error| mlua::Error::runtime(format!("mount: {error}")))?;
            let app = TestApp::try_new(|runtime| host.apply(ScriptHost::new(runtime, &path)))
                .map_err(|error| mlua::Error::runtime(format!("mount: {error}")))?;
            // The window's size, as `zgx run` opens it.
            let (width, height) = window_size(&dir);
            app.ui.scene.borrow_mut().resize(width, height);
            app.frames(1);
            Ok(Mounted(Rc::new(RefCell::new(app))))
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
struct Mounted(Rc<RefCell<TestApp>>);

impl Mounted {
    /// The node `target` names, as devtools find it.
    fn node(app: &mut TestApp, target: &str) -> mlua::Result<zgx::scene::NodeId> {
        app.ui.prepare_frame();
        crate::driver::find(&app.ui, &app.handle, target).map_err(mlua::Error::runtime)
    }
}

impl UserData for Mounted {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("click", |_, m, target: String| {
            let mut app = m.0.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            let bounds = app.ui.scene.borrow().bounds(node);
            crate::driver::click_at(&mut app.ui, bounds);
            app.settle();
            Ok(())
        });
        methods.add_method("type", |_, m, (target, text): (String, String)| {
            let mut app = m.0.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            crate::driver::type_into(&mut app.ui, node, &text);
            app.settle();
            Ok(())
        });
        // A field's whole text replaced, as a person selecting it all and
        // typing does (a form's `dirty` and validation follow).
        methods.add_method("fill", |_, m, (target, text): (String, String)| {
            let mut app = m.0.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            let (all, modifiers) =
                crate::driver::parse_keys("cmd+a").map_err(mlua::Error::runtime)?;
            let bounds = app.ui.scene.borrow().bounds(node);
            crate::driver::click_at(&mut app.ui, bounds);
            crate::driver::press(&mut app.ui, all, modifiers);
            crate::driver::press(&mut app.ui, zgx::input::Key::Backspace, Default::default());
            app.ui.dispatch(zgx::input::InputEvent::Text(text));
            app.settle();
            Ok(())
        });
        methods.add_method("key", |_, m, keys: String| {
            let mut app = m.0.borrow_mut();
            let (key, modifiers) =
                crate::driver::parse_keys(&keys).map_err(mlua::Error::runtime)?;
            crate::driver::press(&mut app.ui, key, modifiers);
            app.settle();
            Ok(())
        });
        methods.add_method("scroll", |_, m, (target, dy): (String, f32)| {
            let mut app = m.0.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            crate::driver::scroll(&mut app.ui, node, dy);
            app.settle();
            Ok(())
        });
        methods.add_method("command", |_, m, id: String| {
            let app = m.0.borrow();
            if !app.host.run_command(&id) {
                return Err(mlua::Error::runtime(format!("no command `{id}`")));
            }
            app.settle();
            Ok(())
        });
        methods.add_method("text", |_, m, target: String| {
            let mut app = m.0.borrow_mut();
            let node = Self::node(&mut app, &target)?;
            let scene = app.ui.scene.borrow();
            Ok(crate::driver::first_text(&scene, node).unwrap_or_default())
        });
        methods.add_method("has", |_, m, target: String| {
            let app = m.0.borrow();
            app.ui.prepare_frame();
            let target = crate::driver::parse(&target).map_err(mlua::Error::runtime)?;
            let found = crate::driver::find_all(&app.ui, &app.handle, &target);
            Ok(found.is_ok_and(|found| !found.is_empty()))
        });
        methods.add_method("count", |_, m, target: String| {
            let app = m.0.borrow();
            app.ui.prepare_frame();
            let target = crate::driver::parse(&target).map_err(mlua::Error::runtime)?;
            let found = crate::driver::find_all(&app.ui, &app.handle, &target);
            Ok(found.map_or(0, |found| found.len()))
        });
        methods.add_method("tree", |_, m, ()| Ok(m.0.borrow_mut().tree()));
        methods.add_method("frames", |_, m, n: Option<usize>| {
            m.0.borrow().frames(n.unwrap_or(1));
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
                let app = m.0.borrow();
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
            let states = m.0.borrow().host.states();
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
            let app = m.0.borrow();
            app.host
                .set_state(&name, value)
                .map_err(mlua::Error::runtime)?;
            app.settle();
            Ok(())
        });
        methods.add_method("error", |_, m, ()| Ok(m.0.borrow().error()));
    }
}
