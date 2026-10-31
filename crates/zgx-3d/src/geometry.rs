//! Shapes: triangles with normals and texture coordinates, shared between
//! the objects drawn with them (a renderer uploads each once).
use crate::math::{Vec3, vec3};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// A vertex: where, which way it faces, and its texture coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Vertex {
    /// Its position.
    pub position: [f32; 3],
    /// Its unit normal.
    pub normal: [f32; 3],
    /// Its texture coordinate, 0 to 1 across and down.
    pub uv: [f32; 2],
}

struct Data {
    id: u64,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    bounds: (Vec3, Vec3),
    /// Each vertex's joints and their weights, if it is skinned
    /// (`with_skin`).
    skin: Option<Arc<SkinWeights>>,
    /// Its shapes to blend toward, if it has any (`with_morph_targets`).
    morphs: Option<Arc<Morphs>>,
    /// Its second texture coordinates, if it has any (`with_second_uvs`).
    second_uvs: Option<Arc<Vec<[f32; 2]>>>,
    /// Its vertices' colours, if it has any (`with_colors`).
    colors: Option<Arc<Vec<[f32; 4]>>>,
}
/// A shape a geometry can blend toward (`Geometry::with_morph_targets`):
/// how far each vertex moves at full weight, and how its normal turns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphTarget {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
}
impl MorphTarget {
    /// One moving each vertex by its entry of `positions`.
    pub fn new(positions: Vec<[f32; 3]>) -> Self {
        Self {
            positions,
            normals: Vec::new(),
        }
    }
    /// It turning each vertex's normal by its entry of `normals`, added to
    /// the normal at full weight (normals stay as they are without).
    pub fn with_normals(mut self, normals: Vec<[f32; 3]>) -> Self {
        self.normals = normals;
        self
    }
}
/// A geometry's morph targets as a renderer reads them.
pub(crate) struct Morphs {
    targets: usize,
    /// Vertex by vertex, each target's position delta then normal delta.
    deltas: Vec<[f32; 4]>,
    /// Each target's least and greatest position delta.
    reach: Vec<(Vec3, Vec3)>,
}
impl Morphs {
    /// How many targets it has.
    pub(crate) fn targets(&self) -> usize {
        self.targets
    }
    /// Every delta, for the GPU (`at` says where one is).
    pub(crate) fn deltas(&self) -> &[[f32; 4]] {
        &self.deltas
    }
    /// Where `target`'s position delta for `vertex` is in `deltas`; its
    /// normal delta is the next.
    pub(crate) fn at(&self, vertex: usize, target: usize) -> usize {
        (vertex * self.targets + target) * 2
    }
}
/// A skinned geometry's joints (indices into its object's `Skin`) and their
/// weights, a vertex's four each.
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct Influence {
    pub(crate) joints: [u16; 4],
    pub(crate) weights: [f32; 4],
}
pub(crate) type SkinWeights = Vec<Influence>;

