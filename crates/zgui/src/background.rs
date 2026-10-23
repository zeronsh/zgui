//! Bounded background work with wake-driven results on the UI thread.
//!
//! Keep a pool in an application service. Only owned `Send` values cross to its
//! workers; await a result in a local UI task before writing signals. Dropping a
//! task releases queued captures immediately and discards a running result. Running closures cannot
//! be forcibly interrupted. Dropping the last pool cancels queued work without
//! blocking the UI, and workers exit after their current closure finishes.
//!
//! ```no_run
//! use zgui::{background::BackgroundPool, task::LocalExecutor};
//! let pool = BackgroundPool::new(2, 16).unwrap();
//! let job = pool.spawn(|| std::fs::read("document.txt")).unwrap();
//! let mut ui_tasks = LocalExecutor::new();
//! ui_tasks.spawn(async move {
//!     match job.await {
//!         Ok(Ok(bytes)) => println!("Loaded {} bytes", bytes.len()),
//!         Ok(Err(error)) => eprintln!("Read failed: {error}"),
//!         Err(error) => eprintln!("Task failed: {error}"),
//!     }
//! });
//! // A native host ticks its executor on wake events and retains the pool.
//! ```
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Waker},
};

type Job = Box<dyn FnOnce() + Send>;
struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}
struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
    capacity: usize,
}
struct Owner(Arc<Shared>);
impl Drop for Owner {
    fn drop(&mut self) {
        let jobs = {
            let mut queue = self.0.queue.lock().unwrap();
            queue.closed = true;
            std::mem::take(&mut queue.jobs)
        };
        self.0.ready.notify_all();
        // Completion wakes user executors; never invoke them under the queue lock.
        drop(jobs);
    }
}

/// An explicitly sized worker pool. No threads or queues exist until constructed.
#[derive(Clone)]
pub struct BackgroundPool {
    owner: Arc<Owner>,
}
/// Why [`BackgroundPool::spawn`] refused work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// `capacity` closures are already queued.
    QueueFull,
    /// The pool has shut down.
    Closed,
}
/// Why a background task gave no result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskError {
    /// The task was cancelled, or its pool dropped it before it ran.
    Cancelled,
    /// The closure panicked, with this message (`non-string panic` when the payload
    /// was not a string).
    Panicked(String),
}
impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::QueueFull => "background queue is full",
            Self::Closed => "background pool is closed",
        })
    }
}
impl std::error::Error for SpawnError {}
impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("background task cancelled"),
            Self::Panicked(message) => write!(f, "background task panicked: {message}"),
        }
    }
}
impl std::error::Error for TaskError {}

struct ResultState<T> {
    result: Option<Result<T, TaskError>>,
    work: Option<Box<dyn FnOnce() -> T + Send>>,
    cancelled: bool,
    finished: bool,
    waker: Option<Waker>,
}
struct Completion<T>(Arc<Mutex<ResultState<T>>>);
impl<T> Completion<T> {
    fn finish(&self, result: Result<T, TaskError>) {
        let (wake, work) = {
            let mut state = self.0.lock().unwrap();
            if state.cancelled || state.finished {
                return;
            }
            state.finished = true;
            state.result = Some(result);
            (state.waker.take(), state.work.take())
        };
        drop(work);
        if let Some(waker) = wake {
            waker.wake();
        }
    }
}
impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        self.finish(Err(TaskError::Cancelled));
    }
}

