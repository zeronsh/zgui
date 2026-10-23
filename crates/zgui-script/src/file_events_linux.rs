//! Changes to a set of files, from the kernel (inotify): a watching host
//! waits for one with nothing running meanwhile, where polling stats every
//! file a few times a second. Each file and the directory holding it are
//! watched, so a file an editor replaces (a save as write-and-rename) is
//! seen as well as one written in place; of a directory's entries only the
//! watched files' count, so an editor's swap file written beside them wakes
//! nothing.
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    os::{fd::RawFd, unix::ffi::OsStrExt},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// What a watch descriptor reports: any change (a file, or a directory
/// watched whole), or a change to one of these entries of its directory.
type Watches = HashMap<i32, Option<HashSet<OsString>>>;

pub(crate) struct FileEvents {
    inotify: RawFd,
    /// Written when this goes: its thread closes both and ends.
    wake: RawFd,
    done: Arc<AtomicBool>,
    watches: Arc<Mutex<Watches>>,
    /// What is watched now: files, then directories.
    watched: (Vec<PathBuf>, Vec<PathBuf>),
    changed: zgui::task::LatestReceiver<()>,
}

impl FileEvents {
    /// An inotify instance and the thread that reads it; `None` if the
    /// kernel gives none.
    pub fn new() -> Option<Self> {
        let inotify = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if inotify < 0 {
            return None;
        }
        let wake = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if wake < 0 {
            unsafe { libc::close(inotify) };
            return None;
        }
        let (tell, changed) = zgui::task::latest::<()>();
        let done = Arc::new(AtomicBool::new(false));
        let watches = Arc::new(Mutex::new(Watches::new()));
        let (ended, heard) = (done.clone(), watches.clone());
        let spawned = std::thread::Builder::new()
            .name("zgui-script file events".into())
            .spawn(move || {
                // Aligned for `inotify_event`, and room for many at once.
                let mut buffer = [0u64; 512];
                loop {
                    let mut fds = [
                        libc::pollfd {
                            fd: inotify,
                            events: libc::POLLIN,
                            revents: 0,
                        },
                        libc::pollfd {
                            fd: wake,
                            events: libc::POLLIN,
                            revents: 0,
                        },
                    ];
                    let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
                    if ended.load(Ordering::Acquire)
                        || (n < 0
                            && std::io::Error::last_os_error().kind()
                                != std::io::ErrorKind::Interrupted)
                    {
                        unsafe {
                            libc::close(inotify);
                            libc::close(wake);
                        }
                        return;
                    }
                    if n <= 0 || fds[0].revents & libc::POLLIN == 0 {
                        continue;
                    }
                    let read = unsafe {
                        libc::read(
                            inotify,
                            buffer.as_mut_ptr().cast(),
                            std::mem::size_of_val(&buffer),
                        )
                    };
                    if read <= 0 {
                        continue;
                    }
                    let bytes = unsafe {
                        std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), read as usize)
                    };
                    if concerns(bytes, &heard.lock().unwrap()) {
                        tell.send(());
                    }
                }
            });
        if spawned.is_err() {
            unsafe {
                libc::close(inotify);
                libc::close(wake);
            }
            return None;
        }
        Some(Self {
            inotify,
            wake,
            done,
            watches,
            watched: Default::default(),
            changed,
        })
    }

    /// Watch `paths` (and their directories) and the entries of `whole` from
    /// now on, instead of what was watched before. Watching what is already
    /// watched adds nothing, unless a file was replaced (its watch still
    /// follows the old one).
    pub fn watch(&mut self, mut paths: Vec<PathBuf>, mut whole: Vec<PathBuf>, replaced: bool) {
        paths.sort();
        paths.dedup();
        whole.sort();
        whole.dedup();
        if (&paths, &whole) == (&self.watched.0, &self.watched.1) && !replaced {
            return;
        }
        let file = libc::IN_MODIFY
            | libc::IN_ATTRIB
            | libc::IN_CLOSE_WRITE
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF;
        let entries = libc::IN_MODIFY
            | libc::IN_ATTRIB
            | libc::IN_CLOSE_WRITE
            | libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_ONLYDIR;
        // Held while watches are added, so the thread reads nothing they
        // report before it knows them.
        let mut watches = self.watches.lock().unwrap();
        let mut next = Watches::new();
        let add = |path: &std::path::Path, mask: u32| {
            let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
            let wd = unsafe { libc::inotify_add_watch(self.inotify, c_path.as_ptr(), mask) };
            (wd >= 0).then_some(wd)
        };
        for path in &paths {
            if let Some(wd) = add(path, file) {
                next.insert(wd, None);
            }
        }
        for dir in &whole {
            if let Some(wd) = add(dir, entries) {
                next.insert(wd, None);
            }
        }
        for path in &paths {
            let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
                continue;
            };
            let dir = if dir.as_os_str().is_empty() {
                std::path::Path::new(".")
            } else {
                dir
            };
            if let Some(wd) = add(dir, entries)
                && let Some(names) = next.entry(wd).or_insert_with(|| Some(HashSet::new()))
            {
                names.insert(name.to_owned());
            }
        }
        for wd in watches.keys() {
            if !next.contains_key(wd) {
                unsafe { libc::inotify_rm_watch(self.inotify, *wd) };
            }
        }
        *watches = next;
        self.watched = (paths, whole);
    }

    /// Wait for a change to what is watched; `false` once nothing can
    /// change any more.
    pub async fn changed(&mut self) -> bool {
        self.changed.recv().await.is_some()
    }
}

