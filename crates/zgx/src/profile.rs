//! What each phase of a frame costs, with `ZGX_PROFILE=1`.
//!
//! The engine marks its phases with [`span`]. With `ZGX_PROFILE` unset a
//! span is one relaxed load and a branch, and nothing else happens. Set, each
//! top-level phase (updating, layout, damage, the paint walk, encoding,
//! submitting, presenting, waiting for the display) records the instructions it retired and its time,
//! excluding the phases it encloses. Built with the `profile-nested` feature,
//! nested phases (styles inside an update, text inside layout) record their
//! time too; their instructions stay their enclosing phase's. Every [`REPORT_FRAMES`] frames (and at [`report`]) one line per
//! phase goes to standard error:
//!
//! ```text
//! profile 120 frames: update 182.4k instr 41.2 us | style 30.1 us | layout ...
//! ```
//!
//! Instructions are the calling thread's (macOS `thread_selfcounts`, Linux
//! `perf_event_open`); elsewhere only time is kept. The counter's own cost is measured once and taken off.
//! On macOS each top-level span is also an `os_signpost` interval
//! (subsystem `dev.zgx`, category `phases`), so Instruments' os_signpost
//! track shows the phases on the timeline.
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU8, Ordering},
    time::{Duration, Instant},
};

/// A part of a frame's work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Dispatching input: hit testing, handlers, default actions.
    Input,
    /// A script running (Luau, in `zgx-script`), wherever it is called
    /// from: what a handler, effect or computed spends in the script itself.
    Script,
    /// Tasks and reactive effects: what the app changed.
    Update,
    /// Style effects re-running (within an update).
    Style,
    /// Laying out the scene.
    Layout,
    /// Measuring and shaping text (within layout).
    Text,
    /// Finding what changed on screen.
    Damage,
    /// Walking the scene into draw lists.
    Paint,
    /// Encoding GPU commands.
    Encode,
    /// Handing commands to the GPU.
    Submit,
    /// Showing the frame.
    Present,
    /// Waiting for the display: a frame held to render or hand over at its
    /// point in a refresh (macOS's pacing). Time, not work.
    Wait,
}

/// Every phase, in report order.
pub const PHASES: [Phase; 12] = [
    Phase::Input,
    Phase::Script,
    Phase::Update,
    Phase::Style,
    Phase::Layout,
    Phase::Text,
    Phase::Damage,
    Phase::Paint,
    Phase::Encode,
    Phase::Submit,
    Phase::Present,
    Phase::Wait,
];

/// Frames between reports.
pub const REPORT_FRAMES: u64 = 120;

impl Phase {
    /// Its name in reports and signposts.
    pub fn name(self) -> &'static str {
        match self {
            Phase::Input => "input",
            Phase::Script => "script",
            Phase::Update => "update",
            Phase::Style => "style",
            Phase::Layout => "layout",
            Phase::Text => "text",
            Phase::Damage => "damage",
            Phase::Paint => "paint",
            Phase::Encode => "encode",
            Phase::Submit => "submit",
            Phase::Present => "present",
            Phase::Wait => "wait",
        }
    }
    /// Phases that run many times a frame inside another: timed, but not
    /// counted (a count costs a system call).
    fn nested(self) -> bool {
        matches!(self, Phase::Style | Phase::Text)
    }
}

/// 0: not yet read, 1: off, 2: on (`ZGX_PROFILE`, reported on stderr),
/// 3: recording for devtools alone ([`set_recording`], not reported).
static STATE: AtomicU8 = AtomicU8::new(0);

/// Whether phases are measured: `ZGX_PROFILE` is set (to anything but
/// `0`), or devtools are recording ([`set_recording`]).
#[inline]
pub fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 | 3 => true,
        _ => read_state(),
    }
}

/// Whether what is measured is reported (`ZGX_PROFILE`): on stderr,
/// with marks and signposts. Not while devtools record alone.
#[inline]
fn reported() -> bool {
    STATE.load(Ordering::Relaxed) == 2
}

/// Measure phases for devtools (`zgx inspect frames`, each frame's split),
/// without `ZGX_PROFILE`'s reports on stderr; `false` stops (unless
/// `ZGX_PROFILE` is set). Off, a span is one relaxed load and a branch.
pub fn set_recording(on: bool) {
    let reported = match STATE.load(Ordering::Relaxed) {
        0 => read_state(),
        state => state == 2,
    };
    if !reported {
        STATE.store(if on { 3 } else { 1 }, Ordering::Relaxed);
    }
}

