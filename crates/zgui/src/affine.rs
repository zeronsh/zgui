//! Two-dimensional paint transformations. Layout allocations remain unchanged.
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
}
