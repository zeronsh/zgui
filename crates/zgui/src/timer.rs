//! Shared deadline scheduler. Sleeping tasks do not poll or allocate a thread per timer.
//!
//! A scheduler thread fires timers unless an event loop [`host`]s them: then
//! the loop fires them as it wakes for its own deadlines, and a timer costs no
//! thread wake-ups or cross-thread messages.
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
type Wake = Arc<dyn Fn() + Send + Sync>;
struct Driver {
    waiters: Mutex<BTreeMap<(Instant, u64), Waker>>,
    changed: Condvar,
    next: AtomicU64,
    host: Mutex<Host>,
}
#[derive(Default)]
struct Host {
    /// Wakes the event loop hosting the timers: see [`host`].
    wake: Option<Wake>,
    /// The scheduler thread is running.
    thread: bool,
}
fn driver() -> &'static Arc<Driver> {
    static DRIVER: OnceLock<Arc<Driver>> = OnceLock::new();
    DRIVER.get_or_init(|| {
        Arc::new(Driver {
            waiters: Mutex::new(BTreeMap::new()),
            changed: Condvar::new(),
            next: AtomicU64::new(1),
            host: Mutex::default(),
        })
    })
}
impl Driver {
    /// The earliest deadline changed: wake whoever fires timers.
    fn notify(self: &Arc<Self>) {
        let (wake, spawn) = {
            let mut host = self.host.lock().unwrap();
            let spawn = host.wake.is_none() && !host.thread;
            host.thread |= spawn;
            (host.wake.clone(), spawn)
        };
        if let Some(wake) = wake {
            wake();
        } else if spawn {
            let shared = self.clone();
            std::thread::Builder::new()
                .name("zgui-timers".into())
                .stack_size(128 * 1024)
                .spawn(move || shared.run())
                .expect("create timer scheduler");
        } else {
            self.changed.notify_one();
        }
    }
    fn host(self: &Arc<Self>, wake: Wake) -> Hosted {
        self.host.lock().unwrap().wake = Some(wake.clone());
        // The thread checks for a host under the waiters lock before it waits.
        drop(self.waiters.lock().unwrap());
        self.changed.notify_all();
        Hosted {
            driver: self.clone(),
            wake,
        }
    }
    /// The scheduler thread: fires timers until an event loop hosts them.
    fn run(&self) {
        loop {
            let mut waiters = self.waiters.lock().unwrap();
            loop {
                {
                    let mut host = self.host.lock().unwrap();
                    if host.wake.is_some() {
                        host.thread = false;
                        return;
                    }
                }
                let Some((&(deadline, _), _)) = waiters.first_key_value() else {
                    waiters = self.changed.wait(waiters).unwrap();
                    continue;
                };
                let now = Instant::now();
                if deadline > now {
                    waiters = self
                        .changed
                        .wait_timeout(waiters, deadline - now)
                        .unwrap()
                        .0;
                    continue;
                }
                let ready = due(&mut waiters, now);
                drop(waiters);
                for waker in ready {
                    waker.wake();
                }
                break;
            }
        }
    }
}
fn due(waiters: &mut BTreeMap<(Instant, u64), Waker>, now: Instant) -> Vec<Waker> {
    let mut ready = Vec::new();
    while waiters
        .first_key_value()
        .is_some_and(|((deadline, _), _)| *deadline <= now)
    {
        ready.push(waiters.pop_first().unwrap().1);
    }
    ready
}
/// Fire timers from an event loop instead of the scheduler thread, while the
/// returned [`Hosted`] lives. The loop calls [`Hosted::fire`] as it wakes and
/// waits no later than [`Hosted::deadline`]. `wake` must wake the loop: it
/// runs when a sleep becomes the earliest deadline, which a loop that reads
/// the deadline after its own work only needs from other threads.
pub fn host(wake: impl Fn() + Send + Sync + 'static) -> Hosted {
    driver().host(Arc::new(wake))
}
/// An event loop's hold on the timers: see [`host`]. Dropping it returns them
/// to the scheduler thread.
pub struct Hosted {
    driver: Arc<Driver>,
    wake: Wake,
}
impl Hosted {
    /// Wake every sleep whose deadline has passed.
    pub fn fire(&self) {
        let ready = due(&mut self.driver.waiters.lock().unwrap(), Instant::now());
        for waker in ready {
            waker.wake();
        }
    }
    /// The earliest pending deadline.
    pub fn deadline(&self) -> Option<Instant> {
        let waiters = self.driver.waiters.lock().unwrap();
        waiters
            .first_key_value()
            .map(|(&(deadline, _), _)| deadline)
    }
}
impl Drop for Hosted {
    fn drop(&mut self) {
        let mut host = self.driver.host.lock().unwrap();
        if host
            .wake
            .as_ref()
            .is_some_and(|wake| Arc::ptr_eq(wake, &self.wake))
        {
            host.wake = None;
            drop(host);
            if !self.driver.waiters.lock().unwrap().is_empty() {
                self.driver.notify();
            }
        }
    }
}
/// A future that resolves at its deadline (see [`sleep`]).
#[must_use = "Futures do nothing unless awaited or polled"]
pub struct Sleep {
    deadline: Instant,
    registration: Option<(Arc<Driver>, u64)>,
}
/// A future that resolves `duration` from now.
pub fn sleep(duration: Duration) -> Sleep {
    sleep_until(Instant::now() + duration)
}
/// A future that resolves at `deadline` (at once if it has passed).
pub fn sleep_until(deadline: Instant) -> Sleep {
    Sleep {
        deadline,
        registration: None,
    }
}
impl Future for Sleep {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if Instant::now() >= self.deadline {
            if let Some((d, id)) = self.registration.take() {
                let removed = d.waiters.lock().unwrap().remove(&(self.deadline, id));
                drop(removed);
            }
            return Poll::Ready(());
        }
        if self.registration.is_none() {
            let d = driver().clone();
            let id = d.next.fetch_add(1, Ordering::Relaxed);
            self.registration = Some((d, id));
        }
        let (d, id) = self.registration.as_ref().unwrap();
        // Waker clone/drop may execute user code. Keep both outside the
        // scheduler lock, including replacing a task's previous waker.
        let next = cx.waker().clone();
        let key = (self.deadline, *id);
        let (removed, earliest) = {
            let mut waiters = d.waiters.lock().unwrap();
            match waiters.get(&key) {
                Some(w) if w.will_wake(&next) => (Some(next), false),
                Some(_) => (waiters.insert(key, next), false),
                None => {
                    waiters.insert(key, next);
                    (
                        None,
                        waiters.first_key_value().is_some_and(|(k, _)| *k == key),
                    )
                }
            }
        };
        drop(removed);
        // Only an earlier deadline changes when timers must fire next.
        if earliest {
            d.notify();
        }
        Poll::Pending
    }
}
impl Drop for Sleep {
    fn drop(&mut self) {
        if let Some((d, id)) = self.registration.take() {
            let removed = d.waiters.lock().unwrap().remove(&(self.deadline, id));
            drop(removed);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::task::Wake;
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn cancellation_unregisters_and_deadline_wakes_once() {
        let c = Arc::new(Count(AtomicUsize::new(0)));
        let w = Waker::from(c.clone());
        let mut cx = Context::from_waker(&w);
        let mut cancelled = sleep(Duration::from_secs(10));
        assert!(Pin::new(&mut cancelled).poll(&mut cx).is_pending());
        let (d, id) = cancelled.registration.as_ref().unwrap();
        let d = d.clone();
        let id = *id;
        let deadline = cancelled.deadline;
        drop(cancelled);
        assert!(!d.waiters.lock().unwrap().contains_key(&(deadline, id)));
        let mut future = sleep(Duration::from_millis(10));
        assert!(Pin::new(&mut future).poll(&mut cx).is_pending());
        let end = Instant::now() + Duration::from_secs(2);
        while c.0.load(Ordering::SeqCst) == 0 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(c.0.load(Ordering::SeqCst), 1);
        assert!(Pin::new(&mut future).poll(&mut cx).is_ready());
    }
}

#[cfg(test)]
mod hosted_tests {
    use super::*;
    use std::{sync::atomic::AtomicUsize, task::Wake};
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn hosted_timers_fire_from_the_loop_and_return_to_the_thread() {
        let driver = Arc::new(Driver {
            waiters: Mutex::new(BTreeMap::new()),
            changed: Condvar::new(),
            next: AtomicU64::new(1),
            host: Mutex::default(),
        });
        let woken = Arc::new(AtomicUsize::new(0));
        let hosted = driver.host(Arc::new({
            let woken = woken.clone();
            move || {
                woken.fetch_add(1, Ordering::SeqCst);
            }
        }));
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        let deadline = Instant::now() + Duration::from_millis(20);
        let mut first = Sleep {
            deadline,
            registration: Some((driver.clone(), 1)),
        };
        assert!(Pin::new(&mut first).poll(&mut cx).is_pending());
        // The earliest deadline wakes the hosting loop; no thread starts.
        assert_eq!(woken.load(Ordering::SeqCst), 1);
        assert!(!driver.host.lock().unwrap().thread);
        assert_eq!(hosted.deadline(), Some(deadline));
        // A later one changes nothing the loop waits for. Far enough later
        // that a slow machine's oversleep below does not make it due too.
        let mut later = Sleep {
            deadline: deadline + Duration::from_millis(400),
            registration: Some((driver.clone(), 2)),
        };
        assert!(Pin::new(&mut later).poll(&mut cx).is_pending());
        assert_eq!(woken.load(Ordering::SeqCst), 1);
        hosted.fire();
        assert_eq!(count.0.load(Ordering::SeqCst), 0, "not yet due");
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
        );
        hosted.fire();
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert!(Pin::new(&mut first).poll(&mut cx).is_ready());
        // Without a host, the thread fires what remains.
        drop(hosted);
        assert!(driver.host.lock().unwrap().thread);
        let end = Instant::now() + Duration::from_secs(2);
        while count.0.load(Ordering::SeqCst) < 2 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(count.0.load(Ordering::SeqCst), 2);
        assert!(Pin::new(&mut later).poll(&mut cx).is_ready());
    }
}

#[cfg(test)]
mod waker_cleanup_tests {
    use super::*;
    use std::{
        sync::{Weak, atomic::AtomicBool},
        task::Wake,
    };
    struct CheckDrop {
        driver: Weak<Driver>,
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
            if let Some(driver) = self.driver.upgrade() {
                self.locked
                    .store(driver.waiters.try_lock().is_err(), Ordering::SeqCst);
            }
        }
    }
    #[test]
    fn replacement_and_cancellation_drop_user_wakers_outside_timer_lock() {
        for mode in ["cancel", "replace", "ready"] {
            let driver = Arc::new(Driver {
                waiters: Mutex::new(BTreeMap::new()),
                changed: Condvar::new(),
                next: AtomicU64::new(2),
                host: Mutex::default(),
            });
            let locked = Arc::new(AtomicBool::new(false));
            let dropped = Arc::new(AtomicBool::new(false));
            let deadline = if mode == "ready" {
                Instant::now()
            } else {
                Instant::now() + Duration::from_secs(60)
            };
            driver.waiters.lock().unwrap().insert(
                (deadline, 1),
                Waker::from(Arc::new(CheckDrop {
                    driver: Arc::downgrade(&driver),
                    locked: locked.clone(),
                    dropped: dropped.clone(),
                })),
            );
            let mut future = Sleep {
                deadline,
                registration: Some((driver.clone(), 1)),
            };
            if mode == "ready" {
                assert!(
                    Pin::new(&mut future)
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_ready()
                );
            } else if mode == "replace" {
                assert!(
                    Pin::new(&mut future)
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending()
                );
            } else {
                drop(future);
            }
            assert!(dropped.load(Ordering::SeqCst), "user waker was not dropped");
            assert!(
                !locked.load(Ordering::SeqCst),
                "user waker destructor ran under timer lock (mode={mode})"
            );
        }
    }
}
