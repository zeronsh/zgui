//! Two-dimensional paint transformations, and the projective ones a tilt in
//! perspective makes. Layout allocations remain unchanged.
use crate::scene::Rect;

/// Column-vector affine matrix: `(a*x + c*y + tx, b*x + d*y + ty)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// The new x's factor of x.
    pub a: f32,
    /// The new y's factor of x.
    pub b: f32,
    /// The new x's factor of y.
    pub c: f32,
    /// The new y's factor of y.
    pub d: f32,
    /// Added to the new x.
    pub tx: f32,
    /// Added to the new y.
    pub ty: f32,
}
impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Affine {
    /// Leaves every point where it is.
    pub const IDENTITY: Self = Self {
        a: 1.,
        b: 0.,
        c: 0.,
        d: 1.,
        tx: 0.,
        ty: 0.,
    };
    /// Moves points `x` across and `y` down.
    pub fn translation(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }
    /// Scales by `x` across and `y` down, about the origin.
    pub fn scale(x: f32, y: f32) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }
    /// Rotation in radians, clockwise in screen coordinates (positive Y down).
    pub fn rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.,
            ty: 0.,
        }
    }
    /// Apply `self`, then `next`.
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            tx: next.a * self.tx + next.c * self.ty + next.tx,
            ty: next.b * self.tx + next.d * self.ty + next.ty,
        }
    }
    /// `self` about the point `x`, `y` instead of the origin.
    pub fn around(self, x: f32, y: f32) -> Self {
        Self::translation(-x, -y)
            .then(self)
            .then(Self::translation(x, y))
    }
    /// Whether every component is finite.
    pub fn is_finite(self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|n| n.is_finite())
    }
    /// Where it moves the point `x`, `y`.
    pub fn point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }
    /// Singular or non-finite matrices have no usable inverse.
    pub fn inverse(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }
        let determinant = self.a as f64 * self.d as f64 - self.b as f64 * self.c as f64;
        if determinant == 0. {
            return None;
        }
        let inverse = Self {
            a: (self.d as f64 / determinant) as f32,
            b: (-self.b as f64 / determinant) as f32,
            c: (-self.c as f64 / determinant) as f32,
            d: (self.a as f64 / determinant) as f32,
            tx: ((self.c as f64 * self.ty as f64 - self.d as f64 * self.tx as f64) / determinant)
                as f32,
            ty: ((self.b as f64 * self.tx as f64 - self.a as f64 * self.ty as f64) / determinant)
                as f32,
        };
        inverse.is_finite().then_some(inverse)
    }
    /// Axis-aligned coverage of all four transformed corners. Unrepresentable
    /// geometry is empty rather than introducing NaN into damage tracking.
    pub fn bounds(self, bounds: Rect) -> Rect {
        let corners = [
            (bounds.x, bounds.y),
            (bounds.x + bounds.width, bounds.y),
            (bounds.x, bounds.y + bounds.height),
            (bounds.x + bounds.width, bounds.y + bounds.height),
        ];
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for (x, y) in corners {
            let (x, y) = self.point(x, y);
            if !x.is_finite() || !y.is_finite() {
                return Rect::default();
            }
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
        let (width, height) = (right - left, bottom - top);
        if !width.is_finite() || !height.is_finite() {
            return Rect::default();
        }
        Rect::new(left, top, width, height)
    }
}

