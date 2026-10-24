//! Script limits for a bench: the defaults, or with `ZGUI_GATE` set (CI's
//! instruction gate runs the benches under cachegrind, some fifty times
//! slower), time budgets a thousand times as long, so a simulated run is
//! measured, not stopped. Only the benches read it; apps never do.
pub fn limits() -> zgui_script::Limits {
    let limits = zgui_script::Limits::default();
    if std::env::var_os("ZGUI_GATE").is_none() {
        return limits;
    }
    zgui_script::Limits {
        load: limits.load * 1000,
        call: limits.call * 1000,
        ..limits
    }
}
