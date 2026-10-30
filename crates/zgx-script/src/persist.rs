//! `ui.persist(key, initial)`: state saved across runs.
//!
//! Each key is one JSON file in the host's storage directory (one directory
//! per script). A value is read once, the first time a run asks for it, and
//! then lives in the host like `ui.state`, so reloads keep it without reading
//! again. Changes are saved in batches: the first change starts a 250 ms
//! timer, and when it fires every value changed since is turned into JSON
//! on the main thread (once, however often it changed) and handed to a
//! writer thread, which writes a temporary file and renames it over the old
//! one, so a crash leaves the old file or the new one, never half of one.
//! Steady typing is saved every 250 ms rather than never.
use crate::value::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::mpsc,
    time::Duration,
};
use zgx::reactive::{Effect, Runtime, Signal};

/// How long after the last change a value is saved.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// The platform's directory for an application's data: `~/Library/Application
/// Support/<app_id>` on macOS, `%APPDATA%\<app_id>` on Windows,
/// `$XDG_DATA_HOME/<app_id>` (or `~/.local/share/<app_id>`) elsewhere.
/// `None` for an id that is not one name (`..`, `a/b`, an absolute path),
/// which would put the data elsewhere.
pub fn app_data_dir(app_id: &str) -> Option<PathBuf> {
    let mut parts = Path::new(app_id).components();
    if !matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return None;
    }
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else if cfg!(windows) {
        var("APPDATA")?
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))?
    };
    Some(base.join(app_id))
}

struct Entry {
    signal: Signal<Value>,
    /// What a run would start from (the file's value, or the initial one
    /// without a file): a change is a value that differs from it.
    saved: RefCell<Value>,
    path: Option<PathBuf>,
}
enum Job {
    /// Write the JSON to the file.
    Write(PathBuf, String),
    /// Reply once everything sent before it is on disk.
    Barrier(mpsc::Sender<()>),
}

pub(crate) struct Persist {
    runtime: Runtime,
    dir: RefCell<Option<PathBuf>>,
    entries: RefCell<HashMap<String, Rc<Entry>>>,
    /// One per entry; they mark it dirty when its value changes.
    watches: RefCell<Vec<Effect>>,
    dirty: RefCell<HashSet<String>>,
    scheduled: Cell<bool>,
    /// The host view's tasks, for the debounce timer.
    tasks: RefCell<Option<zgx::compose::Tasks>>,
    writer: RefCell<Option<(mpsc::Sender<Job>, std::thread::JoinHandle<()>)>>,
    warned: Cell<bool>,
    warnings: crate::SharedWarnings,
    me: Weak<Persist>,
}
impl Persist {
    /// Says nothing of having no storage: a host that means to have none,
    /// as a `zgx test` app does.
    pub(crate) fn quiet(&self) {
        self.warned.set(true);
    }
    pub fn new(runtime: &Runtime, warnings: crate::SharedWarnings) -> Rc<Self> {
        Rc::new_cyclic(|me| Self {
            runtime: runtime.clone(),
            dir: RefCell::default(),
            entries: RefCell::default(),
            watches: RefCell::default(),
            dirty: RefCell::default(),
            scheduled: Cell::new(false),
            tasks: RefCell::default(),
            writer: RefCell::default(),
            warned: Cell::new(false),
            warnings,
            me: me.clone(),
        })
    }
    pub fn set_dir(&self, dir: Option<PathBuf>) {
        *self.dir.borrow_mut() = dir;
    }
    pub fn dir(&self) -> Option<PathBuf> {
        self.dir.borrow().clone()
    }
    pub fn set_tasks(&self, tasks: zgx::compose::Tasks) {
        *self.tasks.borrow_mut() = Some(tasks);
    }

    /// `script`'s saved state for `key`: read from disk the first time,
    /// `initial` if nothing was saved.
    pub fn state(&self, script: &str, key: &str, initial: Value) -> Signal<Value> {
        let id = format!("{script}\n{key}");
        if let Some(entry) = self.entries.borrow().get(&id) {
            return entry.signal.clone();
        }
        let path = self.dir.borrow().as_ref().map(|dir| {
            dir.join(file_name(script))
                .join(format!("{}.json", file_name(key)))
        });
        let loaded = path
            .as_deref()
            .and_then(|path| load(path, key, &self.warnings));
        // With no file, a run starts from `initial`: nothing to save yet.
        let value = loaded.unwrap_or(initial);
        let entry = Rc::new(Entry {
            signal: self.runtime.signal(value.clone()),
            saved: RefCell::new(value),
            path,
        });
        self.entries.borrow_mut().insert(id.clone(), entry.clone());
        let me = self.me.clone();
        let watched = Rc::downgrade(&entry);
        let watch = self.runtime.untracked(|| {
            self.runtime.effect(move || {
                let Some(entry) = watched.upgrade() else {
                    return;
                };
                // Shared subtrees compare without being walked.
                let changed = entry.signal.with(|v| *v != *entry.saved.borrow());
                if changed && let Some(persist) = me.upgrade() {
                    persist.mark(&id);
                }
            })
        });
        self.watches.borrow_mut().push(watch);
        entry.signal.clone()
    }