/// Triangles to draw objects with: shared (clones are the same shape) and
/// immutable, uploaded once per renderer.
#[derive(Clone)]
pub struct Geometry(Arc<Data>);
impl std::fmt::Debug for Geometry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Geometry")
            .field("vertices", &self.0.vertices.len())
            .field("triangles", &(self.0.indices.len() / 3))
            .finish()
    }
}
impl PartialEq for Geometry {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Geometry {
    /// Triangles of `indices` into `vertices`, three a triangle,
    /// counter-clockwise from their front. Fails on an index past the
    /// vertices or a count not a multiple of three.
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u32>) -> Result<Self, &'static str> {
        if !indices.len().is_multiple_of(3) {
            return Err("geometry indices come in threes");
        }
        if indices.iter().any(|&i| i as usize >= vertices.len()) {
            return Err("geometry index past its vertices");
        }
        let bounds = vertices.iter().fold(
            (Vec3::ONE * f32::MAX, Vec3::ONE * f32::MIN),
            |(low, high), v| {
                let p = Vec3::from(v.position);
                (low.min(p), high.max(p))
            },
        );
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Ok(Self(Arc::new(Data {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            vertices,
            indices,
            bounds: if bounds.0.x <= bounds.1.x {
                bounds
            } else {
                (Vec3::ZERO, Vec3::ZERO)
            },
            skin: None,
            morphs: None,
            second_uvs: None,
            colors: None,
        })))
    }
    /// A new geometry of the same triangles and extras, `change` applied.
    fn derived(&self, change: impl FnOnce(&mut Data)) -> Result<Self, &'static str> {
        let mut made = Self::new(self.0.vertices.clone(), self.0.indices.clone())?;
        let data = Arc::get_mut(&mut made.0).expect("a geometry just made");
        data.skin = self.0.skin.clone();
        data.morphs = self.0.morphs.clone();
        data.second_uvs = self.0.second_uvs.clone();
        data.colors = self.0.colors.clone();
        change(data);
        Ok(made)
    }
    /// It with a second set of texture coordinates, for a map to read
    /// instead of its first (`MapPlacement::uv_set`): often its occlusion's.
    /// A new geometry. Fails unless it has one entry a vertex.
    pub fn with_second_uvs(self, uvs: Vec<[f32; 2]>) -> Result<Self, &'static str> {
        if uvs.len() != self.0.vertices.len() {
            return Err("second texture coordinates have an entry a vertex");
        }
        self.derived(|data| data.second_uvs = Some(Arc::new(uvs)))
    }
    /// It with a colour at each vertex, linear, multiplying its material's
    /// and its base colour map's across each triangle. A new geometry. Fails
    /// unless it has one entry a vertex.
    pub fn with_colors(self, colors: Vec<[f32; 4]>) -> Result<Self, &'static str> {
        if colors.len() != self.0.vertices.len() {
            return Err("vertex colours have an entry a vertex");
        }
        self.derived(|data| data.colors = Some(Arc::new(colors)))
    }
    /// It skinned: each vertex moved by up to four joints of its object's
    /// skin (`World::set_skin`), `joints` their places in it and `weights`
    /// how much each moves it (summing to 1; normalised if not). A new
    /// geometry. Fails unless both have one entry a vertex.
    pub fn with_skin(
        self,
        joints: Vec<[u16; 4]>,
        weights: Vec<[f32; 4]>,
    ) -> Result<Self, &'static str> {
        let vertices = self.0.vertices.len();
        if joints.len() != vertices || weights.len() != vertices {
            return Err("a skin has joints and weights for every vertex");
        }
        let influences = joints
            .into_iter()
            .zip(weights)
            .map(|(joints, weights)| {
                let weights = weights.map(|w| if w.is_finite() { w.max(0.) } else { 0. });
                let total: f32 = weights.iter().sum();
                Influence {
                    joints,
                    weights: if total > 1e-6 {
                        weights.map(|w| w / total)
                    } else {
                        [1., 0., 0., 0.]
                    },
                }
            })
            .collect();
        self.derived(|data| data.skin = Some(Arc::new(influences)))
    }
    /// It with shapes to blend toward: an object's weights
    /// (`World::set_morph_weights`) add that much of each target's movement
    /// to its vertices, on the GPU, before any skin moves them. A new
    /// geometry. Fails unless every target has an entry a vertex (normals
    /// are optional).
    pub fn with_morph_targets(self, targets: Vec<MorphTarget>) -> Result<Self, &'static str> {
        let vertices = self.0.vertices.len();
        if targets.iter().any(|t| {
            t.positions.len() != vertices || !(t.normals.is_empty() || t.normals.len() == vertices)
        }) {
            return Err("a morph target has positions, and any normals, for every vertex");
        }
        let finite = |v: [f32; 3]| v.map(|n| if n.is_finite() { n } else { 0. });
        let mut deltas = Vec::with_capacity(vertices * targets.len() * 2);
        for vertex in 0..vertices {
            for target in &targets {
                let [x, y, z] = finite(target.positions[vertex]);
                let [a, b, c] = target.normals.get(vertex).copied().map_or([0.; 3], finite);
                deltas.push([x, y, z, 0.]);
                deltas.push([a, b, c, 0.]);
            }
        }
        let reach = targets
            .iter()
            .map(|target| {
                target
                    .positions
                    .iter()
                    .fold((Vec3::ZERO, Vec3::ZERO), |(low, high), &p| {
                        let p = Vec3::from(finite(p));
                        (low.min(p), high.max(p))
                    })
            })
            .collect();
        self.derived(|data| {
            data.morphs = (!targets.is_empty()).then(|| {
                Arc::new(Morphs {
                    targets: targets.len(),
                    deltas,
                    reach,
                })
            });
        })
    }
    /// Its vertices' joints and weights, if it is skinned.
    pub(crate) fn skin(&self) -> Option<&[Influence]> {
        self.0.skin.as_deref().map(Vec::as_slice)
    }
    /// Its vertices' colours, if it has any.
    pub(crate) fn colors(&self) -> Option<&[[f32; 4]]> {
        self.0.colors.as_deref().map(Vec::as_slice)
    }
    /// Its second texture coordinates, if it has any.
    pub(crate) fn second_uvs(&self) -> Option<&[[f32; 2]]> {
        self.0.second_uvs.as_deref().map(Vec::as_slice)
    }
    /// Its morph targets, if it has any.
    pub(crate) fn morphs(&self) -> Option<&Morphs> {
        self.0.morphs.as_deref()
    }
    /// The box holding it blended by `active` (`[target, weight]`s).
    pub(crate) fn morphed_bounds(&self, active: &[[f32; 2]]) -> (Vec3, Vec3) {
        let (mut low, mut high) = self.0.bounds;
        if let Some(morphs) = self.morphs() {
            for &[target, weight] in active {
                let Some(&(least, most)) = morphs.reach.get(target as usize) else {
                    continue;
                };
                // A negative weight moves the other way.
                let (a, b) = (least * weight, most * weight);
                low = low + a.min(b);
                high = high + a.max(b);
            }
        }
        (low, high)
    }
    /// Stable for its lifetime: renderers cache its buffers by it.
    pub fn id(&self) -> u64 {
        self.0.id
    }
    /// Its vertices.
    pub fn vertices(&self) -> &[Vertex] {
        &self.0.vertices
    }
    /// Its triangles' vertex indices, three a triangle.
    pub fn indices(&self) -> &[u32] {
        &self.0.indices
    }
    /// The box holding it: its least and greatest corners.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.0.bounds
    }

    /// A box `width` by `height` by `depth` about the origin.
    pub fn cuboid(width: f32, height: f32, depth: f32) -> Self {
        let half = vec3(width, height, depth) * 0.5;
        let faces: [(Vec3, Vec3, Vec3); 6] = [
            (vec3(1., 0., 0.), vec3(0., 0., -1.), vec3(0., 1., 0.)),
            (vec3(-1., 0., 0.), vec3(0., 0., 1.), vec3(0., 1., 0.)),
            (vec3(0., 1., 0.), vec3(1., 0., 0.), vec3(0., 0., -1.)),
            (vec3(0., -1., 0.), vec3(1., 0., 0.), vec3(0., 0., 1.)),
            (vec3(0., 0., 1.), vec3(1., 0., 0.), vec3(0., 1., 0.)),
            (vec3(0., 0., -1.), vec3(-1., 0., 0.), vec3(0., 1., 0.)),
        ];
        let (mut vertices, mut indices) = (Vec::with_capacity(24), Vec::with_capacity(36));
        for (normal, u, v) in faces {
            let base = vertices.len() as u32;
            for (a, b) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
                let p = (normal + u * a + v * b).scaled(half);
                vertices.push(Vertex {
                    position: p.to_array(),
                    normal: normal.to_array(),
                    uv: [(a + 1.) * 0.5, (1. - b) * 0.5],
                });
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Self::new(vertices, indices).expect("a box's indices")
    }
    /// A sphere of `radius` about the origin, `segments` around (at least 3)
    /// and half as many from pole to pole.
    pub fn sphere(radius: f32, segments: u32) -> Self {
        let around = segments.max(3);
        let down = (around / 2).max(2);
        Self::revolved(around, down, |u, v| {
            let (theta, phi) = (u * std::f32::consts::TAU, v * std::f32::consts::PI);
            let normal = vec3(phi.sin() * theta.cos(), phi.cos(), -phi.sin() * theta.sin());
            (normal * radius, normal)
        })
    }
    /// A flat square `width` by `depth` on the ground (facing up), about the
    /// origin.
    pub fn plane(width: f32, depth: f32) -> Self {
        let (w, d) = (width * 0.5, depth * 0.5);
        let vertices = [
            (-w, d, 0., 1.),
            (w, d, 1., 1.),
            (w, -d, 1., 0.),
            (-w, -d, 0., 0.),
        ]
        .map(|(x, z, u, v)| Vertex {
            position: [x, 0., z],
            normal: [0., 1., 0.],
            uv: [u, v],
        })
        .to_vec();
        Self::new(vertices, vec![0, 1, 2, 0, 2, 3]).expect("a plane's indices")
    }
    /// A cylinder of `radius`, `height` tall, about the origin along Y,
    /// capped, `segments` around (at least 3).
    pub fn cylinder(radius: f32, height: f32, segments: u32) -> Self {
        Self::capped(radius, radius, height, segments)
    }
    /// A cone of `radius` at its base, `height` tall, about the origin along
    /// Y, its base capped, `segments` around (at least 3).
    pub fn cone(radius: f32, height: f32, segments: u32) -> Self {
        Self::capped(radius, 0., height, segments)
    }
    /// A ring about Y: `radius` to the middle of its tube, the tube `tube`
    /// thick in radius, `segments` around the ring and half as many around
    /// the tube.
    pub fn torus(radius: f32, tube: f32, segments: u32) -> Self {
        let around = segments.max(3);
        Self::revolved(around, (around / 2).max(3), |u, v| {
            // The tube's angle runs against the ring's, so the grid's cells
            // wind counter-clockwise seen from outside, as the sphere's do.
            let (theta, phi) = (u * std::f32::consts::TAU, -v * std::f32::consts::TAU);
            let ring = vec3(theta.cos(), 0., -theta.sin());
            let normal = ring * phi.cos() + Vec3::Y * phi.sin();
            (ring * radius + normal * tube, normal)
        })
    }

    /// A grid of `around + 1` by `down + 1` vertices from `at(u, v)`
    /// (position, normal), u and v 0 to 1, two triangles a cell.
    fn revolved(around: u32, down: u32, at: impl Fn(f32, f32) -> (Vec3, Vec3)) -> Self {
        let mut vertices = Vec::with_capacity(((around + 1) * (down + 1)) as usize);
        for j in 0..=down {
            for i in 0..=around {
                let (u, v) = (i as f32 / around as f32, j as f32 / down as f32);
                let (position, normal) = at(u, v);
                vertices.push(Vertex {
                    position: position.to_array(),
                    normal: normal.normalized().to_array(),
                    uv: [u, v],
                });
            }
        }
        let mut indices = Vec::with_capacity((around * down * 6) as usize);
        let row = around + 1;
        for j in 0..down {
            for i in 0..around {
                let (a, b) = (j * row + i, (j + 1) * row + i);
                indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        Self::new(vertices, indices).expect("a grid's indices")
    }
    /// A tube from `bottom` radius to `top` radius, `height` tall, with a
    /// disc at each end that has a radius.
    fn capped(bottom: f32, top: f32, height: f32, segments: u32) -> Self {
        let around = segments.max(3);
        let half = height * 0.5;
        let slope = (bottom - top) / height.max(f32::EPSILON);
        let (mut vertices, mut indices) = (Vec::new(), Vec::new());
        for i in 0..=around {
            let u = i as f32 / around as f32;
            let theta = u * std::f32::consts::TAU;
            let ring = vec3(theta.cos(), 0., -theta.sin());
            let normal = (ring + Vec3::Y * slope).normalized();
            for (radius, y, v) in [(top, half, 0.), (bottom, -half, 1.)] {
                vertices.push(Vertex {
                    position: (ring * radius + Vec3::Y * y).to_array(),
                    normal: normal.to_array(),
                    uv: [u, v],
                });
            }
        }
        for i in 0..around {
            let a = i * 2;
            indices.extend([a, a + 1, a + 2, a + 2, a + 1, a + 3]);
        }
        for (radius, y, normal) in [(top, half, Vec3::Y), (bottom, -half, -Vec3::Y)] {
            if radius <= 0. {
                continue;
            }
            let centre = vertices.len() as u32;
            vertices.push(Vertex {
                position: [0., y, 0.],
                normal: normal.to_array(),
                uv: [0.5, 0.5],
            });
            for i in 0..=around {
                let theta = i as f32 / around as f32 * std::f32::consts::TAU;
                let (s, c) = theta.sin_cos();
                vertices.push(Vertex {
                    position: [c * radius, y, -s * radius],
                    normal: normal.to_array(),
                    uv: [0.5 + c * 0.5, 0.5 + s * 0.5],
                });
            }
            for i in 0..around {
                let (a, b) = (centre + 1 + i, centre + 2 + i);
                // Counter-clockwise seen from outside: up on top, down below.
                if normal.y > 0. {
                    indices.extend([centre, a, b]);
                } else {
                    indices.extend([centre, b, a]);
                }
            }
        }
        Self::new(vertices, indices).expect("a capped tube's indices")
    }
}
