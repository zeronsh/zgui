//! Whether drawables have settled a refresh behind (`PhaseWatch`).
//!
//! A `CAMetalLayer`'s drawables come back from the window server once
//! shown. Frames that start with the pool full wait most of a refresh for
//! one in `nextDrawable`, every frame, and show a refresh later than they
//! could: the pipeline sits a frame deep and stays so (most launches of an
//! M1 Max on the `CVDisplayLink`, 2026-10-27). One frame not started gives
//! the pool a drawable back and puts frames in phase again: the host skips
//! a tick when the watch says so (`GpuRenderer::take_rephase`).
use std::time::Duration;

/// A wait for a drawable this long is the pool full, not the GPU finishing.
pub(crate) const SLOW_WAIT: Duration = Duration::from_millis(3);
/// Slow waits in a row before frames are put back in phase: a stall now
/// and then fills the pool for a frame or two and drains by itself. Three
/// in a row rephased more often for no gain (M1 Max, 2026-10-27).
pub(crate) const SLOW_RUN: u32 = 8;

/// Slow waits for drawables in a row.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PhaseWatch {
    slow: u32,
}
impl PhaseWatch {
    /// One frame's wait for its drawable; whether to skip a tick now.
    pub(crate) fn observe(&mut self, wait: Duration) -> bool {
        if wait < SLOW_WAIT {
            self.slow = 0;
            return false;
        }
        self.slow += 1;
        if self.slow >= SLOW_RUN {
            self.slow = 0;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SLOW: Duration = Duration::from_millis(7);
    const QUICK: Duration = Duration::from_micros(80);

    #[test]
    fn a_run_of_slow_waits_asks_once_then_starts_again() {
        let mut watch = PhaseWatch::default();
        let asked: Vec<bool> = (0..SLOW_RUN * 2).map(|_| watch.observe(SLOW)).collect();
        let at: Vec<usize> = asked
            .iter()
            .enumerate()
            .filter(|(_, a)| **a)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(at, [SLOW_RUN as usize - 1, SLOW_RUN as usize * 2 - 1]);
    }
    #[test]
    fn a_quick_wait_breaks_the_run() {
        let mut watch = PhaseWatch::default();
        for _ in 0..SLOW_RUN - 1 {
            assert!(!watch.observe(SLOW));
        }
        assert!(!watch.observe(QUICK));
        for _ in 0..SLOW_RUN - 1 {
            assert!(!watch.observe(SLOW));
        }
        assert!(watch.observe(SLOW));
    }
    #[test]
    fn quick_waits_never_ask() {
        let mut watch = PhaseWatch::default();
        assert!((0..1000).all(|_| !watch.observe(QUICK)));
    }
}
