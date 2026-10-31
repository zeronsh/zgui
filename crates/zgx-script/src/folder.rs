//! `ui.open_folder`: a folder a person picked, as a capability. Only the
//! dialog makes a handle, and a handle reaches what is inside its root and
//! nothing else: paths are relative, `..` and absolute paths are refused, and
//! a symbolic link counts where it leads. Everything runs off the main
//! thread, and a write replaces its file whole (a temporary file beside it,
//! renamed over it). Scripts make no links, so a link that changes between
//! the check and the open takes another process. A host may keep a grant
//! past the run (`remember`): the platform's bookmark, where no script reads.
use crate::{
    calls::{PromiseUd, pending, settle_failing},
    errors::{Failure, Kind},
    value::Value,
};
use mlua::{Lua, Table, UserData, UserDataFields, UserDataMethods};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// How many entries a list gives unless asked for another limit.
const LIST_LIMIT: usize = 100_000;

/// Why a path is refused or failed: a refusal is `denied`, anything the
/// system says is `io`.
enum Trouble {
    Refused(String),
    Failed(String),
}
impl Trouble {
    fn failure(self) -> Failure {
        match self {
            Trouble::Refused(message) => Failure::new(Kind::Denied, message),
            Trouble::Failed(message) => Failure::new(Kind::Io, message),
        }
    }
}
fn failed(path: &str, error: std::io::Error) -> Trouble {
    Trouble::Failed(format!("{path}: {error}"))
}

/// The parts of `path`, a relative path within a folder (`/` between them).
fn parts(path: &str) -> Result<Vec<&str>, Trouble> {
    let refused = |why: &str| Trouble::Refused(format!("`{path}`: {why}"));
    if path.is_empty() {
        return Ok(Vec::new());
    }
    if path.contains('\0') {
        return Err(refused("not a path"));
    }
    if path.contains('\\') || path.contains(':') && cfg!(windows) {
        return Err(refused("paths are relative, with `/` between their parts"));
    }
    if path.starts_with('/') {
        return Err(refused("paths are relative to the folder"));
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|part| matches!(*part, "" | "." | "..")) {
        return Err(refused("a folder's handle reaches only what is inside it"));
    }
    Ok(parts)
}

/// Where `path` lies within `root` (resolved), by where its links lead:
/// `new` lets its end not exist yet (a write).
fn resolve(root: &Path, path: &str, new: bool) -> Result<PathBuf, Trouble> {
    let mut at = root.to_path_buf();
    for part in parts(path)? {
        let next = at.join(part);
        match fs::symlink_metadata(&next) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let leads = fs::canonicalize(&next).map_err(|error| failed(path, error))?;
                if !leads.starts_with(root) {
                    return Err(Trouble::Refused(format!(
                        "`{path}`: a link out of the folder"
                    )));
                }
                at = leads;
            }
            Ok(_) => at = next,
            Err(error) if new && error.kind() == std::io::ErrorKind::NotFound => at = next,
            Err(error) => return Err(failed(path, error)),
        }
    }
    Ok(at)
}

/// A file or folder a list found, its path from the root.
struct Entry {
    name: String,
    path: String,
    folder: bool,
    size: u64,
    modified: f64,
    /// Where it is, its links followed: what a `File` of it holds.
    full: PathBuf,
}
/// What `folder:list` gives: `limit` entries at most, by path, each folder's
/// before its contents. A link counts as what it leads to, is left out if
/// that is outside, and isn't entered (a link can loop).
fn list(root: &Path, path: &str, recursive: bool, limit: usize) -> Result<Vec<Entry>, Trouble> {
    let start = resolve(root, path, false)?;
    let mut out = Vec::new();
    let mut waiting = vec![(start, path.to_owned())];
    while let Some((dir, prefix)) = waiting.pop() {
        let mut names: Vec<_> = fs::read_dir(&dir)
            .map_err(|error| failed(&prefix, error))?
            .filter_map(Result::ok)
            .collect();
        names.sort_by_key(fs::DirEntry::file_name);
        let mut inner = Vec::new();
        for entry in names {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(TEMPORARY) {
                continue;
            }
            let Ok(link) = entry.file_type() else {
                continue;
            };
            let full = entry.path();
            let full = if link.is_symlink() {
                match fs::canonicalize(&full) {
                    Ok(leads) if leads.starts_with(root) => leads,
                    _ => continue,
                }
            } else {
                full
            };
            let Ok(meta) = fs::metadata(&full) else {
                continue;
            };
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if recursive && meta.is_dir() && !link.is_symlink() {
                inner.push((full.clone(), relative.clone()));
            }
            out.push(Entry {
                name,
                path: relative,
                folder: meta.is_dir(),
                size: if meta.is_file() { meta.len() } else { 0 },
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0., |at| at.as_secs_f64() * 1000.),
                full,
            });
            if out.len() >= limit {
                return Ok(out);
            }
        }
        // Depth first, in name order.
        waiting.extend(inner.into_iter().rev());
    }
    Ok(out)
}

