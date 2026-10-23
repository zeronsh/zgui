//! Glass resource monitor: launches the kitchen sink, or another example with
//! `--app <example>` (or attaches with `--pid <pid>`, or neither with
//! `--app none`), and charts CPU, GPU and
//! memory over the last minute for any process on the machine. GPU time per
//! process comes from the GPU driver's per-client accounting (macOS: each
//! `AGXDeviceUserClient`'s `AppUsage`), the way Activity Monitor reads it.
//!
//! The interface is Luau (`examples/zgui_monitor/app.luau` and the modules it
//! requires); edit and save to reload it. It lays panels out as the terminal
//! example lays out panes: split beside or below, resized by their dividers,
//! moved by their headers onto another panel's edge (or its middle, to swap),
//! with processes dragged from the sidebar the same way.
//!
//! This file samples: every process, every half second, published to the
//! script as shared state (`procs`, `summary`), and the last minute of each
//! process a panel shows (`history:<pid>`, for the pids in `watch`).
//! Run: cargo run --release -p zgui-desktop --example zgui_monitor
//! (build the target first: cargo build --release -p zgui-desktop --example kitchen_sink)

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    process::{Child, Command},
    rc::Rc,
    time::{Duration, Instant},
};
use zgui::{reactive::Runtime, timer::sleep};
use zgui_desktop::{Application, FontData, Glass, Titlebar, WindowOptions};
use zgui_script::{ScriptHost, Value};

const TITLE: &str = "zgui monitor";
const WIDTH: f64 = 1280.;
const HEIGHT: f64 = 780.;
const INTERVAL: Duration = Duration::from_millis(500);
/// One minute of samples.
const CAPACITY: usize = 120;

#[derive(Clone, Copy, PartialEq)]
struct Sample {
    cpu: f32,
    /// Percent of one GPU's time, like `cpu` is of one core's.
    gpu: f32,
    memory_mb: f32,
}

/// Sampler state per process, kept past exit while a panel shows it.
struct Track {
    start: u64,
    name: Rc<str>,
    cpu_ns: u64,
    /// Accumulated GPU time, if the process has ever used the GPU.
    gpu_ns: Option<u64>,
    at: Instant,
    /// Smoothed CPU for a sort order that doesn't jitter every sample.
    smooth: f32,
    samples: VecDeque<Sample>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/zgui_monitor/app.luau");
    let mut application = Application::new();
    // Geist, the interface's typeface (`font.sans` in zgui_monitor/glass.json).
    for bytes in [
        &include_bytes!("support/fonts/Geist.ttf")[..],
        include_bytes!("support/fonts/Geist-Medium.ttf"),
        include_bytes!("support/fonts/Geist-SemiBold.ttf"),
    ] {
        application = application.font(FontData::new(std::sync::Arc::<[u8]>::from(bytes))?);
    }
    application
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((760., 520.)),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            // Launch the target beside this binary unless attaching.
            let mut error = String::new();
            let child: Rc<RefCell<Option<Child>>> = Rc::default();
            let launched = match attach_pid() {
                Some(pid) => Some(pid),
                None if target() == "none" => None,
                None => match launch_target() {
                    Ok(process) => {
                        let pid = process.id() as i32;
                        *child.borrow_mut() = Some(process);
                        Some(pid)
                    }
                    Err(message) => {
                        error = message;
                        None
                    }
                },
            };

            let host = ScriptHost::new(&cx.ui.runtime, script).app_id("zgui-monitor");
            // Panels stay concentric with the window's corners.
            let radius = host.shared("window_radius", 10.);
            let corner = cx.window_info.corner_radius.clone();
            let follow_radius = cx.ui.runtime.effect(move || {
                let r = corner.get();
                if r > 0. {
                    radius.set(r.into());
                }
            });
            host.shared(
                "launched",
                launched.map_or(Value::Nil, |pid| num(pid as f32)),
            );
            host.shared("error", error.as_str());
            host.shared("interval", INTERVAL.as_millis() as f64);
            host.shared("capacity", CAPACITY as f64);
            let window = cx.window.clone();
            host.action("close_window", move |_| window.close());

