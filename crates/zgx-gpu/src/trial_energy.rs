//! What a present path costs, in what a process
//! can read of itself without privileges, for the full-rate paths' trial
//! (`Trial`): its own CPU energy (`task_info`'s TASK_POWER_INFO_V2), its own
//! GPU time and the window server's (each process's GPU clients in the I/O
//! Registry, as Activity Monitor's GPU Time). The window server's CPU time
//! is not readable unprivileged (proc_pid_rusage and proc_pidinfo on its pid
//! are refused; `ps` reads it only as a setuid tool), so it is not a term.
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use std::ffi::{CStr, c_char, c_void};

/// What the terms read at one moment; two give a path's cost between them.
#[derive(Clone, Debug)]
pub(crate) struct Sample {
    at: std::time::Instant,
    /// This process's CPU energy so far, nanojoules.
    cpu_nj: u64,
    /// GPU seconds so far of each GPU client (by registry entry ID) of this
    /// process and the window server (`true`): a process opens and closes
    /// clients, so costs are each client's growth, not the sums'.
    gpu: Vec<(u64, bool, f64)>,
}
/// A path's cost between two samples.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Cost {
    pub seconds: f64,
    pub cpu_joules: f64,
    pub gpu_seconds: f64,
    pub window_server_gpu_seconds: f64,
}
impl Cost {
    /// Joules a second, with GPU time weighed at `gpu_watts` a busy second.
    pub fn watts(&self, gpu_watts: f64) -> f64 {
        (self.cpu_joules + gpu_watts * (self.gpu_seconds + self.window_server_gpu_seconds))
            / self.seconds.max(1e-3)
    }
}
impl Sample {
    pub(crate) fn now() -> Self {
        let started = std::time::Instant::now();
        let gpu = gpu_clients();
        crate::present_log::log(|| format!("trial sample {} us", started.elapsed().as_micros()));
        Self {
            at: std::time::Instant::now(),
            cpu_nj: cpu_energy(),
            gpu,
        }
    }
    /// The cost from `self` to `later`.
    pub(crate) fn to(&self, later: &Sample) -> Cost {
        // A client opened since counts from zero; one closed since loses what
        // it ran after `self`, so the GPU terms can only undercount.
        let grown = |window_server: bool| {
            later
                .gpu
                .iter()
                .filter(|(_, is, _)| *is == window_server)
                .map(|(id, _, after)| {
                    let before = self
                        .gpu
                        .iter()
                        .find(|(was, ..)| was == id)
                        .map_or(0., |c| c.2);
                    (after - before).max(0.)
                })
                .sum()
        };
        Cost {
            seconds: later.at.duration_since(self.at).as_secs_f64(),
            cpu_joules: later.cpu_nj.saturating_sub(self.cpu_nj) as f64 / 1e9,
            gpu_seconds: grown(false),
            window_server_gpu_seconds: grown(true),
        }
    }
}

#[repr(C)]
#[derive(Default)]
struct TaskPowerInfo {
    total_user: u64,
    total_system: u64,
    task_interrupt_wakeups: u64,
    task_platform_idle_wakeups: u64,
    task_timer_wakeups_bin_1: u64,
    task_timer_wakeups_bin_2: u64,
}
#[repr(C)]
#[derive(Default)]
struct GpuEnergyData {
    task_gpu_utilisation: u64,
    task_gpu_stat_reserved: [u64; 3],
}
/// mach/task_info.h's `task_power_info_v2`, with the fields arm64 adds.
#[repr(C)]
#[derive(Default)]
struct TaskPowerInfoV2 {
    cpu_energy: TaskPowerInfo,
    gpu_energy: GpuEnergyData,
    task_energy: u64,
    task_ptime: u64,
    task_pset_switches: u64,
}
const TASK_POWER_INFO_V2: u32 = 26;
// SAFETY: libSystem's declarations (mach/task.h, mach/mach_init.h): the
// task port is a u32, task_info's info an array of natural_t (u32) of
// `count` entries, kern_return_t an i32.
unsafe extern "C" {
    static mach_task_self_: u32;
    fn task_info(task: u32, flavor: u32, info: *mut u32, count: *mut u32) -> i32;
}
/// This process's CPU energy so far (nanojoules), 0 if unreadable.
fn cpu_energy() -> u64 {
    let mut info = TaskPowerInfoV2::default();
    let mut count = (size_of::<TaskPowerInfoV2>() / size_of::<u32>()) as u32;
    // SAFETY: task_info writes at most `count` natural_ts into `info`, whose
    // layout is the kernel's `task_power_info_v2`.
    let status = unsafe {
        task_info(
            mach_task_self_,
            TASK_POWER_INFO_V2,
            (&mut info as *mut TaskPowerInfoV2).cast(),
            &mut count,
        )
    };
    if status == 0 { info.task_energy } else { 0 }
}

