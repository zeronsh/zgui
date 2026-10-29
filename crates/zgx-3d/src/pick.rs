//! Picking: which object a point of the view's box shows (`World::pick`),
//! found on the CPU from the world's own geometry, so a world never picked
//! costs nothing and one picked costs no GPU work.
use crate::{Geometry, Mat4, ObjectId, Vec3};

/// What a point of the view shows (`World::pick`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The nearest object there.
    pub object: ObjectId,
    /// Where on it, in the world.
    pub point: Vec3,
    /// How far that is from the camera.
    pub distance: f32,
}

/// The segment the camera sees through a point of its box: from the near
/// plane to the far one, in the world. `x` and `y` are from the box's top
/// left, `width` by `height`.
pub(crate) fn segment(
    view_projection: Mat4,
    (x, y): (f32, f32),
    (width, height): (f32, f32),
) -> Option<(Vec3, Vec3)> {
    if !(width > 0. && height > 0. && x.is_finite() && y.is_finite()) {
        return None;
    }
    let inverse = view_projection.inverse()?;
    let (nx, ny) = (2. * x / width - 1., 1. - 2. * y / height);
    // wgpu's depth: 0 at the near plane, 1 at the far one.
    let near = inverse.project(crate::vec3(nx, ny, 0.));
    let far = inverse.project(crate::vec3(nx, ny, 1.));
    (near.is_finite() && far.is_finite()).then_some((near, far))
}

/// Where along `from` + t·`along` (t from 0 to 1) the segment first meets
/// `geometry` placed by `matrix`, if it does: both sides of each triangle
/// count. The same t as in the world, since placing is affine.
pub(crate) fn meet(geometry: &Geometry, matrix: Mat4, from: Vec3, to: Vec3) -> Option<f32> {
    let inverse = matrix.inverse()?;
    let start = inverse.project(from);
    let along = inverse.project(to) - start;
    let (low, high) = geometry.bounds();
    if !through_box(start, along, low, high) {
        return None;
    }
    let vertices = geometry.vertices();
    let mut nearest: Option<f32> = None;
    for &[a, b, c] in geometry.indices().as_chunks::<3>().0 {
        let corner = |i: u32| Vec3::from(vertices[i as usize].position);
        let t = triangle_t(start, along, corner(a), corner(b), corner(c));
        if let Some(t) = t
            && nearest.is_none_or(|n| t < n)
        {
            nearest = Some(t);
        }
    }
    nearest
}

/// Whether the segment meets the box `low` to `high` (slabs).
fn through_box(start: Vec3, along: Vec3, low: Vec3, high: Vec3) -> bool {
    let (mut enter, mut leave) = (0_f32, 1_f32);
    for (s, d, lo, hi) in [
        (start.x, along.x, low.x, high.x),
        (start.y, along.y, low.y, high.y),
        (start.z, along.z, low.z, high.z),
    ] {
        if d.abs() < 1e-12 {
            if s < lo || s > hi {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo - s) / d, (hi - s) / d);
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
        if enter > leave {
            return false;
        }
    }
    true
}

/// Where the segment meets triangle `a`, `b`, `c` (Möller–Trumbore), t in 0
/// to 1.
fn triangle_t(start: Vec3, along: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = along.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let to = start - a;
    let u = to.dot(p) / det;
    if !(0. ..=1.).contains(&u) {
        return None;
    }
    let q = to.cross(ab);
    let v = along.dot(q) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let t = ac.dot(q) / det;
    (0. ..=1.).contains(&t).then_some(t)
}

#[cfg(test)]
mod tests {
    use crate::*;

    const BOX: (f32, f32) = (400., 300.);
    const MIDDLE: (f32, f32) = (200., 150.);

    fn cube(world: &World, at: [f32; 3]) -> ObjectId {
        world.add(Object::mesh(Geometry::cuboid(1., 1., 1.), Material::rgb(0xffffff)).at(at))
    }

