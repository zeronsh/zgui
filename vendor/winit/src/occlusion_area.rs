//! zgx patch: what a window's occlusion check counts on Windows, apart from
//! the system calls that gather it (`platform_impl::windows::occlusion`).
use std::time::{Duration, Instant};

/// Between checks while a window draws.
const CHECK_EVERY: Duration = Duration::from_millis(500);
/// Checks in a row that find it covered before it counts as occluded: a
/// window passing over, or a switcher, stops nothing.
const CHECKS_TO_CONFIRM: u8 = 2;

/// A rectangle of the screen, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Area {
    pub fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    pub fn contains(self, other: Area) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }

    pub fn meet(self, other: Area) -> Area {
        Area {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }
}

/// A monitor, as the system reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Screen {
    pub area: Area,
    pub dpi: u32,
}

/// Monitors whose rectangles can be taken at their word. A secondary monitor
/// left of or above the primary has negative coordinates, and monitors of
/// different scales are where a window's rectangle may not be what's shown;
/// neither has been seen working on real monitors, so neither is trusted.
fn trusted(screens: &[Screen]) -> bool {
    let Some(first) = screens.first() else {
        return false;
    };
    screens.iter().enumerate().all(|(at, screen)| {
        let area = screen.area;
        screen.dpi != 0
            && screen.dpi == first.dpi
            && !area.is_empty()
            && area.left >= 0
            && area.top >= 0
            && screens[..at].iter().all(|other| other.area.meet(area).is_empty())
    })
}

/// One check of a window, fed the windows above it from the top. Only a
/// layout it can be sure of says hidden: the window whole on one monitor, on
/// `trusted` monitors, and every window over it whole on that monitor too.
#[derive(Debug)]
pub(crate) struct Coverage {
    window: Area,
    monitor: Area,
    visible: Vec<Area>,
    doubt: bool,
}

impl Coverage {
    /// `None` when no answer here could be trusted: keep drawing.
    pub fn new(window: Area, screens: &[Screen]) -> Option<Self> {
        if window.is_empty() || !trusted(screens) {
            return None;
        }
        let monitor = screens.iter().find(|screen| screen.area.contains(window))?.area;
        Some(Coverage { window, monitor, visible: vec![window], doubt: false })
    }

    /// Count an opaque window above it, rounded by `radius` if it is.
    pub fn add(&mut self, cover: Area, radius: Option<i32>) {
        if cover.right < cover.left || cover.bottom < cover.top {
            self.doubt = true;
        }
        if cover.meet(self.window).is_empty() {
            return;
        }
        if !self.monitor.contains(cover) {
            self.doubt = true;
            return;
        }
        match radius {
            Some(radius) => cut_rounded(&mut self.visible, cover, radius),
            None => cut(&mut self.visible, cover),
        }
    }

    /// A window above it couldn't be read.
    pub fn distrust(&mut self) {
        self.doubt = true;
    }

    /// Whether further windows can't change the answer.
    pub fn settled(&self) -> bool {
        self.doubt || self.visible.is_empty()
    }

    pub fn hidden(&self) -> bool {
        !self.doubt && self.visible.is_empty()
    }
}

/// Take `cover` out of `visible`, the rectangles still seen.
fn cut(visible: &mut Vec<Area>, cover: Area) {
    if cover.is_empty() {
        return;
    }
    for at in (0..visible.len()).rev() {
        let area = visible[at];
        let hit = area.meet(cover);
        if hit.is_empty() {
            continue;
        }
        visible.swap_remove(at);
        let mut keep = |piece: Area| {
            if !piece.is_empty() {
                visible.push(piece);
            }
        };
        keep(Area { bottom: hit.top, ..area });
        keep(Area { top: hit.bottom, ..area });
        keep(Area { top: hit.top, bottom: hit.bottom, right: hit.left, ..area });
        keep(Area { top: hit.top, bottom: hit.bottom, left: hit.right, ..area });
    }
}

