//! The little linear algebra a scene needs: vectors, rotations as unit
//! quaternions, column-major 4x4 matrices (as WGSL reads them), and an
//! object's placement (`Transform`).
use std::ops::{Add, Mul, Neg, Sub};

/// A point or direction in 3D.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    /// Across (right).
    pub x: f32,
    /// Up.
    pub y: f32,
    /// Toward the viewer (right-handed).
    pub z: f32,
}
/// `Vec3 { x, y, z }`.
pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}
impl Vec3 {
    /// The origin.
    pub const ZERO: Self = vec3(0., 0., 0.);
    /// All ones.
    pub const ONE: Self = vec3(1., 1., 1.);
    /// Up.
    pub const Y: Self = vec3(0., 1., 0.);
    /// Its dot product with `other`.
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    /// Its cross product with `other`.
    pub fn cross(self, other: Self) -> Self {
        vec3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }
    /// Its length.
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    /// It at length 1; the zero vector stays zero.
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length > 0. {
            self * (1. / length)
        } else {
            self
        }
    }
    /// Each part times `other`'s.
    pub fn scaled(self, other: Self) -> Self {
        vec3(self.x * other.x, self.y * other.y, self.z * other.z)
    }
    /// Each part's least with `other`'s.
    pub fn min(self, other: Self) -> Self {
        vec3(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }
    /// Each part's greatest with `other`'s.
    pub fn max(self, other: Self) -> Self {
        vec3(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }
    /// Whether every part is finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    /// Its parts, in order.
    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}
impl From<[f32; 3]> for Vec3 {
    fn from([x, y, z]: [f32; 3]) -> Self {
        vec3(x, y, z)
    }
}
impl Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        vec3(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        vec3(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, by: f32) -> Self {
        vec3(self.x * by, self.y * by, self.z * by)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        vec3(-self.x, -self.y, -self.z)
    }
}

/// A rotation: a unit quaternion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    /// Its vector part, across.
    pub x: f32,
    /// Its vector part, up.
    pub y: f32,
    /// Its vector part, toward the viewer.
    pub z: f32,
    /// Its scalar part.
    pub w: f32,
}
impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Quat {
    /// No rotation.
    pub const IDENTITY: Self = Self {
        x: 0.,
        y: 0.,
        z: 0.,
        w: 1.,
    };
    /// `angle` radians about `axis` (counter-clockwise looking down it).
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let axis = axis.normalized();
        let (s, c) = (angle * 0.5).sin_cos();
        Self {
            x: axis.x * s,
            y: axis.y * s,
            z: axis.z * s,
            w: c,
        }
    }
    /// Turned by `x` radians about X, then `y` about Y, then `z` about Z (in
    /// the scene's frame: CSS's `rotateX() rotateY() rotateZ()` read right to
    /// left), as Luau's `rotation = { x, y, z }` in degrees does.
    pub fn from_euler(x: f32, y: f32, z: f32) -> Self {
        Self::from_axis_angle(vec3(0., 0., 1.), z)
            * Self::from_axis_angle(Vec3::Y, y)
            * Self::from_axis_angle(vec3(1., 0., 0.), x)
    }
    /// `v` turned by it.
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = vec3(self.x, self.y, self.z);
        let t = q.cross(v) * 2.;
        v + t * self.w + q.cross(t)
    }
    /// It at length 1 (the identity where it has none).
    pub fn normalized(self) -> Self {
        let length = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if length > 0. && length.is_finite() {
            Self {
                x: self.x / length,
                y: self.y / length,
                z: self.z / length,
                w: self.w / length,
            }
        } else {
            Self::IDENTITY
        }
    }
}
impl Mul for Quat {
    type Output = Self;
    /// `self` after `other`.
    fn mul(self, o: Self) -> Self {
        Self {
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        }
    }
}

