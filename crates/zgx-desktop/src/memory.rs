//! The process's own lines of `zgx inspect memory`: what the system charges
//! it, and what its allocator holds.
use zgx::memory::Report;

/// What the system charges the process, and what it is called here.
#[cfg(target_os = "macos")]
fn footprint() -> Option<(&'static str, usize)> {
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
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV2) -> i32;
    }
    const RUSAGE_INFO_V2: i32 = 2;
    let mut info = RusageInfoV2::default();
    // SAFETY: the buffer is a correctly sized, writable rusage_info_v2.
    let read = unsafe { proc_pid_rusage(std::process::id() as i32, RUSAGE_INFO_V2, &mut info) };
    (read == 0).then_some(("footprint", info.phys_footprint as usize))
}
#[cfg(target_os = "linux")]
fn footprint() -> Option<(&'static str, usize)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(("resident", kb * 1024))
}
#[cfg(windows)]
fn footprint() -> Option<(&'static str, usize)> {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        size: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        peak_paged_pool: usize,
        paged_pool: usize,
        peak_non_paged_pool: usize,
        non_paged_pool: usize,
        page_file: usize,
        peak_page_file: usize,
        private: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    let mut counters = Counters {
        size: size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: the structure is PROCESS_MEMORY_COUNTERS_EX, with its size.
    let read = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            size_of::<Counters>() as u32,
        )
    };
    (read != 0).then_some(("private", counters.private))
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn footprint() -> Option<(&'static str, usize)> {
    None
}

/// What the allocator has from the system, and what is in use of it: the
/// gap is what it keeps for later.
#[cfg(feature = "mimalloc")]
fn heap(report: &mut Report) {
    use std::ffi::c_void;
    #[repr(C)]
    struct Area {
        blocks: *mut c_void,
        reserved: usize,
        committed: usize,
        used: usize,
        block_size: usize,
    }
    type Visitor =
        extern "C" fn(*const c_void, *const Area, *mut c_void, usize, *mut c_void) -> bool;
    unsafe extern "C" {
        fn mi_process_info(
            elapsed_msecs: *mut usize,
            user_msecs: *mut usize,
            system_msecs: *mut usize,
            current_rss: *mut usize,
            peak_rss: *mut usize,
            current_commit: *mut usize,
            peak_commit: *mut usize,
            page_faults: *mut usize,
        );
        fn mi_heap_main() -> *mut c_void;
        fn mi_heap_visit_blocks(
            heap: *const c_void,
            visit_blocks: bool,
            visitor: Visitor,
            arg: *mut c_void,
        ) -> bool;
    }
    extern "C" fn area(
        _heap: *const c_void,
        area: *const Area,
        _block: *mut c_void,
        _size: usize,
        used: *mut c_void,
    ) -> bool {
        // SAFETY: mimalloc passes its area, and `used` is the caller's.
        unsafe {
            if let Some(area) = area.as_ref() {
                *used.cast::<usize>() += area.used * area.block_size;
            }
        }
        true
    }
    let (mut commit, mut peak, mut used) = (0, 0, 0usize);
    let none = std::ptr::null_mut();
    // SAFETY: mimalloc's own: null outputs are skipped, and its main heap is
    // visited by area alone, no block touched.
    unsafe {
        mi_process_info(none, none, none, none, none, &mut commit, &mut peak, none);
        mi_heap_visit_blocks(mi_heap_main(), false, area, (&raw mut used).cast());
    }
    report.add("heap", "mimalloc committed", None, Some(commit));
    report.add("heap", "mimalloc peak committed", None, Some(peak));
    report.add("heap", "mimalloc in use", None, Some(used));
}
#[cfg(all(not(feature = "mimalloc"), target_os = "macos"))]
fn heap(report: &mut Report) {
    #[repr(C)]
    #[derive(Default)]
    struct Statistics {
        blocks_in_use: u32,
        size_in_use: usize,
        max_size_in_use: usize,
        size_allocated: usize,
    }
    unsafe extern "C" {
        fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Statistics);
    }
    let mut stats = Statistics::default();
    // SAFETY: a null zone asks for every zone's, into a local.
    unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
    report.add("heap", "malloc in use", None, Some(stats.size_in_use));
    report.add("heap", "malloc allocated", None, Some(stats.size_allocated));
}
#[cfg(all(not(feature = "mimalloc"), target_os = "linux", target_env = "gnu"))]
fn heap(report: &mut Report) {
    #[repr(C)]
    struct MallInfo {
        arena: i32,
        ordblks: i32,
        smblks: i32,
        hblks: i32,
        hblkhd: i32,
        usmblks: i32,
        fsmblks: i32,
        uordblks: i32,
        fordblks: i32,
        keepcost: i32,
    }
    unsafe extern "C" {
        fn mallinfo() -> MallInfo;
    }
    // SAFETY: glibc's, reading its own counters (each wraps at 4 GiB).
    let info = unsafe { mallinfo() };
    let bytes = |n: i32| n as u32 as usize;
    let in_use = bytes(info.uordblks) + bytes(info.hblkhd);
    report.add("heap", "malloc in use", None, Some(in_use));
    report.add("heap", "malloc free", None, Some(bytes(info.fordblks)));
}
#[cfg(not(any(
    feature = "mimalloc",
    target_os = "macos",
    all(target_os = "linux", target_env = "gnu")
)))]
fn heap(_report: &mut Report) {}

/// The process's lines.
pub(crate) fn process(report: &mut Report) {
    if let Some((name, bytes)) = footprint() {
        report.add("process", name, None, Some(bytes));
    }
    heap(report);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_says_what_the_system_charges_it_and_its_allocator_holds() {
        let held = vec![1u8; 8 << 20];
        let mut report = Report::default();
        process(&mut report);
        std::hint::black_box(&held);
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
        {
            let process = &report.lines[0];
            assert_eq!(process.area, "process");
            assert!(process.bytes.unwrap() > 1 << 20, "{process:?}");
        }
        #[cfg(any(feature = "mimalloc", target_os = "macos"))]
        {
            let in_use = (report.lines.iter())
                .find(|l| l.name.ends_with("in use"))
                .unwrap_or_else(|| panic!("{report:?}"));
            assert!(in_use.bytes.unwrap() >= 8 << 20, "{in_use:?}");
        }
    }
}