/// Take a window with rounded corners out of `visible`, leaving a square of
/// `radius` at each corner: what shows through them is still seen.
fn cut_rounded(visible: &mut Vec<Area>, cover: Area, radius: i32) {
    let Area { left, top, right, bottom } = cover;
    cut(visible, Area { left: left + radius, right: right - radius, ..cover });
    cut(visible, Area { right: left + radius, top: top + radius, bottom: bottom - radius, ..cover });
    cut(visible, Area { left: right - radius, top: top + radius, bottom: bottom - radius, ..cover });
}

/// One window's occlusion: when to check it, and what was last reported.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    next_check: Option<Instant>,
    covered: u8,
    occluded: bool,
}

impl Tracker {
    /// Whether a frame about to present should check: occluded windows hear
    /// of a change from the system instead.
    pub fn due(&mut self, now: Instant) -> bool {
        if self.occluded || self.next_check.is_some_and(|next| now < next) {
            return false;
        }
        self.next_check = Some(now + CHECK_EVERY);
        true
    }

    /// A frame's check found this; whether it is time to settle it.
    pub fn checked(&mut self, hidden: bool) -> bool {
        self.covered = if hidden { self.covered.saturating_add(1) } else { 0 };
        self.covered >= CHECKS_TO_CONFIRM
    }

    /// Settle on what a check found, returning the change to report.
    pub fn settle(&mut self, hidden: bool) -> Option<bool> {
        if hidden == self.occluded {
            return None;
        }
        self.occluded = hidden;
        self.covered = 0;
        self.next_check = None;
        Some(hidden)
    }