/// A 4x4 matrix, column-major: `columns[c][r]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    /// Its columns.
    pub columns: [[f32; 4]; 4],
}
impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Mat4 {
    /// The identity.
    pub const IDENTITY: Self = Self {
        columns: [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ],
    };
    /// Scaled by `scale`, then turned by `rotation`, then moved by `position`.
    pub fn from_transform(position: Vec3, rotation: Quat, scale: Vec3) -> Self {
        let x = rotation.rotate(vec3(scale.x, 0., 0.));
        let y = rotation.rotate(vec3(0., scale.y, 0.));
        let z = rotation.rotate(vec3(0., 0., scale.z));
        Self {
            columns: [
                [x.x, x.y, x.z, 0.],
                [y.x, y.y, y.z, 0.],
                [z.x, z.y, z.z, 0.],
                [position.x, position.y, position.z, 1.],
            ],
        }
    }
    /// From `eye` looking at `target`, `up` as near up as it can be.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let forward = (target - eye).normalized();
        let mut right = forward.cross(up).normalized();
        if right == Vec3::ZERO {
            // Looking straight along `up`: any right will do.
            right = forward.cross(vec3(0., 0., -1.)).normalized();
        }
        let up = right.cross(forward);
        Self {
            columns: [
                [right.x, up.x, -forward.x, 0.],
                [right.y, up.y, -forward.y, 0.],
                [right.z, up.z, -forward.z, 0.],
                [-right.dot(eye), -up.dot(eye), forward.dot(eye), 1.],
            ],
        }
    }
    /// A perspective of `fov_y` radians top to bottom, `aspect` wide for 1
    /// high, depth 0 at `near` to 1 at `far` (as wgpu clips).
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1. / (fov_y * 0.5).tan();
        let range = far / (near - far);
        Self {
            columns: [
                [f / aspect, 0., 0., 0.],
                [0., f, 0., 0.],
                [0., 0., range, -1.],
                [0., 0., near * range, 0.],
            ],
        }
    }
    /// An orthographic view `height` tall, `aspect` wide for 1 high, depth 0
    /// at `near` to 1 at `far`.
    pub fn orthographic(height: f32, aspect: f32, near: f32, far: f32) -> Self {
        let (h, w) = (2. / height, 2. / (height * aspect));
        let range = 1. / (near - far);
        Self {
            columns: [
                [w, 0., 0., 0.],
                [0., h, 0., 0.],
                [0., 0., range, 0.],
                [0., 0., near * range, 1.],
            ],
        }
    }
    /// The point `p` mapped (with its perspective divide).
    pub fn project(&self, p: Vec3) -> Vec3 {
        let c = &self.columns;
        let x = c[0][0] * p.x + c[1][0] * p.y + c[2][0] * p.z + c[3][0];
        let y = c[0][1] * p.x + c[1][1] * p.y + c[2][1] * p.z + c[3][1];
        let z = c[0][2] * p.x + c[1][2] * p.y + c[2][2] * p.z + c[3][2];
        let w = c[0][3] * p.x + c[1][3] * p.y + c[2][3] * p.z + c[3][3];
        vec3(x / w, y / w, z / w)
    }
}
impl Mul for Mat4 {
    type Output = Self;
    /// `self` after `other`.
    fn mul(self, other: Self) -> Self {
        let mut columns = [[0.; 4]; 4];
        for (c, column) in columns.iter_mut().enumerate() {
            for (r, value) in column.iter_mut().enumerate() {
                *value = (0..4)
                    .map(|k| self.columns[k][r] * other.columns[c][k])
                    .sum();
            }
        }
        Self { columns }
    }
}

/// Where an object is: scaled by `scale`, turned by `rotation`, moved to
/// `position`, in its parent's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Where its origin is.
    pub position: Vec3,
    /// How it is turned.
    pub rotation: Quat,
    /// How large it is along each of its axes.
    pub scale: Vec3,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}
impl Transform {
    /// At `position`, unturned and unscaled.
    pub fn at(position: impl Into<Vec3>) -> Self {
        Self {
            position: position.into(),
            ..Self::default()
        }
    }
    /// Its matrix.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_transform(self.position, self.rotation.normalized(), self.scale)
    }
    /// It with non-finite parts as the identity's: a scene never draws NaN.
    pub(crate) fn sanitized(self) -> Self {
        let default = Self::default();
        let rotation = self.rotation;
        Self {
            position: if self.position.is_finite() {
                self.position
            } else {
                default.position
            },
            rotation: if [rotation.x, rotation.y, rotation.z, rotation.w]
                .iter()
                .all(|v| v.is_finite())
            {
                rotation.normalized()
            } else {
                Quat::IDENTITY
            },
            scale: if self.scale.is_finite() {
                self.scale
            } else {
                default.scale
            },
        }
    }
}
