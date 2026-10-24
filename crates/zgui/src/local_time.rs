//! How far local time is from UTC, which the host knows and zgui does not
//! (it reads no time zone database): a date's "today", a clock's "now".
use std::sync::OnceLock;

static OFFSET: OnceLock<fn() -> i32> = OnceLock::new();

/// Tell zgui how far local time is from UTC: `offset` answers, in seconds
/// east of it, as it is now (daylight saving moves it). Set once, by the
/// host (zgui-desktop does); until then local time is UTC.
pub fn set_offset(offset: fn() -> i32) {
    let _ = OFFSET.set(offset);
}

/// Seconds east of UTC local time is now: what the host says, else 0.
pub fn offset() -> i32 {
    OFFSET.get().map_or(0, |offset| offset())
}

/// Seconds since 1 January 1970, local time: UTC's, moved by [`offset`].
pub fn now_seconds() -> i64 {
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    utc + i64::from(offset())
}