            let sampler = Sampler {
                host: host.clone(),
                runtime: cx.ui.runtime.clone(),
                tracks: Rc::default(),
                procs: host.shared("procs", Value::List(Rc::new([]))),
                summary: host.shared("summary", Value::Nil),
                watch: host.shared("watch", Value::List(Rc::new([]))),
            };
            // A panel opening on a process shows its last minute at once,
            // not at the next sample.
            let publish = {
                let sampler = sampler.clone();
                let watch = sampler.watch.clone();
                cx.ui.runtime.effect(move || {
                    watch.get();
                    sampler.publish_history();
                })
            };
            cx.tasks.spawn(async move {
                loop {
                    sampler.sample();
                    sleep(INTERVAL).await;
                }
            });

            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));

            // The menus are the script's commands.
            let commands = host.commands();
            let windows = cx.windows.clone();
            let menus = cx.ui.runtime.effect(move || {
                let list = commands.get();
                if let Err(error) = windows.set_menus(zgui_run::menus(&list)) {
                    eprintln!("zgui_monitor: menus: {error}");
                }
            });
            let commands = host.clone();
            cx.on_menu_action(move |action| {
                commands.run_command(&action.0);
            });
            // The monitor owns the process it launched.
            let keep = RefCell::new(Some((menus, follow_radius, publish)));
            cx.on_closed(move || {
                drop(keep.borrow_mut().take());
                if let Some(mut process) = child.borrow_mut().take() {
                    let _ = process.kill();
                    let _ = process.wait();
                }
            });
            cx.render(host.view());
        })
}

fn attach_pid() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == "--pid")
        .and_then(|i| args.get(i + 1))
        .and_then(|pid| pid.parse().ok())
}

/// The example to launch: `--app <name>`, the kitchen sink by default;
/// `--app none` launches nothing.
fn target() -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == "--app")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "kitchen_sink".into())
}

fn launch_target() -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = target();
    let path = exe.with_file_name(&name);
    Command::new(&path).spawn().map_err(|e| {
        format!(
            "Could not start {} ({e}). Build it with: cargo build --release -p zgui-desktop --example {name}",
            path.display()
        )
    })
}

fn num(value: f32) -> Value {
    Value::Num(value as f64)
}

