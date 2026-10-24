//! Native display refresh sources for [`zgui::frame::FrameClock`].
//!
//! macOS uses a `CVDisplayLink` per display (see `display_link`), which
//! windows follow as they move between displays. Windows waits on the
//! compositor's clock (see `compositor_clock`). Wayland paces through compositor frame
//! callbacks (driven from the host's redraw requests). Everything else, and
//! any platform whose native source is unavailable, uses [`FrameTimer`]:
//! event-loop deadlines on a fixed grid at the display's reported refresh rate.
use std::time::{Duration, Instant};
use zgui::frame::Frame;

pub(crate) const DEFAULT_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// A native source that stops ticking while frames are wanted is backed up by
/// the timer after this long, so animations degrade instead of freezing.
#[cfg(any(target_os = "macos", windows))]
pub(crate) const WATCHDOG: Duration = Duration::from_millis(250);

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
    /// (`zgui::frame::divisor`), so frames the timer paces keep its cadence
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
pub(crate) use compositor_clock::DisplayLink;
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
    //! `CVDisplayLinkStop` returns before the link's thread has necessarily
    //! finished its last callback, and releasing a link or anything its
    //! callback reads races that callback. So, as in GPUI: one link per
    //! display, created on first use and never released; its callback context
    //! is the display id, not a pointer, and it reads only the static registry.
    //! Windows subscribe with a thread-safe delivery function. Links start and
    //! stop only on the main thread and never while the registry lock is held,
    //! since the callback takes that lock under CoreVideo's own locks.
    use super::*;
    use objc2::{msg_send, rc::Retained, runtime::AnyObject};
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
        deliver: Box<dyn Fn(Frame) + Send + Sync>,
    }
    struct Entry {
        /// Immortal: see the module comment.
        link: usize,
        running: bool,
        subscribers: Vec<Arc<Subscriber>>,
        /// The refresh the link last reported starting (host time), and the
        /// display's refresh period (ns): its refresh grid.
        refreshed: AtomicU64,
        period: AtomicU64,
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
        let interval = Duration::from_secs_f64(period / scale);
        let index = (output.video_time as f64 / period).round().max(0.) as u64;
        // SAFETY: reads the monotonic host clock.
        let host_now = unsafe { mach_absolute_time() };
        let ahead = nanos(output.host_time as i128 - host_now as i128).clamp(0, 1_000_000_000);
        let frame = Frame {
            index,
            time: Instant::now() + Duration::from_nanos(ahead as u64),
            interval,
        };
        let refresh = scale / period;
        let guard = registry();
        let Some(entry) = guard.as_ref().and_then(|r| r.displays.get(&display)) else {
            return 0;
        };
        // The output time: exactly on the display's refresh grid (the
        // callback's `now`, when it runs, wanders by up to 2 ms).
        entry.refreshed.store(output.host_time, Ordering::Release);
        entry
            .period
            .store(interval.as_nanos() as u64, Ordering::Release);
        for subscriber in &entry.subscribers {
            if !subscriber.active.load(Ordering::Acquire) {
                continue;
            }
            let max_rate = subscriber.max_rate.load(Ordering::Relaxed);
            if max_rate > 0 {
                let divisor = zgui::frame::divisor(refresh, max_rate as f64 / 1000.);
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
                .any(|s| s.active.load(Ordering::Acquire));
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
            refreshed: AtomicU64::new(0),
            period: AtomicU64::new(0),
        });
        true
    }

    /// A window's subscription to its display's refreshes.
    pub(crate) struct DisplayLink {
        view: Retained<AnyObject>,
        display: u32,
        subscriber: Arc<Subscriber>,
        /// When the window's display was last checked (an AppKit round trip).
        checked: Instant,
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
            let subscriber = {
                let mut guard = registry();
                let registry = guard.as_mut()?;
                registry.next_id += 1;
                let subscriber = Arc::new(Subscriber {
                    id: registry.next_id,
                    active: AtomicBool::new(false),
                    pending: AtomicBool::new(false),
                    max_rate: AtomicU64::new(0),
                    deliver: Box::new(deliver),
                });
                registry
                    .displays
                    .get_mut(&display)?
                    .subscribers
                    .push(subscriber.clone());
                subscriber
            };
            Some(Self {
                view,
                display,
                subscriber,
                checked: Instant::now(),
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
                reconcile(self.display);
            }
        }
        /// How far into a refresh of the window's display `at` falls, and the
        /// refresh period, on the display's refresh grid as the link last
        /// reported it (within `fresh`): refreshes start when the previous
        /// one shows.
        pub(crate) fn refresh_phase(
            &self,
            at: Instant,
            fresh: Duration,
        ) -> Option<(Duration, Duration)> {
            let (refreshed, period) = {
                let guard = registry();
                let entry = guard.as_ref()?.displays.get(&self.display)?;
                (
                    entry.refreshed.load(Ordering::Acquire),
                    entry.period.load(Ordering::Acquire),
                )
            };
            if refreshed == 0 || period == 0 {
                return None;
            }
            // SAFETY: reads the monotonic host clock.
            let host_now = unsafe { mach_absolute_time() };
            // The output time is a refresh or two ahead; the grid continues
            // back from it.
            let since = nanos(host_now as i128 - refreshed as i128)
                + at.saturating_duration_since(Instant::now()).as_nanos() as i128;
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
                    let divisor = zgui::frame::divisor(rate, max_rate as f64 / 1000.);
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
                    .name("zgui-compositor-clock".into())
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
