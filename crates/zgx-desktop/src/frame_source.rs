//! Native display refresh sources for [`zgx::frame::FrameClock`].
//!
//! macOS uses a `CVDisplayLink` per display (see `display_link`), which
//! windows follow as they move between displays. Windows waits on the
//! compositor's clock (see `compositor_clock`). Wayland paces through compositor frame
//! callbacks (driven from the host's redraw requests). Everything else, and
//! any platform whose native source is unavailable, uses [`FrameTimer`]:
//! event-loop deadlines on a fixed grid at the display's reported refresh rate.
use std::time::{Duration, Instant};
use zgx::frame::Frame;

pub(crate) const DEFAULT_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// A native source that stops ticking while frames are wanted is backed up by
/// the timer after this long, so animations degrade instead of freezing.
#[cfg(any(target_os = "macos", windows))]
pub(crate) const WATCHDOG: Duration = Duration::from_millis(250);

/// Spacings between a display link's consecutive output times that
/// `refresh_period` judges from.
#[cfg(any(target_os = "macos", test))]
pub(crate) const PERIOD_SAMPLES: usize = 8;
/// The display's refresh period from what its link delivers: the shortest of
/// the recent spacings between consecutive output times (`spacings`, in
/// nanoseconds, 0 where none yet), which sit on the display's refresh grid. A
/// missed or paused callback only lengthens a spacing, never shortens one.
/// The link's nominal period stands until three spacings are known, or when
/// none is a plausible refresh (2 to 100 ms). The mode the display reports
/// can be another (a 60 Hz mode read on a panel refreshing at 120 Hz), which
/// would pace full-rate content at half rate.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn refresh_period(nominal: Duration, spacings: &[u64; PERIOD_SAMPLES]) -> Duration {
    let plausible = spacings
        .iter()
        .copied()
        .filter(|ns| (2_000_000..=100_000_000).contains(ns));
    match plausible.clone().count() >= 3 {
        true => plausible.min().map_or(nominal, Duration::from_nanos),
        false => nominal,
    }
}
/// How long before the compositor's vblank (Windows) a present may miss
/// the refresh it was meant for: DWM latches frames for a refresh shortly
/// before its vblank. Measured on a 60 Hz display, the share of presents
/// followed by a missed refresh against the time since the vblank they were
/// made at: none from 0 to 9.5 ms, about 2% from 13.5 to 15 ms, 13% in the
/// last 1.5 ms.
#[cfg(any(windows, test))]
pub(crate) const LATCH_BEFORE: Duration = Duration::from_micros(3500);
/// Held frames present this long past the vblank, clear of it.
#[cfg(any(windows, test))]
pub(crate) const LATCH_AFTER: Duration = Duration::from_micros(1000);
/// While frames are held, the window widens by this much: a timer whose
/// phase sits at the window's edge does not alternate between held frames
/// (shown a refresh later) and frames presented at once, each switch a
/// refresh with no new frame.
#[cfg(any(windows, test))]
pub(crate) const LATCH_KEEP: Duration = Duration::from_micros(1000);

/// Whether a frame of the app's own timing, ready `phase` after the
/// compositor's last vblank in a refresh of `period`, waits before it
/// presents, and how long: one ready near the latch (`LATCH_BEFORE` before
/// the vblank, or `LATCH_AFTER` after it) waits until `LATCH_AFTER` past the
/// vblank, so that every frame of a steady run lands on one side of the
/// latch. A timer drifting slowly against the display otherwise sits near
/// the latch for seconds, and frames alternately make and miss it: in
/// captures, a launch in eight lost about 60 refreshes in 8 s. `holding`:
/// the last frame waited.
#[cfg(any(windows, test))]
pub(crate) fn latch_wait(phase: Duration, period: Duration, holding: bool) -> Option<Duration> {
    let before = LATCH_BEFORE + if holding { LATCH_KEEP } else { Duration::ZERO };
    if phase >= period || period <= before + LATCH_AFTER {
        return None;
    }
    if phase + before >= period {
        Some(period - phase + LATCH_AFTER)
    } else if phase < LATCH_AFTER {
        Some(LATCH_AFTER - phase)
    } else {
        None
    }
}

pub(crate) fn interval_for(refresh_hz: Option<f64>) -> Duration {
    refresh_hz
        .filter(|hz| hz.is_finite() && *hz >= 1.)
        .map_or(DEFAULT_INTERVAL, |hz| Duration::from_secs_f64(1. / hz))
}

/// Refresh-aligned deadlines without a native vsync signal. Ticks land on a
/// fixed grid from `epoch`, so a late wake-up does not drift later ticks.
pub(crate) struct FrameTimer {
    epoch: Instant,
    /// The index of the refresh starting at `epoch`.
    first: u64,
}
impl FrameTimer {
    pub(crate) fn new(epoch: Instant) -> Self {
        Self { epoch, first: 0 }
    }
    /// Ticks continuing a native source's: refresh `first` starts at `epoch`.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn aligned(epoch: Instant, first: u64) -> Self {
        Self { epoch, first }
    }
    fn index(&self, at: Instant, interval: Duration) -> u64 {
        (at.saturating_duration_since(self.epoch).as_secs_f64() / interval.as_secs_f64()) as u64
    }
    /// The first refresh boundary strictly after `now`.
    pub(crate) fn next_deadline(&self, now: Instant, interval: Duration) -> Instant {
        if now < self.epoch {
            return self.epoch;
        }
        let next = self.index(now, interval) + 1;
        self.epoch + interval.mul_f64(next as f64)
    }
    /// The first refresh after `now` whose index is a multiple of `every`:
    /// the refreshes a native source paced to every `every`th delivers
    /// (`zgx::frame::divisor`), so frames the timer paces keep its cadence
    /// as one hands over to the other.
    #[cfg(target_os = "macos")]
    pub(crate) fn next_deadline_every(
        &self,
        now: Instant,
        interval: Duration,
        every: u64,
    ) -> Instant {
        let every = every.max(1);
        let current = if now < self.epoch {
            self.first.saturating_sub(1)
        } else {
            self.first + self.index(now, interval)
        };
        let next = (current / every + 1) * every;
        self.epoch + interval.mul_f64(next.saturating_sub(self.first) as f64)
    }
    /// The frame for the boundary at or before `now`. It is expected on screen
    /// one refresh later, once rendered.
    pub(crate) fn frame(&self, now: Instant, interval: Duration) -> Frame {
        let index = self.index(now, interval);
        Frame {
            index: self.first + index,
            time: self.epoch + interval.mul_f64((index + 1) as f64),
            interval,
        }
    }
}

