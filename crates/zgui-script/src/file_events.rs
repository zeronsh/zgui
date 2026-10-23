//! Changes to a set of files, from the kernel (kqueue): a watching host
//! waits for one with nothing running meanwhile, where polling stats every
//! file a few times a second. Each file and the directory holding it are
//! watched, so a file an editor replaces (a save as write-and-rename) is
//! seen as well as one written in place.
use std::{
    os::fd::RawFd,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct FileEvents {
    queue: RawFd,
    /// Set when this goes: its thread closes the queue and ends.
    done: Arc<AtomicBool>,
    /// What is watched now, and the descriptors that watch it.
    watched: Vec<PathBuf>,
    fds: Vec<RawFd>,
    changed: zgui::task::LatestReceiver<()>,
}

impl FileEvents {
    /// A queue and the thread that waits on it; `None` if the kernel gives
    /// no queue.
    pub fn new() -> Option<Self> {
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return None;
        }
        // Its own event, to wake the thread when this goes.
        let wake = libc::kevent {
            ident: 0,
            filter: libc::EVFILT_USER,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        unsafe { libc::kevent(queue, &wake, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        let (tell, changed) = zgui::task::latest::<()>();
        let done = Arc::new(AtomicBool::new(false));
        let ended = done.clone();
        std::thread::Builder::new()
            .name("zgui-script file events".into())
            .spawn(move || {
                let mut events: [libc::kevent; 8] = unsafe { std::mem::zeroed() };
                loop {
                    let n = unsafe {
                        libc::kevent(
                            queue,
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
                        unsafe { libc::close(queue) };
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
        for fd in self.fds.drain(..) {
            // Closing a descriptor drops what watched it.
            unsafe { libc::close(fd) };
        }
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
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_EVTONLY | libc::O_CLOEXEC) };
            if fd < 0 {
                continue;
            }
            let change = libc::kevent {
                ident: fd as usize,
                filter: libc::EVFILT_VNODE,
                flags: libc::EV_ADD | libc::EV_CLEAR,
                fflags: notes,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            unsafe {
                libc::kevent(
                    self.queue,
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
        for fd in self.fds.drain(..) {
            unsafe { libc::close(fd) };
        }
        self.done.store(true, Ordering::Release);
        let wake = libc::kevent {
            ident: 0,
            filter: libc::EVFILT_USER,
            flags: 0,
            fflags: libc::NOTE_TRIGGER,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        unsafe {
            libc::kevent(
                self.queue,
                &wake,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
    }
}