/// A single-consumer result future. Dropping it cancels interest in the work.
#[must_use = "Dropping this future cancels the background task"]
pub struct BackgroundTask<T> {
    state: Arc<Mutex<ResultState<T>>>,
    consumed: bool,
}
impl<T> BackgroundTask<T> {
    /// Cancel the task: queued work is dropped without running, a running closure's
    /// result is discarded, and awaiting gives `Err(TaskError::Cancelled)`.
    pub fn cancel(&self) {
        let (wake, work, result) = {
            let mut state = self.state.lock().unwrap();
            if state.cancelled {
                return;
            }
            state.cancelled = true;
            state.finished = true;
            let result = state.result.replace(Err(TaskError::Cancelled));
            (state.waker.take(), state.work.take(), result)
        };
        drop(work);
        drop(result);
        if let Some(waker) = wake {
            waker.wake();
        }
    }
    /// Whether its result, or its cancellation, is in, or it has been awaited.
    pub fn is_finished(&self) -> bool {
        self.consumed || self.state.lock().unwrap().finished
    }
}
impl<T> Future for BackgroundTask<T> {
    type Output = Result<T, TaskError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        assert!(!self.consumed, "background result polled after completion");
        // Raw waker clone/drop callbacks can reenter the consumer. Do not
        // execute them while protecting its shared result state.
        let next = cx.waker().clone();
        let (result, removed) = {
            let mut state = self.state.lock().unwrap();
            if let Some(result) = state.result.take() {
                (Some(result), Some(next))
            } else {
                let removed = if state.waker.as_ref().is_none_or(|w| !w.will_wake(&next)) {
                    state.waker.replace(next)
                } else {
                    Some(next)
                };
                (None, removed)
            }
        };
        drop(removed);
        if let Some(result) = result {
            self.consumed = true;
            Poll::Ready(result)
        } else {
            Poll::Pending
        }
    }
}
impl<T> Drop for BackgroundTask<T> {
    fn drop(&mut self) {
        let (work, result, waker) = {
            let mut state = self.state.lock().unwrap();
            state.cancelled = true;
            (state.work.take(), state.result.take(), state.waker.take())
        };
        drop(work);
        drop(result);
        drop(waker);
    }
}
impl BackgroundPool {
    /// `capacity` bounds queued closures, in addition to at most `workers` running.
    /// Both values must be nonzero. Thread creation errors are returned to callers.
    pub fn new(workers: usize, capacity: usize) -> std::io::Result<Self> {
        if workers == 0 || capacity == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "workers and capacity must be nonzero",
            ));
        }
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                jobs: VecDeque::new(),
                closed: false,
            }),
            ready: Condvar::new(),
            capacity,
        });
        let pool = Self {
            owner: Arc::new(Owner(shared.clone())),
        };
        for index in 0..workers {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name(format!("zgui-worker-{index}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let mut queue = shared.queue.lock().unwrap();
                            loop {
                                if queue.closed {
                                    return;
                                }
                                if let Some(job) = queue.jobs.pop_front() {
                                    break job;
                                }
                                queue = shared.ready.wait(queue).unwrap();
                            }
                        };
                        job();
                    }
                })?;
        }
        Ok(pool)
    }
    /// How many closures are waiting for a worker; running ones are not counted.
    pub fn queued(&self) -> usize {
        self.owner.0.queue.lock().unwrap().jobs.len()
    }
    /// Queue `work` for a worker thread; await the returned task for its result.
    /// Fails with `QueueFull` when `capacity` closures are already waiting.
    pub fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<BackgroundTask<T>, SpawnError> {
        let shared = &self.owner.0;
        let mut queue = shared.queue.lock().unwrap();
        if queue.closed {
            return Err(SpawnError::Closed);
        }
        if queue.jobs.len() >= shared.capacity {
            return Err(SpawnError::QueueFull);
        }
        let state = Arc::new(Mutex::new(ResultState {
            result: None,
            work: Some(Box::new(work)),
            cancelled: false,
            finished: false,
            waker: None,
        }));
        let complete = Completion(state.clone());
        queue.jobs.push_back(Box::new(move || {
            let work = complete.0.lock().unwrap().work.take();
            let Some(work) = work else {
                return;
            };
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).map_err(|panic| {
                    let message = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "non-string panic".into());
                    TaskError::Panicked(message)
                });
            complete.finish(result);
        }));
        drop(queue);
        shared.ready.notify_one();
        Ok(BackgroundTask {
            state,
            consumed: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, task::Wake, time::Duration};
    struct Notify(mpsc::Sender<()>);
    impl Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    fn result<T>(mut task: BackgroundTask<T>) -> Result<T, TaskError> {
        let (send, receive) = mpsc::channel();
        let waker = Waker::from(Arc::new(Notify(send)));
        let mut context = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(result) = Pin::new(&mut task).poll(&mut context) {
                return result;
            }
            receive.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }
    #[test]
    fn bounded_queue_cancellation_and_shutdown_do_not_block() {
        let pool = BackgroundPool::new(1, 1).unwrap();
        let (started, running) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let first = pool
            .spawn(move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
                7
            })
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let queued = pool
            .spawn(|| panic!("cancelled work must not run"))
            .unwrap();
        assert!(matches!(pool.spawn(|| 0), Err(SpawnError::QueueFull)));
        drop(pool);
        assert_eq!(result(queued), Err(TaskError::Cancelled));
        release.send(()).unwrap();
        assert_eq!(result(first), Ok(7));
    }
    #[test]
    fn worker_panic_becomes_result_and_worker_survives() {
        let pool = BackgroundPool::new(1, 2).unwrap();
        assert_eq!(
            result(pool.spawn(|| panic!("test panic")).unwrap()),
            Err(TaskError::Panicked("test panic".into()))
        );
        assert_eq!(result(pool.spawn(|| "success").unwrap()), Ok("success"));
    }
    #[test]
    fn explicit_cancel_wakes_pending_consumer_and_skips_queued_closure() {
        let pool = BackgroundPool::new(1, 2).unwrap();
        let (started, running) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let first = pool
            .spawn(move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
            })
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let captured = Arc::new(());
        let weak = Arc::downgrade(&captured);
        let queued = pool
            .spawn(move || {
                let _capture = captured;
                panic!("cancelled queued job ran")
            })
            .unwrap();
        queued.cancel();
        assert!(
            weak.upgrade().is_none(),
            "queued capture must be freed while the worker is still busy"
        );
        assert_eq!(result(queued), Err(TaskError::Cancelled));
        release.send(()).unwrap();
        assert_eq!(result(first), Ok(()));
        assert_eq!(result(pool.spawn(|| 42).unwrap()), Ok(42));
    }
}