#[cold]
fn read_state() -> bool {
    let on = crate::env::var_os("ZGX_PROFILE").is_some_and(|value| value != "0");
    STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
    on
}

/// Marks a phase until it drops.
#[must_use = "a span measures until it drops"]
pub struct Span {
    phase: Option<Phase>,
}

/// Start measuring `phase`; it ends when the returned span drops.
#[inline]
pub fn span(phase: Phase) -> Span {
    if !enabled() {
        return Span { phase: None };
    }
    enter(phase);
    Span { phase: Some(phase) }
}

impl Drop for Span {
    #[inline]
    fn drop(&mut self) {
        if let Some(phase) = self.phase {
            leave(phase);
        }
    }
}

/// Start measuring a phase that runs many times inside another (`Style`,
/// `Text`). Only builds with the `profile-nested` feature measure these: in
/// others this is nothing at all, not even the check [`span`] makes.
#[inline(always)]
pub fn nested(phase: Phase) -> Span {
    #[cfg(feature = "profile-nested")]
    {
        span(phase)
    }
    #[cfg(not(feature = "profile-nested"))]
    {
        let _ = phase;
        Span { phase: None }
    }
}

/// One frame is done: counts toward the next report.
#[inline]
pub fn frame() {
    if enabled() {
        end_frame();
    }
}

/// A point in starting up: with `ZGX_PROFILE` set, prints the process's
/// instructions retired and time since it started (macOS; elsewhere the
/// thread's, and time since the first mark), as
/// `profile mark 412.3M instr 81.2 ms: window created`. Marks sit at a
/// start's milestones (window, renderer, fonts, the app's build, the first
/// frame), so what each step costs is the difference between two lines.
pub fn mark(what: &str) {
    if enabled() && reported() {
        mark_now(what);
    }
}

#[cold]
fn mark_now(what: &str) {
    let (instructions, ms) = counter::since_start();
    eprintln!(
        "profile mark {:.1}M instr {ms:.1} ms: {what}",
        instructions as f64 / 1e6
    );
}

/// What each phase cost since the last report (or take), its instructions
/// and time, and start counting again: a benchmark's own split of each
/// interaction. Empty with `ZGX_PROFILE` unset.
pub fn take() -> Vec<(Phase, u64, Duration)> {
    if !enabled() || !reported() {
        return Vec::new();
    }
    PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        let taken = PHASES
            .iter()
            .zip(profile.totals)
            .map(|(phase, totals)| (*phase, totals.instructions, totals.time))
            .collect();
        profile.totals = Default::default();
        profile.frames = 0;
        taken
    })
}

/// Report what was measured since the last report now (a benchmark's end).
pub fn report() {
    if enabled() && reported() {
        PROFILE.with(|profile| profile.borrow_mut().report());
    }
}

#[derive(Default, Clone, Copy)]
struct Totals {
    instructions: u64,
    time: Duration,
}

struct Profile {
    counter: counter::Counter,
    /// What a counter read costs, taken off each counted interval.
    overhead: u64,
    /// Open spans: phase, when it (or its child last) resumed, count then.
    open: Vec<(Phase, Instant, u64)>,
    totals: [Totals; PHASES.len()],
    /// This frame's, for devtools' traces.
    frame: [Totals; PHASES.len()],
    frames: u64,
}

thread_local! {
    static PROFILE: RefCell<Profile> = RefCell::new(Profile::new());
}

impl Profile {
    fn new() -> Self {
        let counter = counter::Counter::new();
        let overhead = (0..8)
            .map(|_| {
                let before = counter.read();
                counter.read().saturating_sub(before)
            })
            .min()
            .unwrap_or(0);
        Self {
            counter,
            overhead,
            open: Vec::new(),
            totals: Default::default(),
            frame: Default::default(),
            frames: 0,
        }
    }
    /// The count now, or the last open counted span's when `phase` is nested
    /// (nested spans don't read the counter).
    fn now(&self, phase: Phase) -> u64 {
        if phase.nested() {
            self.open.last().map_or(0, |open| open.2)
        } else {
            self.counter.read()
        }
    }
    /// Credit the open span with what ran since it last resumed.
    fn credit(&mut self, now: Instant, count: u64, counted: bool) {
        if let Some((phase, since, from)) = self.open.last().copied() {
            let time = now - since;
            let instructions = match counted && !phase.nested() {
                true => count.saturating_sub(from + self.overhead),
                false => 0,
            };
            let frame = &mut self.frame[phase as usize];
            frame.time += time;
            frame.instructions += instructions;
            // What is reported, with `ZGX_PROFILE` alone.
            if reported() {
                let totals = &mut self.totals[phase as usize];
                totals.time += time;
                totals.instructions += instructions;
            }
        }
    }
    fn report(&mut self) {
        if self.frames == 0 && self.totals.iter().all(|t| t.time.is_zero()) {
            return;
        }
        let frames = self.frames.max(1);
        let mut line = format!("profile {} frames:", self.frames);
        for (i, phase) in PHASES.iter().enumerate() {
            let totals = self.totals[i];
            if totals.time.is_zero() {
                continue;
            }
            let micros = totals.time.as_secs_f64() * 1e6 / frames as f64;
            if phase.nested() {
                line += &format!(" | {} {micros:.1} us", phase.name());
            } else {
                let thousands = totals.instructions as f64 / 1e3 / frames as f64;
                line += &format!(" | {} {thousands:.1}k instr {micros:.1} us", phase.name());
            }
        }
        eprintln!("{line}");
        self.totals = Default::default();
        self.frames = 0;
    }
}