    /// `script`'s saved state for `key`, if a run asked for it (templates
    /// such as `"{count}"` find it by name).
    /// Every value held, by key (scripts' keys, without their script), in
    /// key order.
    pub fn entries(&self) -> Vec<(String, Signal<Value>)> {
        let mut out: Vec<_> = self
            .entries
            .borrow()
            .iter()
            .map(|(id, e)| {
                let key = id.split_once('\n').map_or(id.as_str(), |(_, key)| key);
                (key.to_string(), e.signal.clone())
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    pub fn get(&self, script: &str, key: &str) -> Option<Signal<Value>> {
        let id = format!("{script}\n{key}");
        self.entries.borrow().get(&id).map(|e| e.signal.clone())
    }

    fn mark(&self, id: &str) {
        if !self.dirty.borrow_mut().insert(id.to_owned()) || self.scheduled.replace(true) {
            return;
        }
        let tasks = self.tasks.borrow().clone();
        match tasks {
            Some(tasks) => {
                let me = self.me.clone();
                tasks.spawn(async move {
                    zgx::timer::sleep(DEBOUNCE).await;
                    if let Some(persist) = me.upgrade() {
                        persist.flush();
                    }
                });
            }
            // No view mounted (and so no timer): save at once.
            None => self.flush(),
        }
    }

    /// Hands every changed value to the writer.
    pub fn flush(&self) {
        self.scheduled.set(false);
        let dirty = std::mem::take(&mut *self.dirty.borrow_mut());
        for id in dirty {
            let Some(entry) = self.entries.borrow().get(&id).cloned() else {
                continue;
            };
            let value = entry.signal.with_untracked(Clone::clone);
            if value == *entry.saved.borrow() {
                continue;
            }
            let Some(path) = entry.path.clone() else {
                if !self.warned.replace(true) {
                    crate::warning(
                        Some(&self.warnings),
                        "ui.persist: the host has no storage (ScriptHost::app_id or \
                         ScriptHost::storage), so saved state lasts until the app quits"
                            .into(),
                    );
                }
                continue;
            };
            let json = value.to_json();
            *entry.saved.borrow_mut() = value;
            self.send(Job::Write(path, json));
        }
    }
    /// Saves what changed and waits until it is on disk.
    pub fn save(&self) {
        self.flush();
        if self.writer.borrow().is_none() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.send(Job::Barrier(tx));
        let _ = rx.recv();
    }
    fn send(&self, job: Job) {
        let mut writer = self.writer.borrow_mut();
        let (tx, _) = writer.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel();
            let thread = std::thread::Builder::new()
                .name("zgx-persist".into())
                .stack_size(256 * 1024)
                .spawn(move || write_all(rx))
                .expect("start the ui.persist writer");
            (tx, thread)
        });
        let _ = tx.send(job);
    }
}
impl Drop for Persist {
    /// What changed in the last 250 ms is saved before the host goes.
    fn drop(&mut self) {
        self.flush();
        if let Some((tx, thread)) = self.writer.take() {
            drop(tx);
            let _ = thread.join();
        }
    }
}

/// The value saved at `path`, if any.
fn load(path: &Path, key: &str, warnings: &crate::SharedWarnings) -> Option<Value> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            crate::warning(
                Some(warnings),
                format!("ui.persist(\"{key}\"): {}: {e}", path.display()),
            );
            return None;
        }
    };
    let value = Value::from_json(&text);
    if value.is_none() {
        crate::warning(
            Some(warnings),
            format!(
                "ui.persist(\"{key}\"): {} is not JSON; starting from the initial value",
                path.display()
            ),
        );
    }
    value
}

/// The writer thread: the latest write for each file, a batch at a time.
fn write_all(jobs: mpsc::Receiver<Job>) {
    while let Ok(first) = jobs.recv() {
        let mut latest: Vec<(PathBuf, String)> = Vec::new();
        let mut barriers = Vec::new();
        for job in std::iter::once(first).chain(jobs.try_iter()) {
            match job {
                Job::Write(path, json) => match latest.iter_mut().find(|(p, _)| *p == path) {
                    Some(slot) => slot.1 = json,
                    None => latest.push((path, json)),
                },
                Job::Barrier(reply) => barriers.push(reply),
            }
        }
        for (path, json) in latest {
            if let Err(e) = write(&path, &json) {
                eprintln!("ui.persist: {}: {e}", path.display());
            }
        }
        for reply in barriers {
            let _ = reply.send(());
        }
    }
}
/// Replaces the file at `path` whole: a temporary file renamed over it.
fn write(path: &Path, json: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = dir.join(format!(".{name}.tmp"));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&temp, path)
}

/// `name` as a file name: letters, digits, `-`, `_` and inner `.` as they
/// are, every other byte as `%XX`.
fn file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, byte) in name.bytes().enumerate() {
        let plain =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') || (byte == b'.' && i > 0);
        if plain {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    if out.is_empty() {
        out.push_str("%00");
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_names_keep_plain_names_and_escape_the_rest() {
        assert_eq!(super::file_name("notes"), "notes");
        assert_eq!(super::file_name("app.luau"), "app.luau");
        assert_eq!(
            super::file_name("lib/x.luau::open"),
            "lib%2Fx.luau%3A%3Aopen"
        );
        assert_eq!(super::file_name(".hidden"), "%2Ehidden");
        assert_eq!(super::file_name(""), "%00");
    }
    #[test]
    fn an_app_id_names_one_directory_inside_the_platforms() {
        if let Some(dir) = super::app_data_dir("mail") {
            assert!(dir.ends_with("mail"));
        }
        for id in ["", "..", ".", "a/b", "/etc", "../elsewhere"] {
            assert_eq!(super::app_data_dir(id), None, "{id}");
        }
    }
}