/// In a temporary file's name, which a list leaves out.
const TEMPORARY: &str = ".zgx-write-";

/// `text` to `path` inside `root`, whole or not at all: its folders made,
/// and a temporary file beside it renamed over it, with its permissions.
fn write(root: &Path, path: &str, text: &str) -> Result<(), Trouble> {
    if parts(path)?.is_empty() {
        return Err(Trouble::Refused("a file's path is needed".into()));
    }
    let target = resolve(root, path, true)?;
    let parent = target
        .parent()
        .filter(|parent| parent.starts_with(root))
        .ok_or_else(|| Trouble::Refused(format!("`{path}`: outside the folder")))?;
    fs::create_dir_all(parent).map_err(|error| failed(path, error))?;
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let name = target
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let temporary = parent.join(format!(
        ".{name}{TEMPORARY}{}-{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        if let Ok(meta) = fs::metadata(&target) {
            fs::set_permissions(&temporary, meta.permissions())?;
        }
        fs::rename(&temporary, &target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written.map_err(|error| failed(path, error))
}

/// A folder a person picked. Only `ui.open_folder` makes one, and
/// `ui.remembered_folder` from a grant it kept.
pub struct Folder {
    root: Arc<Path>,
    name: String,
    /// Kept past the run under this key, by this host.
    kept: RefCell<Option<(String, Rc<Access>)>>,
    /// Ends the access a kept grant began, when the handle goes.
    release: Option<(zgx::platform::PlatformService, PathBuf)>,
}
impl std::fmt::Debug for Folder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Folder")
            .field("root", &self.root)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        if let Some((platform, path)) = self.release.take() {
            platform.0.release_folder(&path);
        }
    }
}
impl Folder {
    /// The folder at `root` (canonical) as a script holds it.
    fn value(self) -> Value {
        Value::Element(crate::value::ElementHandle(Rc::new(
            crate::value::Held::Folder(Rc::new(self)),
        )))
    }
}

/// What a host lets scripts have of folders, and the grants it keeps past a
/// run (`ui.open_folder { remember }`): the platform's, in its storage
/// directory's `.folders`, which no script reads (scripts read only what
/// was picked), or in memory without one.
pub(crate) struct Access {
    /// `ScriptHost::refuse_folders`: no folder at all.
    pub(crate) refused: Cell<bool>,
    /// `ScriptHost::refuse_remembered_folders`: none kept past a run.
    pub(crate) forgetful: Cell<bool>,
    persist: Rc<crate::persist::Persist>,
    memory: RefCell<HashMap<String, Vec<u8>>>,
}
impl Access {
    pub(crate) fn new(persist: Rc<crate::persist::Persist>) -> Self {
        Self {
            refused: Cell::new(false),
            forgetful: Cell::new(false),
            persist,
            memory: RefCell::default(),
        }
    }
    /// Where `key`'s grant lies, with a storage directory: its key in hex.
    fn file(&self, key: &str) -> Option<PathBuf> {
        let name: String = key.bytes().map(|b| format!("{b:02x}")).collect();
        Some(self.persist.dir()?.join(".folders").join(name))
    }
    fn keep(&self, key: &str, grant: Vec<u8>) {
        match self.file(key) {
            Some(file) => {
                let written = file
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| fs::write(&file, grant));
                if let Err(error) = written {
                    eprintln!("zgx: keeping folder `{key}`: {}: {error}", file.display());
                }
            }
            None => {
                self.memory.borrow_mut().insert(key.to_owned(), grant);
            }
        }
    }
    fn grant(&self, key: &str) -> Option<Vec<u8>> {
        match self.file(key) {
            Some(file) => fs::read(file).ok(),
            None => self.memory.borrow().get(key).cloned(),
        }
    }
    fn forget(&self, key: &str) {
        match self.file(key) {
            Some(file) => {
                let _ = fs::remove_file(file);
            }
            None => {
                self.memory.borrow_mut().remove(key);
            }
        }
    }
}

