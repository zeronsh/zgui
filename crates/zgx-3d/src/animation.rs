//! A glTF model's animations (feature `gltf`): clips whose channels move
//! its nodes (`Model::clips`), posed on a model placed in a world
//! (`World::place_model`, `Placed::pose`) at any time, as often as wanted.
use crate::{ObjectId, Quat, Transform, Vec3, World, vec3};

/// One of a model's animations: its channels move nodes over `duration`
/// seconds.
#[derive(Clone, Debug)]
pub struct Clip {
    pub(crate) name: Option<String>,
    pub(crate) duration: f32,
    pub(crate) channels: Vec<Channel>,
}
impl Clip {
    /// Its name in the file, if it has one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// How long it runs, in seconds: its last key's time.
    pub fn duration(&self) -> f32 {
        self.duration
    }
}

/// What a channel moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Path {
    Translation,
    Rotation,
    Scale,
    /// A morph target's weight (a channel a target: `sample`'s first value).
    Weight(usize),
}
/// How a channel goes between its keys (glTF's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Interpolation {
    /// Each key's value until the next.
    Step,
    /// Straight between keys (rotations along the shortest arc).
    Linear,
    /// Hermite curves through the keys, their tangents given: three values
    /// a key (in-tangent, value, out-tangent).
    Cubic,
}
/// A node's translation, rotation or scale over time.
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    pub(crate) node: usize,
    pub(crate) path: Path,
    pub(crate) interpolation: Interpolation,
    /// Keys' times, rising.
    pub(crate) times: Vec<f32>,
    /// Values, a key's (three for `Cubic`), xyz or xyzw.
    pub(crate) values: Vec<[f32; 4]>,
}
impl Channel {
    /// Its value at `time` (seconds), held before its first key and after
    /// its last.
    pub(crate) fn sample(&self, time: f32) -> [f32; 4] {
        let per = if self.interpolation == Interpolation::Cubic {
            3
        } else {
            1
        };
        let value = |key: usize| self.values[key * per + per / 2];
        let last = self.times.len() - 1;
        // Before its first key, or no time (NaN): its first value.
        if time.partial_cmp(&self.times[0]) != Some(std::cmp::Ordering::Greater) {
            return value(0);
        }
        if time >= self.times[last] {
            return value(last);
        }
        let next = self.times.partition_point(|&t| t <= time);
        let key = next - 1;
        let (t0, t1) = (self.times[key], self.times[next]);
        let span = t1 - t0;
        let s = if span > 0. { (time - t0) / span } else { 0. };
        let rotation = self.path == Path::Rotation;
        match self.interpolation {
            Interpolation::Step => value(key),
            Interpolation::Linear if rotation => slerp(value(key), value(next), s),
            Interpolation::Linear => lerp(value(key), value(next), s),
            Interpolation::Cubic => {
                let out_tangent = self.values[key * 3 + 2];
                let in_tangent = self.values[next * 3];
                let (s2, s3) = (s * s, s * s * s);
                let (h00, h10) = (2. * s3 - 3. * s2 + 1., s3 - 2. * s2 + s);
                let (h01, h11) = (-2. * s3 + 3. * s2, s3 - s2);
                let (p0, p1) = (value(key), value(next));
                let mut out = [0.; 4];
                for i in 0..4 {
                    out[i] = h00 * p0[i]
                        + h10 * span * out_tangent[i]
                        + h01 * p1[i]
                        + h11 * span * in_tangent[i];
                }
                if rotation { normalized(out) } else { out }
            }
        }
    }
}
fn lerp(a: [f32; 4], b: [f32; 4], s: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * s)
}
fn normalized(q: [f32; 4]) -> [f32; 4] {
    let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length > 1e-12 {
        q.map(|v| v / length)
    } else {
        [0., 0., 0., 1.]
    }
}
/// The rotation `s` of the way from `a` to `b`, along the shorter arc.
fn slerp(a: [f32; 4], b: [f32; 4], s: f32) -> [f32; 4] {
    let mut dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let b = if dot < 0. {
        dot = -dot;
        b.map(|v| -v)
    } else {
        b
    };
    if dot > 0.9995 {
        return normalized(lerp(a, b, s));
    }
    let angle = dot.clamp(-1., 1.).acos();
    let (wa, wb) = (
        ((1. - s) * angle).sin() / angle.sin(),
        (s * angle).sin() / angle.sin(),
    );
    std::array::from_fn(|i| a[i] * wa + b[i] * wb)
}

