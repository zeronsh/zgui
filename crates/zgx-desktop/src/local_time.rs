//! Local time's offset from UTC, for zgx's dates and clocks
//! (`zgx::local_time::set_offset`): the C library's on Unix; on
//! Windows, the system's local and universal clocks compared.

/// Seconds east of UTC, now.
pub(crate) fn offset() -> i32 {
    imp::offset()
}

#[cfg(all(unix, target_pointer_width = "64"))]
mod imp {
    use std::ffi::{c_char, c_int, c_long};

    /// C's `struct tm`, as glibc, musl, bionic, Darwin and the BSDs lay it
    /// out on 64-bit targets (nine `int`s, then `long tm_gmtoff` and
    /// `const char *tm_zone`): C writes every field, zgx reads the offset.
    /// Where the C library's is shorter (illumos has no `tm_gmtoff`), it
    /// writes a prefix of this one and the offset reads as the zero it was.
    #[repr(C)]
    #[allow(dead_code)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }
    // SAFETY: POSIX's declaration, `time_t` being 64 bits on these targets.
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    pub(super) fn offset() -> i32 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
        // SAFETY: both pointers are valid for the call, and `localtime_r`
        // writes at most a `struct tm` to `tm` and keeps neither. It reads
        // `TZ` from the environment: a concurrent `set_var` is that call's
        // unsafety, not this one's.
        if unsafe { localtime_r(&now, tm.as_mut_ptr()) }.is_null() {
            return 0;
        }
        // SAFETY: zeroed (a valid `Tm`: integers and a null pointer), then
        // written by `localtime_r` with valid values.
        unsafe { tm.assume_init() }.gmtoff as i32
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::{
        Foundation::SYSTEMTIME,
        System::SystemInformation::{GetLocalTime, GetSystemTime},
    };

    pub(super) fn offset() -> i32 {
        // SAFETY: each only reads the clock.
        let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
        // Days since 1970 (Howard Hinnant's `days_from_civil`).
        let days = |t: &SYSTEMTIME| {
            let (m, d) = (i64::from(t.wMonth), i64::from(t.wDay));
            let y = i64::from(t.wYear) - i64::from(m <= 2);
            let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
            let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
            era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
        };
        let minutes =
            |t: &SYSTEMTIME| days(t) * 1440 + i64::from(t.wHour) * 60 + i64::from(t.wMinute);
        ((minutes(&local) - minutes(&utc)) * 60) as i32
    }
}

#[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
mod imp {
    pub(super) fn offset() -> i32 {
        0
    }
}