#[cfg(test)]
mod waker_cleanup_tests {
    use super::*;
    use std::{
        sync::{
            Weak,
            atomic::{AtomicBool, Ordering},
        },
        task::Wake,
    };
    struct CheckDrop {
        state: Weak<Mutex<ResultState<()>>>,
        locked: Arc<AtomicBool>,
        dropped: Arc<AtomicBool>,
    }
    #[allow(
        clippy::manual_noop_waker,
        reason = "custom Drop is the behavior under test"
    )]
    impl Wake for CheckDrop {
        fn wake(self: Arc<Self>) {}
    }
    impl Drop for CheckDrop {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
            if let Some(state) = self.state.upgrade() {
                self.locked
                    .store(state.try_lock().is_err(), Ordering::SeqCst);
            }
        }
    }
    #[test]
    fn replacing_consumer_waker_drops_it_outside_result_lock() {
        let state = Arc::new(Mutex::new(ResultState {
            result: None,
            work: None,
            cancelled: false,
            finished: false,
            waker: None,
        }));
        let locked = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        state.lock().unwrap().waker = Some(Waker::from(Arc::new(CheckDrop {
            state: Arc::downgrade(&state),
            locked: locked.clone(),
            dropped: dropped.clone(),
        })));
        let mut task = BackgroundTask {
            state,
            consumed: false,
        };
        assert!(
            Pin::new(&mut task)
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(dropped.load(Ordering::SeqCst), "user waker was not dropped");
        assert!(
            !locked.load(Ordering::SeqCst),
            "user waker destructor ran under result lock"
        );
    }
}