    #[test]
    fn the_middle_shows_the_cube_in_front_and_a_corner_nothing() {
        let world = World::new();
        let id = cube(&world, [0., 0., 0.]);
        let hit = world.pick(MIDDLE, BOX).expect("the cube");
        assert_eq!(hit.object, id);
        // On its surface, facing the camera (0, 1.5, 5).
        let p = hit.point;
        assert!(p.x.abs() <= 0.5 + 1e-4 && p.y.abs() <= 0.5 + 1e-4 && p.z.abs() <= 0.5 + 1e-4);
        assert!(
            p.z > 0.49 || p.y > 0.49,
            "a face seen from the front: {p:?}"
        );
        let seen = (p - world.camera().position).length();
        assert!((hit.distance - seen).abs() < 1e-4);
        assert_eq!(world.pick((5., 5.), BOX), None);
    }

    #[test]
    fn the_nearest_of_two_and_what_hides_it() {
        let world = World::new();
        let front = cube(&world, [0., 0., 0.]);
        let back = cube(&world, [0., 0., -3.]);
        world.set_camera(Camera::looking_at([0., 0., 5.], [0., 0., 0.]));
        assert_eq!(world.pick(MIDDLE, BOX).map(|h| h.object), Some(front));
        world.update(front, |o| o.visible = false);
        assert_eq!(world.pick(MIDDLE, BOX).map(|h| h.object), Some(back));
        world.update(front, |o| o.visible = true);
        world.remove(front);
        assert_eq!(world.pick(MIDDLE, BOX).map(|h| h.object), Some(back));
    }

    #[test]
    fn a_child_is_picked_where_its_parent_places_it() {
        let world = World::new();
        let parent = world.add(Object::group().at([3., 0., 0.]));
        let child = world
            .add(Object::mesh(Geometry::sphere(0.5, 24), Material::rgb(0xffffff)).child_of(parent));
        world.set_camera(Camera::looking_at([3., 0., 5.], [3., 0., 0.]));
        let hit = world.pick(MIDDLE, BOX).expect("the child");
        assert_eq!(hit.object, child);
        assert!((hit.point.x - 3.).abs() < 0.05 && (hit.point.z - 0.5).abs() < 0.05);
        // Hiding the parent hides it.
        world.update(parent, |o| o.visible = false);
        assert_eq!(world.pick(MIDDLE, BOX), None);
    }

    #[test]
    fn an_orthographic_camera_picks_straight_ahead() {
        let world = World::new();
        let id = cube(&world, [0., 0., 0.]);
        world.set_camera(Camera {
            orthographic: true,
            fov: 4.,
            ..Camera::looking_at([0., 0., 5.], [0., 0., 0.])
        });
        // A quarter of the way across: x = -1 in a view 4 tall, 5.33 wide.
        assert_eq!(world.pick((100., 150.), BOX), None);
        let hit = world.pick((180., 150.), BOX).expect("the cube");
        assert_eq!(hit.object, id);
        assert!((hit.point.z - 0.5).abs() < 1e-4);
        assert!(
            (hit.point.x + 4. / 3. * 0.2).abs() < 1e-3,
            "{:?}",
            hit.point
        );
        let hit = world.pick(MIDDLE, BOX).expect("the cube");
        assert!((hit.distance - 4.5).abs() < 1e-3);
    }

    #[test]
    fn an_empty_box_picks_nothing() {
        let world = World::new();
        cube(&world, [0., 0., 0.]);
        assert_eq!(world.pick(MIDDLE, (0., 300.)), None);
        assert_eq!(world.pick((f32::NAN, 0.), BOX), None);
    }

    #[test]
    fn a_matrix_and_its_inverse_undo_each_other() {
        let m = Mat4::from_transform(
            vec3(1., -2., 3.),
            Quat::from_euler(0.3, -1.1, 0.7),
            vec3(2., 0.5, 1.5),
        );
        let identity = m * m.inverse().unwrap();
        for (c, column) in identity.columns.iter().enumerate() {
            for (r, v) in column.iter().enumerate() {
                let want = if c == r { 1. } else { 0. };
                assert!((v - want).abs() < 1e-5, "{c},{r}: {v}");
            }
        }
        let flat = Mat4::from_transform(Vec3::ZERO, Quat::IDENTITY, vec3(1., 0., 1.));
        assert_eq!(flat.inverse(), None);
    }
}