/// A model placed in a world (`World::place_model`): its root, and the
/// object each of its nodes became, to pose its clips on.
#[derive(Clone, Debug)]
pub struct Placed {
    /// The group holding it: place, turn and scale it as any object.
    pub root: ObjectId,
    /// By node index; `None` for a node its scene doesn't show.
    pub(crate) nodes: Vec<Option<ObjectId>>,
    /// Each node's meshes, by node index.
    pub(crate) meshes: Vec<Vec<ObjectId>>,
    /// Its lights (`KHR_lights_punctual`), each following its node.
    pub(crate) lights: Vec<crate::LightId>,
    /// Its cameras, with the nodes holding them, in the order placed.
    pub(crate) cameras: Vec<(ObjectId, crate::gltf::ModelCamera)>,
}
impl Placed {
    /// Its lights, each following its node: set, turn off or remove them as
    /// any (`World::set_light`, `World::remove_light`).
    pub fn lights(&self) -> &[crate::LightId] {
        &self.lights
    }
    /// Its cameras where its nodes are now, in its scene's order, for
    /// `World::set_camera` (one under a hidden node left out). A camera
    /// looks along its node's −z, without its roll.
    pub fn cameras(&self, world: &World) -> Vec<crate::Camera> {
        let placed = |(node, camera): &(ObjectId, crate::gltf::ModelCamera)| {
            Some(camera.placed(&world.world_matrix(*node)?))
        };
        self.cameras.iter().filter_map(placed).collect()
    }
    /// Pose `clip` at `seconds` into it (held at its ends: loop by taking
    /// the time modulo its `duration`): the nodes its channels move take
    /// their values then. A frame is drawn only if something moved.
    pub fn pose(&self, world: &World, clip: &Clip, seconds: f32) {
        for channel in &clip.channels {
            let Some(Some(id)) = self.nodes.get(channel.node) else {
                continue;
            };
            let [x, y, z, w] = channel.sample(seconds);
            if let Path::Weight(target) = channel.path {
                // Its meshes share the node's weights.
                for &mesh in self.meshes.get(channel.node).into_iter().flatten() {
                    world.set_morph_weight(mesh, target, x);
                }
                continue;
            }
            world.update(*id, |object| {
                let transform: &mut Transform = &mut object.transform;
                match channel.path {
                    Path::Translation => transform.position = vec3(x, y, z),
                    Path::Rotation => transform.rotation = Quat { x, y, z, w },
                    Path::Scale => transform.scale = Vec3::from([x, y, z]),
                    Path::Weight(_) => {}
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(interpolation: Interpolation, path: Path, values: Vec<[f32; 4]>) -> Channel {
        Channel {
            node: 0,
            path,
            interpolation,
            times: vec![1., 3.],
            values,
        }
    }

    #[test]
    fn keys_are_held_at_the_ends_and_joined_between() {
        let linear = channel(
            Interpolation::Linear,
            Path::Translation,
            vec![[0., 0., 0., 0.], [4., 2., 0., 0.]],
        );
        assert_eq!(linear.sample(0.), [0., 0., 0., 0.]);
        assert_eq!(linear.sample(2.), [2., 1., 0., 0.]);
        assert_eq!(linear.sample(9.), [4., 2., 0., 0.]);
        let step = channel(
            Interpolation::Step,
            Path::Translation,
            vec![[0., 0., 0., 0.], [4., 2., 0., 0.]],
        );
        assert_eq!(step.sample(2.9), [0., 0., 0., 0.]);
        assert_eq!(step.sample(3.), [4., 2., 0., 0.]);
    }

    #[test]
    fn rotations_turn_the_short_way_at_an_even_pace() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        // None to a quarter turn about y, its second key written negated
        // (the same rotation, the long way round if taken literally).
        let rotation = channel(
            Interpolation::Linear,
            Path::Rotation,
            vec![[0., 0., 0., 1.], [0., -half, 0., -half]],
        );
        let [x, y, z, w] = rotation.sample(2.);
        let turned = Quat { x, y, z, w }.rotate(vec3(1., 0., 0.));
        // An eighth of a turn about y, the short way.
        let eighth = std::f32::consts::FRAC_PI_4;
        assert!(
            (turned - vec3(eighth.cos(), 0., -eighth.sin())).length() < 1e-4,
            "{turned:?}"
        );
    }

    #[test]
    fn a_cubic_channel_passes_through_its_keys_along_its_tangents() {
        // Keys 0 and 2, tangents 0: an ease in and out between them.
        let zero = [0.; 4];
        let cubic = channel(
            Interpolation::Cubic,
            Path::Translation,
            vec![zero, zero, zero, zero, [2., 0., 0., 0.], zero],
        );
        assert_eq!(cubic.sample(1.)[0], 0.);
        assert_eq!(cubic.sample(3.)[0], 2.);
        assert!(
            (cubic.sample(2.)[0] - 1.).abs() < 1e-6,
            "halfway at the middle"
        );
        assert!(
            cubic.sample(1.5)[0] < 0.5,
            "slower out of its first key than a straight line"
        );
    }
}
