//! `ZGX_PRESENT_LOG=1`: a line on stderr for each presentation event (the
//! surface a frame used, layer hand-offs, trims, focus), stamped with the
//! host clock (`CLOCK_UPTIME_RAW`, as ScreenCaptureKit and Core Media stamp
//! frames; on Windows the performance counter, as DXGI and DWM stamp them),
//! to line a screen recording's frames up with what happened.
use std::sync::OnceLock;

pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| zgx::env::var_os("ZGX_PRESENT_LOG").is_some_and(|v| v != "0"))
}

/// Nanoseconds on the host clock.
pub fn now_ns() -> u64 {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: as `<time.h>` declares it (`clockid_t` is a 32-bit enum).
        unsafe extern "C" {
            fn clock_gettime_nsec_np(clock: u32) -> u64;
        }
        // SAFETY: takes a clock id and returns its time, 0 for an unknown
        // clock; 8 is CLOCK_UPTIME_RAW.
        unsafe { clock_gettime_nsec_np(8) }
    }
    #[cfg(windows)]
    {
        crate::dxgi::now_ns()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        0
    }
}

/// Logs `what` when `ZGX_PRESENT_LOG` is set; costs one load otherwise.
pub fn log(what: impl FnOnce() -> String) {
    if enabled() {
        eprintln!("PRESENT {:.6} {}", now_ns() as f64 / 1e9, what());
    }
}
