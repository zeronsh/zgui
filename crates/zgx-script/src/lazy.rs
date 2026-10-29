//! What `ui` and the globals make only once a script names it. A VM's
//! setup made every function of every module a script might use (anime's
//! alone compiled a chunk of Luau); most scripts use a few. Each module is
//! registered here with the names it provides and how to make them, and
//! one function, the `__index` of `ui` and of the globals, makes a module
//! the first time a script reads one of its names (Luau resolves `ui.x`
//! and `anime` as it loads a chunk, so a script pays at load for the names
//! it uses, and only those).
use mlua::{Lua, Table, Value};
use std::cell::RefCell;

/// How a module makes its names: into `ui`, or (a global) into the globals.
type Make = Box<dyn FnOnce(&Lua, &Table) -> mlua::Result<()>>;

struct Module {
    names: &'static [&'static str],
    global: bool,
    make: Option<Make>,
}

/// The modules a VM has not made yet.
#[derive(Default)]
pub(crate) struct Lazy(RefCell<Vec<Module>>);

impl Lazy {
    /// `make` sets `names` in `ui` (open to it) the first time one is read.
    pub(crate) fn ui(
        &self,
        names: &'static [&'static str],
        make: impl FnOnce(&Lua, &Table) -> mlua::Result<()> + 'static,
    ) {
        self.add(names, false, Box::new(make));
    }
    /// `make` sets the globals `names` the first time one is read.
    pub(crate) fn global(
        &self,
        names: &'static [&'static str],
        make: impl FnOnce(&Lua, &Table) -> mlua::Result<()> + 'static,
    ) {
        self.add(names, true, Box::new(make));
    }
    fn add(&self, names: &'static [&'static str], global: bool, make: Make) {
        self.0.borrow_mut().push(Module {
            names,
            global,
            make: Some(make),
        });
    }
    /// The maker of `name`'s module, taken: each is made once.
    fn take(&self, name: &[u8], global: bool) -> Option<Make> {
        let mut modules = self.0.borrow_mut();
        let module = modules
            .iter_mut()
            .find(|m| m.global == global && m.names.iter().any(|n| n.as_bytes() == name))?;
        module.make.take()
    }
    /// The names `name`'s `ui` module is registered under.
    fn names_of(&self, name: &[u8]) -> Option<&'static [&'static str]> {
        let modules = self.0.borrow();
        let module = modules
            .iter()
            .find(|m| !m.global && m.names.iter().any(|n| n.as_bytes() == name))?;
        Some(module.names)
    }
}

/// `table`'s string keys (checking what a module made, in debug builds).
fn keys(table: &Table) -> std::collections::BTreeSet<String> {
    table
        .pairs::<mlua::Value, Value>()
        .filter_map(|pair| match pair.ok()?.0 {
            Value::String(key) => Some(key.to_string_lossy()),
            _ => None,
        })
        .collect()
}

/// Puts `lazy`'s modules behind `ui` and the globals. Last of a VM's
/// setup: a table with a metatable is written through a protected call
/// (mlua), which every `ui.set` after this would pay.
pub(crate) fn attach(lua: &Lua, ui: &Table, lazy: Lazy) -> mlua::Result<()> {
    let ui_at = ui.to_pointer();
    lua.set_app_data(lazy);
    let index = lua.create_function(move |lua, (table, key): (Table, Value)| {
        let Value::String(name) = &key else {
            return Ok(Value::Nil);
        };
        let global = table.to_pointer() != ui_at;
        let make = lua
            .app_data_ref::<Lazy>()
            .and_then(|lazy| lazy.take(&name.as_bytes(), global));
        let Some(make) = make else {
            return Ok(Value::Nil);
        };
        if global {
            // Into the globals scripts read (the sandbox's, once there is
            // one), where the next read finds it.
            make(lua, &lua.globals())?;
            return lua.globals().raw_get(key);
        }
        // `ui` is frozen once the VM is made: open it for this module.
        let frozen = table.is_readonly();
        let before = cfg!(debug_assertions).then(|| keys(&table));
        table.set_readonly(false);
        let made = make(lua, &table);
        table.set_readonly(frozen);
        made?;
        if let Some(before) = before {
            // A name a module sets must be among those it is registered
            // under, or reading it first would find nothing.
            let names = lua
                .app_data_ref::<Lazy>()
                .and_then(|lazy| lazy.names_of(&name.as_bytes()))
                .unwrap_or_default();
            for new in keys(&table).difference(&before) {
                debug_assert!(
                    names.contains(&new.as_str()),
                    "ui.{new} is made by the module of ui.{}, which isn't registered under it",
                    name.to_string_lossy()
                );
            }
        }
        table.raw_get(key)
    })?;
    let meta = lua.create_table()?;
    meta.raw_set("__index", &index)?;
    ui.set_metatable(Some(meta))?;
    let meta = lua.create_table()?;
    meta.raw_set("__index", index)?;
    lua.globals().set_metatable(Some(meta))?;
    Ok(())
}

