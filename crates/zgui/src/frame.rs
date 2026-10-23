//! Display-paced animation frames.
//!
//! A task awaits [`FrameClock::next`] once per step of an animation. The host
//! delivers one tick per display refresh while any task is waiting, so idle
//! windows do no work and hidden windows pause. Animate from [`Frame::time`],
//! the expected presentation time, rather than `Instant::now()`: it advances by
//! whole refresh intervals even when the UI thread runs late.
//!
//! ```
//! # use zgui::frame::FrameClock;
//! # async fn spin(frames: FrameClock, angle: zgui::reactive::Signal<f32>) {
//! let start = frames.next().await.time;
//! loop {
//!     // A decorative loader does not need every refresh of a 120 Hz display.
//!     let frame = frames.next().max_rate(30.).await;
//!     angle.set((frame.time - start).as_secs_f32() * 3.);
//! }
//! # }
//! ```
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

/// One display refresh delivered to waiting animations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Display refresh count. Frames delivered to a rate-limited request are
    /// spaced by the same number of refreshes.
    pub index: u64,
    /// When this frame is expected to reach the screen.
    pub time: Instant,
    /// The display's refresh interval.
    pub interval: Duration,
}
impl Frame {
    /// The display refresh rate in hertz.
    pub fn refresh_rate(&self) -> f64 {
        1. / self.interval.as_secs_f64().max(1e-6)
    }
}

/// Shared by a window's host and every animation in it. Clones share state.
#[derive(Clone, Default)]
pub struct FrameClock(Rc<Inner>);
#[derive(Default)]
struct Inner {
    waiters: RefCell<BTreeMap<u64, Waiter>>,
    next_id: Cell<u64>,
    last: Cell<Option<Frame>>,
    max_rate: Cell<Option<f64>>,
    /// Last refresh index delivered to each refresh divisor. Requests with the
    /// same divisor tick together and stay evenly spaced.
    cadence: RefCell<HashMap<u64, u64>>,
}
struct Waiter {
    max_rate: Option<f64>,
    waker: Option<Waker>,
    frame: Option<Frame>,
}

impl FrameClock {
    /// A clock with no requests waiting.
    pub fn new() -> Self {
        Self::default()
    }
    /// Resolve on the next display refresh. Chain [`NextFrame::max_rate`] to
    /// accept fewer frames.
    pub fn next(&self) -> NextFrame {
        NextFrame {
            clock: self.clone(),
            max_rate: None,
            id: None,
        }
    }
    /// The most recently delivered frame, if any.
    pub fn last(&self) -> Option<Frame> {
        self.0.last.get()
    }
    /// Upper bound on the rate delivered to every request, in hertz. `None`,
    /// zero, negative and non-finite values remove the cap.
    pub fn set_max_rate(&self, hz: Option<f64>) {
        self.0.max_rate.set(valid_rate(hz));
    }
    /// The cap `set_max_rate` set, if any.
    pub fn max_rate(&self) -> Option<f64> {
        self.0.max_rate.get()
    }
    /// True while any request is waiting. Hosts run their frame source only then.
    pub fn wants_frame(&self) -> bool {
        self.0
            .waiters
            .borrow()
            .values()
            .any(|waiter| waiter.frame.is_none())
    }
    /// The highest rate any waiting request can accept at `refresh` hertz,
    /// after caps. Hosts may lower the display's refresh to this.
    pub fn demanded_rate(&self, refresh: f64) -> Option<f64> {
        let waiters = self.0.waiters.borrow();
        waiters
            .values()
            .filter(|waiter| waiter.frame.is_none())
            .map(|waiter| refresh / self.divisor(waiter.max_rate, refresh) as f64)
            .reduce(f64::max)
    }
    /// Host entry point: deliver one display refresh. Returns how many
    /// requests it resolved.
    pub fn deliver(&self, frame: Frame) -> usize {
        self.0.last.set(Some(frame));
        let refresh = frame.refresh_rate();
        let mut wakers = Vec::new();
        {
            let mut waiters = self.0.waiters.borrow_mut();
            let mut cadence = self.0.cadence.borrow_mut();
            let mut due: HashMap<u64, bool> = HashMap::new();
            for waiter in waiters.values_mut().filter(|waiter| waiter.frame.is_none()) {
                let divisor = self.divisor(waiter.max_rate, refresh);
                let ready = *due.entry(divisor).or_insert_with(|| {
                    cadence
                        .get(&divisor)
                        // A restarted source counts from a lower index again.
                        .is_none_or(|&last| {
                            frame.index >= last.saturating_add(divisor) || frame.index < last
                        })
                });
                if ready {
                    waiter.frame = Some(frame);
                    wakers.extend(waiter.waker.take());
                }
            }
            for (divisor, ready) in due {
                if ready {
                    cadence.insert(divisor, frame.index);
                }
            }
        }
        // Wakers may run user code; keep them outside the borrows.
        let resolved = wakers.len();
        for waker in wakers {
            waker.wake();
        }
        resolved
    }
    /// Refreshes between frames for a request, combining its rate with the cap.
    fn divisor(&self, max_rate: Option<f64>, refresh: f64) -> u64 {
        [max_rate, self.0.max_rate.get()]
            .into_iter()
            .flatten()
            .map(|rate| divisor(refresh, rate))
            .max()
            .unwrap_or(1)
    }
}

