//! What the app was launched with, and what the system asks it to open:
//! `ui.args`, the arguments after the script (`zgx-run app/ notes.txt`),
//! and `ui.on_open`, files and URLs handed to it (a file opened with the
//! app, a link to it, a second launch's), as [`crate::ScriptHost::open`]
//! delivers them. Files it is given are the user's choice, as a dialog's
//! are: `ui.read_file` and `ui.write_file` take them.
use crate::{Vm, io::Picked, value::Value};
use mlua::{Function, Lua, Table};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::{Rc, Weak},
};

/// A host's arguments, `ui.on_open` handlers, and what was asked to open
/// before any script took it.
#[derive(Default)]
pub(crate) struct Opens {
    args: RefCell<Vec<String>>,
    handlers: RefCell<Vec<(Weak<Vm>, Function)>>,
    waiting: RefCell<Vec<(Vec<PathBuf>, Vec<String>)>>,
}
impl Opens {
    pub(crate) fn set_args(&self, args: Vec<String>) {
        *self.args.borrow_mut() = args;
    }
    /// `files` and `urls` to the scripts' handlers; kept for the first if
    /// none is there yet (the launch's files, before the script loads).
    pub(crate) fn open(&self, picked: &Picked, files: Vec<PathBuf>, urls: Vec<String>) {
        if files.is_empty() && urls.is_empty() {
            return;
        }
        picked.borrow_mut().extend(files.iter().cloned());
        self.handlers
            .borrow_mut()
            .retain(|(vm, _)| vm.strong_count() > 0);
        let handlers: Vec<_> = self
            .handlers
            .borrow()
            .iter()
            .filter_map(|(vm, f)| Some((vm.upgrade()?, f.clone())))
            .collect();
        if handlers.is_empty() {
            self.waiting.borrow_mut().push((files, urls));
            return;
        }
        let event = event(&files, &urls);
        for (vm, handler) in handlers {
            if let Err(error) = vm.call::<()>(&handler, event.clone()) {
                vm.report(error);
            }
        }
    }
    /// What waited for a handler, now that a script has one (after it loaded).
    pub(crate) fn deliver_waiting(&self, picked: &Picked) {
        let live = self
            .handlers
            .borrow()
            .iter()
            .any(|(vm, _)| vm.strong_count() > 0);
        if !live {
            return;
        }
        let waiting = std::mem::take(&mut *self.waiting.borrow_mut());
        for (files, urls) in waiting {
            self.open(picked, files, urls);
        }
    }
}

/// `{ files = { { name, path }, … }, urls = { … } }`.
fn event(files: &[PathBuf], urls: &[String]) -> Value {
    let files: Vec<Value> = files
        .iter()
        .map(|path| crate::io::file_value(path))
        .collect();
    let urls: Vec<Value> = urls
        .iter()
        .map(|url| Value::Str(url.as_str().into()))
        .collect();
    Value::Map(
        vec![
            (Rc::from("files"), Value::List(files.into())),
            (Rc::from("urls"), Value::List(urls.into())),
        ]
        .into(),
    )
}

/// Installs `ui.args` and `ui.on_open` (a plugin's `ui.on_open` is an
/// error: what the system opens is the app's).
pub(crate) fn install(lua: &Lua, ui: &Table, opens: Rc<Opens>, plugin: bool) -> mlua::Result<()> {
    let args = lua.create_sequence_from(opens.args.borrow().iter().cloned())?;
    args.set_readonly(true);
    ui.set("args", args)?;
    ui.set(
        "on_open",
        lua.create_function(move |lua, handler: Function| {
            if plugin {
                return Err(mlua::Error::runtime(
                    "ui.on_open is the app's: a plugin is not handed the files the system opens",
                ));
            }
            let vm = crate::node::env(lua)?.vm.clone();
            opens.handlers.borrow_mut().push((vm, handler));
            Ok(())
        })?,
    )?;
    Ok(())
}