/// A projective map of the plane (a homography): `Affine`'s terms, then
/// divided by `w = px*x + py*y + pw`, as a plane tilted in perspective
/// projects onto the screen. Affine when `px` and `py` are 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projective {
    /// The new x's factor of x.
    pub a: f32,
    /// The new y's factor of x.
    pub b: f32,
    /// The new x's factor of y.
    pub c: f32,
    /// The new y's factor of y.
    pub d: f32,
    /// Added to the new x.
    pub tx: f32,
    /// Added to the new y.
    pub ty: f32,
    /// The divisor's factor of x.
    pub px: f32,
    /// The divisor's factor of y.
    pub py: f32,
    /// Added to the divisor.
    pub pw: f32,
}
impl Default for Projective {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl From<Affine> for Projective {
    fn from(m: Affine) -> Self {
        Self {
            a: m.a,
            b: m.b,
            c: m.c,
            d: m.d,
            tx: m.tx,
            ty: m.ty,
            px: 0.,
            py: 0.,
            pw: 1.,
        }
    }
}
/// Below this a point's divisor is behind the eye (or at it): where a
/// projection maps it is no place on screen.
const MIN_W: f32 = 1e-4;
impl Projective {
    /// Leaves every point where it is.
    pub const IDENTITY: Self = Self {
        a: 1.,
        b: 0.,
        c: 0.,
        d: 1.,
        tx: 0.,
        ty: 0.,
        px: 0.,
        py: 0.,
        pw: 1.,
    };
    /// Rows of the 3×3 matrix.
    fn rows(self) -> [[f64; 3]; 3] {
        let f = |v: f32| v as f64;
        [
            [f(self.a), f(self.c), f(self.tx)],
            [f(self.b), f(self.d), f(self.ty)],
            [f(self.px), f(self.py), f(self.pw)],
        ]
    }
    fn from_rows(m: [[f64; 3]; 3]) -> Self {
        // Scaled so the divisor's constant is 1 where it can be: equal maps
        // then compare equal, and affine ones are `Affine`'s.
        let k = if m[2][2].abs() > 1e-12 { m[2][2] } else { 1. };
        let f = |v: f64| (v / k) as f32;
        Self {
            a: f(m[0][0]),
            c: f(m[0][1]),
            tx: f(m[0][2]),
            b: f(m[1][0]),
            d: f(m[1][1]),
            ty: f(m[1][2]),
            px: f(m[2][0]),
            py: f(m[2][1]),
            pw: f(m[2][2]),
        }
    }
    /// Apply `self`, then `next`.
    pub fn then(self, next: Self) -> Self {
        let (a, b) = (next.rows(), self.rows());
        let mut m = [[0.; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value = (0..3).map(|k| a[i][k] * b[k][j]).sum();
            }
        }
        Self::from_rows(m)
    }
    /// Its affine map, if it has no perspective.
    pub fn affine(self) -> Option<Affine> {
        (self.px == 0. && self.py == 0. && self.pw == 1.).then_some(Affine {
            a: self.a,
            b: self.b,
            c: self.c,
            d: self.d,
            tx: self.tx,
            ty: self.ty,
        })
    }
    /// Whether every component is finite.
    pub fn is_finite(self) -> bool {
        [
            self.a, self.b, self.c, self.d, self.tx, self.ty, self.px, self.py, self.pw,
        ]
        .iter()
        .all(|n| n.is_finite())
    }
    /// The divisor at the point `x`, `y`: at or below 0 it is behind the eye.
    pub fn w(self, x: f32, y: f32) -> f32 {
        self.px * x + self.py * y + self.pw
    }
    /// Where it moves the point `x`, `y` (non-finite behind the eye).
    pub fn point(self, x: f32, y: f32) -> (f32, f32) {
        let w = self.w(x, y);
        if w < MIN_W {
            return (f32::NAN, f32::NAN);
        }
        (
            (self.a * x + self.c * y + self.tx) / w,
            (self.b * x + self.d * y + self.ty) / w,
        )
    }
    /// Singular or non-finite maps have no usable inverse.
    pub fn inverse(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }
        let m = self.rows();
        let cofactor = |r0: usize, r1: usize, c0: usize, c1: usize| {
            m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
        };
        let determinant = m[0][0] * cofactor(1, 2, 1, 2) - m[0][1] * cofactor(1, 2, 0, 2)
            + m[0][2] * cofactor(1, 2, 0, 1);
        if determinant.abs() < 1e-12 {
            return None;
        }
        let adjugate = [
            [
                cofactor(1, 2, 1, 2),
                -cofactor(0, 2, 1, 2),
                cofactor(0, 1, 1, 2),
            ],
            [
                -cofactor(1, 2, 0, 2),
                cofactor(0, 2, 0, 2),
                -cofactor(0, 1, 0, 2),
            ],
            [
                cofactor(1, 2, 0, 1),
                -cofactor(0, 2, 0, 1),
                cofactor(0, 1, 0, 1),
            ],
        ];
        let inverse = Self::from_rows(adjugate.map(|row| row.map(|v| v / determinant)));
        inverse.is_finite().then_some(inverse)
    }
    /// Axis-aligned coverage of all four mapped corners. A corner behind the
    /// eye has no place: the bounds are then everything, never too little.
    pub fn bounds(self, bounds: Rect) -> Rect {
        if let Some(affine) = self.affine() {
            return affine.bounds(bounds);
        }
        let corners = [
            (bounds.x, bounds.y),
            (bounds.x + bounds.width, bounds.y),
            (bounds.x, bounds.y + bounds.height),
            (bounds.x + bounds.width, bounds.y + bounds.height),
        ];
        let everything = Rect::new(-f32::MAX / 4., -f32::MAX / 4., f32::MAX / 2., f32::MAX / 2.);
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for (x, y) in corners {
            let (x, y) = self.point(x, y);
            if !x.is_finite() || !y.is_finite() {
                return everything;
            }
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
        Rect::new(left, top, right - left, bottom - top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_inverse_and_reflection() {
        let matrix = Affine::scale(-2., 3.)
            .then(Affine::rotation(0.7))
            .then(Affine::translation(50., 80.));
        let (x, y) = matrix.point(7., -9.);
        let (x, y) = matrix.inverse().unwrap().point(x, y);
        assert!((x - 7.).abs() < 0.0001 && (y + 9.).abs() < 0.0001);
        assert!(Affine::scale(0., 1.).inverse().is_none());
        assert!(Affine::translation(f32::NAN, 0.).inverse().is_none());
    }
    #[test]
    fn rotated_bounds_use_all_corners_and_preserve_pivot() {
        let transform = Affine::rotation(std::f32::consts::FRAC_PI_2).around(30., 25.);
        let (x, y) = transform.point(30., 25.);
        assert!((x - 30.).abs() < 0.0001 && (y - 25.).abs() < 0.0001);
        let result = transform.bounds(Rect::new(10., 15., 40., 20.));
        assert!((result.x - 20.).abs() < 0.0001 && (result.y - 5.).abs() < 0.0001);
        assert!((result.width - 20.).abs() < 0.0001 && (result.height - 40.).abs() < 0.0001);
    }
    #[test]
    fn projective_maps_compose_invert_and_bound_their_corners() {
        // A plane tilted in perspective: points nearer the eye spread.
        let tilt = Projective {
            px: 0.002,
            ..Projective::IDENTITY
        };
        let m = Projective::from(Affine::translation(-50., -50.))
            .then(tilt)
            .then(Projective::from(Affine::translation(50., 50.)));
        let (x, y) = m.point(80., 20.);
        let (x, y) = m.inverse().unwrap().point(x, y);
        assert!((x - 80.).abs() < 0.001 && (y - 20.).abs() < 0.001);
        let b = m.bounds(Rect::new(0., 0., 100., 100.));
        let (_, near) = m.point(0., 0.);
        let (_, far) = m.point(100., 0.);
        assert!(near < far, "the side nearer the eye is taller");
        assert!((b.y - near).abs() < 0.001);
        assert_eq!(
            Projective::from(Affine::scale(2., 3.)).affine(),
            Some(Affine::scale(2., 3.))
        );
        // Behind the eye: no place, and bounds that miss nothing.
        let behind = Projective {
            px: -1.,
            ..Projective::IDENTITY
        };
        assert!(behind.point(5., 0.).0.is_nan());
        assert!(behind.bounds(Rect::new(0., 0., 10., 10.)).width > 1e30);
    }
}