#[cold]
fn enter(phase: Phase) {
    // Nested phases run hundreds of times a frame: too many intervals to
    // read, and each signpost costs.
    if !phase.nested() && reported() {
        signpost::begin(phase);
    }
    PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        let counted = !phase.nested();
        let count = profile.now(phase);
        let now = Instant::now();
        profile.credit(now, count, counted);
        profile.open.push((phase, now, count));
    });
}

#[cold]
fn leave(phase: Phase) {
    PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        let counted = !phase.nested();
        let count = profile.now(phase);
        let now = Instant::now();
        profile.credit(now, count, counted);
        profile.open.pop();
        // The parent resumes now, and counts on from here when this counted
        // (a nested span's instructions stay its counted parent's).
        if let Some(open) = profile.open.last_mut() {
            open.1 = now;
            if counted {
                open.2 = count;
            }
        }
    });
    if !phase.nested() && reported() {
        signpost::end(phase);
    }
}

#[cold]
fn end_frame() {
    PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        profile.frames += 1;
        // This frame's split, for devtools' trace of it.
        let frame = std::mem::take(&mut profile.frame);
        #[cfg(feature = "trace")]
        if crate::trace::enabled() {
            let phases = PHASES
                .iter()
                .zip(frame)
                .filter(|(_, t)| !t.time.is_zero())
                .map(|(phase, t)| (phase.name(), t.instructions, t.time.as_secs_f32() * 1e6))
                .collect();
            crate::trace::end_frame(phases);
        }
        #[cfg(not(feature = "trace"))]
        let _ = frame;
        // Reported on stderr with `ZGX_PROFILE` alone, not for devtools.
        if profile.frames >= REPORT_FRAMES && reported() {
            profile.report();
        }
    });
}