type IoObject = u32;
// SAFETY: IOKit's declarations (IOKitLib.h): io_object_t and its iterators
// are mach ports (u32), the main port 0 (kIOMainPortDefault), boolean_t an
// int, CFTypeRefs pointers; IORegistryEntryCreateCFProperty returns a
// retained object or null.
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegistryCreateIterator(
        main: u32,
        plane: *const c_char,
        options: u32,
        iterator: *mut IoObject,
    ) -> i32;
    fn IOIteratorNext(iterator: IoObject) -> IoObject;
    fn IOIteratorIsValid(iterator: IoObject) -> i32;
    fn IOObjectConformsTo(object: IoObject, class: *const c_char) -> i32;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *mut c_void;
    fn IOObjectRelease(object: IoObject) -> i32;
    fn IORegistryEntryGetRegistryEntryID(entry: IoObject, id: *mut u64) -> i32;
}
/// A registry entry's property, a retained object (toll-free bridged).
fn property(entry: IoObject, key: &NSString) -> Option<Retained<AnyObject>> {
    // SAFETY: an NSString is a CFStringRef; the property returned is +1,
    // which `from_raw` takes over.
    unsafe {
        let value = IORegistryEntryCreateCFProperty(
            entry,
            (key as *const NSString).cast(),
            std::ptr::null(),
            0,
        );
        Retained::from_raw(value.cast::<AnyObject>())
    }
}
/// GPU seconds so far of each of this process's GPU clients and the window
/// server's (each client's `AppUsage` entries' `accumulatedGPUTime`,
/// nanoseconds), by registry entry ID.
fn gpu_clients() -> Vec<(u64, bool, f64)> {
    let own = std::process::id();
    let usage_key = NSString::from_str("AppUsage");
    let creator_key = NSString::from_str("IOUserClientCreator");
    let time_key = NSString::from_str("accumulatedGPUTime");
    // The walk ends early if the registry changes under it: try again.
    for _ in 0..5 {
        let mut iterator: IoObject = 0;
        // SAFETY: a recursive iterator over the service plane, released below.
        if unsafe { IORegistryCreateIterator(0, c"IOService".as_ptr(), 1, &mut iterator) } != 0 {
            return Vec::new();
        }
        let mut clients = Vec::new();
        loop {
            // SAFETY: the iterator made above; each entry is released below.
            let entry = unsafe { IOIteratorNext(iterator) };
            if entry == 0 {
                break;
            }
            // SAFETY: a live registry entry; the class name a C string.
            if unsafe { IOObjectConformsTo(entry, c"IOUserClient".as_ptr()) } != 0
                && let Some(creator) = property(entry, &creator_key)
                && let Some(usage) = property(entry, &usage_key)
            {
                // SAFETY: `IOUserClientCreator` is a string ("pid 123, WindowServer"),
                // `AppUsage` an array of dictionaries of numbers.
                unsafe {
                    let text: *const c_char = msg_send![&*creator, UTF8String];
                    let text = if text.is_null() {
                        ""
                    } else {
                        CStr::from_ptr(text).to_str().unwrap_or("")
                    };
                    let pid = text
                        .strip_prefix("pid ")
                        .and_then(|rest| rest.split(',').next())
                        .and_then(|pid| pid.trim().parse::<u32>().ok());
                    let is_window_server = text.ends_with(", WindowServer");
                    if pid == Some(own) || is_window_server {
                        let count: usize = msg_send![&*usage, count];
                        let mut nanos = 0.;
                        for at in 0..count {
                            let entry: *mut AnyObject = msg_send![&*usage, objectAtIndex: at];
                            let time: *mut AnyObject = msg_send![entry, objectForKey: &*time_key];
                            if !time.is_null() {
                                let value: f64 = msg_send![time, doubleValue];
                                nanos += value;
                            }
                        }
                        let mut id = 0;
                        if IORegistryEntryGetRegistryEntryID(entry, &mut id) == 0 {
                            clients.push((id, pid != Some(own), nanos / 1e9));
                        }
                    }
                }
            }
            // SAFETY: the entry IOIteratorNext returned, released once.
            unsafe { IOObjectRelease(entry) };
        }
        // SAFETY: the iterator made above, released once.
        let complete = unsafe { IOIteratorIsValid(iterator) } != 0;
        unsafe { IOObjectRelease(iterator) };
        if complete {
            return clients;
        }
    }
    Vec::new()
}
