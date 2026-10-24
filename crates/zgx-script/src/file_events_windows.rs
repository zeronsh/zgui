//! Changes to a set of files, from the system (directory change
//! notifications): a watching host waits for one with nothing running
//! meanwhile, where polling stats every file a few times a second. The
//! directories holding the files are watched, so a file an editor replaces
//! (a save as write-and-rename) is seen as well as one written in place; a
//! change to another file there only has the host look again.
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        Storage::FileSystem::{
            FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
            FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
        },
        System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects},
    },
    core::HSTRING,
};

/// What a wait takes besides its own event (WaitForMultipleObjects' limit).
const MOST: usize = 63;

/// Both sides': the directories to watch, whether to stop, and the event
/// that wakes the thread for either (closed when both sides have gone).
struct Shared {
    dirs: Mutex<Option<Vec<PathBuf>>>,
    done: AtomicBool,
    /// A HANDLE is not Send: kept as its value.
    wake: usize,
}

impl Shared {
    fn wake(&self) {
        // SAFETY: the event, open while either side holds this.
        let _ = unsafe { SetEvent(HANDLE(self.wake as *mut _)) };
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // SAFETY: the event, closed once, after both sides are done with it.
        let _ = unsafe { CloseHandle(HANDLE(self.wake as *mut _)) };
    }
}

pub(crate) struct FileEvents {
    shared: Arc<Shared>,
    watched: Vec<PathBuf>,
    changed: zgx::task::LatestReceiver<()>,
}

/// One directory's notification handle, closed when dropped.
struct Watch(HANDLE);

impl Drop for Watch {
    fn drop(&mut self) {
        // SAFETY: our notification handle, closed once.
        let _ = unsafe { FindCloseChangeNotification(self.0) };
    }
}

impl FileEvents {
    /// The thread that waits on the directories; `None` if it cannot start.
    pub fn new() -> Option<Self> {
        // SAFETY: an auto-reset event of ours, closed with `Shared`.
        let wake = unsafe { CreateEventW(None, false, false, None) }.ok()?;
        let shared = Arc::new(Shared {
            dirs: Mutex::new(None),
            done: AtomicBool::new(false),
            wake: wake.0 as usize,
        });
        let (tell, changed) = zgx::task::latest::<()>();
        let thread = shared.clone();
        std::thread::Builder::new()
            .name("zgx-script file events".into())
            .spawn(move || {
                let wake = HANDLE(thread.wake as *mut _);
                let mut watches: Vec<Watch> = Vec::new();
                loop {
                    if let Some(dirs) = thread.dirs.lock().unwrap().take() {
                        watches = dirs
                            .iter()
                            .take(MOST)
                            .filter_map(|dir| {
                                // SAFETY: a handle of ours, closed by `Watch`.
                                unsafe {
                                    FindFirstChangeNotificationW(
                                        &HSTRING::from(dir.as_os_str()),
                                        false,
                                        FILE_NOTIFY_CHANGE_FILE_NAME
                                            | FILE_NOTIFY_CHANGE_LAST_WRITE
                                            | FILE_NOTIFY_CHANGE_SIZE,
                                    )
                                }
                                .ok()
                                .map(Watch)
                            })
                            .collect();
                    }
                    let handles: Vec<HANDLE> = std::iter::once(wake)
                        .chain(watches.iter().map(|watch| watch.0))
                        .collect();
                    // SAFETY: waits on handles of ours, alive across the wait.
                    let woke = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
                    if thread.done.load(Ordering::Acquire) {
                        break;
                    }
                    let index = woke.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
                    if index == 0 {
                        continue;
                    }
                    let Some(watch) = watches.get(index - 1) else {
                        break;
                    };
                    tell.send(());
                    // SAFETY: our handle, listening again.
                    let _ = unsafe { FindNextChangeNotification(watch.0) };
                }
            })
            .ok()?;
        Some(Self {
            shared,
            watched: Vec::new(),
            changed,
        })
    }

    /// Watch `paths` (through their directories) and the entries of `whole`
    /// from now on, instead of what was watched before. A directory notices
    /// a replaced file, so `replaced` changes nothing here.
    pub fn watch(&mut self, paths: Vec<PathBuf>, whole: Vec<PathBuf>, _replaced: bool) {
        let mut dirs: Vec<PathBuf> = paths
            .iter()
            .filter_map(|path| path.parent().map(PathBuf::from))
            .chain(whole)
            .collect();
        dirs.sort();
        dirs.dedup();
        if dirs == self.watched {
            return;
        }
        *self.shared.dirs.lock().unwrap() = Some(dirs.clone());
        self.watched = dirs;
        self.shared.wake();
    }

    /// Wait for a change to what is watched; `false` once nothing can
    /// change any more.
    pub async fn changed(&mut self) -> bool {
        self.changed.recv().await.is_some()
    }
}

impl Drop for FileEvents {
    fn drop(&mut self) {
        self.shared.done.store(true, Ordering::Release);
        self.shared.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::FileEvents;
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };

    /// Whether a change is heard within `within`.
    fn heard(events: &mut FileEvents, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        let mut changed = pin!(events.changed());
        let mut cx = Context::from_waker(Waker::noop());
        while Instant::now() < deadline {
            if let Poll::Ready(more) = changed.as_mut().poll(&mut cx) {
                return more;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn a_watched_file_written_or_replaced_and_a_new_file_in_a_watched_folder_are_heard() {
        let dir = std::env::temp_dir().join(format!("zgx-file-events-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.luau");
        std::fs::write(&file, "return 1").unwrap();
        let mut events = FileEvents::new().expect("change notifications");
        let plugins = dir.join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        events.watch(vec![file.clone()], vec![plugins.clone()], false);
        // Settle: nothing changes, nothing is heard.
        assert!(!heard(&mut events, Duration::from_millis(200)));
        std::fs::write(&file, "return 2").unwrap();
        assert!(
            heard(&mut events, Duration::from_secs(5)),
            "a write in place"
        );
        // As an editor saves: a new file renamed over the old.
        let temp = dir.join("app.luau.tmp");
        std::fs::write(&temp, "return 3").unwrap();
        std::fs::rename(&temp, &file).unwrap();
        assert!(heard(&mut events, Duration::from_secs(5)), "a replacement");
        // A folder watched whole: a file added to it.
        std::fs::write(plugins.join("new.luau"), "return {}").unwrap();
        assert!(heard(&mut events, Duration::from_secs(5)), "a new plugin");
        drop(events);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
