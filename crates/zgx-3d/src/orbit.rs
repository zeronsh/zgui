//! A camera turned about a point by dragging and moved in and out by
//! scrolling or pinching, as a model viewer's is (`Orbit`).
use crate::{
    math::{Vec3, vec3},
    world::Camera,
};

/// Where an orbiting camera is: `distance` from `target`, turned `azimuth`
/// radians about the vertical (0 looks down -Z, toward the scene's front)
/// and raised `elevation` radians above the horizon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    /// What it turns about and looks at.
    pub target: Vec3,
    /// About the vertical, in radians.
    pub azimuth: f32,
    /// Above the horizon, in radians.
    pub elevation: f32,
    /// From the target.
    pub distance: f32,
    /// The least and greatest elevation (default: just short of straight
    /// down and up, where the view would flip).
    pub elevations: (f32, f32),
    /// The least and greatest distance.
    pub distances: (f32, f32),
    /// Radians a point of drag turns it.
    pub sensitivity: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            azimuth: 0.,
            elevation: 0.3,
            distance: 5.,
            elevations: (-1.5, 1.5),
            distances: (0.1, 1000.),
            sensitivity: 0.008,
        }
    }
}
impl Orbit {
    /// The camera it puts in `base`'s place (`base`'s lens, its own
    /// position and target).
    pub fn camera(&self, base: Camera) -> Camera {
        let (sa, ca) = self.azimuth.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        Camera {
            position: self.target + vec3(sa * ce, se, ca * ce) * self.distance,
            target: self.target,
            ..base
        }
    }
    /// Turned by a drag of `dx`, `dy` points: right turns the scene right,
    /// down tips it toward the viewer.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        if !(dx.is_finite() && dy.is_finite()) {
            return;
        }
        self.azimuth -= dx * self.sensitivity;
        self.elevation =
            (self.elevation + dy * self.sensitivity).clamp(self.elevations.0, self.elevations.1);
    }
    /// Moved in by `factor` (above 1: closer, as pinching out zooms in).
    pub fn zoom(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0. {
            self.distance = (self.distance / factor).clamp(self.distances.0, self.distances.1);
        }
    }
}
impl Orbit {
    /// Its target moved by a drag of `dx`, `dy` points across the view
    /// (panning): the scene follows the pointer, a point moving the target
    /// 0.0015 times the distance (about one point of the scene's at a
    /// 45° view some 550 points tall).
    pub fn pan(&mut self, dx: f32, dy: f32) {
        if !(dx.is_finite() && dy.is_finite()) {
            return;
        }
        let (sa, ca) = self.azimuth.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        let right = vec3(ca, 0., -sa);
        let up = vec3(-sa * se, ce, -ca * se);
        let step = self.distance * 0.0015;
        self.target = self.target - right * (dx * step) + up * (dy * step);
    }
}

/// How an orbiting camera answers its view's input, as a model viewer's
/// does: drag to turn it (with the primary button), drag with the
/// secondary or middle button or with Shift to pan, scroll or pinch to
/// zoom (Shift-scroll pans), and, focused, the arrow keys turn it (pan with
/// Shift), `+` and `-` zoom and Home puts it back. A flick coasts to a stop
/// (`coast`), unless `inertia` is off (for reduced motion).
#[derive(Clone, Debug)]
pub struct OrbitControls {
    /// Where the camera is.
    pub orbit: Orbit,
    /// Where Home puts it back.
    pub home: Orbit,
    /// Whether scrolling, pinching and `+` and `-` zoom.
    pub zoom: bool,
    /// Whether it pans.
    pub pan: bool,
    /// Whether a flick coasts on (off for reduced motion).
    pub inertia: bool,
    drag: Option<Drag>,
    /// Points a second it coasts (turning or panning) since let go.
    velocity: (f32, f32),
    panning: bool,
}
#[derive(Clone, Copy, Debug)]
struct Drag {
    at: (f32, f32),
    when: std::time::Instant,
    panning: bool,
    /// Points a second, smoothed over its last moves.
    velocity: (f32, f32),
}
/// What an event did to `OrbitControls`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// The camera moved: set it on the world.
    pub moved: bool,
    /// The event was its own: nothing else should take it (a scroll that
    /// zoomed, a key that turned it).
    pub taken: bool,
    /// It's coasting: call `coast` each frame until it says it's still.
    pub coasting: bool,
}

/// Degrees an arrow key turns it.
const KEY_TURN: f32 = 10.;
/// Points an arrow key pans it.
const KEY_PAN: f32 = 40.;
/// How fast coasting slows: its speed falls by e each 1 / this seconds.
const FRICTION: f32 = 5.;
/// Points a second a flick coasts at most.
const MAX_SPEED: f32 = 5000.;

