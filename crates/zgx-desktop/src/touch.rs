//! Touch and pen, as zgx's pointer: winit reports touchscreen contacts and
//! pens as `Touch`es (on Windows it takes the pointer messages Windows would
//! otherwise have turned into a mouse's), and zgx knows a pointer.
//!
//! A pen is a mouse. A finger presses where it lands, as in a browser, and
//! then:
//! - lifts where it was: a click; held there first (`HOLD`), the secondary
//!   button's click, as Windows' press-and-hold right-clicks;
//! - moves: once past `SLOP` the move is offered to the scroll views under
//!   it as a scroll. One that scrolls makes it a pan, the press is cancelled
//!   and the content follows the finger, coasting on after it lifts
//!   (`Fling`); none, and it is a drag of what it pressed (a slider, a
//!   selection), as a mouse's.
//!
//! A second finger pans by the fingers' middle. Pinching does nothing: zgx
//! has no gesture for it.
use std::time::{Duration, Instant};
use winit::event::TouchPhase;
use zgx::input::{InputEvent, PointerButton};

/// How far a finger moves before it pans or drags, in logical pixels.
const SLOP: f32 = 8.;
/// How long a finger rests before lifting right-clicks.
const HOLD: Duration = Duration::from_millis(600);
/// How long a fling's speed takes to fall to 1/e.
const COAST: f32 = 0.325;
/// Slower than this (logical pixels a second), a lift does not fling, and a
/// fling stops.
const STILL: f32 = 40.;
/// How far back a lift looks for the finger's speed.
const SPEED_WINDOW: Duration = Duration::from_millis(80);

/// What the host does for a touch.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    Dispatch(InputEvent),
    /// The press was the start of a pan: cancel it, and give focus back
    /// to what had it before the finger landed (a pan focuses nothing, as
    /// in a browser).
    Unpress,
    /// Offer this movement to the scroll views at (`x`, `y`) as a scroll and
    /// say whether one took it (`Touches::decided`): a pan, or a drag.
    TryPan {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Idle,
    Pen,
    /// A finger down and still within the slop.
    Pressed {
        start: (f32, f32),
        at: Instant,
    },
    /// Waiting on `decided` for the first move past the slop.
    Deciding,
    Dragging,
    Panning,
    /// Held on selectable text: the word is selected, and moves extend it.
    Selecting,
}

/// A fling: the speed a pan lifted at, falling off with time.
#[derive(Clone, Copy, Debug)]
struct Fling {
    at: (f32, f32),
    speed: (f32, f32),
    began: Instant,
    /// How far into the fling its last step went, in seconds.
    elapsed: f32,
}

pub(crate) struct Touches {
    mode: Mode,
    /// The fingers down: id and position.
    contacts: Vec<(u64, (f32, f32))>,
    /// Where the pan last was: the fingers' middle.
    pan: (f32, f32),
    /// The pan's recent positions, for its speed at lift.
    trail: Vec<(Instant, (f32, f32))>,
    /// A touchpad scroll's recent steps (`wheel`), for its speed at lift.
    #[cfg(target_os = "linux")]
    wheel_trail: Vec<(Instant, (f32, f32))>,
    fling: Option<Fling>,
    /// The press has been held long enough, and the host told (`hold`).
    held: bool,
}

impl Default for Touches {
    fn default() -> Self {
        Self {
            mode: Mode::Idle,
            contacts: Vec::new(),
            pan: (0., 0.),
            trail: Vec::new(),
            #[cfg(target_os = "linux")]
            wheel_trail: Vec::new(),
            fling: None,
            held: false,
        }
    }
}

fn middle(contacts: &[(u64, (f32, f32))]) -> (f32, f32) {
    let n = contacts.len().max(1) as f32;
    let (x, y) = contacts
        .iter()
        .fold((0., 0.), |(x, y), (_, (cx, cy))| (x + cx, y + cy));
    (x / n, y / n)
}