/// Instructions retired so far.
mod counter {
    #[cfg(target_os = "macos")]
    pub struct Counter {
        /// Whether the kernel counts this thread's instructions
        /// (`thread_selfcounts`); if not, the process's are read.
        thread: bool,
    }
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        /// libsystem_kernel's per-thread counters; kind 1 (THSC_CPI) fills
        /// instructions then cycles.
        fn thread_selfcounts(kind: i32, buffer: *mut u64, size: usize) -> i32;
    }
    #[cfg(target_os = "macos")]
    impl Counter {
        pub fn new() -> Self {
            let mut counts = [0u64; 2];
            // SAFETY: Two u64s for THSC_CPI.
            let thread = unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) } == 0;
            Self { thread }
        }
        /// The calling thread's retired instructions (else the process's).
        pub fn read(&self) -> u64 {
            if self.thread {
                let mut counts = [0u64; 2];
                // SAFETY: As in `new`.
                unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) };
                return counts[0];
            }
            #[repr(C)]
            struct RusageInfoV4 {
                uuid: [u8; 16],
                fields: [u64; 40],
            }
            unsafe extern "C" {
                fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV4) -> i32;
            }
            let mut info = RusageInfoV4 {
                uuid: [0; 16],
                fields: [0; 40],
            };
            // SAFETY: RUSAGE_INFO_V4 (4) fills a rusage_info_v4: a 16-byte
            // UUID then 40 u64 fields, of which ri_instructions is the 30th.
            let ok = unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut info) } == 0;
            if ok { info.fields[29] } else { 0 }
        }
    }
    /// The process's (every thread) instructions so far and milliseconds
    /// since it started.
    #[cfg(target_os = "macos")]
    pub fn since_start() -> (u64, f64) {
        #[repr(C)]
        struct RusageInfoV4 {
            uuid: [u8; 16],
            fields: [u64; 40],
        }
        #[repr(C)]
        struct Timebase {
            numer: u32,
            denom: u32,
        }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV4) -> i32;
            fn mach_absolute_time() -> u64;
            fn mach_timebase_info(info: *mut Timebase) -> i32;
        }
        let mut info = RusageInfoV4 {
            uuid: [0; 16],
            fields: [0; 40],
        };
        let mut base = Timebase { numer: 1, denom: 1 };
        // SAFETY: As in `read`; the timebase fills two u32s. Field 8 is
        // ri_proc_start_abstime, in mach_absolute_time's units.
        let (now, ok) = unsafe {
            mach_timebase_info(&mut base);
            let ok = proc_pid_rusage(std::process::id() as i32, 4, &mut info) == 0;
            (mach_absolute_time(), ok)
        };
        if !ok {
            return (0, 0.);
        }
        let ticks = now.saturating_sub(info.fields[8]) as f64;
        (
            info.fields[29],
            ticks * base.numer as f64 / base.denom.max(1) as f64 / 1e6,
        )
    }

    #[cfg(target_os = "linux")]
    pub struct Counter {
        fd: i32,
    }
    #[cfg(target_os = "linux")]
    impl Counter {
        pub fn new() -> Self {
            /// `struct perf_event_attr` up to the fields set here (its
            /// first published size, PERF_ATTR_SIZE_VER0).
            #[repr(C)]
            struct Attr {
                kind: u32,
                size: u32,
                config: u64,
                sample: u64,
                sample_type: u64,
                read_format: u64,
                flags: u64,
                wakeup: u32,
                bp_type: u32,
                bp: u64,
            }
            unsafe extern "C" {
                fn syscall(number: i64, ...) -> i64;
            }
            #[cfg(target_arch = "x86_64")]
            const PERF_EVENT_OPEN: i64 = 298;
            #[cfg(target_arch = "aarch64")]
            const PERF_EVENT_OPEN: i64 = 241;
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            const PERF_EVENT_OPEN: i64 = -1;
            let attr = Attr {
                kind: 0,   // PERF_TYPE_HARDWARE
                size: 64,  // PERF_ATTR_SIZE_VER0
                config: 1, // PERF_COUNT_HW_INSTRUCTIONS
                sample: 0,
                sample_type: 0,
                read_format: 0,
                // exclude_kernel (bit 5) and exclude_hv (bit 6): user code.
                flags: (1 << 5) | (1 << 6),
                wakeup: 0,
                bp_type: 0,
                bp: 0,
            };
            // SAFETY: perf_event_open(attr, this thread, any CPU, no group,
            // no flags); a failure (-1) leaves the counter reading 0.
            let fd = if PERF_EVENT_OPEN < 0 {
                -1
            } else {
                (unsafe {
                    syscall(
                        PERF_EVENT_OPEN,
                        &attr as *const Attr,
                        0i64,
                        -1i64,
                        -1i64,
                        0u64,
                    )
                }) as i32
            };
            Self { fd }
        }
        /// The calling thread's retired user instructions.
        pub fn read(&self) -> u64 {
            unsafe extern "C" {
                fn read(fd: i32, buffer: *mut std::ffi::c_void, count: usize) -> isize;
            }
            if self.fd < 0 {
                return 0;
            }
            let mut value = 0u64;
            // SAFETY: A counting perf event reads as one u64.
            unsafe { read(self.fd, (&raw mut value).cast(), 8) };
            value
        }
    }

    /// This thread's instructions so far and milliseconds since the first
    /// mark.
    #[cfg(not(target_os = "macos"))]
    pub fn since_start() -> (u64, f64) {
        use std::sync::OnceLock;
        static FIRST: OnceLock<std::time::Instant> = OnceLock::new();
        thread_local!(static COUNTER: Counter = Counter::new());
        let first = *FIRST.get_or_init(std::time::Instant::now);
        let count = COUNTER.with(Counter::read);
        (count, first.elapsed().as_secs_f64() * 1e3)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub struct Counter;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    impl Counter {
        pub fn new() -> Self {
            Self
        }
        pub fn read(&self) -> u64 {
            0
        }
    }
}

/// Phases as os_signpost intervals, for Instruments.
#[cfg(target_os = "macos")]
mod signpost {
    use super::{PHASES, Phase};
    use std::{ffi::c_void, sync::OnceLock};

