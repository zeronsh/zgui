//! Local futures are polled only when woken. Wakers may be used across threads.
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    pin::Pin,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

#[derive(Default)]
struct Ready {
    queue: VecDeque<u64>,
    queued: HashSet<u64>,
}
struct Scheduler {
    ready: Mutex<Ready>,
    wake_event_loop: Box<dyn Fn() + Send + Sync>,
}
struct TaskWake {
    id: u64,
    active: AtomicBool,
    cancelled: AtomicBool,
    scheduler: Arc<Scheduler>,
}
impl Wake for TaskWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let notify = {
            let mut ready = self.scheduler.ready.lock().unwrap();
            if !self.active.load(Ordering::Acquire) {
                return;
            }
            if !ready.queued.insert(self.id) {
                return;
            }
            let notify = ready.queue.is_empty();
            ready.queue.push_back(self.id);
            notify
        };
        if notify {
            (self.scheduler.wake_event_loop)();
        }
    }
}
struct Task {
    future: Pin<Box<dyn Future<Output = ()>>>,
    wake: Arc<TaskWake>,
}
impl Drop for Task {
    fn drop(&mut self) {
        let mut ready = self.wake.scheduler.ready.lock().unwrap();
        self.wake.active.store(false, Ordering::Release);
        if ready.queued.remove(&self.wake.id) {
            ready.queue.retain(|id| *id != self.wake.id);
        }
    }
}

/// Ownership token for a scoped task. Dropping it requests cancellation and wakes the UI.
#[must_use = "Dropping the handle cancels the task"]
pub struct TaskHandle {
    wake: Arc<TaskWake>,
}
impl TaskHandle {
    /// Cancel the task: the executor drops its future at its next tick.
    pub fn cancel(&self) {
        self.wake.cancelled.store(true, Ordering::Release);
        self.wake.wake_by_ref();
    }
    /// Whether its future is gone: finished, cancelled or dropped with the
    /// executor.
    pub fn is_finished(&self) -> bool {
        !self.wake.active.load(Ordering::Acquire)
    }
}
impl Drop for TaskHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Single-threaded future storage, with a thread-safe wake queue.
/// Call `tick` from the UI event loop after its wake event; no idle polling needed.
pub struct LocalExecutor {
    tasks: HashMap<u64, Task>,
    next: Rc<Cell<u64>>,
    pending: Rc<RefCell<Vec<Task>>>,
    scheduler: Arc<Scheduler>,
}
impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}
impl LocalExecutor {
    /// An executor that wakes no event loop: its host ticks it.
    pub fn new() -> Self {
        Self::with_wake(|| {})
    }
    /// The callback should post an event, rather than synchronously run `tick`.
    pub fn with_wake(wake_event_loop: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            tasks: HashMap::new(),
            next: Rc::new(Cell::new(0)),
            pending: Rc::new(RefCell::new(Vec::new())),
            scheduler: Arc::new(Scheduler {
                ready: Mutex::new(Ready::default()),
                wake_event_loop: Box::new(wake_event_loop),
            }),
        }
    }
    /// Run `future` until it finishes; it is first polled at the next tick. Only
    /// dropping the executor cancels it (see `spawn_scoped`).
    pub fn spawn(&mut self, future: impl Future<Output = ()> + 'static) {
        self.spawn_inner(future);
    }
    /// Cancels on handle drop. Cancellation drops the future at the next ready tick.
    pub fn spawn_scoped(&mut self, future: impl Future<Output = ()> + 'static) -> TaskHandle {
        TaskHandle {
            wake: self.spawn_inner(future),
        }
    }
    fn spawn_inner(&mut self, future: impl Future<Output = ()> + 'static) -> Arc<TaskWake> {
        let id = self.next.get();
        self.next
            .set(id.checked_add(1).expect("task identifiers exhausted"));
        let wake = Arc::new(TaskWake {
            id,
            active: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
            scheduler: self.scheduler.clone(),
        });
        self.tasks.insert(
            id,
            Task {
                future: Box::pin(future),
                wake: wake.clone(),
            },
        );
        wake.wake_by_ref();
        wake
    }
    /// Polls each task ready at the start of this tick at most once. A yielded
    /// task waits until the next tick, allowing input and rendering between ticks.
    /// Returns the number of futures polled.
    pub fn tick(&mut self) -> usize {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for task in pending {
            self.tasks.insert(task.wake.id, task);
        }
        let ready: Vec<_> = {
            let mut ready = self.scheduler.ready.lock().unwrap();
            let tasks = ready.queue.drain(..).collect();
            ready.queued.clear();
            tasks
        };
        let mut polled = 0;
        let mut ready = ready.into_iter();
        while let Some(id) = ready.next() {
            if self
                .tasks
                .get(&id)
                .is_some_and(|task| task.wake.cancelled.load(Ordering::Acquire))
            {
                self.tasks.remove(&id);
                continue;
            }
            if let Some(task) = self.tasks.get_mut(&id) {
                polled += 1;
                let waker = Waker::from(task.wake.clone());
                let mut cx = Context::from_waker(&waker);
                match catch_unwind(AssertUnwindSafe(|| task.future.as_mut().poll(&mut cx))) {
                    Ok(Poll::Ready(())) => {
                        self.tasks.remove(&id);
                    }
                    Ok(Poll::Pending) => {}
                    // The panic goes on to whoever ticks. A future that
                    // panicked can't be polled again, and the tasks after it
                    // stay ready for the next tick.
                    Err(panic) => {
                        self.tasks.remove(&id);
                        let mut queue = self.scheduler.ready.lock().unwrap();
                        for id in ready {
                            if queue.queued.insert(id) {
                                queue.queue.push_back(id);
                            }
                        }
                        drop(queue);
                        (self.scheduler.wake_event_loop)();
                        resume_unwind(panic);
                    }
                }
            }
        }
        polled
    }
    /// Whether a task is waiting to be polled.
    pub fn has_ready(&self) -> bool {
        !self.scheduler.ready.lock().unwrap().queue.is_empty()
    }
    /// Whether it holds no tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.pending.borrow().is_empty()
    }
    /// Obtain a weak spawning capability that can be used inside a running task.
    /// New tasks join the next tick without borrowing the executor itself.
    pub fn spawner(&self) -> LocalSpawner {
        LocalSpawner {
            pending: Rc::downgrade(&self.pending),
            next: self.next.clone(),
            scheduler: self.scheduler.clone(),
        }
    }
}

