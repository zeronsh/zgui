//! Elastic (rubber-band) scrolling, as macOS's scroll views and Safari's pages
//! do it: a trackpad gesture pushed past a scroll view's edge stretches it,
//! harder the further it goes, and letting go springs it back. Momentum that
//! reaches an edge bounces. The offset an app sees never leaves its range;
//! only the content's translation shows the stretch.
//!
//! The host turns it on (`Ui::set_elastic_scrolling`) and reports a trackpad
//! gesture's phases (`InputEvent::ScrollGesture`); a mouse wheel's notches,
//! and scrolling an app does, never stretch. Off, nothing here runs.
use crate::{
    input::{DispatchResult, InputEvent, ScrollGesture},
    reactive::Signal,
    scene::NodeId,
    widgets::Ui,
};
use std::{
    cell::Cell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

/// How stiff the stretch is (AppKit's and UIKit's rubber-band constant).
const RESISTANCE: f32 = 0.55;
/// How fast it springs back: critically damped, settled in about 0.4 s.
const SPRING: f32 = 14.;
/// Momentum that reaches an edge pushes past it this long, then springs back.
const BOUNCE: Duration = Duration::from_millis(90);

/// One scroll view's stretch.
pub(crate) struct Elastic {
    pub(crate) horizontal: bool,
    /// The view's size and its content's (along both axes).
    size: Signal<(f32, f32)>,
    extent: Signal<(f32, f32)>,
    offset: Signal<f32>,
    /// What the gesture pushed past the edge: before the spring's curve
    /// (negative before the start, positive after the end).
    pushed: Cell<f32>,
    /// What shows: `pushed` through the curve, or the spring's position.
    pub(crate) shown: Signal<f32>,
    /// Springing back: from where and since when, and its next step.
    spring: Cell<Option<(f32, Instant)>>,
    next: Cell<Option<Instant>>,
    /// Momentum reached the edge then: it springs back `BOUNCE` after.
    bounced: Cell<Option<Instant>>,
}
impl Elastic {
    fn main(&self, size: (f32, f32)) -> f32 {
        if self.horizontal { size.0 } else { size.1 }
    }
    fn length(&self) -> f32 {
        self.main(self.size.with_untracked(|s| *s))
    }
    fn limit(&self) -> f32 {
        (self.main(self.extent.with_untracked(|s| *s)) - self.length()).max(0.)
    }
    /// `pushed` as it shows: ever less for each further pixel, never past
    /// the view's length.
    fn curve(&self, pushed: f32) -> f32 {
        let length = self.length().max(1.);
        let shown = (1. - 1. / (pushed.abs() * RESISTANCE / length + 1.)) * length;
        shown.copysign(pushed)
    }
    /// What push shows `shown`: `curve`'s inverse.
    fn uncurve(&self, shown: f32) -> f32 {
        let length = self.length().max(1.);
        let shown_abs = shown.abs().min(length - 0.5);
        ((length / (length - shown_abs) - 1.) * length / RESISTANCE).copysign(shown)
    }
    fn push(&self, delta: f32) {
        // Caught as it springs back: on from where it shows.
        if self.spring.take().is_some() {
            self.pushed
                .set(self.uncurve(self.shown.with_untracked(|s| *s)));
            self.next.set(None);
        }
        let pushed = self.pushed.get() + delta;
        self.pushed.set(pushed);
        self.shown.set(self.curve(pushed));
    }
    fn release(&self, now: Instant) {
        if self.shown.with_untracked(|s| *s) == 0. {
            return;
        }
        self.spring
            .set(Some((self.shown.with_untracked(|s| *s), now)));
        self.next.set(Some(now));
        self.bounced.set(None);
    }
    fn stretched(&self) -> bool {
        self.shown.with_untracked(|s| *s) != 0.
    }
}

/// The document's elastic state.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) enabled: bool,
    /// Every scroll view's, by its root.
    pub(crate) views: Vec<(NodeId, Weak<Elastic>)>,
    /// The trackpad's fingers are down, or its momentum runs.
    fingers: bool,
    momentum: bool,
    /// The momentum bounced off an edge: the rest of it is spent there, as
    /// AppKit's is, not bounced again each time the spring settles.
    bounced: bool,
    /// The view the gesture stretches (or springs back).
    active: Option<Weak<Elastic>>,
}

/// `root`'s scroll view stretches: its translation adds `shown`.
pub(crate) fn register(
    ui: &mut Ui,
    root: NodeId,
    horizontal: bool,
    size: Signal<(f32, f32)>,
    extent: Signal<(f32, f32)>,
    offset: Signal<f32>,
) -> Signal<f32> {
    let shown = ui.signal(0_f32);
    let elastic = Rc::new(Elastic {
        horizontal,
        size,
        extent,
        offset,
        pushed: Cell::new(0.),
        shown: shown.clone(),
        spring: Cell::new(None),
        next: Cell::new(None),
        bounced: Cell::new(None),
    });
    {
        let mut state = ui.elastic_state().borrow_mut();
        // Unmounted views' entries go now and then, not on every mount.
        if state.views.len() >= 64 && state.views.len().is_power_of_two() {
            state.views.retain(|(_, view)| view.strong_count() > 0);
        }
        state.views.push((root, Rc::downgrade(&elastic)));
    }
    ui.retain(root, elastic);
    shown
}

fn active(ui: &Ui) -> Option<Rc<Elastic>> {
    ui.elastic_state().borrow().active.as_ref()?.upgrade()
}