    unsafe extern "C" {
        static __dso_handle: u8;
        fn os_log_create(subsystem: *const i8, category: *const i8) -> *mut c_void;
        fn os_signpost_enabled(log: *mut c_void) -> bool;
        fn _os_signpost_emit_with_name_impl(
            dso: *const u8,
            log: *mut c_void,
            kind: u8,
            id: u64,
            name: *const i8,
            format: *const i8,
            buffer: *mut u8,
            size: u32,
        );
    }
    /// The phases' names as C strings, static in this image (signpost
    /// names must be).
    static NAMES: [&std::ffi::CStr; PHASES.len()] = [
        c"input", c"script", c"update", c"style", c"layout", c"text", c"damage", c"paint",
        c"encode", c"submit", c"present", c"wait",
    ];
    struct Log(*mut c_void);
    // SAFETY: An os_log_t is usable from any thread.
    unsafe impl Send for Log {}
    unsafe impl Sync for Log {}
    fn log() -> *mut c_void {
        static LOG: OnceLock<Log> = OnceLock::new();
        // SAFETY: Two static C strings.
        LOG.get_or_init(|| Log(unsafe { os_log_create(c"dev.zgx".as_ptr(), c"phases".as_ptr()) }))
            .0
    }
    fn emit(phase: Phase, kind: u8) {
        let log = log();
        // SAFETY: A live log; the name and empty format are static strings
        // in this image, as `__dso_handle` names it; a two-byte buffer is an
        // argument-less os_log payload.
        unsafe {
            if !os_signpost_enabled(log) {
                return;
            }
            let mut buffer = [0u8; 2];
            _os_signpost_emit_with_name_impl(
                &__dso_handle,
                log,
                kind,
                // OS_SIGNPOST_ID_EXCLUSIVE: one interval of a name at a time.
                0xEEEEB0B5B2B2EEEE,
                NAMES[phase as usize].as_ptr(),
                c"".as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            );
        }
    }
    pub fn begin(phase: Phase) {
        emit(phase, 1); // OS_SIGNPOST_INTERVAL_BEGIN
    }
    pub fn end(phase: Phase) {
        emit(phase, 2); // OS_SIGNPOST_INTERVAL_END
    }
}

#[cfg(not(target_os = "macos"))]
mod signpost {
    use super::Phase;
    pub fn begin(_: Phase) {}
    pub fn end(_: Phase) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_measure_only_when_enabled_and_exclude_what_they_enclose() {
        STATE.store(1, Ordering::Relaxed);
        assert!(span(Phase::Update).phase.is_none(), "off: a span is inert");
        assert!(nested(Phase::Style).phase.is_none() || cfg!(feature = "profile-nested"));
        STATE.store(2, Ordering::Relaxed);
        let started = Instant::now();
        {
            let _update = span(Phase::Update);
            std::thread::sleep(Duration::from_millis(2));
            {
                let _layout = span(Phase::Layout);
                std::thread::sleep(Duration::from_millis(4));
            }
        }
        let wall = started.elapsed();
        let totals = PROFILE.with(|profile| profile.borrow().totals);
        STATE.store(1, Ordering::Relaxed);
        let (update, layout) = (
            totals[Phase::Update as usize].time,
            totals[Phase::Layout as usize].time,
        );
        assert!(layout >= Duration::from_millis(4), "{layout:?}");
        assert!(update >= Duration::from_millis(2), "{update:?}");
        // The update's own time leaves out the layout it enclosed: together
        // they are no more than the time that passed.
        assert!(
            update + layout <= wall,
            "{update:?} + {layout:?} > {wall:?}"
        );
        assert!(PROFILE.with(|profile| profile.borrow().open.is_empty()));

        // Recording for devtools measures the frame's split alone: nothing
        // toward ZGX_PROFILE's reports, takes or marks.
        PROFILE.with(|profile| {
            let mut profile = profile.borrow_mut();
            profile.totals = Default::default();
            profile.frame = Default::default();
        });
        STATE.store(3, Ordering::Relaxed);
        {
            let _layout = span(Phase::Layout);
            std::thread::sleep(Duration::from_millis(1));
        }
        let (totals, frame) = PROFILE.with(|profile| {
            let profile = profile.borrow();
            (profile.totals, profile.frame)
        });
        let taken = take();
        STATE.store(1, Ordering::Relaxed);
        assert!(frame[Phase::Layout as usize].time >= Duration::from_millis(1));
        assert!(totals.iter().all(|t| t.time.is_zero()));
        assert!(taken.is_empty());
    }

    #[test]
    fn marks_count_from_the_start() {
        let (first, at) = counter::since_start();
        let (second, later) = counter::since_start();
        assert!(later >= at && at >= 0., "{at} then {later}");
        // Counts never go back (they are 0 where there is no counter to
        // read, as in virtual machines).
        assert!(second >= first);
    }
}