/// How long a refresh grid learned from the display link paces frames
/// before the link runs again to resync it (see `Host::sync_frames`).
#[cfg(target_os = "macos")]
pub(crate) const VSYNC_RESYNC: Duration = Duration::from_secs(1);

#[cfg(windows)]
pub(crate) use compositor_clock::{DisplayLink, refresh_phase, sleep_precisely};
#[cfg(target_os = "macos")]
pub(crate) use display_link::DisplayLink;

#[cfg(target_os = "macos")]
mod display_link {
    //! `CVDisplayLink` pacing, shared per display.
    //!
    //! A view's `CADisplayLink` calls back as late in the refresh as it thinks
    //! the app can afford: about 2 ms before the deadline for a fast renderer.
    //! Any hiccup then misses the compositor's latch, the frame shows a refresh
    //! late, a second present queues behind it, and from then on every
    //! drawable acquisition waits on the compositor (a frame lost every few
    //! refreshes). `CVDisplayLink` fires on its own thread at the start of each
    //! refresh, leaving the whole interval to render and present.
    //!
    //! Frames presented as `CAMetalLayer` drawables are the exception
    //! (`DisplayLink::set_view_clock`): Metal hands each to the window server
    //! for its refresh, and on the `CVDisplayLink` most launches of an M1 Max
    //! settled with the pool full, each frame waiting most of a refresh for a
    //! drawable and showing a refresh later (2026-10-27). On the view's
    //! `CADisplayLink`, with a tick skipped when that starts (the host's
    //! rephase), they show early: the window server's CPU 31.3% against 34.2
    //! (M1 Max), and the frame's start to its showing 16 ms against 27–32 (M5
    //! Max, a pool of 2).
    //!
    //! `CVDisplayLinkStop` returns before the link's thread has necessarily
    //! finished its last callback, and releasing a link or anything its
    //! callback reads races that callback. So, as in GPUI: one link per
    //! display, created on first use and never released; its callback context
    //! is the display id, not a pointer, and it reads only the static registry.
    //! Windows subscribe with a thread-safe delivery function. Links start and
    //! stop only on the main thread and never while the registry lock is held,
    //! since the callback takes that lock under CoreVideo's own locks.
    use super::*;
    use objc2::{
        DeclaredClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
        runtime::AnyObject, sel,
    };
    use objc2_foundation::{NSObject, NSObjectProtocol};
    use std::{
        collections::HashMap,
        ffi::c_void,
        sync::{
            Arc, Mutex, MutexGuard, PoisonError,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
    };
    use winit::{
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SmpteTime {
        subframes: i16,
        subframe_divisor: i16,
        counter: u32,
        kind: u32,
        flags: u32,
        hours: i16,
        minutes: i16,
        seconds: i16,
        frames: i16,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CvTimeStamp {
        version: u32,
        video_time_scale: i32,
        video_time: i64,
        host_time: u64,
        rate_scalar: f64,
        video_refresh_period: i64,
        smpte_time: SmpteTime,
        flags: u64,
        reserved: u64,
    }
    // CoreVideo's CVTimeStamp is 80 bytes (CVSMPTETime 24).
    const _: () = assert!(size_of::<CvTimeStamp>() == 80 && size_of::<SmpteTime>() == 24);
    type CvDisplayLink = c_void;
    type OutputCallback = extern "C" fn(
        *mut CvDisplayLink,
        *const CvTimeStamp,
        *const CvTimeStamp,
        u64,
        *mut u64,
        *mut c_void,
    ) -> i32;
    // SAFETY: CoreVideo's declarations (CVDisplayLink.h): CGDirectDisplayID
    // is u32, CVReturn i32, CVOptionFlags u64, and the output callback's
    // signature is `OutputCallback`'s.
    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVDisplayLinkCreateWithCGDisplay(display: u32, link: *mut *mut CvDisplayLink) -> i32;
        fn CVDisplayLinkSetOutputCallback(
            link: *mut CvDisplayLink,
            callback: OutputCallback,
            context: *mut c_void,
        ) -> i32;
        fn CVDisplayLinkStart(link: *mut CvDisplayLink) -> i32;
        fn CVDisplayLinkStop(link: *mut CvDisplayLink) -> i32;
    }
    #[repr(C)]
    #[derive(Default)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    // SAFETY: libSystem's declarations (mach/mach_time.h): the clock is a
    // u64, the timebase two u32s, kern_return_t i32.
    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    fn nanos(ticks: i128) -> i128 {
        static TIMEBASE: std::sync::OnceLock<(i128, i128)> = std::sync::OnceLock::new();
        let (numer, denom) = *TIMEBASE.get_or_init(|| {
            let mut info = Timebase::default();
            // SAFETY: writes two integers into a local.
            unsafe { mach_timebase_info(&mut info) };
            (info.numer as i128, info.denom.max(1) as i128)
        });
        ticks * numer / denom
    }

    /// One window's subscription, reachable from the link thread.
    struct Subscriber {
        id: u64,
        /// Delivering: the window wants refreshes.
        active: AtomicBool,
        /// A delivered refresh has not been consumed yet; later ones coalesce.
        pending: AtomicBool,
        /// Deliver at most this often (millihertz; 0 for every refresh).
        max_rate: AtomicU64,
        /// The view's `CADisplayLink` delivers, not the display's link
        /// (`DisplayLink::set_view_clock`).
        view_clock: AtomicBool,
        deliver: Box<dyn Fn(Frame) + Send + Sync>,
    }
    /// The refresh a display's link last reported starting (host time), and
    /// the display's refresh period (ns): its refresh grid, which windows on
    /// it read without the registry's lock (`DisplayLink::refresh_phase`).
    #[derive(Default)]
    struct Grid {
        refreshed: AtomicU64,
        period: AtomicU64,
    }
    struct Entry {
        /// Immortal: see the module comment.
        link: usize,
        running: bool,
        subscribers: Vec<Arc<Subscriber>>,
        grid: Arc<Grid>,
        /// The latest spacings between consecutive output times (ns), a ring
        /// `refresh_period` reads, and where the next goes.
        spacings: [AtomicU64; super::PERIOD_SAMPLES],
        spaced: AtomicU64,
    }
    #[derive(Default)]
    struct Registry {
        displays: HashMap<u32, Entry>,
        next_id: u64,
    }
    static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);
    fn registry() -> MutexGuard<'static, Option<Registry>> {
        // The map stays consistent across a panic, and the extern callback
        // must not unwind.
        REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
    }

