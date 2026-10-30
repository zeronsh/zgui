//! The machine beyond the window, through the host's platform
//! (`zgx::platform`, which a desktop window provides): `ui.clipboard`,
//! file dialogs (`ui.open_files`, `ui.save_file`) and the files the user
//! picked in them (`ui.read_file`, `ui.write_file`). A script reaches only
//! files the user chose, as a web page does; reads and writes run off the
//! main thread. A plugin gets none of it unless its host grants it.
use crate::{
    calls::{PromiseUd, pending, settle_failing, settle_with},
    value::Value,
};
use mlua::{IntoLua, Lua, Table};
use std::{
    cell::RefCell,
    collections::HashSet,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use zgx::platform::{FilePicker, PlatformService};

/// The window's platform, once the host's view mounts in one.
pub(crate) type SharedPlatform = Rc<RefCell<Option<PlatformService>>>;
/// The files the user picked in this host's dialogs: the only ones scripts
/// may read or write.
pub(crate) type Picked = Rc<RefCell<HashSet<PathBuf>>>;

pub(crate) fn platform(shared: &SharedPlatform, what: &str) -> mlua::Result<PlatformService> {
    shared.borrow().clone().ok_or_else(|| {
        mlua::Error::runtime(format!(
            "{what}: no platform here (the clipboard and file dialogs need a desktop window)"
        ))
    })
}
pub(crate) fn denied(what: &str, capability: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{what}: a plugin does this only if the host grants it \
         (`ScriptHost::grant(plugin, Capability::{capability})`)"
    ))
}

/// Call each clicked notification's `on_click` (`clicks`, by tag), the
/// app brought forward first: a task waiting on the platform's clicks,
/// started with the script's first `on_click` and gone with the script.
fn listen(
    lua: &Lua,
    shared: SharedPlatform,
    clicks: Rc<std::cell::RefCell<std::collections::HashMap<String, mlua::Function>>>,
) -> mlua::Result<()> {
    let env = crate::node::env(lua)?;
    let vm = env.vm.clone();
    env.timers.spawn(async move {
        loop {
            let next = shared
                .borrow()
                .clone()
                .map(|platform| platform.0.next_notification_click());
            let Some(next) = next else { return };
            let Some(tag) = next.await else { return };
            let Some(vm) = vm.upgrade() else { return };
            let Some(on_click) = clicks.borrow().get(&tag).cloned() else {
                continue;
            };
            vm.runtime.clone().batch(|| {
                if let Err(e) = vm.call::<()>(&on_click, ()) {
                    vm.report(e);
                }
            });
        }
    });
    Ok(())
}

/// `{ path, name, size, type }`, as dialogs and drops give files to
/// scripts (the web's `File`): `size` in bytes (0 for what is not a file
/// to read, a folder), `type` its media type by its extension
/// (`"image/png"`), `""` when none is known.
pub(crate) fn file_value(path: &Path) -> Value {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map_or(0, |m| m.len());
    Value::Map(
        vec![
            (Rc::from("name"), Value::Str(name.into())),
            (Rc::from("path"), Value::Str(path.to_string_lossy().into())),
            (Rc::from("size"), Value::Num(size as f64)),
            (Rc::from("type"), Value::Str(media_type(path).into())),
        ]
        .into(),
    )
}

/// A file's media type by its extension, as browsers give `File.type`:
/// the common ones a person attaches, `""` for the rest.
pub(crate) fn media_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("heic") => "image/heic",
        Some("avif") => "image/avif",
        Some("bmp") => "image/bmp",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("json") => "application/json",
        Some("txt" | "log") => "text/plain",
        Some("md") => "text/markdown",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "",
    }
}
/// A dialog's options: `title`, `directory`, `name` (to save as),
/// `filters` (`{ { "Images", { "png", "jpg" } } }`) and `multiple`.
pub(crate) fn picker(opts: Option<Table>, title: &str) -> mlua::Result<FilePicker> {
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
    settle_with(lua, &promise, crate::errors::Kind::Io, async move {
        match heard.recv().await {
            Some(Ok(Some(text))) => Ok(Value::Str(text.into())),
            Some(Ok(None)) => Ok(Value::Nil),
            Some(Err(error)) => Err(error),
            None => Err("the file thread stopped".into()),
        }
    })?;
    Ok(PromiseUd(promise))
}

/// `on_paste`'s `files`: those on the clipboard, which a script may then
/// read as it does what the person picks. None for a plugin without both
/// grants, or where no platform reads them.
pub(crate) fn pasted_files(lua: &Lua) -> mlua::Result<mlua::Value> {
    let env = crate::node::env(lua)?;
    let allowed = env.grants.allows(&crate::Capability::Files)
        && env.grants.allows(&crate::Capability::Clipboard);
    let platform = env.platform.borrow().clone().filter(|_| allowed);
    let paths = platform.map_or_else(Vec::new, |platform| platform.0.clipboard_files());
    env.picked.borrow_mut().extend(paths.iter().cloned());
    Value::List(paths.iter().map(|path| file_value(path)).collect()).into_lua(lua)
}