impl OrbitControls {
    /// Controls starting at, and coming home to, `orbit`: zooming,
    /// panning, coasting.
    pub fn new(orbit: Orbit) -> Self {
        Self {
            orbit,
            home: orbit,
            zoom: true,
            pan: true,
            inertia: true,
            drag: None,
            velocity: (0., 0.),
            panning: false,
        }
    }

    /// Answer `event` (with `modifiers` held, as the pointer's or the key's),
    /// arriving `now`.
    pub fn handle(
        &mut self,
        event: &zgx::input::InputEvent,
        modifiers: zgx::input::Modifiers,
        now: std::time::Instant,
    ) -> Answer {
        use zgx::input::{InputEvent, Key, PointerButton};
        let before = self.orbit;
        let mut taken = false;
        match event {
            InputEvent::PointerDown { x, y, button } => {
                let panning = self.pan
                    && (modifiers.shift
                        || matches!(button, PointerButton::Secondary | PointerButton::Middle));
                self.velocity = (0., 0.);
                self.drag = Some(Drag {
                    at: (*x, *y),
                    when: now,
                    panning,
                    velocity: (0., 0.),
                });
            }
            InputEvent::PointerMove { x, y } => {
                if let Some(drag) = &mut self.drag {
                    let (dx, dy) = (x - drag.at.0, y - drag.at.1);
                    // Moves come at most some 240 a second: closer ones
                    // came together, not faster.
                    let seconds = now
                        .saturating_duration_since(drag.when)
                        .as_secs_f32()
                        .max(1. / 240.);
                    let fresh = (dx / seconds, dy / seconds);
                    // Half the last move's speed, half before: a steady
                    // estimate of the hand's speed as it lets go, no faster
                    // than a hand flicks.
                    let (vx, vy) = (
                        (drag.velocity.0 + fresh.0) * 0.5,
                        (drag.velocity.1 + fresh.1) * 0.5,
                    );
                    let speed = vx.hypot(vy);
                    let cap = if speed > MAX_SPEED {
                        MAX_SPEED / speed
                    } else {
                        1.
                    };
                    drag.velocity = (vx * cap, vy * cap);
                    drag.at = (*x, *y);
                    drag.when = now;
                    let panning = drag.panning;
                    self.apply(dx, dy, panning);
                }
            }
            InputEvent::PointerUp { .. } => {
                if let Some(drag) = self.drag.take() {
                    // Held still before letting go: no flick.
                    let still = now.saturating_duration_since(drag.when).as_secs_f32() > 0.05;
                    if self.inertia && !still {
                        self.velocity = drag.velocity;
                        self.panning = drag.panning;
                    }
                }
            }
            InputEvent::PointerCancel => self.drag = None,
            InputEvent::Scroll {
                delta_x, delta_y, ..
            } => {
                if modifiers.shift && self.pan {
                    self.orbit.pan(-delta_x, -delta_y);
                    taken = true;
                } else if self.zoom {
                    self.orbit.zoom((delta_y * 0.004).exp());
                    taken = true;
                }
            }
            InputEvent::Pinch { delta, .. } if self.zoom => {
                self.orbit.zoom(1. + delta);
                taken = true;
            }
            InputEvent::KeyDown { key, .. } => {
                let turn = KEY_TURN.to_radians() / self.orbit.sensitivity;
                let (dx, dy) = match key {
                    Key::ArrowLeft => (-1., 0.),
                    Key::ArrowRight => (1., 0.),
                    Key::ArrowUp => (0., -1.),
                    Key::ArrowDown => (0., 1.),
                    _ => (0., 0.),
                };
                if (dx, dy) != (0., 0.) {
                    if modifiers.shift && self.pan {
                        self.orbit.pan(dx * KEY_PAN, dy * KEY_PAN);
                    } else {
                        self.orbit.drag(dx * turn, dy * turn);
                    }
                    taken = true;
                }
                match key {
                    Key::Character(c) if self.zoom && (c == "+" || c == "=") => {
                        self.orbit.zoom(1.25);
                        taken = true;
                    }
                    Key::Character(c) if self.zoom && c == "-" => {
                        self.orbit.zoom(0.8);
                        taken = true;
                    }
                    Key::Home => {
                        self.orbit = self.home;
                        self.velocity = (0., 0.);
                        taken = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if taken {
            // A key or the wheel stops a coast.
            self.velocity = (0., 0.);
        }
        Answer {
            moved: self.orbit != before,
            taken,
            coasting: self.coasting(),
        }
    }

    /// Coast on for `seconds` since the last frame, slowing; `true` if
    /// it moved (and so is still coasting, or just stopped).
    pub fn coast(&mut self, seconds: f32) -> bool {
        if !self.coasting() || !seconds.is_finite() || seconds <= 0. {
            return false;
        }
        let seconds = seconds.min(0.1);
        let (vx, vy) = self.velocity;
        let panning = self.panning;
        self.apply(vx * seconds, vy * seconds, panning);
        let slowed = (-FRICTION * seconds).exp();
        self.velocity = (vx * slowed, vy * slowed);
        if self.velocity.0.hypot(self.velocity.1) < 10. {
            self.velocity = (0., 0.);
        }
        true
    }

    /// Whether it's coasting.
    pub fn coasting(&self) -> bool {
        self.velocity != (0., 0.) && self.drag.is_none()
    }

    fn apply(&mut self, dx: f32, dy: f32, panning: bool) {
        if panning {
            self.orbit.pan(dx, dy);
        } else {
            self.orbit.drag(dx, dy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use zgx::input::{InputEvent, Key, Modifiers, PointerButton};

    fn press(c: &mut OrbitControls, button: PointerButton, at: Instant) {
        c.handle(
            &InputEvent::PointerDown {
                x: 0.,
                y: 0.,
                button,
            },
            Modifiers::default(),
            at,
        );
    }
    fn mv(c: &mut OrbitControls, x: f32, at: Instant) -> Answer {
        c.handle(
            &InputEvent::PointerMove { x, y: 0. },
            Modifiers::default(),
            at,
        )
    }
    fn up(c: &mut OrbitControls, x: f32, at: Instant) -> Answer {
        c.handle(
            &InputEvent::PointerUp {
                x,
                y: 0.,
                button: PointerButton::Primary,
            },
            Modifiers::default(),
            at,
        )
    }

    #[test]
    fn a_flick_coasts_to_a_stop_and_a_held_release_does_not() {
        let start = Instant::now();
        let ms = |n: u64| start + Duration::from_millis(n);
        let mut c = OrbitControls::new(Orbit::default());
        press(&mut c, PointerButton::Primary, ms(0));
        for i in 1..=5 {
            assert!(mv(&mut c, i as f32 * 10., ms(i * 16)).moved);
        }
        let answer = up(&mut c, 50., ms(90));
        assert!(answer.coasting);
        let turned = c.orbit.azimuth;
        let mut frames = 0;
        while c.coast(1. / 60.) {
            frames += 1;
            assert!(frames < 600, "it stops");
        }
        assert!(c.orbit.azimuth < turned, "coasted on the way it was going");
        assert!(frames > 10);
        // Held still a while before letting go: no coast.
        press(&mut c, PointerButton::Primary, ms(1000));
        mv(&mut c, 20., ms(1016));
        assert!(!up(&mut c, 20., ms(1200)).coasting);
        // Reduced motion: none either.
        c.inertia = false;
        press(&mut c, PointerButton::Primary, ms(2000));
        mv(&mut c, 20., ms(2016));
        assert!(!up(&mut c, 40., ms(2030)).coasting);
    }

    #[test]
    fn the_secondary_button_pans_and_home_comes_back() {
        let at = Instant::now();
        let mut c = OrbitControls::new(Orbit::default());
        press(&mut c, PointerButton::Secondary, at);
        mv(&mut c, 100., at + Duration::from_millis(16));
        up(&mut c, 100., at + Duration::from_secs(1));
        // Dragged right: the target moves left, the camera's turn kept.
        assert!(c.orbit.target.x < -0.1, "{:?}", c.orbit.target);
        assert_eq!(c.orbit.azimuth, c.home.azimuth);
        let home = c.handle(
            &InputEvent::KeyDown {
                key: Key::Home,
                modifiers: Modifiers::default(),
                repeat: false,
            },
            Modifiers::default(),
            at,
        );
        assert!(home.moved && home.taken);
        assert_eq!(c.orbit, c.home);
    }

    #[test]
    fn keys_turn_pan_and_zoom_it() {
        let at = Instant::now();
        let mut c = OrbitControls::new(Orbit::default());
        let key = |c: &mut OrbitControls, key: Key, shift: bool| {
            let modifiers = Modifiers {
                shift,
                ..Modifiers::default()
            };
            c.handle(
                &InputEvent::KeyDown {
                    key,
                    modifiers,
                    repeat: false,
                },
                modifiers,
                at,
            )
        };
        assert!(key(&mut c, Key::ArrowRight, false).taken);
        assert!((c.orbit.azimuth + 10f32.to_radians()).abs() < 1e-4);
        key(&mut c, Key::ArrowUp, true);
        assert!(
            c.orbit.target.y < 0.,
            "panned: the scene up, the target down"
        );
        let distance = c.orbit.distance;
        key(&mut c, Key::Character("+".into()), false);
        assert!(c.orbit.distance < distance);
        assert!(!key(&mut c, Key::Character("a".into()), false).taken);
        c.zoom = false;
        assert!(!key(&mut c, Key::Character("-".into()), false).taken);
    }
}