    extern "C" fn output(
        _link: *mut CvDisplayLink,
        _now: *const CvTimeStamp,
        output: *const CvTimeStamp,
        _flags_in: u64,
        _flags_out: *mut u64,
        context: *mut c_void,
    ) -> i32 {
        // SAFETY: CoreVideo passes a valid timestamp for this callback.
        let output = unsafe { *output };
        let display = context as usize as u32;
        let period = output.video_refresh_period.max(1) as f64;
        let scale = output.video_time_scale.max(1) as f64;
        let nominal = Duration::from_secs_f64(period / scale);
        let index = (output.video_time as f64 / period).round().max(0.) as u64;
        // SAFETY: reads the monotonic host clock.
        let host_now = unsafe { mach_absolute_time() };
        let ahead = nanos(output.host_time as i128 - host_now as i128).clamp(0, 1_000_000_000);
        let guard = registry();
        let Some(entry) = guard.as_ref().and_then(|r| r.displays.get(&display)) else {
            return 0;
        };
        // The output time: exactly on the display's refresh grid (the
        // callback's `now`, when it runs, wanders by up to 2 ms). Its spacing
        // from the last is the display's period, whatever mode it reports.
        let previous = entry
            .grid
            .refreshed
            .swap(output.host_time, Ordering::AcqRel);
        if previous != 0 && output.host_time > previous {
            let at = entry.spaced.fetch_add(1, Ordering::Relaxed) as usize % super::PERIOD_SAMPLES;
            let spacing =
                nanos(output.host_time as i128 - previous as i128).clamp(0, u64::MAX as i128);
            entry.spacings[at].store(spacing as u64, Ordering::Relaxed);
        }
        let spacings = std::array::from_fn(|at| entry.spacings[at].load(Ordering::Relaxed));
        let interval = super::refresh_period(nominal, &spacings);
        // Refreshes counted on the measured grid where the nominal period
        // is another: video time in nominal periods would repeat an index
        // (two refreshes to one), and the frame clock resolves each once.
        let off = (interval.as_secs_f64() / nominal.as_secs_f64() - 1.).abs() > 0.01;
        let index = match off {
            true => {
                (nanos(output.host_time as i128) as f64 / interval.as_nanos() as f64).round() as u64
            }
            false => index,
        };
        let frame = Frame {
            index,
            time: Instant::now() + Duration::from_nanos(ahead as u64),
            interval,
        };
        let refresh = 1. / interval.as_secs_f64();
        entry
            .grid
            .period
            .store(interval.as_nanos() as u64, Ordering::Release);
        for subscriber in &entry.subscribers {
            if !subscriber.active.load(Ordering::Acquire)
                || subscriber.view_clock.load(Ordering::Acquire)
            {
                continue;
            }
            let max_rate = subscriber.max_rate.load(Ordering::Relaxed);
            if max_rate > 0 {
                let divisor = zgx::frame::divisor(refresh, max_rate as f64 / 1000.);
                if !index.is_multiple_of(divisor) {
                    continue;
                }
            }
            if !subscriber.pending.swap(true, Ordering::AcqRel) {
                (subscriber.deliver)(frame);
            }
        }
        0
    }

    /// Start or stop `display`'s link to match its active subscribers. Main
    /// thread only; CoreVideo is called outside the lock.
    fn reconcile(display: u32) {
        let change = {
            let mut guard = registry();
            let Some(entry) = guard.as_mut().and_then(|r| r.displays.get_mut(&display)) else {
                return;
            };
            let wanted = entry
                .subscribers
                .iter()
                .any(|s| s.active.load(Ordering::Acquire) && !s.view_clock.load(Ordering::Acquire));
            (wanted != entry.running).then(|| {
                entry.running = wanted;
                (entry.link, wanted)
            })
        };
        if let Some((link, start)) = change {
            let link = link as *mut CvDisplayLink;
            // SAFETY: a link `ensure_link` created and never releases; its
            // callback reads only the registry (see the module comment).
            unsafe {
                if start {
                    CVDisplayLinkStart(link);
                } else {
                    CVDisplayLinkStop(link);
                }
            }
        }
    }

    /// Ensure `display` has a link, creating it (outside the lock) if needed.
    fn ensure_link(display: u32) -> bool {
        if registry()
            .as_ref()
            .is_some_and(|r| r.displays.contains_key(&display))
        {
            return true;
        }
        let mut link: *mut CvDisplayLink = std::ptr::null_mut();
        // SAFETY: CoreVideo writes a new link to the local on success.
        if unsafe { CVDisplayLinkCreateWithCGDisplay(display, &mut link) } != 0 || link.is_null() {
            return false;
        }
        // SAFETY: the link was just made; `output` never unwinds (an extern
        // "C" panic aborts) and takes its context as the display id, never
        // dereferenced.
        unsafe { CVDisplayLinkSetOutputCallback(link, output, display as usize as *mut c_void) };
        let mut guard = registry();
        let registry = guard.get_or_insert_with(Registry::default);
        // Another subscriber may have raced us; the spare link is leaked
        // deliberately (never started, so it never calls back).
        registry.displays.entry(display).or_insert(Entry {
            link: link as usize,
            running: false,
            subscribers: Vec::new(),
            grid: Arc::default(),
            spacings: Default::default(),
            spaced: AtomicU64::new(0),
        });
        true
    }

