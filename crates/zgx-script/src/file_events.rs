//! Changes to a set of files, from the kernel (kqueue): a watching host
//! waits for one with nothing running meanwhile, where polling stats every
//! file a few times a second. Each file and the directory holding it are
//! watched, so a file an editor replaces (a save as write-and-rename) is
//! seen as well as one written in place.
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct FileEvents {
    /// The queue, shared with its thread: it closes when both have let go,
    /// so neither uses its number after the other closed it (when the
    /// system may have given the number to another file).
    queue: Arc<OwnedFd>,
    /// Set when this goes: its thread lets go of the queue and ends.
    done: Arc<AtomicBool>,
    /// What is watched now, and the descriptors that watch it (closing
    /// one drops what watched it).
    watched: Vec<PathBuf>,
    fds: Vec<OwnedFd>,
    changed: zgx::task::LatestReceiver<()>,
}

impl FileEvents {
    /// A queue and the thread that waits on it; `None` if the kernel gives
    /// no queue.
    pub fn new() -> Option<Self> {
        // SAFETY: `kqueue` takes no arguments; it returns a new descriptor,
        // or -1.
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return None;
        }
        // SAFETY: `queue` was just opened, and nothing else owns it.
        let queue = Arc::new(unsafe { OwnedFd::from_raw_fd(queue) });
        // Its own event, to wake the thread when this goes.
        let wake = libc::kevent {
            ident: 0,
            filter: libc::EVFILT_USER,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // SAFETY: `queue` is open; one change is read from `wake`, and no
        // event is written.
        unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                &wake,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        let (tell, changed) = zgx::task::latest::<()>();
        let done = Arc::new(AtomicBool::new(false));
        let ended = done.clone();
        let waiting = queue.clone();
        std::thread::Builder::new()
            .name("zgx-script file events".into())
            .spawn(move || {
                // SAFETY: `kevent` is plain data, for which zeroes are valid
                // (a null `udata`).
                let mut events: [libc::kevent; 8] = unsafe { std::mem::zeroed() };
                loop {
                    // SAFETY: `waiting` is open while this thread holds it,
                    // and `events` has room for the `events.len()` events
                    // the kernel may write.
                    let n = unsafe {
                        libc::kevent(
                            waiting.as_raw_fd(),
                            std::ptr::null(),
                            0,
                            events.as_mut_ptr(),
                            events.len() as libc::c_int,
                            std::ptr::null(),
                        )
                    };
                    if ended.load(Ordering::Acquire)
                        || (n < 0
                            && std::io::Error::last_os_error().kind()
                                != std::io::ErrorKind::Interrupted)
                    {
                        // Dropping `waiting` closes the queue if this was
                        // the last holder.
                        return;
                    }
                    if n > 0 {
                        tell.send(());
                    }
                }
            })
            .ok()?;
        Some(Self {
            queue,
            done,
            watched: Vec::new(),
            fds: Vec::new(),
            changed,
        })
    }

    /// Watch `paths` (and their directories) and the entries of `whole`
    /// from now on, instead of what was watched before. Watching what is
    /// already watched opens nothing again, unless a file was replaced (its
    /// descriptor still names the old one).
    pub fn watch(&mut self, mut paths: Vec<PathBuf>, whole: Vec<PathBuf>, replaced: bool) {
        let mut dirs: Vec<PathBuf> = paths
            .iter()
            .filter_map(|path| path.parent().map(PathBuf::from))
            .chain(whole)
            .collect();
        dirs.sort();
        dirs.dedup();
        paths.sort();
        paths.dedup();
        let wanted: Vec<PathBuf> = paths.iter().chain(&dirs).cloned().collect();
        if wanted == self.watched && !replaced {
            return;
        }
        // Closing a descriptor drops what watched it.
        self.fds.clear();
        let file = libc::NOTE_WRITE
            | libc::NOTE_EXTEND
            | libc::NOTE_ATTRIB
            | libc::NOTE_DELETE
            | libc::NOTE_RENAME
            | libc::NOTE_REVOKE;
        for (path, notes) in paths
            .iter()
            .map(|p| (p, file))
            .chain(dirs.iter().map(|d| (d, libc::NOTE_WRITE)))
        {
            let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
                continue;
            };
            // SAFETY: `c_path` is a NUL-terminated path, read for the call.
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_EVTONLY | libc::O_CLOEXEC) };
            if fd < 0 {
                continue;
            }
            // SAFETY: `fd` was just opened, and nothing else owns it.
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            let change = libc::kevent {
                ident: fd.as_raw_fd() as usize,
                filter: libc::EVFILT_VNODE,
                flags: libc::EV_ADD | libc::EV_CLEAR,
                fflags: notes,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: the queue is open while this holds it; one change is
            // read from `change`, and no event is written.
            unsafe {
                libc::kevent(
                    self.queue.as_raw_fd(),
                    &change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            };
            self.fds.push(fd);
        }
        self.watched = wanted;
    }

    /// Wait for a change to what is watched; `false` once nothing can
    /// change any more.
    pub async fn changed(&mut self) -> bool {
        self.changed.recv().await.is_some()
    }
}

impl Drop for FileEvents {
    fn drop(&mut self) {
        self.fds.clear();
        self.done.store(true, Ordering::Release);
        let wake = libc::kevent {
            ident: 0,
            filter: libc::EVFILT_USER,
            flags: 0,
            fflags: libc::NOTE_TRIGGER,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // SAFETY: the queue is still open, as this still holds it (its
        // thread may have let go already); one change is read from `wake`.
        unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                &wake,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::FileEvents;
    use std::{
        future::Future,
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

    /// The queue is shared with the thread waiting on it, so watching again
    /// and dropping use a descriptor that is still the queue's.
    #[test]
    fn writes_are_heard_and_watches_come_and_go() {
        let dir = std::env::temp_dir().join(format!("zgx-kqueue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("app.luau");
        std::fs::write(&app, "return 1").unwrap();
        let mut events = FileEvents::new().unwrap();
        events.watch(vec![app.clone()], Vec::new(), false);
        std::fs::write(&app, "return 2").unwrap();
        assert!(heard(&mut events, Duration::from_secs(5)));
        events.watch(vec![app.clone()], Vec::new(), true);
        while heard(&mut events, Duration::from_millis(100)) {}
        std::fs::write(&app, "return 3").unwrap();
        assert!(heard(&mut events, Duration::from_secs(5)));
        drop(events);
        // Each one's thread wakes and ends as it goes.
        for _ in 0..16 {
            let mut events = FileEvents::new().unwrap();
            events.watch(vec![app.clone()], Vec::new(), false);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