impl Touches {
    /// A touch of contact `id` at logical (`x`, `y`); `pen` if it is a pen's.
    pub(crate) fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        (x, y): (f32, f32),
        pen: bool,
        now: Instant,
    ) -> Vec<Step> {
        use InputEvent::*;
        let primary = PointerButton::Primary;
        if pen || self.mode == Mode::Pen {
            return match phase {
                TouchPhase::Started => {
                    self.mode = Mode::Pen;
                    self.fling = None;
                    vec![
                        Step::Dispatch(PointerMove { x, y }),
                        Step::Dispatch(PointerDown {
                            x,
                            y,
                            button: primary,
                        }),
                    ]
                }
                // A pen over the screen moves the pointer as a mouse does.
                TouchPhase::Moved => vec![Step::Dispatch(PointerMove { x, y })],
                TouchPhase::Ended => {
                    self.mode = Mode::Idle;
                    vec![Step::Dispatch(PointerUp {
                        x,
                        y,
                        button: primary,
                    })]
                }
                TouchPhase::Cancelled => {
                    self.mode = Mode::Idle;
                    vec![Step::Dispatch(PointerCancel)]
                }
            };
        }
        match phase {
            TouchPhase::Started => {
                self.contacts.retain(|(c, _)| *c != id);
                self.contacts.push((id, (x, y)));
                match self.mode {
                    Mode::Idle => {
                        // A finger landing stops a fling.
                        self.fling = None;
                        self.held = false;
                        self.mode = Mode::Pressed {
                            start: (x, y),
                            at: now,
                        };
                        self.trail.clear();
                        vec![
                            Step::Dispatch(PointerMove { x, y }),
                            Step::Dispatch(PointerDown {
                                x,
                                y,
                                button: primary,
                            }),
                        ]
                    }
                    // A second finger: the press was not a click, and the
                    // fingers pan together.
                    Mode::Pressed { .. } | Mode::Deciding => {
                        self.mode = Mode::Panning;
                        self.pan = middle(&self.contacts);
                        self.trail = vec![(now, self.pan)];
                        vec![Step::Unpress]
                    }
                    Mode::Panning => {
                        self.pan = middle(&self.contacts);
                        Vec::new()
                    }
                    Mode::Dragging | Mode::Selecting | Mode::Pen => Vec::new(),
                }
            }
            TouchPhase::Moved => {
                let Some(contact) = self.contacts.iter_mut().find(|(c, _)| *c == id) else {
                    return Vec::new();
                };
                contact.1 = (x, y);
                let first = self.contacts.first().is_some_and(|(c, _)| *c == id);
                match self.mode {
                    Mode::Pressed { start, .. } => {
                        if !first || (x - start.0).hypot(y - start.1) < SLOP {
                            return Vec::new();
                        }
                        self.mode = Mode::Deciding;
                        self.pan = (x, y);
                        self.trail = vec![(now, (x, y))];
                        vec![Step::TryPan {
                            x,
                            y,
                            dx: start.0 - x,
                            dy: start.1 - y,
                        }]
                    }
                    Mode::Dragging | Mode::Selecting if first => {
                        vec![Step::Dispatch(PointerMove { x, y })]
                    }
                    Mode::Panning => {
                        let at = middle(&self.contacts);
                        let (dx, dy) = (self.pan.0 - at.0, self.pan.1 - at.1);
                        self.pan = at;
                        self.trail.push((now, at));
                        self.trail
                            .retain(|(t, _)| now.duration_since(*t) <= SPEED_WINDOW * 2);
                        if dx == 0. && dy == 0. {
                            return Vec::new();
                        }
                        vec![Step::Dispatch(Scroll {
                            x: at.0,
                            y: at.1,
                            delta_x: dx,
                            delta_y: dy,
                        })]
                    }
                    _ => Vec::new(),
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let cancelled = phase == TouchPhase::Cancelled;
                self.contacts.retain(|(c, _)| *c != id);
                if !self.contacts.is_empty() {
                    // A finger left the pan: it goes on from those still down.
                    if self.mode == Mode::Panning {
                        self.pan = middle(&self.contacts);
                        self.trail = vec![(now, self.pan)];
                    }
                    return Vec::new();
                }
                let mode = std::mem::replace(&mut self.mode, Mode::Idle);
                let mut steps = match mode {
                    _ if cancelled => vec![Step::Dispatch(PointerCancel)],
                    Mode::Pressed { at, .. } if now.duration_since(at) >= HOLD => {
                        let secondary = PointerButton::Secondary;
                        vec![
                            Step::Dispatch(PointerCancel),
                            Step::Dispatch(PointerDown {
                                x,
                                y,
                                button: secondary,
                            }),
                            Step::Dispatch(PointerUp {
                                x,
                                y,
                                button: secondary,
                            }),
                        ]
                    }
                    Mode::Pressed { .. } | Mode::Dragging | Mode::Deciding => {
                        vec![Step::Dispatch(PointerUp {
                            x,
                            y,
                            button: primary,
                        })]
                    }
                    // The selection stays, and its menu opens (Copy), as a
                    // browser's does after a long press.
                    Mode::Selecting => {
                        let secondary = PointerButton::Secondary;
                        vec![
                            Step::Dispatch(PointerUp {
                                x,
                                y,
                                button: primary,
                            }),
                            Step::Dispatch(PointerDown {
                                x,
                                y,
                                button: secondary,
                            }),
                            Step::Dispatch(PointerUp {
                                x,
                                y,
                                button: secondary,
                            }),
                        ]
                    }
                    Mode::Panning => {
                        self.fling = self.lift_speed(now).map(|speed| Fling {
                            at: self.pan,
                            speed,
                            began: now,
                            elapsed: 0.,
                        });
                        Vec::new()
                    }
                    Mode::Idle | Mode::Pen => Vec::new(),
                };
                // No finger hovers: what it pressed shows no hover after.
                steps.push(Step::Dispatch(PointerLeave));
                steps
            }
        }
    }

    /// When a finger resting within the slop has been held long enough
    /// (`hold`), for the host to wake then.
    pub(crate) fn hold_deadline(&self) -> Option<Instant> {
        match self.mode {
            Mode::Pressed { at, .. } if !self.held => Some(at + HOLD),
            _ => None,
        }
    }

    /// Where the finger rests, once, when it has been held long enough: the
    /// host selects the word there, if it is selectable text (`selecting`).
    pub(crate) fn hold(&mut self, now: Instant) -> Option<(f32, f32)> {
        let deadline = self.hold_deadline()?;
        if now < deadline {
            return None;
        }
        self.held = true;
        self.contacts.first().map(|(_, at)| *at)
    }

    /// The held finger selected a word: moves extend it, and lifting opens
    /// its menu.
    pub(crate) fn selecting(&mut self) {
        self.mode = Mode::Selecting;
    }

    /// No finger is down: a touch starting now begins a gesture.
    pub(crate) fn idle(&self) -> bool {
        self.contacts.is_empty()
    }

    /// Whether the scroll `TryPan` offered was taken: a pan, or a drag.
    pub(crate) fn decided(&mut self, panned: bool) -> Vec<Step> {
        if self.mode != Mode::Deciding {
            return Vec::new();
        }
        if panned {
            self.mode = Mode::Panning;
            vec![Step::Unpress]
        } else {
            self.mode = Mode::Dragging;
            let (x, y) = self.pan;
            vec![Step::Dispatch(InputEvent::PointerMove { x, y })]
        }
    }

    /// The pan's speed as the fingers lifted, if it was moving.
    fn lift_speed(&self, now: Instant) -> Option<(f32, f32)> {
        let (_, last) = *self.trail.last()?;
        let (since, first) = *self
            .trail
            .iter()
            .find(|(t, _)| now.duration_since(*t) <= SPEED_WINDOW)?;
        let seconds = now.duration_since(since).as_secs_f32().max(1. / 240.);
        // Scrolling runs against the finger.
        let speed = ((first.0 - last.0) / seconds, (first.1 - last.1) / seconds);
        (speed.0.hypot(speed.1) >= STILL).then_some(speed)
    }

    /// Whether a fling is coasting: frames should follow until it stops.
    pub(crate) fn flinging(&self) -> bool {
        self.fling.is_some()
    }

    /// The fling's scroll since its last step, at `now`: where, and how far.
    pub(crate) fn coast(&mut self, now: Instant) -> Option<(f32, f32, f32, f32)> {
        let fling = self.fling.as_mut()?;
        let t = now.duration_since(fling.began).as_secs_f32();
        // Distance travelled is the speed's integral: speed · COAST · (1 - e^(-t/COAST)).
        let along = |t: f32| COAST * (1. - (-t / COAST).exp());
        let step = along(t) - along(fling.elapsed);
        fling.elapsed = t;
        let speed = (-t / COAST).exp() * fling.speed.0.hypot(fling.speed.1);
        let (x, y) = fling.at;
        let delta = (fling.speed.0 * step, fling.speed.1 * step);
        if speed < STILL {
            self.fling = None;
        }
        Some((x, y, delta.0, delta.1))
    }

    /// A touchpad's two-finger scroll of `delta` at `at` (Linux, where
    /// Wayland reports the fingers lifting: libinput's axis stop, `Ended`).
    /// The fingers lifting at speed fling it, as GTK coasts it; fingers
    /// back on the touchpad stop the fling, as does a click (`stop`).
    #[cfg(target_os = "linux")]
    pub(crate) fn wheel(
        &mut self,
        phase: TouchPhase,
        at: (f32, f32),
        delta: (f32, f32),
        now: Instant,
    ) {
        if phase == TouchPhase::Started {
            self.fling = None;
            self.wheel_trail.clear();
        }
        self.wheel_trail
            .retain(|(t, _)| now.duration_since(*t) <= SPEED_WINDOW);
        self.wheel_trail.push((now, delta));
        match phase {
            TouchPhase::Ended => {
                if let Some(speed) = wheel_speed(&self.wheel_trail)
                    && self.mode == Mode::Idle
                {
                    self.fling = Some(Fling {
                        at,
                        speed,
                        began: now,
                        elapsed: 0.,
                    });
                }
                self.wheel_trail.clear();
            }
            TouchPhase::Cancelled => self.wheel_trail.clear(),
            _ => {}
        }
    }

    /// The fling hit the end of what it scrolls: it stops.
    pub(crate) fn stop(&mut self) {
        self.fling = None;
    }
}