    /// A `CADisplayLink` target's state: the window's subscription and the
    /// grid its ticks report.
    struct Tick {
        subscriber: Arc<Subscriber>,
        grid: Arc<Grid>,
    }
    // SAFETY: an NSObject subclass, main-thread only (a display link made
    // from a view calls back on the run loop it is added to, the main one),
    // with nothing to clean up beyond its ivars; `tick:` takes the link.
    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[ivars = Tick]
        struct LinkTarget;
        unsafe impl NSObjectProtocol for LinkTarget {}
        impl LinkTarget {
            #[unsafe(method(tick:))]
            fn tick(&self, link: &AnyObject) {
                self.deliver(link);
            }
        }
    );
    impl LinkTarget {
        fn deliver(&self, link: &AnyObject) {
            let tick = self.ivars();
            // SAFETY: CADisplayLink's -timestamp (the refresh that started),
            // -targetTimestamp (the next) and -duration, CFTimeIntervals in
            // seconds on the host clock (CACurrentMediaTime's).
            let (timestamp, target, duration): (f64, f64, f64) = unsafe {
                (
                    msg_send![link, timestamp],
                    msg_send![link, targetTimestamp],
                    msg_send![link, duration],
                )
            };
            let interval = match duration > 0. && duration.is_finite() {
                true => Duration::from_secs_f64(duration),
                false => DEFAULT_INTERVAL,
            };
            // Host ticks from host seconds: the inverse of `nanos`.
            let ns_per_tick = nanos(1_000_000_000) as f64 / 1e9;
            let ticks = |seconds: f64| (seconds * 1e9 / ns_per_tick).max(0.) as u64;
            tick.grid
                .refreshed
                .store(ticks(timestamp), Ordering::Release);
            tick.grid
                .period
                .store(interval.as_nanos() as u64, Ordering::Release);
            let subscriber = &tick.subscriber;
            if !subscriber.active.load(Ordering::Acquire)
                || !subscriber.view_clock.load(Ordering::Acquire)
            {
                return;
            }
            // SAFETY: reads the monotonic host clock.
            let now = nanos(unsafe { mach_absolute_time() } as i128) as f64 / 1e9;
            let ahead = (target - now).clamp(0., 1.);
            let index = (target / interval.as_secs_f64()).round().max(0.) as u64;
            let frame = Frame {
                index,
                time: Instant::now() + Duration::from_secs_f64(ahead),
                interval,
            };
            let max_rate = subscriber.max_rate.load(Ordering::Relaxed);
            if max_rate > 0 {
                let refresh = 1. / interval.as_secs_f64();
                let divisor = zgx::frame::divisor(refresh, max_rate as f64 / 1000.);
                if !index.is_multiple_of(divisor) {
                    return;
                }
            }
            if !subscriber.pending.swap(true, Ordering::AcqRel) {
                (subscriber.deliver)(frame);
            }
        }
    }
    /// The view's `CADisplayLink` and its target (`DisplayLink::set_view_clock`).
    struct ViewLink {
        link: Retained<AnyObject>,
        _target: Retained<LinkTarget>,
        /// The refreshes its ticks report.
        grid: Arc<Grid>,
    }
    impl ViewLink {
        fn new(view: &AnyObject, subscriber: Arc<Subscriber>) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            // SAFETY: NSObject's -respondsToSelector:, on the main thread.
            let has: bool = unsafe {
                msg_send![view, respondsToSelector: sel!(displayLinkWithTarget:selector:)]
            };
            if !has {
                return None;
            }
            let grid = Arc::new(Grid::default());
            let target = LinkTarget::alloc(mtm).set_ivars(Tick {
                subscriber,
                grid: grid.clone(),
            });
            // SAFETY: NSObject's `init`, on a fresh allocation with its ivars set.
            let target: Retained<LinkTarget> = unsafe { msg_send![super(target), init] };
            // SAFETY: NSView -displayLinkWithTarget:selector: (macOS 14) returns a
            // CADisplayLink calling `tick:` on `target` (which it retains until
            // -invalidate); -setPaused: and -addToRunLoop:forMode: are its own,
            // with the main run loop.
            unsafe {
                let link: Option<Retained<AnyObject>> =
                    msg_send![view, displayLinkWithTarget: &*target, selector: sel!(tick:)];
                let link = link?;
                let _: () = msg_send![&*link, setPaused: true];
                let run_loop: Retained<AnyObject> =
                    msg_send![objc2::class!(NSRunLoop), mainRunLoop];
                // Added in the common modes alone, by their name, it never
                // fired (2026-10-27): the default mode too.
                for mode in ["kCFRunLoopCommonModes", "kCFRunLoopDefaultMode"] {
                    let mode = objc2_foundation::NSString::from_str(mode);
                    let _: () = msg_send![&*link, addToRunLoop: &*run_loop, forMode: &*mode];
                }
                Some(Self {
                    link,
                    _target: target,
                    grid,
                })
            }
        }
        fn set_paused(&self, paused: bool) {
            // SAFETY: CADisplayLink's -setPaused:, on the main thread.
            let _: () = unsafe { msg_send![&*self.link, setPaused: paused] };
        }
    }
    impl Drop for ViewLink {
        fn drop(&mut self) {
            // SAFETY: CADisplayLink's -invalidate: removes it from its run loop
            // and releases its target.
            let _: () = unsafe { msg_send![&*self.link, invalidate] };
        }
    }

    /// A window's subscription to its display's refreshes.
    pub(crate) struct DisplayLink {
        view: Retained<AnyObject>,
        display: u32,
        subscriber: Arc<Subscriber>,
        /// `display`'s.
        grid: Arc<Grid>,
        /// When the window's display was last checked (an AppKit round trip).
        checked: Instant,
        /// The view's own link, made the first time the view clock is asked for.
        view_link: Option<ViewLink>,
    }

    impl DisplayLink {
        /// `deliver` runs on the link's thread, at most once per consumed
        /// refresh; hosts forward it to the UI thread.
        pub(crate) fn new(
            window: &Window,
            deliver: impl Fn(Frame) + Send + Sync + 'static,
        ) -> Option<Self> {
            let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
                return None;
            };
            // SAFETY: winit keeps this NSView alive while the window exists
            // (borrowed here); retaining it keeps it valid for the link's
            // lifetime. winit hands out an AppKit handle only on the main
            // thread, and `Self` is not `Send` (it holds the view), so the
            // view is only used there.
            let view: Retained<AnyObject> =
                unsafe { Retained::retain(handle.ns_view.as_ptr().cast())? };
            let display = display_of(&view)?;
            if !ensure_link(display) {
                return None;
            }
            let (subscriber, grid) = {
                let mut guard = registry();
                let registry = guard.as_mut()?;
                registry.next_id += 1;
                let subscriber = Arc::new(Subscriber {
                    id: registry.next_id,
                    active: AtomicBool::new(false),
                    pending: AtomicBool::new(false),
                    max_rate: AtomicU64::new(0),
                    view_clock: AtomicBool::new(false),
                    deliver: Box::new(deliver),
                });
                let entry = registry.displays.get_mut(&display)?;
                entry.subscribers.push(subscriber.clone());
                (subscriber, entry.grid.clone())
            };
            Some(Self {
                view,
                display,
                subscriber,
                grid,
                checked: Instant::now(),
                view_link: None,
            })
        }
        pub(crate) fn running(&self) -> bool {
            self.subscriber.active.load(Ordering::Acquire)
        }
        pub(crate) fn set_running(&mut self, running: bool) {
            // Recheck the display when starting, and about once a second while
            // running, in case the window moved to another screen.
            if running && (!self.running() || self.checked.elapsed() > Duration::from_secs(1)) {
                self.checked = Instant::now();
                self.follow_display();
            }
            if self.subscriber.active.swap(running, Ordering::AcqRel) != running {
                self.subscriber.pending.store(false, Ordering::Release);
                if let Some(view_link) = &self.view_link {
                    view_link.set_paused(!(running && self.on_view_clock()));
                }
                reconcile(self.display);
            }
        }
        /// Refreshes come from the view's `CADisplayLink` (`on`), for frames
        /// presented as drawables, or the display's `CVDisplayLink` (see the
        /// module comment). Main thread only.
        pub(crate) fn set_view_clock(&mut self, on: bool) {
            if self.on_view_clock() == on {
                return;
            }
            if on && self.view_link.is_none() {
                self.view_link = ViewLink::new(&self.view, self.subscriber.clone());
                if self.view_link.is_none() {
                    return;
                }
            }
            self.subscriber.view_clock.store(on, Ordering::Release);
            self.subscriber.pending.store(false, Ordering::Release);
            if let Some(view_link) = &self.view_link {
                view_link.set_paused(!(on && self.running()));
            }
            reconcile(self.display);
        }
        pub(crate) fn on_view_clock(&self) -> bool {
            self.subscriber.view_clock.load(Ordering::Acquire)
        }
        /// How far into a refresh of the window's display now is, and the
        /// refresh period, on the display's refresh grid as the link last
        /// reported it (within `fresh`): refreshes start when the previous
        /// one shows. Read every frame, without a lock or a second clock: a
        /// period read beside a refresh reported after it is off only while
        /// the display changes mode.
        pub(crate) fn refresh_phase(&self, fresh: Duration) -> Option<(Duration, Duration)> {
            let grid = match &self.view_link {
                Some(view_link) if self.on_view_clock() => &view_link.grid,
                _ => &self.grid,
            };
            let refreshed = grid.refreshed.load(Ordering::Acquire);
            let period = grid.period.load(Ordering::Acquire);
            if refreshed == 0 || period == 0 {
                return None;
            }
            // SAFETY: reads the monotonic host clock.
            let host_now = unsafe { mach_absolute_time() };
            // The output time is a refresh or two ahead; the grid continues
            // back from it.
            let since = nanos(host_now as i128 - refreshed as i128);
            if since > fresh.as_nanos() as i128 {
                return None;
            }
            let phase = since.rem_euclid(period as i128) as u64;
            Some((Duration::from_nanos(phase), Duration::from_nanos(period)))
        }
        /// The UI thread took the last delivered refresh; deliver the next.
        pub(crate) fn consumed(&self) {
            self.subscriber.pending.store(false, Ordering::Release);
        }
        /// Deliver at most `hz` refreshes (whole-refresh spacing); `None`
        /// for every refresh.
        pub(crate) fn set_preferred_rate(&mut self, hz: Option<f64>) {
            let rate = hz
                .filter(|hz| hz.is_finite() && *hz > 0.)
                .map_or(0, |hz| (hz * 1000.) as u64);
            self.subscriber.max_rate.store(rate, Ordering::Relaxed);
        }
        /// Move the subscription when the window moves to another display.
        fn follow_display(&mut self) {
            let Some(display) = display_of(&self.view) else {
                return;
            };
            if display == self.display || !ensure_link(display) {
                return;
            }
            let old = self.display;
            {
                let mut guard = registry();
                let Some(registry) = guard.as_mut() else {
                    return;
                };
                if let Some(entry) = registry.displays.get_mut(&old) {
                    entry.subscribers.retain(|s| s.id != self.subscriber.id);
                }
                if let Some(entry) = registry.displays.get_mut(&display) {
                    entry.subscribers.push(self.subscriber.clone());
                    self.grid = entry.grid.clone();
                }
            }
            self.display = display;
            reconcile(old);
            reconcile(display);
        }
    }
    impl Drop for DisplayLink {
        fn drop(&mut self) {
            self.subscriber.active.store(false, Ordering::Release);
            self.view_link = None;
            if let Some(registry) = registry().as_mut()
                && let Some(entry) = registry.displays.get_mut(&self.display)
            {
                entry.subscribers.retain(|s| s.id != self.subscriber.id);
            }
            reconcile(self.display);
        }
    }

    /// The `CGDirectDisplayID` of the screen showing `view`'s window.
    fn display_of(view: &AnyObject) -> Option<u32> {
        // SAFETY: NSView -window, NSWindow -screen, NSScreen
        // -deviceDescription (each an object or nil, retained here),
        // NSDictionary -objectForKey: and NSNumber -unsignedIntValue (a
        // `unsigned int`), on the main thread (`DisplayLink`'s view).
        unsafe {
            let window: Option<Retained<AnyObject>> = msg_send![view, window];
            let screen: Option<Retained<AnyObject>> = msg_send![&*window?, screen];
            let description: Option<Retained<AnyObject>> = msg_send![&*screen?, deviceDescription];
            let key = objc2_foundation::NSString::from_str("NSScreenNumber");
            let number: Option<Retained<AnyObject>> =
                msg_send![&*description?, objectForKey: &*key];
            let id: u32 = msg_send![&*number?, unsignedIntValue];
            Some(id)
        }
    }
}

