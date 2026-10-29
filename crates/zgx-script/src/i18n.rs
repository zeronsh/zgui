//! Localized messages for scripts, on `zgx::i18n`: `ui.i18n { dir, source }`
//! loads a directory of locale files (`en.json`, `ar.json`, …),
//! `ui.t(key, args)` makes a message that text and properties show as it
//! follows the locale, and `ui.locale` asks for one (nil: the system's).
//!
//! One catalog per host, kept across reloads (a reload asking for the same
//! directory keeps its signals); its files are read again when they change,
//! setting only the messages whose text did (`reload_if_changed`).
use crate::{Files, node::env, value::Value};
use mlua::{Lua, Table, UserData, UserDataFields, UserDataMethods};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::SystemTime};
use zgx::i18n::{ArgValue, I18n, Message};

/// The host's catalog, once a script asked for one.
#[derive(Default)]
pub(crate) struct Locales {
    set: RefCell<Option<Set>>,
    /// Keeps the script root running the way the locale does (`follow`).
    direction: RefCell<Option<zgx::reactive::Effect>>,
}
struct Set {
    i18n: I18n,
    dir: PathBuf,
    source: String,
    /// Each locale file and its modification time when read.
    files: Vec<(PathBuf, Option<SystemTime>)>,
    warnings: crate::SharedWarnings,
}

impl Locales {
    /// The catalog, if `ui.i18n` made one.
    pub(crate) fn handle(&self) -> Option<I18n> {
        self.set.borrow().as_ref().map(|set| set.i18n.clone())
    }
    /// Reads again the locale files edited since they were read; whether
    /// any were. What their new text changes shows in place, as theme
    /// files' does, without reloading the scripts.
    pub(crate) fn reload_if_changed(&self, files: Files) -> bool {
        let mut set = self.set.borrow_mut();
        let Some(set) = set.as_mut() else {
            return false;
        };
        let mut changed = false;
        for (path, stamp) in &mut set.files {
            let now = modified(path);
            if now != *stamp {
                *stamp = now;
                read(&set.i18n, files, path, &set.warnings);
                changed = true;
            }
        }
        changed
    }
    /// Runs `root` (the script's mounted root) the way the locale does,
    /// from now: replaces what an earlier view's root followed.
    pub(crate) fn follow(
        &self,
        runtime: &zgx::reactive::Runtime,
        scene: Rc<RefCell<zgx::scene::Scene>>,
        root: zgx::scene::NodeId,
    ) {
        let Some(i18n) = self.handle() else {
            return;
        };
        let effect = runtime.effect(move || {
            let direction = i18n.direction();
            if let Ok(mut scene) = scene.try_borrow_mut()
                && scene.contains(root)
            {
                scene.set_direction(root, Some(direction));
            }
        });
        *self.direction.borrow_mut() = Some(effect);
    }
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Loads (or loads again) the locale file at `path`, named by its stem
/// (`pt-BR.json` is `pt-BR`), warning of what fails to parse: its keys
/// fall back to another locale's.
fn read(i18n: &I18n, files: Files, path: &std::path::Path, warnings: &crate::SharedWarnings) {
    let Some(locale) = path.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let text = match files.read(path).map(String::from_utf8) {
        Ok(Ok(text)) => text,
        Ok(Err(_)) => {
            crate::warning(Some(warnings), format!("{}: not UTF-8", path.display()));
            return;
        }
        Err(error) => {
            crate::warning(Some(warnings), format!("{}: {error}", path.display()));
            return;
        }
    };
    if let Err(errors) = i18n.insert(locale, &text) {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into());
        for error in errors {
            crate::warning(Some(warnings), format!("{name}: {error}"));
        }
    }
}

/// A message to show (`ui.t`): text and properties that take it follow its
/// locale, its file and the states its arguments read.
pub(crate) struct MessageUd(pub Message);
impl UserData for MessageUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Its text now (tracked where it is read: a computed follows it).
        methods.add_method("get", |_, m, ()| Ok(m.0.get()));
        methods.add_method("key", |_, m, ()| Ok(m.0.key().to_owned()));
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, m, ()| Ok(m.0.get()));
    }
}
/// The message `value` is, if it is one.
pub(crate) fn message_of(value: &mlua::Value) -> Option<Message> {
    match value {
        mlua::Value::UserData(ud) => ud.borrow::<MessageUd>().ok().map(|m| m.0.clone()),
        _ => None,
    }
}