/// A local, weak executor capability. It never prolongs executor or future lifetime.
#[derive(Clone)]
pub struct LocalSpawner {
    pending: Weak<RefCell<Vec<Task>>>,
    next: Rc<Cell<u64>>,
    scheduler: Arc<Scheduler>,
}
impl LocalSpawner {
    /// Returns `None` and drops the future if the executor has been disposed.
    /// Retain the returned guard; dropping it cancels the queued/running future.
    pub fn spawn_scoped(&self, future: impl Future<Output = ()> + 'static) -> Option<TaskHandle> {
        let pending = self.pending.upgrade()?;
        let id = self.next.get();
        self.next
            .set(id.checked_add(1).expect("task identifiers exhausted"));
        let wake = Arc::new(TaskWake {
            id,
            active: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
            scheduler: self.scheduler.clone(),
        });
        pending.borrow_mut().push(Task {
            future: Box::pin(future),
            wake: wake.clone(),
        });
        wake.wake_by_ref();
        Some(TaskHandle { wake })
    }
}

/// Cooperatively return control to the UI loop for one tick.
pub fn yield_now() -> impl Future<Output = ()> {
    struct Yield(bool);
    impl Future for Yield {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    Yield(false)
}

/// A value another thread produces, for a task on the UI thread: the
/// sender keeps only the latest (values sent before the task reads them
/// coalesce) and sending wakes the task, so nothing polls while nothing
/// changes. A background reader of a socket hands the UI what it heard.
pub fn latest<T: Send + 'static>() -> (LatestSender<T>, LatestReceiver<T>) {
    let shared = Arc::new(Mutex::new(Latest {
        value: None,
        waker: None,
        senders: 1,
        receiving: true,
    }));
    (LatestSender(shared.clone()), LatestReceiver(shared))
}
struct Latest<T> {
    value: Option<T>,
    waker: Option<Waker>,
    senders: usize,
    receiving: bool,
}
/// The sending side of [`latest`]: any thread, cloneable.
pub struct LatestSender<T>(Arc<Mutex<Latest<T>>>);
impl<T> LatestSender<T> {
    /// Replace what waits to be read with `value` and wake the reader.
    /// False once the receiver is gone (stop producing).
    pub fn send(&self, value: T) -> bool {
        let waker = {
            let mut latest = self.0.lock().unwrap();
            if !latest.receiving {
                return false;
            }
            latest.value = Some(value);
            latest.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        true
    }
}
impl<T> Clone for LatestSender<T> {
    fn clone(&self) -> Self {
        self.0.lock().unwrap().senders += 1;
        Self(self.0.clone())
    }
}
impl<T> Drop for LatestSender<T> {
    fn drop(&mut self) {
        let waker = {
            let mut latest = self.0.lock().unwrap();
            latest.senders -= 1;
            if latest.senders > 0 {
                return;
            }
            latest.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
/// The receiving side of [`latest`], for one task on the UI thread.
pub struct LatestReceiver<T>(Arc<Mutex<Latest<T>>>);
impl<T> LatestReceiver<T> {
    /// The next value sent (the latest of those sent since the last read),
    /// or `None` once every sender is gone and nothing waits.
    pub fn recv(&mut self) -> impl Future<Output = Option<T>> + '_ {
        std::future::poll_fn(move |cx| {
            let mut latest = self.0.lock().unwrap();
            if let Some(value) = latest.value.take() {
                return Poll::Ready(Some(value));
            }
            if latest.senders == 0 {
                return Poll::Ready(None);
            }
            latest.waker = Some(cx.waker().clone());
            Poll::Pending
        })
    }
}
impl<T> Drop for LatestReceiver<T> {
    fn drop(&mut self) {
        let mut latest = self.0.lock().unwrap();
        latest.receiving = false;
        latest.value = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc, sync::atomic::AtomicUsize};
    #[test]
    fn a_panicking_task_goes_and_the_tasks_after_it_still_run() {
        let mut executor = LocalExecutor::new();
        let polls = Rc::new(Cell::new(0));
        let ran = Rc::new(Cell::new(false));
        let (first, second) = (polls.clone(), ran.clone());
        executor.spawn(async move {
            first.set(first.get() + 1);
            panic!("in a task");
        });
        executor.spawn(async move { second.set(true) });
        let caught = catch_unwind(AssertUnwindSafe(|| executor.tick()));
        assert!(caught.is_err(), "the panic reaches whoever ticks");
        assert!(!ran.get());
        executor.tick();
        assert!(ran.get(), "the next task was still ready");
        assert_eq!(polls.get(), 1, "a panicked future isn't polled again");
        assert!(executor.is_empty());
    }
    #[test]
    fn yield_resumes_next_tick_and_idle_does_not_poll() {
        let mut executor = LocalExecutor::new();
        let stage = Rc::new(Cell::new(0));
        executor.spawn({
            let stage = stage.clone();
            async move {
                stage.set(1);
                yield_now().await;
                stage.set(2);
            }
        });
        assert_eq!(executor.tick(), 1);
        assert_eq!(stage.get(), 1);
        assert!(executor.has_ready());
        assert_eq!(executor.tick(), 1);
        assert_eq!(stage.get(), 2);
        assert!(executor.is_empty());
        assert_eq!(executor.tick(), 0);
        executor.spawn(std::future::pending());
        assert_eq!(executor.tick(), 1);
        assert!(!executor.has_ready());
        assert_eq!(executor.tick(), 0);
        assert!(!executor.is_empty());
    }
    #[test]
    fn cross_thread_wakes_coalesce_and_finished_tasks_ignore_wakes() {
        let events = Arc::new(AtomicUsize::new(0));
        let saved = Arc::new(Mutex::new(None::<Waker>));
        let mut executor = LocalExecutor::with_wake({
            let events = events.clone();
            move || {
                events.fetch_add(1, Ordering::Relaxed);
            }
        });
        executor.spawn({
            let saved = saved.clone();
            let mut first = true;
            std::future::poll_fn(move |cx| {
                *saved.lock().unwrap() = Some(cx.waker().clone());
                if first {
                    first = false;
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });
        executor.tick();
        assert!(!executor.has_ready());
        let waker = saved.lock().unwrap().clone().unwrap();
        std::thread::spawn({
            let waker = waker.clone();
            move || {
                for _ in 0..100 {
                    waker.wake_by_ref();
                }
            }
        })
        .join()
        .unwrap();
        assert_eq!(events.load(Ordering::Relaxed), 2);
        assert_eq!(executor.tick(), 1);
        assert!(executor.is_empty());
        waker.wake_by_ref();
        assert!(!executor.has_ready());
    }

    #[test]
    fn latest_values_wake_the_reader_and_coalesce() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let mut executor = LocalExecutor::with_wake({
            let wakes = wakes.clone();
            move || {
                wakes.fetch_add(1, Ordering::Relaxed);
            }
        });
        let (sender, mut receiver) = latest::<u32>();
        let seen = Rc::new(RefCell::new(Vec::new()));
        executor.spawn({
            let seen = seen.clone();
            async move {
                while let Some(value) = receiver.recv().await {
                    seen.borrow_mut().push(value);
                }
                seen.borrow_mut().push(0);
            }
        });
        executor.tick();
        assert!(!executor.has_ready(), "nothing sent: the reader waits");
        let before = wakes.load(Ordering::Relaxed);
        // From another thread, three values before the reader runs: one wake,
        // and only the last arrives.
        std::thread::spawn({
            let sender = sender.clone();
            move || {
                for value in 1..=3 {
                    assert!(sender.send(value));
                }
            }
        })
        .join()
        .unwrap();
        assert_eq!(wakes.load(Ordering::Relaxed) - before, 1);
        executor.tick();
        assert_eq!(&*seen.borrow(), &[3]);
        assert!(!executor.has_ready());
        // Every sender gone: the reader ends.
        drop(sender);
        executor.tick();
        assert_eq!(&*seen.borrow(), &[3, 0]);
        assert!(executor.is_empty());
    }
    #[test]
    fn latest_senders_stop_once_the_receiver_is_gone() {
        let (sender, receiver) = latest::<u32>();
        assert!(sender.send(1));
        drop(receiver);
        assert!(!sender.send(2));
    }

    #[test]
    fn waking_then_completing_does_not_leave_stale_work() {
        let mut executor = LocalExecutor::new();
        executor.spawn(std::future::poll_fn(|cx| {
            cx.waker().wake_by_ref();
            Poll::Ready(())
        }));
        assert_eq!(executor.tick(), 1);
        assert!(executor.is_empty());
        assert!(!executor.has_ready());
        assert_eq!(executor.tick(), 0);
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    fn cancellation_drops_sleeping_capture_without_polling_again() {
        let value = std::rc::Rc::new(());
        let weak = std::rc::Rc::downgrade(&value);
        let mut executor = LocalExecutor::new();
        let task = executor.spawn_scoped(async move {
            let _value = value;
            std::future::pending::<()>().await;
        });
        assert_eq!(executor.tick(), 1);
        assert!(!executor.has_ready());
        assert!(weak.upgrade().is_some());
        drop(task);
        assert!(executor.has_ready());
        assert_eq!(executor.tick(), 0);
        assert!(executor.is_empty());
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
mod spawner_tests {
    use super::*;
    #[test]
    fn nested_spawning_is_deferred_without_reborrowing_executor() {
        let executor = Rc::new(RefCell::new(LocalExecutor::new()));
        let spawn = executor.borrow().spawner();
        let handles = Rc::new(RefCell::new(Vec::new()));
        let called = Rc::new(Cell::new(0));
        executor.borrow_mut().spawn({
            let handles = handles.clone();
            let called = called.clone();
            async move {
                called.set(1);
                let child = called.clone();
                handles.borrow_mut().push(
                    spawn
                        .spawn_scoped(async move {
                            child.set(2);
                        })
                        .unwrap(),
                );
            }
        });
        assert_eq!(executor.borrow_mut().tick(), 1);
        assert_eq!(called.get(), 1);
        assert!(executor.borrow().has_ready());
        assert_eq!(executor.borrow_mut().tick(), 1);
        assert_eq!(called.get(), 2);
        assert!(executor.borrow().is_empty());
    }
    #[test]
    fn pending_tasks_are_cancelled_and_weak_spawner_cannot_resurrect_executor() {
        let mut executor = LocalExecutor::new();
        let spawn = executor.spawner();
        let capture = Rc::new(());
        let weak = Rc::downgrade(&capture);
        let task = spawn
            .spawn_scoped(async move {
                let _capture = capture;
                std::future::pending::<()>().await;
            })
            .unwrap();
        assert!(!executor.is_empty());
        drop(task);
        assert_eq!(executor.tick(), 0);
        assert!(weak.upgrade().is_none());
        let pending = spawn.spawn_scoped(std::future::pending()).unwrap();
        drop(executor);
        assert!(pending.is_finished());
        assert!(spawn.spawn_scoped(async {}).is_none());
    }
}