/// A script table from its fields, keys shared across every table made.
fn map<const N: usize>(fields: [(&'static str, Value); N]) -> Value {
    thread_local! {
        static KEYS: RefCell<HashMap<&'static str, Rc<str>>> = RefCell::default();
    }
    let mut fields: Vec<(Rc<str>, Value)> = KEYS.with_borrow_mut(|keys| {
        fields
            .into_iter()
            .map(|(key, value)| (keys.entry(key).or_insert_with(|| key.into()).clone(), value))
            .collect()
    });
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Map(fields.into())
}

/// Samples every readable process, so a newly opened panel already has its
/// last minute of history.
#[derive(Clone)]
struct Sampler {
    host: ScriptHost,
    runtime: Runtime,
    tracks: Rc<RefCell<HashMap<i32, Track>>>,
    /// `{ pid, name, cpu, gpu, mem, smooth }` for every live process.
    procs: zgui_script::Shared,
    /// `{ count, cpu, gpu }`: the whole machine.
    summary: zgui_script::Shared,
    /// The pids the script's panels show (it writes this).
    watch: zgui_script::Shared,
}
impl Sampler {
    fn watched(&self) -> HashSet<i32> {
        self.watch.with_untracked(|watch| {
            watch
                .list()
                .iter()
                .filter_map(Value::as_f64)
                .map(|pid| pid as i32)
                .collect()
        })
    }
    fn sample(&self) {
        let watched = self.watched();
        let procs = sample(&mut self.tracks.borrow_mut(), &watched);
        let cpu: f32 = procs.iter().map(|p| p.2).sum();
        let summary = map([
            ("count", num(procs.len() as f32)),
            ("cpu", num(cpu)),
            ("gpu", usage::device_gpu().map_or(Value::Nil, num)),
        ]);
        let rows: Vec<Value> = procs
            .into_iter()
            .map(|(pid, name, cpu, gpu, memory_mb, smooth)| {
                map([
                    ("pid", num(pid as f32)),
                    ("name", Value::Str(name)),
                    ("cpu", num(cpu)),
                    ("gpu", num(gpu)),
                    ("mem", num(memory_mb)),
                    ("smooth", num(smooth)),
                ])
            })
            .collect();
        self.runtime.batch(|| {
            self.procs.set(Value::List(rows.into()));
            self.summary.set(summary);
            self.publish_history();
        });
    }
    /// Each watched process's last minute, as `history:<pid>`:
    /// `{ name, cpu = { … }, gpu = { … }, mem = { … } }`, oldest first.
    fn publish_history(&self) {
        let tracks = self.tracks.borrow();
        for pid in self.watched() {
            let Some(track) = tracks.get(&pid) else {
                continue;
            };
            let series = |read: fn(&Sample) -> f32| {
                Value::List(track.samples.iter().map(|s| num(read(s))).collect())
            };
            let history = map([
                ("name", Value::Str(track.name.clone())),
                ("cpu", series(|s| s.cpu)),
                ("gpu", series(|s| s.gpu)),
                ("mem", series(|s| s.memory_mb)),
            ]);
            self.host
                .shared(&format!("history:{pid}"), Value::Nil)
                .set(history);
        }
    }
}

/// One reading per live process: pid, name, CPU, GPU, memory (MB) and the
/// smoothed CPU.
fn sample(
    tracks: &mut HashMap<i32, Track>,
    open: &HashSet<i32>,
) -> Vec<(i32, Rc<str>, f32, f32, f32, f32)> {
    let now = Instant::now();
    let mut seen = HashSet::new();
    let mut procs = Vec::new();
    let gpu_times = usage::gpu_times();
    for pid in usage::pids() {
        let Some(usage) = usage::read(pid) else {
            continue;
        };
        seen.insert(pid);
        let memory_mb = usage.footprint as f32 / (1024. * 1024.);
        let track = match tracks.get_mut(&pid) {
            // A reused pid is a different process.
            Some(track) if track.start == usage.start => track,
            _ => {
                let name: Rc<str> = usage::name(pid).into();
                tracks.insert(
                    pid,
                    Track {
                        start: usage.start,
                        name: name.clone(),
                        cpu_ns: usage.cpu_ns,
                        gpu_ns: gpu_times.get(&pid).copied(),
                        at: now,
                        smooth: 0.,
                        samples: VecDeque::new(),
                    },
                );
                procs.push((pid, name, 0., 0., memory_mb, 0.));
                continue;
            }
        };
        let wall = now.duration_since(track.at).as_nanos().max(1) as f64;
        let cpu = (usage.cpu_ns.saturating_sub(track.cpu_ns) as f64 / wall * 100.) as f32;
        track.cpu_ns = usage.cpu_ns;
        let gpu_ns = gpu_times.get(&pid).copied();
        // A process that closes its last GPU client drops back to zero.
        let gpu = match (gpu_ns, track.gpu_ns) {
            (Some(now_ns), Some(then)) => (now_ns.saturating_sub(then) as f64 / wall * 100.) as f32,
            _ => 0.,
        };
        track.gpu_ns = gpu_ns;
        track.at = now;
        track.smooth = track.smooth * 0.8 + cpu * 0.2;
        if track.samples.len() == CAPACITY {
            track.samples.pop_front();
        }
        track.samples.push_back(Sample {
            cpu,
            gpu,
            memory_mb,
        });
        procs.push((pid, track.name.clone(), cpu, gpu, memory_mb, track.smooth));
    }
    // Exited processes keep their history while a panel shows them.
    tracks.retain(|pid, _| seen.contains(pid) || open.contains(pid));
    procs
}

/// Per-process CPU time and physical footprint (what Activity Monitor shows).
#[cfg(target_os = "macos")]
mod usage {
    use std::sync::OnceLock;
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV2 {
        uuid: [u8; 16],
        user_time: u64,
        system_time: u64,
        pkg_idle_wkups: u64,
        interrupt_wkups: u64,
        pageins: u64,
        wired_size: u64,
        resident_size: u64,
        phys_footprint: u64,
        proc_start_abstime: u64,
        proc_exit_abstime: u64,
        child_user_time: u64,
        child_system_time: u64,
        child_pkg_idle_wkups: u64,
        child_interrupt_wkups: u64,
        child_pageins: u64,
        child_elapsed_abstime: u64,
        diskio_bytesread: u64,
        diskio_byteswritten: u64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV2) -> i32;
        fn proc_listallpids(buffer: *mut i32, size: i32) -> i32;
        fn proc_pidpath(pid: i32, buffer: *mut u8, size: u32) -> i32;
        fn proc_name(pid: i32, buffer: *mut u8, size: u32) -> i32;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    const RUSAGE_INFO_V2: i32 = 2;

    pub struct Usage {
        pub cpu_ns: u64,
        pub footprint: u64,
        /// Distinguishes a reused pid.
        pub start: u64,
    }
    pub fn pids() -> Vec<i32> {
        // SAFETY: a null buffer asks for the current count.
        let count = unsafe { proc_listallpids(std::ptr::null_mut(), 0) }.max(0) as usize;
        // Room for processes started between the two calls.
        let mut pids = vec![0_i32; count + 64];
        let bytes = (pids.len() * size_of::<i32>()) as i32;
        // SAFETY: the buffer holds `bytes` writable bytes.
        let count = unsafe { proc_listallpids(pids.as_mut_ptr(), bytes) }.max(0) as usize;
        pids.truncate(count.min(pids.len()));
        pids.retain(|pid| *pid > 0);
        pids
    }
    /// The executable's file name (proc_name truncates at 32 bytes).
    pub fn name(pid: i32) -> String {
        let mut buffer = [0_u8; 4096];
        // SAFETY: the buffer is writable for its full length.
        let len = unsafe { proc_pidpath(pid, buffer.as_mut_ptr(), buffer.len() as u32) };
        if len > 0 {
            let path = String::from_utf8_lossy(&buffer[..len as usize]);
            if let Some(name) = path.rsplit('/').next().filter(|n| !n.is_empty()) {
                return name.to_owned();
            }
        }
        // SAFETY: as above.
        let len = unsafe { proc_name(pid, buffer.as_mut_ptr(), buffer.len() as u32) };
        if len > 0 {
            return String::from_utf8_lossy(&buffer[..len as usize]).into_owned();
        }
        format!("pid {pid}")
    }
    pub fn read(pid: i32) -> Option<Usage> {
        static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
        let mut info = RusageInfoV2::default();
        // SAFETY: the buffer is a correctly sized, writable rusage_info_v2.
        if unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V2, &mut info) } != 0 {
            return None;
        }
        // CPU times are in Mach absolute-time units (not ns on Apple silicon).
        let &(numer, denom) = TIMEBASE.get_or_init(|| {
            let mut timebase = Timebase::default();
            // SAFETY: writes two integers into a local.
            unsafe { mach_timebase_info(&mut timebase) };
            (timebase.numer, timebase.denom.max(1))
        });
        let ticks = info.user_time + info.system_time;
        let cpu_ns = (ticks as u128 * numer as u128 / denom as u128) as u64;
        Some(Usage {
            cpu_ns,
            footprint: info.phys_footprint,
            start: info.proc_start_abstime,
        })
    }

    // GPU time per process -----------------------------------------------------
    //
    // The GPU driver keeps per-client accounting in the IORegistry: every
    // `AGXDeviceUserClient` names its creator ("pid 395, WindowServer") and
    // lists `AppUsage` entries with `accumulatedGPUTime` in nanoseconds.
    // `IOAccelerator`'s `PerformanceStatistics` hold the device-wide
    // "Device Utilization %". Both are readable without privileges.
    use std::{
        collections::HashMap,
        ffi::{CStr, c_char, c_void},
    };
    type CfType = *const c_void;
    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const c_char) -> *mut c_void;
        fn IOServiceGetMatchingServices(main: u32, matching: *mut c_void, it: *mut u32) -> i32;
        fn IOIteratorNext(it: u32) -> u32;
        fn IOObjectRelease(object: u32) -> i32;
        fn IORegistryEntryGetChildIterator(entry: u32, plane: *const c_char, it: *mut u32) -> i32;
        fn IORegistryEntryCreateCFProperty(
            entry: u32,
            key: CfType,
            allocator: CfType,
            options: u32,
        ) -> CfType;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: CfType,
            text: *const c_char,
            encoding: u32,
        ) -> CfType;
        fn CFStringGetCString(
            text: CfType,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> bool;
        fn CFGetTypeID(object: CfType) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFDictionaryGetTypeID() -> usize;
        fn CFNumberGetTypeID() -> usize;
        fn CFArrayGetCount(array: CfType) -> isize;
        fn CFArrayGetValueAtIndex(array: CfType, index: isize) -> CfType;
        fn CFDictionaryGetValue(dictionary: CfType, key: CfType) -> CfType;
        fn CFNumberGetValue(number: CfType, kind: isize, value: *mut c_void) -> bool;
        fn CFRelease(object: CfType);
    }
    const UTF8: u32 = 0x0800_0100;
    const SINT64: isize = 4;

    /// An owned CFString key.
    struct Key(CfType);
    impl Key {
        fn new(text: &CStr) -> Self {
            // SAFETY: a NUL-terminated UTF-8 string; released in Drop.
            Self(unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) })
        }
    }
    impl Drop for Key {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: created above and not shared.
                unsafe { CFRelease(self.0) };
            }
        }
    }

    /// Calls `visit` with each registry entry of `class`, then releases it.
    fn each_service(class: &CStr, mut visit: impl FnMut(u32)) {
        let mut iterator = 0_u32;
        // SAFETY: `IOServiceGetMatchingServices` consumes the matching
        // dictionary; the iterator and entries are released here.
        unsafe {
            let matching = IOServiceMatching(class.as_ptr());
            if matching.is_null() || IOServiceGetMatchingServices(0, matching, &mut iterator) != 0 {
                return;
            }
            loop {
                let entry = IOIteratorNext(iterator);
                if entry == 0 {
                    break;
                }
                visit(entry);
                IOObjectRelease(entry);
            }
            IOObjectRelease(iterator);
        }
    }

    /// Calls `visit` with each child of `entry` in the service plane. User
    /// clients are not registered services, so matching can't find them;
    /// they hang off their accelerator like `ioreg` shows.
    fn each_child(entry: u32, mut visit: impl FnMut(u32)) {
        let mut iterator = 0_u32;
        // SAFETY: a valid entry; the iterator and children are released.
        unsafe {
            if IORegistryEntryGetChildIterator(entry, c"IOService".as_ptr(), &mut iterator) != 0 {
                return;
            }
            loop {
                let child = IOIteratorNext(iterator);
                if child == 0 {
                    break;
                }
                visit(child);
                IOObjectRelease(child);
            }
            IOObjectRelease(iterator);
        }
    }

    /// A copied property of `entry`, released when dropped.
    struct Property(CfType);
    impl Property {
        fn of(entry: u32, key: &Key) -> Option<Self> {
            // SAFETY: a valid entry and key; the result is owned (Create rule).
            let value =
                unsafe { IORegistryEntryCreateCFProperty(entry, key.0, std::ptr::null(), 0) };
            // Lazily: an eager `Self(value)` would be dropped, releasing
            // null, when the entry lacks the property.
            (!value.is_null()).then(|| Self(value))
        }
    }
    impl Drop for Property {
        fn drop(&mut self) {
            // SAFETY: owned since `of`.
            unsafe { CFRelease(self.0) };
        }
    }

    fn is(object: CfType, type_id: usize) -> bool {
        // SAFETY: a valid CF object.
        !object.is_null() && unsafe { CFGetTypeID(object) } == type_id
    }

    fn number(object: CfType) -> Option<i64> {
        // SAFETY: type checked before reading.
        unsafe {
            if !is(object, CFNumberGetTypeID()) {
                return None;
            }
            let mut value = 0_i64;
            CFNumberGetValue(object, SINT64, (&raw mut value).cast()).then_some(value)
        }
    }

    fn string(object: CfType) -> Option<String> {
        // SAFETY: type checked; the buffer is writable for its length.
        unsafe {
            if !is(object, CFStringGetTypeID()) {
                return None;
            }
            let mut buffer = [0 as c_char; 256];
            CFStringGetCString(object, buffer.as_mut_ptr(), buffer.len() as isize, UTF8).then(
                || {
                    CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                },
            )
        }
    }

    /// "pid 395, WindowServer" → 395.
    pub fn creator_pid(creator: &str) -> Option<i32> {
        creator
            .strip_prefix("pid ")?
            .split(',')
            .next()?
            .trim()
            .parse()
            .ok()
    }

    /// Accumulated GPU nanoseconds per process with an open GPU client.
    pub fn gpu_times() -> HashMap<i32, u64> {
        let (creator_key, usage_key, time_key) = (
            Key::new(c"IOUserClientCreator"),
            Key::new(c"AppUsage"),
            Key::new(c"accumulatedGPUTime"),
        );
        let mut times = HashMap::new();
        let mut read = |entry: u32| {
            let Some(pid) = Property::of(entry, &creator_key)
                .and_then(|creator| string(creator.0))
                .and_then(|creator| creator_pid(&creator))
            else {
                return;
            };
            let Some(usage) = Property::of(entry, &usage_key) else {
                return;
            };
            // SAFETY: type checked; indices are within the count.
            let total: u64 = unsafe {
                if !is(usage.0, CFArrayGetTypeID()) {
                    return;
                }
                (0..CFArrayGetCount(usage.0))
                    .map(|i| CFArrayGetValueAtIndex(usage.0, i))
                    .filter(|item| is(*item, CFDictionaryGetTypeID()))
                    .filter_map(|item| number(CFDictionaryGetValue(item, time_key.0)))
                    .map(|ns| ns.max(0) as u64)
                    .sum()
            };
            *times.entry(pid).or_insert(0) += total;
        };
        each_service(c"IOAccelerator", |accelerator| {
            each_child(accelerator, &mut read)
        });
        times
    }

    /// The GPU's own "Device Utilization %", if the driver reports it.
    pub fn device_gpu() -> Option<f32> {
        let (stats_key, utilization_key) = (
            Key::new(c"PerformanceStatistics"),
            Key::new(c"Device Utilization %"),
        );
        let mut found = None;
        each_service(c"IOAccelerator", |entry| {
            if found.is_some() {
                return;
            }
            if let Some(stats) = Property::of(entry, &stats_key)
                && is(stats.0, unsafe { CFDictionaryGetTypeID() })
            {
                // SAFETY: a dictionary, checked above.
                found = number(unsafe { CFDictionaryGetValue(stats.0, utilization_key.0) })
                    .map(|percent| percent as f32);
            }
        });
        found
    }
}