/// Installs `ui.clipboard` (text and images), `ui.open_files`, `ui.save_file`, `ui.read_file`
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
    {
        let shared = shared.clone();
        board.set(
            "read_image",
            // A promise: a large image is decoded off the main thread.
            lua.create_function(move |lua, ()| {
                if !clipboard {
                    return Err(denied("ui.clipboard.read_image", "Clipboard"));
                }
                let reading = platform(&shared, "ui.clipboard.read_image")?
                    .0
                    .clipboard_image();
                let promise = pending(lua, "clipboard.read_image".into())?;
                settle_with(lua, &promise, crate::errors::Kind::Io, async move {
                    Ok(reading
                        .await
                        .map_or(Value::Nil, |image| Value::Image(Arc::new(image))))
                })?;
                Ok(PromiseUd(promise))
            })?,
        )?;
    }
    {
        let shared = shared.clone();
        board.set(
            "write_image",
            // A promise too: encoding a large image takes frames.
            lua.create_function(
                move |lua, image: mlua::UserDataRef<crate::node::BitmapUd>| {
                    if !clipboard {
                        return Err(denied("ui.clipboard.write_image", "Clipboard"));
                    }
                    let writing = platform(&shared, "ui.clipboard.write_image")?
                        .0
                        .set_clipboard_image(image.0.clone());
                    let promise = pending(lua, "clipboard.write_image".into())?;
                    settle_with(lua, &promise, crate::errors::Kind::Io, async move {
                        Ok(Value::Bool(writing.await))
                    })?;
                    Ok(PromiseUd(promise))
                },
            )?,
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
                settle_with(lua, &promise, crate::errors::Kind::Io, async move {
                    let paths = dialog.await?;
                    picked.borrow_mut().extend(paths.iter().cloned());
                    Ok(Value::List(paths.iter().map(|p| file_value(p)).collect()))
                })?;
                Ok(PromiseUd(promise))
            })?,
        )?;
    }
    // `ui.notify { title, body, tag }`: a system notification, the app's
    // (a plugin posts none); a promise settled once the system has it, or
    // with why not (`kind` "unsupported", "denied", or "io").
    let app = !crate::node::env(lua).is_ok_and(|env| matches!(*env.grants, crate::Grants::Only(_)));
    {
        let shared = shared.clone();
        // Each notification's `on_click`, by its tag; and whether this
        // script listens for clicks yet (from its first `on_click`).
        let clicks: Rc<std::cell::RefCell<std::collections::HashMap<String, mlua::Function>>> =
            Rc::default();
        let listening = Rc::new(std::cell::Cell::new(false));
        let made = std::cell::Cell::new(0u64);
        ui.set(
            "notify",
            lua.create_function(move |lua, opts: Table| {
                if !app {
                    return Err(mlua::Error::runtime(
                        "ui.notify: the app's own, not a plugin's",
                    ));
                }
                let title: String = opts
                    .get::<Option<String>>("title")?
                    .ok_or_else(|| mlua::Error::runtime("ui.notify needs a `title`"))?;
                let body = opts.get::<Option<String>>("body")?.unwrap_or_default();
                // Its tag, the script's or one of its own: what a click is
                // told by.
                let tag = match opts.get::<Option<String>>("tag")? {
                    Some(tag) => tag,
                    None => {
                        made.set(made.get() + 1);
                        format!("zgx-notification-{}", made.get())
                    }
                };
                let notification = zgx::platform::Notification::new(title, body).tag(tag.clone());
                if let Some(on_click) = opts.get::<Option<mlua::Function>>("on_click")? {
                    clicks.borrow_mut().insert(tag, on_click);
                    if !listening.replace(true) {
                        listen(lua, shared.clone(), clicks.clone())?;
                    }
                }
                let promise = pending(lua, "notify".into())?;
                // The platform looked for as the promise first runs: a
                // script calling as it loads is mounted by then.
                let shared = shared.clone();
                settle_failing(lua, &promise, async move {
                    use crate::errors::{Failure, Kind};
                    use zgx::platform::NotifyError;
                    let shown =
                        (shared.borrow().clone()).map(|platform| platform.0.notify(notification));
                    let Some(shown) = shown else {
                        return Err(Failure::new(
                            Kind::Unsupported,
                            "ui.notify: no system here (headless)",
                        ));
                    };
                    shown.await.map(|()| Value::Nil).map_err(|e| match e {
                        NotifyError::Unsupported(why) => Failure::new(Kind::Unsupported, why),
                        NotifyError::Denied(why) => Failure::new(Kind::Denied, why),
                        NotifyError::Failed(why) => Failure::new(Kind::Io, why),
                    })
                })?;
                Ok(PromiseUd(promise))
            })?,
        )?;
    }
    // `ui.badge(n)`: a count on the app's icon (a number or text; 0 or nil
    // takes it off); whether the system shows one.
    {
        let shared = shared.clone();
        ui.set(
            "badge",
            lua.create_function(move |_, badge: mlua::Value| {
                if !app {
                    return Err(mlua::Error::runtime(
                        "ui.badge: the app's own, not a plugin's",
                    ));
                }
                let badge = match badge {
                    mlua::Value::Nil => None,
                    mlua::Value::Integer(0) => None,
                    mlua::Value::Number(0.) => None,
                    mlua::Value::Integer(n) => Some(n.to_string()),
                    mlua::Value::Number(n) => Some(format!("{}", n.round() as i64)),
                    mlua::Value::String(text) => Some(text.to_str()?.to_string()),
                    _ => return Err(mlua::Error::runtime("ui.badge takes a number, text or nil")),
                };
                Ok(match &*shared.borrow() {
                    Some(platform) => platform.0.set_badge(badge.as_deref()),
                    None => false,
                })
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
                settle_with(lua, &promise, crate::errors::Kind::Io, async move {
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
