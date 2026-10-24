//! The machine beyond the window, through the host's platform
//! (`zgx::platform`, which a desktop window provides): `ui.clipboard`,
//! file dialogs (`ui.open_files`, `ui.save_file`) and the files the user
//! picked in them (`ui.read_file`, `ui.write_file`). A script reaches only
//! files the user chose, as a web page does; reads and writes run off the
//! main thread. A plugin gets none of it unless its host grants it.
use crate::{
    calls::{PromiseUd, pending, settle_with},
    value::Value,
};
use mlua::{Lua, Table};
use std::{
    cell::RefCell,
    collections::HashSet,
    path::{Path, PathBuf},
    rc::Rc,
};
use zgx::platform::{FilePicker, PlatformService};

/// The window's platform, once the host's view mounts in one.
pub(crate) type SharedPlatform = Rc<RefCell<Option<PlatformService>>>;
/// The files the user picked in this host's dialogs: the only ones scripts
/// may read or write.
pub(crate) type Picked = Rc<RefCell<HashSet<PathBuf>>>;

fn platform(shared: &SharedPlatform, what: &str) -> mlua::Result<PlatformService> {
    shared.borrow().clone().ok_or_else(|| {
        mlua::Error::runtime(format!(
            "{what}: no platform here (the clipboard and file dialogs need a desktop window)"
        ))
    })
}
fn denied(what: &str, capability: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{what}: a plugin does this only if the host grants it \
         (`ScriptHost::grant(plugin, Capability::{capability})`)"
    ))
}

/// `{ path, name }`, as dialogs give files to scripts.
pub(crate) fn file_value(path: &Path) -> Value {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Value::Map(
        vec![
            (Rc::from("name"), Value::Str(name.into())),
            (Rc::from("path"), Value::Str(path.to_string_lossy().into())),
        ]
        .into(),
    )
}
/// A dialog's options: `title`, `directory`, `name` (to save as),
/// `filters` (`{ { "Images", { "png", "jpg" } } }`) and `multiple`.
fn picker(opts: Option<Table>, title: &str) -> mlua::Result<FilePicker> {
    let mut picker = FilePicker {
        title: title.into(),
        ..Default::default()
    };
    let Some(opts) = opts else {
        return Ok(picker);
    };
    if let Some(title) = opts.get::<Option<String>>("title")? {
        picker.title = title;
    }
    picker.directory = opts.get::<Option<String>>("directory")?.map(PathBuf::from);
    picker.file_name = opts.get::<Option<String>>("name")?;
    picker.multiple = opts.get::<Option<bool>>("multiple")?.unwrap_or(false);
    if let Some(filters) = opts.get::<Option<Table>>("filters")? {
        for filter in filters.sequence_values::<Table>() {
            let filter = filter?;
            let name: String = filter.get(1)?;
            let extensions: Vec<String> = filter.get(2)?;
            picker.filters.push((name, extensions));
        }
    }
    Ok(picker)
}
/// A file a script names (`{ path }` from a dialog, or its path), if the
/// user picked it.
fn chosen(picked: &Picked, file: &mlua::Value, what: &str) -> mlua::Result<PathBuf> {
    let path = match file {
        mlua::Value::String(path) => path.to_str()?.to_string(),
        mlua::Value::Table(file) => file.get::<String>("path")?,
        other => {
            return Err(mlua::Error::runtime(format!(
                "{what} takes a file from ui.open_files or ui.save_file, not {}",
                other.type_name()
            )));
        }
    };
    let path = PathBuf::from(path);
    if !picked.borrow().contains(&path) {
        return Err(mlua::Error::runtime(format!(
            "{what}: a script reads and writes only files the user picked \
             (ui.open_files, ui.save_file), not {}",
            path.display()
        )));
    }
    Ok(path)
}
/// Run `work` on a thread of its own and settle a promise with what it
/// gives: text, or nil.
fn off_thread(
    lua: &Lua,
    name: String,
    work: impl FnOnce() -> Result<Option<String>, String> + Send + 'static,
) -> mlua::Result<PromiseUd> {
    let (tell, mut heard) = zgx::task::latest::<Result<Option<String>, String>>();
    let promise = pending(lua, name)?;
    std::thread::spawn(move || {
        tell.send(work());
    });
    settle_with(lua, &promise, async move {
        match heard.recv().await {
            Some(Ok(Some(text))) => Ok(Value::Str(text.into())),
            Some(Ok(None)) => Ok(Value::Nil),
            Some(Err(error)) => Err(error),
            None => Err("the file thread stopped".into()),
        }
    })?;
    Ok(PromiseUd(promise))
}