#[cfg(not(target_os = "macos"))]
mod usage {
    /// No per-process GPU accounting here yet.
    pub fn gpu_times() -> std::collections::HashMap<i32, u64> {
        std::collections::HashMap::new()
    }
    pub fn device_gpu() -> Option<f32> {
        None
    }
    pub struct Usage {
        pub cpu_ns: u64,
        pub footprint: u64,
        pub start: u64,
    }
    pub fn pids() -> Vec<i32> {
        std::fs::read_dir("/proc")
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn name(pid: i32) -> String {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|name| name.trim_end().to_owned())
            .unwrap_or_else(|_| format!("pid {pid}"))
    }
    /// Linux: /proc/<pid>/stat CPU ticks (assumed 100 Hz) and resident memory.
    pub fn read(pid: i32) -> Option<Usage> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fields: Vec<&str> = stat.rsplit(')').next()?.split_whitespace().collect();
        let ticks: u64 =
            fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
        let pages: u64 = fields.get(21)?.parse().ok()?;
        Some(Usage {
            cpu_ns: ticks * 10_000_000,
            footprint: pages * 4096,
            start: fields.get(19)?.parse().ok()?,
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::usage;

    #[test]
    fn creator_names_its_pid() {
        assert_eq!(usage::creator_pid("pid 395, WindowServer"), Some(395));
        assert_eq!(usage::creator_pid("pid 7,x"), Some(7));
        assert_eq!(usage::creator_pid("WindowServer"), None);
    }

    #[test]
    fn window_server_has_gpu_time() {
        // Every logged-in Mac composites through WindowServer on the GPU.
        let times = usage::gpu_times();
        assert!(times.values().any(|ns| *ns > 0), "no GPU accounting read");
    }
}