    pub fn occluded(&self) -> bool {
        self.occluded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area { left: 0, top: 0, right: 1000, bottom: 800 };
    const WINDOW: Area = Area { left: 100, top: 100, right: 500, bottom: 400 };

    fn after(covers: &[Area]) -> Vec<Area> {
        let mut visible = vec![WINDOW];
        for &cover in covers {
            cut(&mut visible, cover);
        }
        visible
    }

    fn pixels(areas: &[Area]) -> i64 {
        areas.iter().map(|a| (a.right - a.left) as i64 * (a.bottom - a.top) as i64).sum()
    }

    #[test]
    fn a_larger_window_covers_all_of_it() {
        assert!(after(&[SCREEN]).is_empty());
    }

    #[test]
    fn one_pixel_uncovered_is_still_seen() {
        let cover = Area { right: 499, ..WINDOW };
        assert_eq!(after(&[cover]), [Area { left: 499, ..WINDOW }]);
    }

    #[test]
    fn two_windows_side_by_side_cover_it_together_only_when_they_meet() {
        let left = Area { right: 300, ..WINDOW };
        assert!(after(&[left, Area { left: 300, ..WINDOW }]).is_empty());
        let gap = after(&[left, Area { left: 301, ..WINDOW }]);
        assert_eq!(pixels(&gap), 300);
    }

    #[test]
    fn a_window_inside_it_leaves_a_frame_around() {
        let inner = Area { left: 200, top: 200, right: 300, bottom: 300 };
        assert_eq!(pixels(&after(&[inner])), 400 * 300 - 100 * 100);
    }

    #[test]
    fn apart_windows_and_empty_rectangles_change_nothing() {
        let apart = Area { left: 600, top: 100, right: 900, bottom: 400 };
        let empty = Area { left: 200, top: 200, right: 200, bottom: 300 };
        assert_eq!(after(&[apart, empty]), [WINDOW]);
    }

    #[test]
    fn rounded_corners_of_the_same_size_leave_the_corners_seen() {
        let mut visible = vec![WINDOW];
        cut_rounded(&mut visible, WINDOW, 8);
        assert_eq!(pixels(&visible), 4 * 8 * 8);
    }

    #[test]
    fn a_rounded_window_past_every_edge_covers_it() {
        let mut visible = vec![WINDOW];
        let past = Area { left: 90, top: 90, right: 510, bottom: 410 };
        cut_rounded(&mut visible, past, 8);
        assert!(visible.is_empty());
    }

    #[test]
    fn a_rounded_window_barely_past_the_edges_leaves_its_corners_seen() {
        let mut visible = vec![WINDOW];
        let past = Area { left: 99, top: 99, right: 501, bottom: 401 };
        cut_rounded(&mut visible, past, 8);
        assert_eq!(visible.len(), 4);
    }

    const LEFT: Area = Area { left: 0, top: 0, right: 1920, bottom: 1080 };
    const RIGHT: Area = Area { left: 1920, top: 0, right: 3840, bottom: 1080 };
    const ON_LEFT: Area = Area { left: 100, top: 100, right: 500, bottom: 400 };
    const ON_RIGHT: Area = Area { left: 2000, top: 100, right: 2400, bottom: 400 };

    fn screen(area: Area, dpi: u32) -> Screen {
        Screen { area, dpi }
    }

    /// Whether `window` is found hidden under `covers`, on these monitors.
    fn hidden(window: Area, screens: &[Screen], covers: &[Area]) -> bool {
        Coverage::new(window, screens).is_some_and(|mut coverage| {
            for &cover in covers {
                coverage.add(cover, None);
            }
            coverage.hidden()
        })
    }

    #[test]
    fn one_monitor_is_what_it_was() {
        let one = [screen(LEFT, 96)];
        assert!(hidden(ON_LEFT, &one, &[LEFT]));
        assert!(!hidden(ON_LEFT, &one, &[]));
        assert!(!hidden(ON_LEFT, &one, &[Area { right: 499, ..ON_LEFT }]));
    }

    #[test]
    fn a_window_whole_on_one_of_two_alike_monitors_can_be_covered() {
        let two = [screen(LEFT, 96), screen(RIGHT, 96)];
        assert!(hidden(ON_LEFT, &two, &[LEFT]));
        assert!(hidden(ON_RIGHT, &two, &[RIGHT]));
    }

    #[test]
    fn a_window_spanning_two_monitors_is_never_hidden() {
        let two = [screen(LEFT, 96), screen(RIGHT, 96)];
        let spanning = Area { left: 1800, top: 100, right: 2100, bottom: 400 };
        assert!(Coverage::new(spanning, &two).is_none());
        let everywhere = Area { left: 0, top: 0, right: 3840, bottom: 1080 };
        assert!(!hidden(spanning, &two, &[everywhere]));
    }

    #[test]
    fn a_cover_reaching_onto_another_monitor_is_not_believed() {
        let two = [screen(LEFT, 96), screen(RIGHT, 96)];
        let spanning = Area { left: 0, top: 0, right: 2100, bottom: 1080 };
        assert!(!hidden(ON_LEFT, &two, &[spanning]));
        // Not even with a window whole on the monitor below it: doubt stays.
        assert!(!hidden(ON_LEFT, &two, &[spanning, LEFT]));
    }

    #[test]
    fn a_window_on_another_monitor_changes_nothing() {
        let two = [screen(LEFT, 96), screen(RIGHT, 96)];
        assert!(hidden(ON_LEFT, &two, &[RIGHT, LEFT]));
        assert!(!hidden(ON_LEFT, &two, &[RIGHT]));
    }

    #[test]
    fn a_cover_past_the_monitors_edge_is_not_believed() {
        let one = [screen(LEFT, 96)];
        let past = Area { left: -40, top: 60, right: 600, bottom: 500 };
        assert!(!hidden(ON_LEFT, &one, &[past]));
        let below = Area { left: 0, top: 0, right: 1920, bottom: 1200 };
        assert!(!hidden(ON_LEFT, &one, &[below]));
    }

    #[test]
    fn a_window_past_the_monitors_edge_is_never_hidden() {
        let one = [screen(LEFT, 96)];
        let off = Area { left: -60, top: 100, right: 300, bottom: 400 };
        assert!(Coverage::new(off, &one).is_none());
        let off = Area { left: 1700, top: 100, right: 2000, bottom: 400 };
        assert!(Coverage::new(off, &one).is_none());
    }

    #[test]
    fn monitors_of_different_scales_are_not_trusted() {
        let mixed = [screen(LEFT, 96), screen(RIGHT, 144)];
        assert!(Coverage::new(ON_LEFT, &mixed).is_none());
        assert!(Coverage::new(ON_RIGHT, &mixed).is_none());
        assert!(!hidden(ON_LEFT, &mixed, &[LEFT]));
    }

    #[test]
    fn a_monitor_at_negative_coordinates_is_not_trusted() {
        let west = Area { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert!(Coverage::new(ON_LEFT, &[screen(LEFT, 96), screen(west, 96)]).is_none());
        let north = Area { left: 0, top: -1080, right: 1920, bottom: 0 };
        assert!(Coverage::new(ON_LEFT, &[screen(LEFT, 96), screen(north, 96)]).is_none());
    }

    #[test]
    fn overlapping_or_unknown_monitors_are_not_trusted() {
        assert!(Coverage::new(ON_LEFT, &[]).is_none());
        assert!(Coverage::new(ON_LEFT, &[screen(LEFT, 96), screen(LEFT, 96)]).is_none());
        assert!(Coverage::new(ON_LEFT, &[screen(LEFT, 0)]).is_none());
        let none = Area { left: 0, top: 0, right: 0, bottom: 0 };
        assert!(Coverage::new(ON_LEFT, &[screen(LEFT, 96), screen(none, 96)]).is_none());
    }

    #[test]
    fn an_empty_window_is_never_hidden() {
        let empty = Area { left: 100, top: 100, right: 100, bottom: 100 };
        assert!(Coverage::new(empty, &[screen(LEFT, 96)]).is_none());
    }

    #[test]
    fn a_cover_with_its_edges_crossed_is_doubted() {
        let crossed = Area { left: 800, top: 100, right: 700, bottom: 400 };
        assert!(!hidden(ON_LEFT, &[screen(LEFT, 96)], &[crossed, LEFT]));
    }

    #[test]
    fn a_cover_that_cant_be_read_is_doubted() {
        let mut coverage = Coverage::new(ON_LEFT, &[screen(LEFT, 96)]).unwrap();
        coverage.distrust();
        assert!(coverage.settled());
        coverage.add(LEFT, None);
        assert!(!coverage.hidden());
    }

    #[test]
    fn a_rounded_cover_counts_its_corners_on_any_layout() {
        let two = [screen(LEFT, 96), screen(RIGHT, 96)];
        let mut coverage = Coverage::new(ON_RIGHT, &two).unwrap();
        coverage.add(ON_RIGHT, Some(8));
        assert!(!coverage.hidden());
        assert!(!coverage.settled());
        coverage.add(RIGHT, Some(8));
        assert!(coverage.hidden());
    }

    #[test]
    fn checks_keep_to_their_interval() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        assert!(tracker.due(start));
        assert!(!tracker.due(start + CHECK_EVERY / 2));
        assert!(tracker.due(start + CHECK_EVERY));
    }

    #[test]
    fn one_covered_check_settles_nothing() {
        let mut tracker = Tracker::default();
        assert!(!tracker.checked(true));
        assert!(!tracker.checked(false));
        assert!(!tracker.checked(true));
        assert!(tracker.checked(true));
    }

    #[test]
    fn an_occluded_window_reports_each_change_once_and_stops_checking() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        assert_eq!(tracker.settle(false), None);
        assert_eq!(tracker.settle(true), Some(true));
        assert_eq!(tracker.settle(true), None);
        assert!(!tracker.due(now));
        assert_eq!(tracker.settle(false), Some(false));
        assert_eq!(tracker.settle(false), None);
        assert!(tracker.due(now));
    }

    #[test]
    fn a_covered_run_starts_again_after_a_report() {
        let mut tracker = Tracker::default();
        tracker.checked(true);
        tracker.checked(true);
        tracker.settle(true);
        tracker.settle(false);
        assert!(!tracker.checked(true));
    }
}
