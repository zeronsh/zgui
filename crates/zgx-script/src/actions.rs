//! `ui.actions { name = function(arg) … end }`: actions the app's own
//! script defines, called as a Rust host's are (`ui.invoke(name, arg)`,
//! `ui.action(name, arg)` as a handler), by the app and by the plugins its
//! host exposes them to (`ScriptHost::expose_action`, or zgx.json's
//! `expose.actions`). A Luau app run by `zgx run` has no Rust of its own:
//! this is how its plugins reach what it decides they may do.
//!
//! Defined again as a reload runs the script again; a plugin defines none
//! (it calls the app's).
use crate::{Actions, value::Value};
use mlua::{Function, Lua, Table};
use std::rc::Rc;

pub(crate) fn install(lua: &Lua, ui: &Table, actions: Actions, app: bool) -> mlua::Result<()> {
    ui.set(
        "actions",
        lua.create_function(move |lua, table: Table| {
            if !app {
                return Err(mlua::Error::runtime(
                    "ui.actions: only the app's script defines actions; a plugin calls them \
                     (ui.invoke)",
                ));
            }
            let vm = crate::node::env(lua)?.vm.clone();
            for pair in table.pairs::<String, Function>() {
                let (name, function) = pair.map_err(|_| {
                    mlua::Error::runtime("ui.actions takes { name = function(arg) … end }")
                })?;
                let vm = vm.clone();
                let action = move |arg: Value| {
                    if let Some(vm) = vm.upgrade()
                        && let Err(error) = vm.call::<()>(&function, arg)
                    {
                        vm.report(error);
                    }
                };
                actions.borrow_mut().insert(name, Rc::new(action));
            }
            Ok(())
        })?,
    )
}