/// One of zgx's own chunks of Luau, `source` named `name`: compiled once a
/// thread (a VM's setup compiled anime's on every load) and its bytecode
/// loaded since. Bytecode zgx compiled itself; a script's own is source.
pub(crate) fn builtin(
    lua: &Lua,
    name: &'static str,
    source: &'static str,
) -> mlua::Result<mlua::chunk::Chunk<'static>> {
    /// Each chunk's bytecode, by name.
    type Compiled = Vec<(&'static str, std::rc::Rc<[u8]>)>;
    thread_local! {
        static COMPILED: RefCell<Compiled> = const { RefCell::new(Vec::new()) };
    }
    let bytecode = COMPILED.with_borrow_mut(|compiled| {
        if let Some((_, bytecode)) = compiled.iter().find(|(n, _)| *n == name) {
            return Ok(bytecode.clone());
        }
        let bytecode: std::rc::Rc<[u8]> = mlua::chunk::Compiler::new().compile(source)?.into();
        compiled.push((name, bytecode.clone()));
        Ok::<_, mlua::Error>(bytecode)
    })?;
    Ok(lua.load(bytecode.to_vec()).set_name(name))
}

/// Makes every module `lua` has not made, each into a `ui` (or globals)
/// open to it, and checks that it sets exactly the names it is registered
/// under: a name it sets and isn't registered under would read as nil
/// until another of its module's names was read, and one it is registered
/// under and doesn't set reads as nil always. For tests, under whichever
/// features a build has (so each module's list is checked against what
/// its maker does, not against a list kept beside it).
pub(crate) fn check(lua: &Lua) -> Result<(), String> {
    let ui: Table = lua.globals().get("ui").map_err(|e| e.to_string())?;
    let modules = lua
        .app_data_ref::<Lazy>()
        .map_or(0, |lazy| lazy.0.borrow().len());
    let mut wrong = Vec::new();
    for at in 0..modules {
        let taken = lua.app_data_ref::<Lazy>().and_then(|lazy| {
            let mut modules = lazy.0.borrow_mut();
            let module = &mut modules[at];
            Some((module.names, module.global, module.make.take()?))
        });
        let Some((names, global, make)) = taken else {
            continue;
        };
        let table = if global { lua.globals() } else { ui.clone() };
        let before = keys(&table);
        let frozen = table.is_readonly();
        table.set_readonly(false);
        let made = make(lua, &table);
        table.set_readonly(frozen);
        made.map_err(|e| format!("{}: {e}", names[0]))?;
        let added: std::collections::BTreeSet<String> =
            keys(&table).difference(&before).cloned().collect();
        let registered: std::collections::BTreeSet<String> =
            names.iter().map(|n| n.to_string()).collect();
        if added != registered {
            wrong.push(format!("registered as {registered:?}, made {added:?}"));
        }
    }
    match wrong.is_empty() {
        true => Ok(()),
        false => Err(wrong.join("\n")),
    }
}
