//! Switches for a fast path, to compare it with what it stands in for in one
//! running process (`zgx inspect switch`): a rebuild and a relaunch between
//! two measurements change more than the switch (a fresh window, a cold GPU,
//! other apps), and measured a window's frames twice apart as often as the
//! change did.
//!
//! Each is on unless switched off, and is read where its path is chosen: one
//! relaxed load with devtools built in (the `trace` feature, which `zgx
//! inspect` needs to switch one), none without, where every switch is on.
use std::sync::atomic::{AtomicBool, Ordering};

/// A fast path that can be switched off.
pub struct Switch {
    /// Its name in `zgx inspect switch`.
    pub name: &'static str,
    /// What it does, and what stands in for it while off.
    pub about: &'static str,
    on: AtomicBool,
}

impl Switch {
    const fn new(name: &'static str, about: &'static str) -> Self {
        Self {
            name,
            about,
            on: AtomicBool::new(true),
        }
    }
    /// Whether the fast path runs: always, built without the `trace`
    /// feature.
    #[inline]
    pub fn on(&self) -> bool {
        !cfg!(feature = "trace") || self.on.load(Ordering::Relaxed)
    }
    /// Run it, or not (built with the `trace` feature: without, it runs).
    pub fn set(&self, on: bool) {
        self.on.store(on, Ordering::Relaxed);
    }
}

/// A scroll moves the pixels already drawn and paints the strip scrolled in
/// (`zgx-gpu`'s scroll copies); off, it repaints what its view shows.
pub static SCROLL_COPY: Switch = Switch::new(
    "scroll_copy",
    "scrolls copy what is drawn and paint the strip scrolled in; off, they repaint the view",
);
/// Steady frames hand over at a set point before a refresh (macOS's refresh
/// grid and its latch), and on Windows clear of the compositor's latch
/// (`frame_source::latch_wait`); off, at once, as the display link's own
/// frames do.
pub static REFRESH_GRID: Switch = Switch::new(
    "refresh_grid",
    "steady frames present at a set point before a refresh (macOS), or clear of the compositor's latch (Windows); off, as soon as drawn",
);
/// A frame paints only what changed (its damage); off, the whole window.
pub static PARTIAL_REPAINT: Switch = Switch::new(
    "partial_repaint",
    "frames paint only what changed; off, the whole window every frame",
);

/// A declared loop plays from frames drawn once (macOS's `zgx-gpu` loops.rs);
/// off, it draws a frame each tick.
pub static BAKED_LOOPS: Switch = Switch::new(
    "baked_loops",
    "small declared loops play from frames drawn once (macOS); off, they draw each tick",
);

/// Every switch.
pub static ALL: [&Switch; 4] = [&SCROLL_COPY, &REFRESH_GRID, &PARTIAL_REPAINT, &BAKED_LOOPS];

/// The switch named `name`.
pub fn find(name: &str) -> Option<&'static Switch> {
    ALL.iter().copied().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_are_on_until_switched_off_and_found_by_name() {
        let s = find("partial_repaint").expect("named");
        assert!(s.on());
        s.set(false);
        assert_eq!(PARTIAL_REPAINT.on(), !cfg!(feature = "trace"));
        s.set(true);
        assert!(find("nope").is_none());
    }
}