/// A touchpad scroll's speed over its last steps (`SPEED_WINDOW`), in the
/// scroll's direction; `None` below `STILL`. Each step is the scroll since
/// the one before, so the first only marks when the span starts.
#[cfg(target_os = "linux")]
fn wheel_speed(trail: &[(Instant, (f32, f32))]) -> Option<(f32, f32)> {
    let ((first, _), (last, _)) = (trail.first()?, trail.last()?);
    let seconds = last.duration_since(*first).as_secs_f32();
    if seconds < 0.008 {
        return None;
    }
    let (dx, dy) = trail[1..]
        .iter()
        .fold((0., 0.), |(x, y), (_, (dx, dy))| (x + dx, y + dy));
    let speed = (dx / seconds, dy / seconds);
    (speed.0.hypot(speed.1) >= STILL).then_some(speed)
}

/// Whether contact `id` is a pen's (Windows tells; elsewhere a touch).
pub(crate) fn is_pen(id: u64) -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::{Input::Pointer::GetPointerType, WindowsAndMessaging::PT_PEN};
        let mut kind = Default::default();
        // SAFETY: reads the type of a pointer Windows just reported.
        unsafe { GetPointerType(id as u32, &mut kind) }.is_ok() && kind == PT_PEN
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use InputEvent::*;
    use winit::event::TouchPhase::*;

    fn events(steps: Vec<Step>) -> Vec<InputEvent> {
        steps
            .into_iter()
            .map(|step| match step {
                Step::Dispatch(event) => event,
                step => panic!("not a dispatch: {step:?}"),
            })
            .collect()
    }

    /// A two-finger scroll at 120 Hz, `step` points each, lifting after `n`.
    #[cfg(target_os = "linux")]
    fn swipe(touches: &mut Touches, t: Instant, step: f32, n: u64) -> Instant {
        let at = |i: u64| t + Duration::from_micros(8333 * i);
        touches.wheel(Started, (50., 50.), (0., step), at(0));
        for i in 1..n {
            touches.wheel(Moved, (50., 50.), (0., step), at(i));
        }
        // libinput's axis stop: the fingers lift, nothing more scrolled.
        touches.wheel(Ended, (50., 50.), (0., 0.), at(n));
        at(n)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_touchpad_flick_coasts_on_and_a_slow_one_does_not() {
        let mut touches = Touches::default();
        let lifted = swipe(&mut touches, Instant::now(), 20., 12);
        assert!(touches.flinging(), "20 points a frame at 120 Hz is a flick");
        let (_, _, dx, dy) = touches.coast(lifted + Duration::from_millis(16)).unwrap();
        assert_eq!(dx, 0.);
        assert!(dy > 20., "on in the scroll's direction, at its speed: {dy}");
        // It slows, and stops.
        let (_, _, _, later) = touches.coast(lifted + Duration::from_millis(400)).unwrap();
        assert!(later > 0.);
        let mut at = lifted + Duration::from_millis(400);
        while touches.flinging() {
            at += Duration::from_millis(16);
            touches.coast(at);
            assert!(at < lifted + Duration::from_secs(5), "never stopped");
        }

        let mut touches = Touches::default();
        swipe(&mut touches, Instant::now(), 0.2, 12);
        assert!(!touches.flinging(), "24 points a second only scrolls");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn fingers_back_on_the_touchpad_stop_a_fling() {
        let mut touches = Touches::default();
        let lifted = swipe(&mut touches, Instant::now(), 20., 12);
        assert!(touches.flinging());
        touches.wheel(
            Started,
            (50., 50.),
            (0., 1.),
            lifted + Duration::from_millis(100),
        );
        assert!(!touches.flinging());
    }

    #[test]
    fn a_tap_clicks_and_a_held_tap_right_clicks() {
        let mut touches = Touches::default();
        let t = Instant::now();
        let down = events(touches.touch(1, Started, (10., 20.), false, t));
        assert!(matches!(
            down[1],
            PointerDown {
                button: PointerButton::Primary,
                ..
            }
        ));
        let up = events(touches.touch(1, Ended, (12., 21.), false, t + Duration::from_millis(80)));
        assert!(matches!(
            up[0],
            PointerUp {
                button: PointerButton::Primary,
                ..
            }
        ));
        assert_eq!(up[1], PointerLeave);

        touches.touch(2, Started, (10., 20.), false, t);
        let held = events(touches.touch(2, Ended, (10., 20.), false, t + HOLD));
        assert_eq!(held[0], PointerCancel);
        assert!(matches!(
            held[1],
            PointerDown {
                button: PointerButton::Secondary,
                ..
            }
        ));
        assert!(matches!(
            held[2],
            PointerUp {
                button: PointerButton::Secondary,
                ..
            }
        ));
    }

    #[test]
    fn a_hold_on_text_selects_and_moves_extend_then_lifting_opens_the_menu() {
        let t = Instant::now();
        let mut touches = Touches::default();
        touches.touch(1, Started, (40., 10.), false, t);
        assert_eq!(touches.hold_deadline(), Some(t + HOLD));
        assert_eq!(touches.hold(t + HOLD / 2), None);
        assert_eq!(touches.hold(t + HOLD), Some((40., 10.)));
        // Once.
        assert_eq!(touches.hold(t + HOLD * 2), None);
        touches.selecting();
        let moved = events(touches.touch(1, Moved, (80., 10.), false, t + HOLD * 2));
        assert_eq!(moved, vec![PointerMove { x: 80., y: 10. }]);
        let up = events(touches.touch(1, Ended, (80., 10.), false, t + HOLD * 3));
        assert!(matches!(
            up[0],
            PointerUp {
                button: PointerButton::Primary,
                ..
            }
        ));
        assert!(matches!(
            up[1],
            PointerDown {
                button: PointerButton::Secondary,
                ..
            }
        ));
        assert!(matches!(
            up[2],
            PointerUp {
                button: PointerButton::Secondary,
                ..
            }
        ));
        // Not on text: the hold passes, and the lift right-clicks as before.
        let mut touches = Touches::default();
        touches.touch(2, Started, (5., 5.), false, t);
        assert!(touches.hold(t + HOLD).is_some());
        let up = events(touches.touch(2, Ended, (5., 5.), false, t + HOLD));
        assert_eq!(up[0], PointerCancel);
    }

    #[test]
    fn a_move_past_the_slop_pans_what_scrolls_and_drags_the_rest() {
        let t = Instant::now();
        let mut touches = Touches::default();
        touches.touch(1, Started, (100., 100.), false, t);
        assert!(touches.touch(1, Moved, (100., 95.), false, t).is_empty());
        let offered = touches.touch(1, Moved, (100., 80.), false, t);
        assert_eq!(
            offered,
            vec![Step::TryPan {
                x: 100.,
                y: 80.,
                dx: 0.,
                dy: 20.
            }]
        );
        assert_eq!(touches.decided(true), vec![Step::Unpress]);
        let pan = events(touches.touch(1, Moved, (100., 70.), false, t));
        assert_eq!(
            pan,
            vec![Scroll {
                x: 100.,
                y: 70.,
                delta_x: 0.,
                delta_y: 10.
            }]
        );

        let mut touches = Touches::default();
        touches.touch(1, Started, (100., 100.), false, t);
        touches.touch(1, Moved, (130., 100.), false, t);
        assert_eq!(
            events(touches.decided(false)),
            vec![PointerMove { x: 130., y: 100. }]
        );
        let drag = events(touches.touch(1, Moved, (140., 100.), false, t));
        assert_eq!(drag, vec![PointerMove { x: 140., y: 100. }]);
        let up = events(touches.touch(1, Ended, (140., 100.), false, t));
        assert!(matches!(up[0], PointerUp { .. }));
    }

    #[test]
    fn a_pan_lifted_moving_coasts_and_slows_to_a_stop() {
        let t = Instant::now();
        let mut touches = Touches::default();
        touches.touch(1, Started, (100., 300.), false, t);
        touches.touch(1, Moved, (100., 280.), false, t + Duration::from_millis(10));
        touches.decided(true);
        for i in 2..=6 {
            let y = 300. - 20. * i as f32;
            touches.touch(
                1,
                Moved,
                (100., y),
                false,
                t + Duration::from_millis(10 * i),
            );
        }
        touches.touch(1, Ended, (100., 180.), false, t + Duration::from_millis(60));
        assert!(touches.flinging());
        let lift = t + Duration::from_millis(60);
        let (_, _, _, first) = touches.coast(lift + Duration::from_millis(16)).unwrap();
        assert!(first > 0., "it scrolls on the way the finger went: {first}");
        let mut total = first;
        let mut at = lift + Duration::from_millis(16);
        while touches.flinging() {
            at += Duration::from_millis(16);
            total += touches.coast(at).unwrap().3;
            assert!(at < lift + Duration::from_secs(5), "it stops");
        }
        // 2000 px/s coasting 0.325 s: about 650 px, less what is cut short.
        assert!((500. ..700.).contains(&total), "{total}");
    }

    #[test]
    fn two_fingers_pan_by_their_middle_and_a_pen_is_a_mouse() {
        let t = Instant::now();
        let mut touches = Touches::default();
        touches.touch(1, Started, (100., 100.), false, t);
        assert_eq!(
            touches.touch(2, Started, (200., 100.), false, t),
            vec![Step::Unpress]
        );
        let pan = events(touches.touch(2, Moved, (200., 80.), false, t));
        assert_eq!(
            pan,
            vec![Scroll {
                x: 150.,
                y: 90.,
                delta_x: 0.,
                delta_y: 10.
            }]
        );

        let mut touches = Touches::default();
        let down = events(touches.touch(9, Started, (5., 5.), true, t));
        assert!(matches!(down[1], PointerDown { .. }));
        assert_eq!(
            events(touches.touch(9, Moved, (50., 5.), true, t)),
            vec![PointerMove { x: 50., y: 5. }]
        );
        assert!(matches!(
            events(touches.touch(9, Ended, (50., 5.), true, t))[0],
            PointerUp { .. }
        ));
    }
}