/// Installs `ui.clipboard`, `ui.open_files`, `ui.save_file`, `ui.read_file`
/// and `ui.write_file`. `clipboard` and `files` are false for a plugin the
/// host did not grant them.
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    shared: SharedPlatform,
    picked: Picked,
    clipboard: bool,
    files: bool,
) -> mlua::Result<()> {
    let board = lua.create_table()?;
    {
        let shared = shared.clone();
        board.set(
            "read",
            lua.create_function(move |_, ()| {
                if !clipboard {
                    return Err(denied("ui.clipboard.read", "Clipboard"));
                }
                Ok(platform(&shared, "ui.clipboard.read")?.0.clipboard_text())
            })?,
        )?;
    }
    {
        let shared = shared.clone();
        board.set(
            "write",
            lua.create_function(move |_, text: String| {
                if !clipboard {
                    return Err(denied("ui.clipboard.write", "Clipboard"));
                }
                Ok(platform(&shared, "ui.clipboard.write")?
                    .0
                    .set_clipboard_text(text))
            })?,
        )?;
    }
    board.set_readonly(true);
    ui.set("clipboard", board)?;
    {
        let (shared, picked) = (shared.clone(), picked.clone());
        ui.set(
            "open_files",
            lua.create_function(move |lua, opts: Option<Table>| {
                if !files {
                    return Err(denied("ui.open_files", "Files"));
                }
                let dialog = platform(&shared, "ui.open_files")?
                    .0
                    .open_files(picker(opts, "Open")?);
                let promise = pending(lua, "open_files".into())?;
                let picked = picked.clone();
                settle_with(lua, &promise, async move {
                    let paths = dialog.await?;
                    picked.borrow_mut().extend(paths.iter().cloned());
                    Ok(Value::List(paths.iter().map(|p| file_value(p)).collect()))
                })?;
                Ok(PromiseUd(promise))
            })?,
        )?;
    }
    {
        let (shared, picked) = (shared.clone(), picked.clone());
        ui.set(
            "save_file",
            lua.create_function(move |lua, opts: Option<Table>| {
                if !files {
                    return Err(denied("ui.save_file", "Files"));
                }
                let dialog = platform(&shared, "ui.save_file")?
                    .0
                    .save_file(picker(opts, "Save")?);
                let promise = pending(lua, "save_file".into())?;
                let picked = picked.clone();
                settle_with(lua, &promise, async move {
                    Ok(match dialog.await? {
                        Some(path) => {
                            picked.borrow_mut().insert(path.clone());
                            file_value(&path)
                        }
                        None => Value::Nil,
                    })
                })?;
                Ok(PromiseUd(promise))
            })?,
        )?;
    }
    {
        let picked = picked.clone();
        ui.set(
            "read_file",
            lua.create_function(move |lua, file: mlua::Value| {
                if !files {
                    return Err(denied("ui.read_file", "Files"));
                }
                let path = chosen(&picked, &file, "ui.read_file")?;
                off_thread(lua, format!("read {}", path.display()), move || {
                    std::fs::read(&path)
                        .map(|bytes| Some(String::from_utf8_lossy(&bytes).into_owned()))
                        .map_err(|e| format!("{}: {e}", path.display()))
                })
            })?,
        )?;
    }
    ui.set(
        "write_file",
        lua.create_function(move |lua, (file, text): (mlua::Value, String)| {
            if !files {
                return Err(denied("ui.write_file", "Files"));
            }
            let path = chosen(&picked, &file, "ui.write_file")?;
            off_thread(lua, format!("write {}", path.display()), move || {
                // Beside it, then over it: never half a file.
                let temp = path.with_extension("zgx-write");
                std::fs::write(&temp, text)
                    .and_then(|()| std::fs::rename(&temp, &path))
                    .map(|()| None)
                    .map_err(|e| format!("{}: {e}", path.display()))
            })
        })?,
    )?;
    Ok(())
}