/// The folder `key`'s grant names, if it still resolves, its access begun
/// (until the handle goes); nil otherwise.
fn kept_folder(
    access: &Rc<Access>,
    key: String,
    platform: zgx::platform::PlatformService,
) -> Value {
    let Some(grant) = access.grant(&key) else {
        return Value::Nil;
    };
    let Some(found) = platform.0.resolve_folder(&grant) else {
        return Value::Nil;
    };
    if let Some(renewed) = &found.renewed {
        access.keep(&key, renewed.clone());
    }
    let Ok(root) = fs::canonicalize(&found.path) else {
        platform.0.release_folder(&found.path);
        return Value::Nil;
    };
    let name = root
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    Folder {
        root: root.into(),
        name,
        kept: RefCell::new(Some((key, access.clone()))),
        release: Some((platform, found.path)),
    }
    .value()
}

/// Whether the script running is a plugin's.
fn in_plugin(lua: &Lua) -> bool {
    lua.app_data_ref::<crate::node::Env>()
        .is_some_and(|env| matches!(*env.grants, crate::Grants::Only(_)))
}
/// A key to keep a folder under: some text.
fn key_of(what: &str, key: String) -> mlua::Result<String> {
    if key.is_empty() {
        return Err(mlua::Error::runtime(format!(
            "{what}: a folder's key is some text"
        )));
    }
    Ok(key)
}
/// A folder as a script holds it.
pub(crate) struct FolderUd(pub(crate) Rc<Folder>);

/// Run `work` on a thread of its own; `done` turns what it gives into the
/// promise's value on the main thread.
fn off_thread<T: Send + 'static>(
    lua: &Lua,
    name: String,
    work: impl FnOnce() -> Result<T, Trouble> + Send + 'static,
    done: impl FnOnce(T) -> Value + 'static,
) -> mlua::Result<PromiseUd> {
    let (tell, mut heard) = zgx::task::latest::<Result<T, Trouble>>();
    let promise = pending(lua, name)?;
    std::thread::spawn(move || {
        tell.send(work());
    });
    settle_failing(lua, &promise, async move {
        match heard.recv().await {
            Some(Ok(value)) => Ok(done(value)),
            Some(Err(trouble)) => Err(trouble.failure()),
            None => Err(Failure::new(Kind::Io, "the file thread stopped".to_owned())),
        }
    })?;
    Ok(PromiseUd(promise))
}

/// An entry as a script reads it; with `picked`, a file's `File` too, which
/// the script may then read, show and hand on as it does a dialog's.
fn entry_value(entry: Entry, picked: Option<&crate::io::Picked>) -> Value {
    let file = match picked {
        Some(picked) if !entry.folder => {
            let file = crate::io::file_value(&entry.full);
            picked.borrow_mut().insert(entry.full);
            file
        }
        _ => Value::Nil,
    };
    Value::record([
        ("file", file),
        ("name", Value::Str(entry.name.into())),
        ("path", Value::Str(entry.path.into())),
        (
            "kind",
            Value::Str(if entry.folder { "folder" } else { "file" }.into()),
        ),
        ("size", Value::Num(entry.size as f64)),
        ("modified", Value::Num(entry.modified)),
    ])
}