/// A trackpad gesture's phase: what lifting the fingers or the momentum's
/// end lets spring back.
pub(crate) fn gesture(ui: &Ui, phase: ScrollGesture) {
    let now = Instant::now();
    let release = {
        let mut state = ui.elastic_state().borrow_mut();
        match phase {
            ScrollGesture::Began => {
                state.fingers = true;
                state.momentum = false;
                state.bounced = false;
                false
            }
            ScrollGesture::Ended => {
                state.fingers = false;
                true
            }
            ScrollGesture::MomentumBegan => {
                state.momentum = true;
                state.bounced = false;
                false
            }
            ScrollGesture::MomentumEnded => {
                state.momentum = false;
                true
            }
        }
    };
    if release && let Some(view) = active(ui) {
        view.release(now);
    }
}

/// Before a scroll is dispatched: a stretched view takes what pulls it back
/// first, and only the rest scrolls. Returns the event to dispatch, if any.
pub(crate) fn before_scroll(ui: &Ui, event: InputEvent) -> Option<InputEvent> {
    let InputEvent::Scroll {
        x,
        y,
        mut delta_x,
        mut delta_y,
    } = event
    else {
        return Some(event);
    };
    let Some(view) = active(ui).filter(|view| view.stretched() && view.spring.get().is_none())
    else {
        return Some(event);
    };
    let delta = if view.horizontal {
        &mut delta_x
    } else {
        &mut delta_y
    };
    let pushed = view.pushed.get();
    // Toward the content: undo the push, then scroll with what is left.
    if pushed * *delta < 0. {
        let back = delta.abs().min(pushed.abs()).copysign(*delta);
        view.push(back);
        *delta -= back;
        if view.pushed.get().abs() < 0.5 {
            view.pushed.set(0.);
            view.shown.set(0.);
        }
    }
    (*delta != 0. || delta_x != 0. || delta_y != 0.).then_some(InputEvent::Scroll {
        x,
        y,
        delta_x,
        delta_y,
    })
}

/// After a scroll no view took: during a trackpad gesture, the view it is
/// latched to stretches, or the outermost one under the pointer that
/// scrolls along its axis (as a page bounces, not a box inside it).
pub(crate) fn after_scroll(ui: &Ui, event: &InputEvent, result: &DispatchResult) {
    let InputEvent::Scroll {
        delta_x, delta_y, ..
    } = *event
    else {
        return;
    };
    let (fingers, momentum) = {
        let state = ui.elastic_state().borrow();
        (state.fingers, state.momentum)
    };
    if result.default_prevented || !(fingers || momentum) {
        return;
    }
    let horizontal = delta_x.abs() > delta_y.abs();
    let delta = if horizontal { delta_x } else { delta_y };
    if delta == 0. || !delta.is_finite() {
        return;
    }
    let now = Instant::now();
    let view = active(ui)
        .filter(|view| view.horizontal == horizontal && view.stretched())
        .or_else(|| {
            let target = result.target?;
            let scene = ui.scene.borrow();
            let state = ui.elastic_state().borrow();
            std::iter::once(target)
                .chain(scene.ancestors(target))
                .filter_map(|node| {
                    state
                        .views
                        .iter()
                        .find(|(root, _)| *root == node)
                        .and_then(|(_, view)| view.upgrade())
                })
                .filter(|view| view.horizontal == horizontal && view.limit() > 0.)
                .last()
        });
    let Some(view) = view else {
        return;
    };
    // Only past the edge the delta pushes toward.
    let at = view.offset.with_untracked(|o| *o);
    let limit = view.limit();
    let outward = (delta < 0. && at <= 0.) || (delta > 0. && at >= limit);
    if !outward {
        return;
    }
    if view.spring.get().is_some() && !fingers {
        // Momentum toward an edge springing back is spent in the spring.
        ui.elastic_state().borrow_mut().bounced = true;
        return;
    }
    if momentum && !fingers {
        // Momentum bounces off the edge: a moment's push, then back.
        match view.bounced.get() {
            // Once a gesture's momentum: what is left of it stops at the edge.
            None if ui.elastic_state().borrow().bounced => return,
            None => {
                ui.elastic_state().borrow_mut().bounced = true;
                view.bounced.set(Some(now));
                view.next.set(Some(now + BOUNCE));
            }
            Some(at) if now.saturating_duration_since(at) >= BOUNCE => {
                view.release(now);
                return;
            }
            Some(_) => {}
        }
    }
    view.push(delta);
    ui.elastic_state().borrow_mut().active = Some(Rc::downgrade(&view));
}

/// When the stretched view next steps back. Nothing stretched, nothing.
pub(crate) fn next_step(ui: &Ui) -> Option<Instant> {
    active(ui)?.next.get()
}

/// Step the spring (or end a bounce) due by `now`; whether it moved.
pub(crate) fn advance(ui: &Ui, now: Instant) -> bool {
    let Some(view) = active(ui) else {
        return false;
    };
    if view.next.get().is_none_or(|next| next > now) {
        return false;
    }
    if view.spring.get().is_none() {
        // A bounce's push is over.
        if view.bounced.get().is_some() {
            view.release(now);
        } else {
            view.next.set(None);
            return false;
        }
    }
    let Some((from, since)) = view.spring.get() else {
        return false;
    };
    let t = now.saturating_duration_since(since).as_secs_f32();
    let shown = from * (1. + SPRING * t) * (-SPRING * t).exp();
    if shown.abs() < 0.5 {
        view.spring.set(None);
        view.next.set(None);
        view.pushed.set(0.);
        view.shown.set(0.);
        ui.elastic_state().borrow_mut().active = None;
    } else {
        view.shown.set(shown);
        // A step a refresh (`Ui::set_refresh_interval`).
        view.next.set(Some(now + ui.refresh_interval()));
    }
    true
}
