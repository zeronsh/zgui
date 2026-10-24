//! Scheduling for retained frames that could not yet reach the native surface.
use std::time::{Duration, Instant};
use zgx_gpu::PresentationStatus;

#[derive(Default)]
pub(crate) struct Presentation {
    pending: bool,
    occluded: bool,
    timeouts: u8,
    retry_at: Option<Instant>,
}
impl Presentation {
    /// Preserve scene damage and stop timed presentation retries while the
    /// platform confirms that no frame can be displayed. Unknown visibility
    /// must remain drawable (notably on compositors without a visibility query).
    pub fn render_allowed(
        &mut self,
        size: (u32, u32),
        visible: Option<bool>,
        minimized: Option<bool>,
        native_occluded: bool,
    ) -> bool {
        let allowed = size.0 > 0
            && size.1 > 0
            && visible != Some(false)
            && minimized != Some(true)
            && !native_occluded
            && !self.occluded;
        if !allowed {
            self.retry_at = None;
            self.timeouts = 0;
        }
        allowed
    }

    pub fn damaged(&mut self) {
        self.pending = true;
        if self.timeouts >= 4 {
            self.timeouts = 0;
        }
    }
    pub fn expose(&mut self) {
        self.occluded = false;
        self.timeouts = 0;
        self.retry_at = None;
    }
    pub fn occlude(&mut self) {
        self.occluded = true;
        self.retry_at = None;
    }
    pub fn ready(&self, now: Instant) -> bool {
        self.pending
            && !self.occluded
            && self.timeouts < 4
            && self.retry_at.is_none_or(|deadline| now >= deadline)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.retry_at
    }
    pub fn completed(&mut self, status: PresentationStatus, now: Instant) {
        self.retry_at = None;
        match status {
            PresentationStatus::Presented | PresentationStatus::Offscreen => {
                self.pending = false;
                self.timeouts = 0;
            }
            PresentationStatus::Timeout => {
                self.timeouts += 1;
                if self.timeouts < 4 {
                    self.retry_at = Some(now + Duration::from_millis(16 << (self.timeouts - 1)));
                }
            }
            PresentationStatus::Occluded => self.occlude(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_retries_retained_frame_without_new_damage_then_sleeps() {
        let mut p = Presentation::default();
        let mut now = Instant::now();
        p.damaged();
        for _ in 0..4 {
            assert!(p.ready(now));
            p.completed(PresentationStatus::Timeout, now);
            assert!(!p.ready(now));
            if let Some(deadline) = p.deadline() {
                assert!(deadline > now);
                now = deadline;
            }
        }
        assert_eq!(p.deadline(), None);
        p.damaged();
        assert!(p.ready(now));
        p.completed(PresentationStatus::Occluded, now);
        p.expose();
        assert!(p.ready(now));
        p.completed(PresentationStatus::Presented, now);
        assert!(!p.ready(now));
        assert_eq!(p.deadline(), None);
    }
    #[test]
    fn occlusion_preserves_new_damage_until_visibility_returns() {
        let mut p = Presentation::default();
        let now = Instant::now();
        p.damaged();
        p.completed(PresentationStatus::Occluded, now);
        p.damaged();
        assert!(!p.ready(now));
        assert_eq!(p.deadline(), None);
        p.expose();
        assert!(p.ready(now));
        p.completed(PresentationStatus::Presented, now);
        assert!(!p.ready(now));
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    use zgx::{compose::prelude::*, widgets::Ui};
    #[test]
    fn hidden_updates_preserve_damage_until_restoration() {
        let mut p = Presentation::default();
        let mut ui = Ui::new(200., 100.);
        let value = ui.signal(0);
        let read = value.clone();
        ui.mount(text_signal(move || read.get().to_string()));
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
        for i in 1..100 {
            value.set(i);
            assert!(!p.render_allowed((200, 100), Some(false), Some(false), false));
        }
        assert!(p.render_allowed((200, 100), Some(true), Some(false), false));
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        assert!(!frame.damage.is_empty());
        assert_eq!(value.get(), 99);
        assert!(ui.scene.borrow_mut().flush().is_idle());
    }
    #[test]
    fn invisible_surfaces_cancel_retry_deadlines_but_keep_pending_frame() {
        let now = Instant::now();
        for (size, visible, minimized, occluded) in [
            ((0, 100), None, None, false),
            ((100, 100), Some(false), None, false),
            ((100, 100), None, Some(true), false),
            ((100, 100), None, None, true),
        ] {
            let mut p = Presentation::default();
            p.damaged();
            p.completed(PresentationStatus::Timeout, now);
            assert!(p.deadline().is_some());
            assert!(!p.render_allowed(size, visible, minimized, occluded));
            assert_eq!(p.deadline(), None);
            assert!(p.render_allowed((100, 100), None, None, false));
            assert!(p.ready(now));
        }
    }
    #[test]
    fn redraw_cannot_override_platform_occlusion() {
        let mut p = Presentation::default();
        p.occlude();
        p.expose(); // application requests a redraw while still covered
        assert!(!p.render_allowed((100, 100), Some(true), None, true));
        assert!(p.render_allowed((100, 100), Some(true), None, false));
        p.completed(PresentationStatus::Occluded, Instant::now());
        assert!(!p.render_allowed((100, 100), None, None, false));
        p.expose();
        assert!(p.render_allowed((100, 100), None, None, false));
    }
}