impl UserData for FolderUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, folder| Ok(folder.0.name.clone()));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // The host keeps it no more: a later launch's
        // `ui.remembered_folder` is nil. This run's handle still reaches it.
        methods.add_method("forget", |_, folder, ()| {
            if let Some((key, access)) = folder.0.kept.borrow_mut().take() {
                access.forget(&key);
            }
            Ok(())
        });
        // `folder:list(path?, { recursive?, limit?, files? })`.
        methods.add_method(
            "list",
            |lua, folder, (path, options): (Option<String>, Option<Table>)| {
                let path = path.unwrap_or_default();
                let (recursive, limit, files) = match &options {
                    Some(options) => (
                        options.get::<Option<bool>>("recursive")?.unwrap_or(false),
                        options.get::<Option<usize>>("limit")?,
                        options.get::<Option<bool>>("files")?.unwrap_or(false),
                    ),
                    None => (false, None, false),
                };
                let picked = files
                    .then(|| crate::node::env(lua).map(|env| env.picked.clone()))
                    .transpose()?;
                let root = folder.0.root.clone();
                off_thread(
                    lua,
                    format!("list {path}"),
                    move || list(&root, &path, recursive, limit.unwrap_or(LIST_LIMIT)),
                    move |entries| {
                        let picked = picked.as_ref();
                        Value::List(
                            entries
                                .into_iter()
                                .map(|e| entry_value(e, picked))
                                .collect(),
                        )
                    },
                )
            },
        );
        // `folder:file(path)`: a file inside it as a `File`, which `ui.image`,
        // `ui.read_file` and `drag_export` take as they do a dialog's.
        methods.add_method("file", |lua, folder, path: String| {
            let root = folder.0.root.clone();
            let picked = crate::node::env(lua)?.picked.clone();
            off_thread(
                lua,
                format!("file {path}"),
                move || {
                    let full = resolve(&root, &path, false)?;
                    let meta = fs::metadata(&full).map_err(|error| failed(&path, error))?;
                    if !meta.is_file() {
                        return Err(Trouble::Failed(format!("{path}: not a file")));
                    }
                    Ok(full)
                },
                move |full| {
                    let file = crate::io::file_value(&full);
                    picked.borrow_mut().insert(full);
                    file
                },
            )
        });
        methods.add_method("read", |lua, folder, path: String| {
            let root = folder.0.root.clone();
            off_thread(
                lua,
                format!("read {path}"),
                move || {
                    let file = resolve(&root, &path, false)?;
                    let bytes = fs::read(file).map_err(|error| failed(&path, error))?;
                    String::from_utf8(bytes)
                        .map_err(|_| Trouble::Failed(format!("{path}: not text")))
                },
                |text| Value::Str(text.into()),
            )
        });
        methods.add_method("write", |lua, folder, (path, text): (String, String)| {
            let root = folder.0.root.clone();
            off_thread(
                lua,
                format!("write {path}"),
                move || write(&root, &path, &text),
                |()| Value::Nil,
            )
        });
    }
}

