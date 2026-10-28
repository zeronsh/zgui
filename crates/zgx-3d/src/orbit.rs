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