/// `ui.locale`: the locale asked for (a string, or nil for the system's
/// preferred languages) and, as `current`, the one shown.
struct LocaleUd(Rc<Locales>);
impl LocaleUd {
    fn i18n(&self) -> mlua::Result<I18n> {
        self.0
            .handle()
            .ok_or_else(|| mlua::Error::runtime("ui.locale: call ui.i18n { dir = … } first"))
    }
}
impl UserData for LocaleUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        // The locale shown: the one asked for, or preferred, as the app
        // has it (tracked).
        fields.add_field_method_get("current", |_, l| Ok(l.i18n()?.current().to_string()));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |_, l, ()| Ok(l.i18n()?.requested().get()));
        methods.add_method("peek", |_, l, ()| {
            Ok(l.i18n()?.requested().with_untracked(Clone::clone))
        });
        methods.add_method("set", |_, l, locale: Option<String>| {
            l.i18n()?.set_locale(locale.as_deref());
            Ok(())
        });
    }
}

/// A script value as a message's argument: a number or text as it is, a
/// state or a function read each time the message formats.
fn argument(lua: &Lua, message: Message, name: &str, value: mlua::Value) -> mlua::Result<Message> {
    if let Some(state) = crate::node::state_of(&value) {
        return Ok(message.arg_with(name, move || state.with(arg_value)));
    }
    Ok(match value {
        mlua::Value::Integer(n) => message.arg(name, n as f64),
        mlua::Value::Number(n) => message.arg(name, n),
        mlua::Value::String(s) => message.arg(name, s.to_string_lossy()),
        mlua::Value::Boolean(b) => message.arg(name, if b { "true" } else { "false" }),
        mlua::Value::Function(f) => {
            let vm = env(lua)?.vm.clone();
            message.arg_with(name, move || {
                let Some(vm) = vm.upgrade() else {
                    return ArgValue::Text("".into());
                };
                match vm.call::<mlua::Value>(&f, ()) {
                    Ok(value) => crate::value::convert(value, None)
                        .map(|value| arg_value(&value))
                        .unwrap_or_else(|_| ArgValue::Text("".into())),
                    Err(error) => {
                        vm.report(error);
                        ArgValue::Text("".into())
                    }
                }
            })
        }
        other => {
            return Err(mlua::Error::runtime(format!(
                "ui.t's argument `{name}` is a number, a string, a state or a function, not {}",
                other.type_name()
            )));
        }
    })
}
/// A state's value as a message's argument.
fn arg_value(value: &Value) -> ArgValue {
    match value {
        Value::Num(n) => ArgValue::Number(*n),
        Value::Str(s) => ArgValue::Text(s.to_string().into()),
        other => ArgValue::Text(other.to_string().into()),
    }
}

/// `ui.i18n`, and `ui.t` and `ui.locale` made the first time a script
/// names either: `ui.i18n`'s function is also `ui`'s `__index` (which keys
/// `ui` has never reach), so a script that uses none pays for the one
/// function and a small metatable as its VM is made, not three functions
/// and a userdata type.
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    locales: Rc<Locales>,
    files: Files,
) -> mlua::Result<()> {
    let set = locales.clone();
    let entry = lua.create_function(move |lua, (first, key): (mlua::Value, mlua::Value)| {
        // `__index(ui, key)`: a key `ui` doesn't have.
        if let mlua::Value::String(name) = &key {
            if !matches!(name.as_bytes().as_ref(), b"t" | b"locale") {
                return Ok(mlua::Value::Nil);
            }
            provide(lua, &set)?;
            let ui: Table = lua.globals().get("ui")?;
            return ui.raw_get::<mlua::Value>(key);
        }
        // `ui.i18n { dir, source }`.
        let options = match first {
            mlua::Value::Table(options) => Some(options),
            mlua::Value::Nil => None,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "ui.i18n takes a table ({{ dir, source }}), not {}",
                    other.type_name()
                )));
            }
        };
        load(lua, &set, files, options)?;
        Ok(mlua::Value::Nil)
    })?;
    ui.set("i18n", entry.clone())?;
    let meta = lua.create_table()?;
    meta.raw_set("__index", entry)?;
    ui.set_metatable(Some(meta))?;
    Ok(())
}