#[cfg(windows)]
mod compositor_clock {
    //! Compositor-clock pacing on Windows.
    //!
    //! Frames on a timer of their own drift against the display's refreshes,
    //! and with DXGI's queue a frame ahead the drift is hidden by blocking:
    //! the UI thread spent most of each animated refresh waiting in acquire,
    //! so a key typed meanwhile showed about three refreshes later (47 ms at
    //! 60 Hz, against 12 ms at rest). Frames started half a refresh after the
    //! compositor ticks render and present at once, a refresh apart, and
    //! leave the thread free for input in between; input draws at once (the
    //! swap chain composes the newest present), so a key shows on the next
    //! refresh, animating or not (12 ms).
    //!
    //! One thread waits on the compositor's clock (`DCompositionWaitFor
    //! CompositorClock`, Windows 11; on Windows 10 `DwmFlush`, the next
    //! composition) while any window wants frames, and parks otherwise.
    //! Windows subscribe with a thread-safe delivery function, as on macOS.
    use super::*;
    use std::sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE, HWND},
        Graphics::Dwm::{DWM_TIMING_INFO, DwmFlush, DwmGetCompositionTimingInfo},
        System::{
            LibraryLoader::{GetProcAddress, LoadLibraryW},
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
            Threading::{
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, GetCurrentThread,
                INFINITE, SetThreadPriority, SetWaitableTimer, THREAD_PRIORITY_HIGHEST,
                TIMER_ALL_ACCESS, WaitForSingleObject,
            },
        },
    };
    use winit::window::Window;

    /// The clock thread's sleeps, on one high-resolution waitable timer.
    /// `std::thread::sleep` makes a timer for every sleep: on an animating
    /// window's thread, two kernel objects and an allocation a refresh.
    struct Timer(Option<HANDLE>);
    impl Timer {
        fn new() -> Self {
            // SAFETY: creates an unnamed timer this value owns; fails before
            // Windows 10 1803, where sleeps fall back to the standard library.
            let handle = unsafe {
                CreateWaitableTimerExW(
                    None,
                    None,
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS.0,
                )
            };
            Self(handle.ok())
        }
        fn sleep(&self, duration: Duration) {
            let Some(timer) = self.0 else {
                std::thread::sleep(duration);
                return;
            };
            // Relative due times are negative, in 100 ns units.
            let due = -((duration.as_nanos() / 100).min(i64::MAX as u128) as i64).max(1);
            // SAFETY: arms the timer this value owns, once, then waits on it.
            unsafe {
                if SetWaitableTimer(timer, &due, 0, None, None, false).is_ok() {
                    WaitForSingleObject(timer, INFINITE);
                } else {
                    std::thread::sleep(duration);
                }
            }
        }
    }
    impl Drop for Timer {
        fn drop(&mut self) {
            if let Some(timer) = self.0 {
                // SAFETY: closes the handle this value owns.
                let _ = unsafe { CloseHandle(timer) };
            }
        }
    }

    /// One window's subscription, reachable from the clock's thread.
    struct Subscriber {
        id: u64,
        /// Delivering: the window wants refreshes.
        active: AtomicBool,
        /// A delivered refresh has not been consumed yet; later ones coalesce.
        pending: AtomicBool,
        /// Deliver at most this often (millihertz; 0 for every refresh).
        max_rate: AtomicU64,
        deliver: Box<dyn Fn(Frame) + Send + Sync>,
    }
    #[derive(Default)]
    struct Clock {
        subscribers: Vec<Arc<Subscriber>>,
        next_id: u64,
        started: bool,
    }
    static CLOCK: Mutex<Option<Clock>> = Mutex::new(None);
    /// Wakes the parked thread when a subscriber starts.
    static WAKE: Condvar = Condvar::new();
    fn clock() -> MutexGuard<'static, Option<Clock>> {
        CLOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn wants_frames(clock: &Option<Clock>) -> bool {
        clock.as_ref().is_some_and(|c| {
            c.subscribers
                .iter()
                .any(|s| s.active.load(Ordering::Acquire))
        })
    }

    type WaitForCompositorClock = unsafe extern "system" fn(u32, *const HANDLE, u32) -> u32;
    /// `DCompositionWaitForCompositorClock`, looked up at run time: Windows 10
    /// has no compositor clock, and a static import would stop the program
    /// loading there.
    fn compositor_clock() -> Option<WaitForCompositorClock> {
        static WAIT: OnceLock<Option<WaitForCompositorClock>> = OnceLock::new();
        *WAIT.get_or_init(|| {
            // SAFETY: loads a system library by name and looks up an export;
            // the library's reference is never released, so the export stays
            // loaded for the process. It has this signature (dcomp.h:
            // `STDAPI_(DWORD) DCompositionWaitForCompositorClock(UINT count,
            // const HANDLE* handles, DWORD timeoutInMs)`).
            unsafe {
                let library = LoadLibraryW(windows::core::w!("dcomp.dll")).ok()?;
                GetProcAddress(
                    library,
                    windows::core::s!("DCompositionWaitForCompositorClock"),
                )
                .map(|f| {
                    std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        WaitForCompositorClock,
                    >(f)
                })
            }
        })
    }

    /// The compositor's last refresh and the refresh period, in
    /// performance-counter ticks. (Its refresh count is no count of the
    /// display's refreshes: some displays compose only on demand, and it
    /// counts compositions there.)
    fn timing() -> Option<(i64, i64)> {
        let mut info = DWM_TIMING_INFO {
            cbSize: size_of::<DWM_TIMING_INFO>() as u32,
            ..Default::default()
        };
        // SAFETY: fills `info`, whose size it is given; a null window asks
        // for the compositor's own timing.
        unsafe { DwmGetCompositionTimingInfo(HWND::default(), &mut info) }.ok()?;
        let period = info.qpcRefreshPeriod as i64;
        (period > 0).then_some((info.qpcVBlank as i64, period))
    }
    fn qpc() -> (i64, i64) {
        let (mut now, mut frequency) = (0, 0);
        // SAFETY: each writes one integer into a local.
        unsafe {
            let _ = QueryPerformanceCounter(&mut now);
            let _ = QueryPerformanceFrequency(&mut frequency);
        }
        (now, frequency.max(1))
    }

    /// Where the compositor's refresh is now: the time since its last
    /// vblank, and the refresh period.
    pub(crate) fn refresh_phase() -> Option<(Duration, Duration)> {
        let (vblank, period) = timing()?;
        let (now, frequency) = qpc();
        let time = |ticks: i64| {
            Duration::from_nanos((ticks as i128 * 1_000_000_000 / frequency as i128) as u64)
        };
        Some((time((now - vblank).rem_euclid(period)), time(period)))
    }

    /// Sleep on a high-resolution timer of this thread's: a millisecond's
    /// wait from `std::thread::sleep` ends up to a timer tick (15.6 ms) late.
    pub(crate) fn sleep_precisely(duration: Duration) {
        thread_local! {
            static TIMER: Timer = Timer::new();
        }
        TIMER.with(|timer| timer.sleep(duration));
    }

    /// Blocks until the compositor's next frame.
    fn wait_for_tick(timer: &Timer) {
        if let Some(wait) = compositor_clock() {
            // SAFETY: no handles to wait on besides the clock (a count of 0
            // with a null array), for at most 100 ms.
            unsafe { wait(0, std::ptr::null(), 100) };
            return;
        }
        let started = Instant::now();
        // SAFETY: no arguments; blocks until the next composition.
        let flushed = unsafe { DwmFlush() }.is_ok();
        // With nothing to compose, DwmFlush can return at once: wait for the
        // next refresh by the clock instead, rather than spin.
        if !flushed || started.elapsed() < Duration::from_millis(1) {
            let wait = timing().map_or(DEFAULT_INTERVAL, |(vblank, period)| {
                let (now, frequency) = qpc();
                let ahead = period - (now - vblank).rem_euclid(period);
                Duration::from_nanos((ahead as u128 * 1_000_000_000 / frequency as u128) as u64)
            });
            timer.sleep(wait);
        }
    }

    fn run() {
        // Delivery is a message post; a prompt wake keeps it at the tick.
        // SAFETY: changes this thread's own priority.
        let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) };
        let timer = Timer::new();
        // Refilled each refresh, so delivery allocates nothing.
        let mut subscribers = Vec::new();
        loop {
            {
                let mut guard = clock();
                while !wants_frames(&guard) {
                    guard = WAKE.wait(guard).unwrap_or_else(PoisonError::into_inner);
                }
            }
            wait_for_tick(&timer);
            let Some((vblank, period)) = timing() else {
                continue;
            };
            // The tick is a refresh starting, on the grid of refreshes from
            // the last one the compositor reported: the nearest, since the
            // clock wakes a little either side of it (rounding down instead
            // let indices skip or repeat, and a 30 Hz cap counting every
            // other index then kept every fourth refresh). The frame shows
            // at the next. Its index counts the grid's refreshes since the
            // counter started.
            let (now, frequency) = qpc();
            let phase = vblank.rem_euclid(period);
            let ticked = ((now - phase) as f64 / period as f64).round() as i64;
            let index = (ticked + 1) as u64;
            let nanos = |ticks: i64| (ticks as u128 * 1_000_000_000 / frequency as u128) as u64;
            // Delivered half a refresh on: a frame presented just after the
            // tick meets the compositor taking the last one, and by a hair
            // either way two presents share a refresh (the newest replaces
            // the other: 7% of an animation's frames never showed) or none
            // does. Mid-refresh, presents are a refresh apart, each taken at
            // the next, half a refresh sooner than from the tick.
            let middle = phase + ticked * period + period / 2;
            if middle > now {
                timer.sleep(Duration::from_nanos(nanos(middle - now)));
            }
            let now = qpc().0;
            let ahead = (phase + (ticked + 1) * period - now).max(0);
            let interval = Duration::from_nanos(nanos(period));
            let frame = Frame {
                index,
                time: Instant::now() + Duration::from_nanos(nanos(ahead)),
                interval,
            };
            let rate = 1. / interval.as_secs_f64();
            subscribers.clear();
            if let Some(clock) = clock().as_ref() {
                subscribers.extend(clock.subscribers.iter().cloned());
            }
            for subscriber in subscribers.drain(..) {
                if !subscriber.active.load(Ordering::Acquire) {
                    continue;
                }
                let max_rate = subscriber.max_rate.load(Ordering::Relaxed);
                if max_rate > 0 {
                    let divisor = zgx::frame::divisor(rate, max_rate as f64 / 1000.);
                    if !index.is_multiple_of(divisor) {
                        continue;
                    }
                }
                if !subscriber.pending.swap(true, Ordering::AcqRel) {
                    (subscriber.deliver)(frame);
                }
            }
        }
    }

    /// A window's subscription to the compositor's refreshes.
    pub(crate) struct DisplayLink {
        subscriber: Arc<Subscriber>,
    }
    impl DisplayLink {
        /// `deliver` runs on the clock's thread, at most once per consumed
        /// refresh; hosts forward it to the UI thread.
        pub(crate) fn new(
            _window: &Window,
            deliver: impl Fn(Frame) + Send + Sync + 'static,
        ) -> Option<Self> {
            let mut guard = clock();
            let clock = guard.get_or_insert_with(Clock::default);
            if !clock.started {
                std::thread::Builder::new()
                    .name("zgx-compositor-clock".into())
                    .spawn(run)
                    .ok()?;
                clock.started = true;
            }
            clock.next_id += 1;
            let subscriber = Arc::new(Subscriber {
                id: clock.next_id,
                active: AtomicBool::new(false),
                pending: AtomicBool::new(false),
                max_rate: AtomicU64::new(0),
                deliver: Box::new(deliver),
            });
            clock.subscribers.push(subscriber.clone());
            Some(Self { subscriber })
        }
        pub(crate) fn running(&self) -> bool {
            self.subscriber.active.load(Ordering::Acquire)
        }
        pub(crate) fn set_running(&mut self, running: bool) {
            if self.subscriber.active.swap(running, Ordering::AcqRel) != running {
                self.subscriber.pending.store(false, Ordering::Release);
                if running {
                    // Under the lock, so the thread cannot miss it between
                    // checking and parking.
                    let _guard = clock();
                    WAKE.notify_all();
                }
            }
        }
        /// The UI thread took the last delivered refresh; deliver the next.
        pub(crate) fn consumed(&self) {
            self.subscriber.pending.store(false, Ordering::Release);
        }
        /// Deliver at most `hz` refreshes (whole-refresh spacing); `None`
        /// for every refresh.
        pub(crate) fn set_preferred_rate(&mut self, hz: Option<f64>) {
            let rate = hz
                .filter(|hz| hz.is_finite() && *hz > 0.)
                .map_or(0, |hz| (hz * 1000.) as u64);
            self.subscriber.max_rate.store(rate, Ordering::Relaxed);
        }
    }
    impl Drop for DisplayLink {
        fn drop(&mut self) {
            self.subscriber.active.store(false, Ordering::Release);
            if let Some(clock) = clock().as_mut() {
                clock.subscribers.retain(|s| s.id != self.subscriber.id);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The clock's sleeps last what they are given, each on the one
        /// high-resolution timer, re-armed.
        #[test]
        fn the_clock_timer_sleeps_as_long_as_asked_and_no_more() {
            let timer = Timer::new();
            assert!(
                timer.0.is_some(),
                "high-resolution timers: Windows 10 1803+"
            );
            for millis in [1, 2, 5, 8] {
                let asked = Duration::from_millis(millis);
                let started = Instant::now();
                timer.sleep(asked);
                let slept = started.elapsed();
                assert!(slept >= asked, "{slept:?} for {asked:?}");
                // Generous for a loaded CI runner; a coarse 15.6 ms tick
                // would show at 1 ms.
                assert!(
                    slept < asked + Duration::from_millis(12),
                    "{slept:?} for {asked:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFRESH: Duration = Duration::from_nanos(16_666_667);
    fn ms(ms: f64) -> Duration {
        Duration::from_secs_f64(ms / 1000.)
    }

    #[test]
    fn a_frame_clear_of_the_latch_presents_at_once() {
        for phase in [1.5, 4., 8.3, 12.] {
            assert_eq!(latch_wait(ms(phase), REFRESH, false), None, "at {phase} ms");
        }
    }

    #[test]
    fn a_frame_near_the_latch_waits_until_just_past_the_vblank() {
        // 2 ms before the vblank: until 1 ms after it.
        let wait = latch_wait(REFRESH - ms(2.), REFRESH, false).unwrap();
        assert_eq!(wait, ms(2.) + LATCH_AFTER);
        // Just after the vblank: the rest of LATCH_AFTER.
        assert_eq!(
            latch_wait(ms(0.3), REFRESH, false),
            Some(LATCH_AFTER - ms(0.3))
        );
    }

    #[test]
    fn holding_widens_the_window_so_frames_at_its_edge_do_not_alternate() {
        // 4 ms before the vblank: outside the window, inside it while holding.
        let phase = REFRESH - ms(4.);
        assert_eq!(latch_wait(phase, REFRESH, false), None);
        assert_eq!(latch_wait(phase, REFRESH, true), Some(ms(4.) + LATCH_AFTER));
        // Further out, holding no longer applies.
        assert_eq!(latch_wait(REFRESH - ms(5.), REFRESH, true), None);
    }

    #[test]
    fn a_period_too_short_for_the_window_never_waits() {
        // 240 Hz: 4.2 ms, no room for the window and a frame between.
        let fast = Duration::from_nanos(4_166_667);
        assert_eq!(latch_wait(ms(3.5), fast, false), None);
    }

    #[test]
    fn the_refresh_period_is_what_the_link_delivers_not_what_the_mode_says() {
        let (fast, slow) = (8_333_333_u64, 16_666_667_u64);
        let nominal = Duration::from_nanos(slow);
        let mut spacings = [0; PERIOD_SAMPLES];
        // Until three spacings are known, the link's nominal period.
        for at in 0..2 {
            spacings[at] = fast;
            assert_eq!(refresh_period(nominal, &spacings), nominal);
        }
        // A 60 Hz mode read on a panel refreshing at 120 Hz: the panel's.
        spacings[2] = fast;
        assert_eq!(
            refresh_period(nominal, &spacings),
            Duration::from_nanos(fast)
        );
        // A missed callback (two refreshes apart) changes nothing.
        spacings[3] = 2 * fast;
        assert_eq!(
            refresh_period(nominal, &spacings),
            Duration::from_nanos(fast)
        );
        // Spacings no refresh has (a pause, a stalled callback) are ignored.
        let paused = [0, 900_000_000, 1_000, 0, 0, 0, 0, 0];
        assert_eq!(refresh_period(nominal, &paused), nominal);
        // A display that switches to 60 Hz is followed once its ring turns over.
        let mut switched = [fast; PERIOD_SAMPLES];
        for at in 0..PERIOD_SAMPLES {
            switched[at] = slow;
            let period = refresh_period(Duration::from_nanos(slow), &switched);
            let expect = if at + 1 == PERIOD_SAMPLES { slow } else { fast };
            assert_eq!(period, Duration::from_nanos(expect), "{at}");
        }
    }

    #[test]
    fn timer_ticks_on_a_fixed_grid_after_late_wakeups() {
        let epoch = Instant::now();
        let timer = FrameTimer::new(epoch);
        let interval = Duration::from_millis(10);
        assert_eq!(timer.next_deadline(epoch, interval), epoch + interval);
        // Waking 3 ms late still schedules the following boundary on the grid.
        let late = epoch + Duration::from_millis(13);
        assert_eq!(timer.next_deadline(late, interval), epoch + interval * 2);
        let frame = timer.frame(late, interval);
        assert_eq!(frame.index, 1);
        assert_eq!(frame.time, epoch + interval * 2);
        assert_eq!(frame.interval, interval);
    }

    /// A timer continuing the display link's refreshes, paced to every
    /// second one, ticks on the refreshes the link would deliver (even ones)
    /// even where its own grid starts on an odd one, and late wake-ups never
    /// make it skip one.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_paced_timer_ticks_on_the_refreshes_the_link_delivers() {
        let epoch = Instant::now();
        let interval = Duration::from_millis(8);
        let timer = FrameTimer::aligned(epoch, 101);
        // From refresh 101, the next even one is 102, a refresh on.
        assert_eq!(
            timer.next_deadline_every(epoch, interval, 2),
            epoch + interval
        );
        let late = epoch + interval + Duration::from_millis(7);
        assert_eq!(
            timer.next_deadline_every(late, interval, 2),
            epoch + interval * 3
        );
        // Every fourth: 104, three refreshes after 101.
        assert_eq!(
            timer.next_deadline_every(epoch, interval, 4),
            epoch + interval * 3
        );
        assert_eq!(
            timer.next_deadline_every(epoch, interval, 1),
            epoch + interval
        );
    }
    #[test]
    fn missing_or_bogus_refresh_rates_fall_back_to_60_hz() {
        assert_eq!(interval_for(None), DEFAULT_INTERVAL);
        assert_eq!(interval_for(Some(0.)), DEFAULT_INTERVAL);
        assert_eq!(interval_for(Some(f64::NAN)), DEFAULT_INTERVAL);
        assert_eq!(interval_for(Some(120.)), Duration::from_secs_f64(1. / 120.));
    }
}