/// Whole refreshes per frame so a request never exceeds `rate`. A 50 Hz cap on
/// a 120 Hz display ticks every third refresh (40 Hz) rather than unevenly.
/// Displays run a little off their nominal rate (60.0002 Hz, measured on
/// Windows), so a rate within 0.1% of a whole division takes it: a 30 Hz
/// cap there is every other refresh, not every third.
pub fn divisor(refresh: f64, rate: f64) -> u64 {
    if !refresh.is_finite() || refresh <= rate {
        return 1;
    }
    ((refresh / rate) - 1e-3).ceil().clamp(1., 1e6) as u64
}

fn valid_rate(hz: Option<f64>) -> Option<f64> {
    hz.filter(|hz| hz.is_finite() && *hz > 0.)
}

/// Future returned by [`FrameClock::next`].
#[must_use = "Futures do nothing unless awaited or polled"]
pub struct NextFrame {
    clock: FrameClock,
    max_rate: Option<f64>,
    id: Option<u64>,
}
impl NextFrame {
    /// Accept at most `hz` frames per second, evenly spaced in whole display
    /// refreshes. Requests with the same effective rate tick together.
    pub fn max_rate(mut self, hz: f64) -> Self {
        self.max_rate = valid_rate(Some(hz));
        self
    }
}
impl Future for NextFrame {
    type Output = Frame;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Frame> {
        let inner = &self.clock.0;
        let Some(id) = self.id else {
            let id = inner.next_id.get();
            inner.next_id.set(id + 1);
            inner.waiters.borrow_mut().insert(
                id,
                Waiter {
                    max_rate: self.max_rate,
                    waker: Some(cx.waker().clone()),
                    frame: None,
                },
            );
            self.id = Some(id);
            return Poll::Pending;
        };
        let mut waiters = inner.waiters.borrow_mut();
        let waiter = waiters.get_mut(&id).expect("registered frame request");
        if let Some(frame) = waiter.frame {
            waiters.remove(&id);
            drop(waiters);
            self.id = None;
            return Poll::Ready(frame);
        }
        if !waiter
            .waker
            .as_ref()
            .is_some_and(|waker| waker.will_wake(cx.waker()))
        {
            let previous = waiter.waker.replace(cx.waker().clone());
            drop(waiters);
            drop(previous);
        }
        Poll::Pending
    }
}
impl Drop for NextFrame {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let removed = self.clock.0.waiters.borrow_mut().remove(&id);
            drop(removed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::Wake,
    };
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn frame(start: Instant, index: u64, hz: f64) -> Frame {
        let interval = Duration::from_secs_f64(1. / hz);
        Frame {
            index,
            time: start + interval * index as u32,
            interval,
        }
    }
    /// Poll `future` for every refresh in `indices`, re-arming after each
    /// frame like an animation loop. Returns the refresh indices received.
    fn run(
        clock: &FrameClock,
        request: impl Fn() -> NextFrame,
        indices: impl IntoIterator<Item = u64>,
        hz: f64,
    ) -> Vec<u64> {
        let start = Instant::now();
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count);
        let mut cx = Context::from_waker(&waker);
        let mut pending = Box::pin(request());
        assert!(pending.as_mut().poll(&mut cx).is_pending());
        let mut received = Vec::new();
        for index in indices {
            clock.deliver(frame(start, index, hz));
            if let Poll::Ready(frame) = pending.as_mut().poll(&mut cx) {
                received.push(frame.index);
                pending = Box::pin(request());
                assert!(pending.as_mut().poll(&mut cx).is_pending());
            }
        }
        received
    }

    #[test]
    fn requests_resolve_on_the_next_refresh_and_wake_once() {
        let clock = FrameClock::new();
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        let mut next = clock.next();
        assert!(!clock.wants_frame());
        assert!(Pin::new(&mut next).poll(&mut cx).is_pending());
        assert!(clock.wants_frame());
        let start = Instant::now();
        assert_eq!(clock.deliver(frame(start, 7, 60.)), 1);
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert!(
            !clock.wants_frame(),
            "a resolved request no longer demands frames"
        );
        let Poll::Ready(received) = Pin::new(&mut next).poll(&mut cx) else {
            panic!("frame was delivered");
        };
        assert_eq!(received.index, 7);
        assert_eq!(clock.last(), Some(received));
        assert_eq!(clock.deliver(frame(start, 8, 60.)), 0);
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dropped_requests_stop_demanding_frames() {
        let clock = FrameClock::new();
        let mut next = clock.next();
        assert!(
            Pin::new(&mut next)
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(clock.wants_frame());
        drop(next);
        assert!(!clock.wants_frame());
        assert!(clock.0.waiters.borrow().is_empty());
    }

    #[test]
    fn rate_limits_space_frames_by_whole_refreshes() {
        let clock = FrameClock::new();
        assert_eq!(
            run(&clock, || clock.next(), 0..6, 120.),
            vec![0, 1, 2, 3, 4, 5]
        );
        let clock = FrameClock::new();
        assert_eq!(
            run(&clock, || clock.next().max_rate(30.), 0..13, 120.),
            vec![0, 4, 8, 12]
        );
        // 50 Hz on 120 Hz rounds down to every third refresh, never above the limit.
        let clock = FrameClock::new();
        assert_eq!(
            run(&clock, || clock.next().max_rate(50.), 0..10, 120.),
            vec![0, 3, 6, 9]
        );
        // A limit at or above the display rate takes every refresh.
        let clock = FrameClock::new();
        assert_eq!(
            run(&clock, || clock.next().max_rate(144.), 0..4, 60.),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn spacing_holds_when_the_display_skips_refreshes() {
        // A source that only fires every other refresh (or drops some) must not
        // stall a request whose cadence it never lands on exactly.
        let clock = FrameClock::new();
        let got = run(
            &clock,
            || clock.next().max_rate(30.),
            [1, 3, 5, 7, 9, 11, 13],
            120.,
        );
        assert_eq!(got, vec![1, 5, 9, 13]);
    }

    #[test]
    fn a_restarted_source_does_not_stall_limited_requests() {
        let clock = FrameClock::new();
        let first = run(&clock, || clock.next().max_rate(30.), 1000..1005, 120.);
        assert_eq!(first, vec![1000, 1004]);
        let restarted = run(&clock, || clock.next().max_rate(30.), 0..5, 120.);
        assert_eq!(restarted, vec![0, 4]);
    }

    #[test]
    fn window_cap_applies_to_every_request_and_can_be_lifted() {
        let clock = FrameClock::new();
        clock.set_max_rate(Some(60.));
        assert_eq!(run(&clock, || clock.next(), 0..6, 120.), vec![0, 2, 4]);
        // The stricter of the request limit and the cap wins.
        assert_eq!(
            run(&clock, || clock.next().max_rate(30.), 8..17, 120.),
            vec![8, 12, 16]
        );
        clock.set_max_rate(Some(f64::NAN));
        assert_eq!(clock.max_rate(), None);
        assert_eq!(run(&clock, || clock.next(), 20..23, 120.), vec![20, 21, 22]);
    }

    #[test]
    fn requests_with_the_same_rate_tick_together() {
        let clock = FrameClock::new();
        let start = Instant::now();
        let mut cx = Context::from_waker(Waker::noop());
        let mut a = Box::pin(clock.next().max_rate(40.));
        let mut b = Box::pin(clock.next().max_rate(40.));
        let mut full = Box::pin(clock.next());
        assert!(a.as_mut().poll(&mut cx).is_pending());
        assert!(b.as_mut().poll(&mut cx).is_pending());
        assert!(full.as_mut().poll(&mut cx).is_pending());
        assert_eq!(clock.demanded_rate(120.), Some(120.));
        assert_eq!(clock.deliver(frame(start, 0, 120.)), 3);
        let _ = (a.as_mut().poll(&mut cx), b.as_mut().poll(&mut cx));
        let mut a = Box::pin(clock.next().max_rate(40.));
        let mut b = Box::pin(clock.next().max_rate(40.));
        assert!(a.as_mut().poll(&mut cx).is_pending());
        assert!(b.as_mut().poll(&mut cx).is_pending());
        assert_eq!(clock.demanded_rate(120.), Some(40.));
        assert_eq!(clock.deliver(frame(start, 1, 120.)), 0);
        assert_eq!(clock.deliver(frame(start, 2, 120.)), 0);
        assert_eq!(clock.deliver(frame(start, 3, 120.)), 2);
    }
}