/// `ui.i18n { dir, source }`: loads the catalog (or keeps the one loaded
/// for them), and makes `ui.t` and `ui.locale`.
fn load(lua: &Lua, set: &Rc<Locales>, files: Files, options: Option<Table>) -> mlua::Result<()> {
    let text = |name: &str, default: &str| -> mlua::Result<String> {
        Ok(options
            .as_ref()
            .map(|t| t.get::<Option<String>>(name))
            .transpose()?
            .flatten()
            .unwrap_or_else(|| default.to_owned()))
    };
    let (dir, source) = (text("dir", "locales")?, text("source", "en")?);
    let env = env(lua)?;
    let base = env
        .vm
        .upgrade()
        .and_then(|vm| vm.base.clone())
        .unwrap_or_default();
    // Canonical, as `within` needs (a bundle's isn't on the disk).
    let base = base.canonicalize().unwrap_or(base);
    let dir = crate::normalize(&base.join(&dir));
    if !crate::within(&base, &dir) {
        return Err(mlua::Error::runtime(
            "ui.i18n's `dir` is inside the script's directory",
        ));
    }
    // A reload asking for the same catalog keeps it, and so its
    // signals: what changed in its files is read by the watcher.
    if set
        .set
        .borrow()
        .as_ref()
        .is_some_and(|s| s.dir == dir && s.source == source)
    {
        return provide(lua, set);
    }
    let mut paths: Vec<PathBuf> = files
        .list(&dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(mlua::Error::runtime(format!(
            "ui.i18n: no locale files (<locale>.json) in {}",
            dir.strip_prefix(&base).unwrap_or(&dir).display()
        )));
    }
    let i18n = I18n::new(&env.runtime, &source);
    let warnings = env.warnings.clone();
    let told = warnings.clone();
    i18n.on_issue(move |issue| {
        let detail = match issue.key.as_str() {
            "" => issue.detail.clone(),
            key => format!("{key} ({}): {}", issue.locale, issue.detail),
        };
        crate::warning(Some(&told), format!("i18n: {detail}"));
    });
    let mut stamps = Vec::with_capacity(paths.len());
    for path in paths {
        read(&i18n, files, &path, &warnings);
        stamps.push((path.clone(), modified(&path)));
    }
    *set.set.borrow_mut() = Some(Set {
        i18n,
        dir,
        source,
        files: stamps,
        warnings,
    });
    provide(lua, set)
}

/// `ui.t` and `ui.locale`, for a script that called `ui.i18n`.
fn provide(lua: &Lua, locales: &Rc<Locales>) -> mlua::Result<()> {
    let ui: Table = lua.globals().get("ui")?;
    if !ui.raw_get::<mlua::Value>("t")?.is_nil() {
        return Ok(());
    }
    // `ui` is frozen once the VM is made: open it for these two alone.
    let frozen = ui.is_readonly();
    ui.set_readonly(false);
    let made = add(lua, &ui, locales);
    ui.set_readonly(frozen);
    made
}
fn add(lua: &Lua, ui: &Table, locales: &Rc<Locales>) -> mlua::Result<()> {
    let catalog = locales.clone();
    ui.raw_set(
        "t",
        lua.create_function(move |lua, (key, args): (String, Option<Table>)| {
            let i18n = catalog.handle().ok_or_else(|| {
                mlua::Error::runtime("ui.t: no catalog loaded; call ui.i18n { dir = … } first")
            })?;
            let mut message = i18n.t(&key);
            if let Some(args) = args {
                for pair in args.pairs::<String, mlua::Value>() {
                    let (name, value) = pair?;
                    message = argument(lua, message, &name, value)?;
                }
            }
            Ok(MessageUd(message))
        })?,
    )?;
    ui.raw_set("locale", LocaleUd(locales.clone()))?;
    Ok(())
}