/// Whether any of the `inotify_event`s in `bytes` is a change to what is
/// watched: a watched file, one of the named entries, a directory's own
/// removal, or events lost to a full queue.
fn concerns(mut bytes: &[u8], watches: &Watches) -> bool {
    const HEADER: usize = std::mem::size_of::<libc::inotify_event>();
    let mut any = false;
    while bytes.len() >= HEADER {
        let event = unsafe {
            bytes
                .as_ptr()
                .cast::<libc::inotify_event>()
                .read_unaligned()
        };
        let end = (HEADER + event.len as usize).min(bytes.len());
        let name = &bytes[HEADER..end];
        let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
        bytes = &bytes[end..];
        any |= event.mask & libc::IN_Q_OVERFLOW != 0
            || match watches.get(&event.wd) {
                None => false,
                Some(None) => event.mask & libc::IN_IGNORED == 0,
                Some(Some(names)) => {
                    event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) != 0
                        || names.contains(std::ffi::OsStr::from_bytes(name))
                }
            };
    }
    any
}

impl Drop for FileEvents {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        let one = 1u64;
        unsafe { libc::write(self.wake, (&one as *const u64).cast(), 8) };
    }
}

#[cfg(test)]
mod tests {
    use super::FileEvents;
    use std::{
        future::Future,
        path::Path,
        pin::pin,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
        time::{Duration, Instant},
    };

    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    /// Whether a change is reported within `limit`.
    fn heard(events: &mut FileEvents, limit: Duration) -> bool {
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut changed = pin!(events.changed());
        let until = Instant::now() + limit;
        loop {
            if let Poll::Ready(more) = changed.as_mut().poll(&mut cx) {
                return more;
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            std::thread::park_timeout(left);
        }
    }

    /// Let a change's last events arrive (one write can be read as two).
    fn settle(events: &mut FileEvents) {
        while heard(events, Duration::from_millis(100)) {}
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("zgui-file-events-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const SOON: Duration = Duration::from_secs(5);
    const QUIET: Duration = Duration::from_millis(200);

    #[test]
    fn writes_and_replacements_of_watched_files_are_heard_and_others_are_not() {
        let dir = scratch("files");
        let app = dir.join("app.luau");
        std::fs::write(&app, "return 1").unwrap();
        let mut events = FileEvents::new().unwrap();
        events.watch(vec![app.clone()], Vec::new(), false);

        // An editor's swap file beside it wakes nothing.
        std::fs::write(dir.join(".app.luau.swp"), "swap").unwrap();
        assert!(!heard(&mut events, QUIET));

        std::fs::write(&app, "return 2").unwrap();
        assert!(heard(&mut events, SOON));
        settle(&mut events);

        // Saved as write-and-rename: a new file under the old name.
        let temp = dir.join("app.luau.tmp");
        std::fs::write(&temp, "return 3").unwrap();
        assert!(!heard(&mut events, QUIET));
        std::fs::rename(&temp, &app).unwrap();
        assert!(heard(&mut events, SOON));
        settle(&mut events);
        events.watch(vec![app.clone()], Vec::new(), true);
        // The replacement's own writes are heard.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&app)
            .and_then(|mut f| std::io::Write::write_all(&mut f, b"\n"))
            .unwrap();
        assert!(heard(&mut events, SOON));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_watched_directory_hears_its_first_entry() {
        let dir = scratch("dirs");
        let plugins = dir.join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        let mut events = FileEvents::new().unwrap();
        events.watch(Vec::new(), vec![plugins.clone()], false);
        assert!(!heard(&mut events, QUIET));
        std::fs::write(plugins.join("first.luau"), "").unwrap();
        assert!(heard(&mut events, SOON));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_heard_once_unwatched() {
        let dir = scratch("unwatched");
        let (a, b) = (dir.join("a.luau"), dir.join("b").join("b.luau"));
        std::fs::create_dir(b.parent().unwrap()).unwrap();
        std::fs::write(&a, "").unwrap();
        std::fs::write(&b, "").unwrap();
        let mut events = FileEvents::new().unwrap();
        events.watch(vec![a.clone(), b.clone()], Vec::new(), false);
        events.watch(vec![a.clone()], Vec::new(), false);
        std::fs::write(&b, "changed").unwrap();
        assert!(!heard(&mut events, QUIET));
        std::fs::write(Path::new(&a), "changed").unwrap();
        assert!(heard(&mut events, SOON));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