/// Installs `ui.open_folder` and `ui.remembered_folder`. `allowed` is false
/// for a plugin the host did not grant folders; `access` says what the host
/// refuses and keeps.
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    shared: crate::io::SharedPlatform,
    allowed: bool,
    access: Rc<Access>,
) -> mlua::Result<()> {
    let (remembering, platforms) = (access.clone(), shared.clone());
    ui.set(
        "remembered_folder",
        lua.create_function(move |lua, key: String| {
            const WHAT: &str = "ui.remembered_folder";
            if !allowed {
                return Err(crate::io::denied(WHAT, "Folders"));
            }
            if in_plugin(lua) {
                return Err(mlua::Error::runtime(format!(
                    "{WHAT}: a plugin keeps no folders"
                )));
            }
            let key = key_of(WHAT, key)?;
            let promise = pending(lua, "remembered_folder".into())?;
            let access = remembering.clone();
            let kept = !access.refused.get() && !access.forgetful.get();
            // The platform as the promise's work first runs: a script asking
            // as it loads has its window's by then.
            let platforms = platforms.clone();
            settle_failing(lua, &promise, async move {
                let platform = platforms.borrow().clone();
                Ok(match (kept, platform) {
                    (true, Some(platform)) => kept_folder(&access, key, platform),
                    _ => Value::Nil,
                })
            })?;
            Ok(PromiseUd(promise))
        })?,
    )?;
    ui.set(
        "open_folder",
        lua.create_function(move |lua, opts: Option<Table>| {
            if !allowed {
                return Err(crate::io::denied("ui.open_folder", "Folders"));
            }
            let remember = match &opts {
                Some(opts) => opts.raw_get::<Option<String>>("remember")?,
                None => None,
            };
            if remember.is_some() && in_plugin(lua) {
                return Err(mlua::Error::runtime(
                    "ui.open_folder: a plugin keeps no folders (`remember`)",
                ));
            }
            let remember = remember
                .map(|key| key_of("ui.open_folder", key))
                .transpose()?
                .filter(|_| !access.forgetful.get());
            let promise = pending(lua, "open_folder".into())?;
            if access.refused.get() {
                let message = "ui.open_folder: this app's host refuses folder access".to_owned();
                settle_failing(lua, &promise, async move {
                    Err(Failure::new(Kind::Denied, message))
                })?;
                return Ok(PromiseUd(promise));
            }
            let picker = crate::io::picker(opts, "Open Folder")?;
            let (access, shared) = (access.clone(), shared.clone());
            settle_failing(lua, &promise, async move {
                let platform = crate::io::later(&shared, "ui.open_folder")
                    .map_err(|e| Failure::new(Kind::Io, e))?;
                let picked = platform
                    .0
                    .open_folder(picker)
                    .await
                    .map_err(|e| Failure::new(Kind::Io, e))?;
                let Some(path) = picked else {
                    return Ok(Value::Nil);
                };
                let root = fs::canonicalize(&path)
                    .map_err(|e| Failure::new(Kind::Io, format!("{}: {e}", path.display())))?;
                let name = root
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                // Kept past the run, if asked and the platform can.
                let kept = remember.and_then(|key| {
                    let grant = platform.0.bookmark_folder(&root)?;
                    access.keep(&key, grant);
                    Some((key, access))
                });
                Ok(Folder {
                    root: root.into(),
                    name,
                    kept: RefCell::new(kept),
                    release: None,
                }
                .value())
            })?;
            Ok(PromiseUd(promise))
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zgx-folder-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("inside/deep")).unwrap();
        fs::write(dir.join("inside/note.md"), "hello").unwrap();
        fs::canonicalize(dir).unwrap()
    }
    fn refused(result: Result<PathBuf, Trouble>) -> bool {
        matches!(result, Err(Trouble::Refused(_)))
    }

    #[test]
    fn a_path_stays_inside_its_folder() {
        let root = folder("paths");
        assert!(resolve(&root, "inside/note.md", false).is_ok());
        for path in [
            "..",
            "../secret",
            "inside/../../secret",
            "inside/..",
            "./inside",
            "/etc/passwd",
            "inside//note.md",
            "inside\\note.md",
            "in\0side",
        ] {
            assert!(refused(resolve(&root, path, false)), "{path}");
            assert!(refused(resolve(&root, path, true)), "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_counts_where_it_leads() {
        use std::os::unix::fs::symlink;
        let root = folder("links");
        let outside = folder("links-outside");
        fs::write(outside.join("secret"), "no").unwrap();
        symlink(&outside, root.join("out")).unwrap();
        symlink(outside.join("secret"), root.join("file-out")).unwrap();
        symlink(root.join("inside"), root.join("in")).unwrap();
        symlink(root.join("gone"), root.join("dangling")).unwrap();

        assert!(refused(resolve(&root, "out/secret", false)));
        assert!(
            refused(resolve(&root, "out/new", true)),
            "a write through it"
        );
        assert!(refused(resolve(&root, "file-out", false)));
        assert!(refused(resolve(&root, "file-out", true)), "a write over it");
        assert!(
            resolve(&root, "dangling", true).is_err(),
            "a link to nowhere"
        );
        assert_eq!(
            resolve(&root, "in/note.md", false).ok(),
            Some(root.join("inside/note.md"))
        );
        let names: Vec<String> = list(&root, "", true, 100)
            .ok()
            .unwrap()
            .into_iter()
            .map(|entry| entry.path)
            .collect();
        assert!(names.contains(&"in".to_owned()), "a link inside is listed");
        assert!(
            !names
                .iter()
                .any(|n| n.starts_with("out") || n == "file-out")
        );
        assert!(
            !names.iter().any(|n| n.starts_with("in/")),
            "and not entered"
        );
    }

    #[test]
    fn a_write_replaces_a_file_whole_and_leaves_nothing_beside_it() {
        let root = folder("writes");
        assert!(write(&root, "new/deeper/file.md", "one").is_ok());
        assert_eq!(
            fs::read_to_string(root.join("new/deeper/file.md")).unwrap(),
            "one"
        );
        assert!(write(&root, "inside/note.md", "two").is_ok());
        assert_eq!(
            fs::read_to_string(root.join("inside/note.md")).unwrap(),
            "two"
        );
        let beside: Vec<_> = fs::read_dir(root.join("inside"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert_eq!(beside.len(), 2, "{beside:?}");
        assert!(matches!(write(&root, "", "x"), Err(Trouble::Refused(_))));
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_a_files_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = folder("modes");
        fs::set_permissions(
            root.join("inside/note.md"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        assert!(write(&root, "inside/note.md", "two").is_ok());
        let mode = fs::metadata(root.join("inside/note.md")).unwrap();
        assert_eq!(mode.permissions().mode() & 0o777, 0o640);
    }

    #[test]
    fn a_list_goes_depth_first_up_to_its_limit() {
        let root = folder("lists");
        fs::write(root.join("a.md"), "12345").unwrap();
        let all: Vec<_> = list(&root, "", true, 100).ok().unwrap();
        let paths: Vec<&str> = all.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["a.md", "inside", "inside/deep", "inside/note.md"]);
        assert_eq!(all[0].size, 5);
        assert!(!all[0].folder && all[1].folder);
        assert_eq!(list(&root, "", true, 2).ok().unwrap().len(), 2);
        assert_eq!(list(&root, "", false, 100).ok().unwrap().len(), 2);
        assert_eq!(
            list(&root, "inside", false, 100).ok().unwrap()[0].path,
            "inside/deep"
        );
    }
}
