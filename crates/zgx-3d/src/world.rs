//! A retained 3D scene (`World`): objects, lights and a camera, drawn by a
//! GPU view (`World::view`) a frame only when one of them changes.
use crate::{
    geometry::Geometry,
    math::{Mat4, Transform, Vec3, vec3},
};
use std::{cell::RefCell, rc::Rc};

/// How a surface takes light: a metallic-roughness material, as glTF's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Its colour in linear light, alpha its opacity (premultiplied as drawn).
    pub color: [f32; 4],
    /// 0: a dielectric (plastic, wood); 1: a metal.
    pub metallic: f32,
    /// 0: a mirror; 1: chalk.
    pub roughness: f32,
    /// Light it gives off itself, linear, added whatever lights it.
    pub emissive: [f32; 3],
    /// Its colour as it is, no light reckoned.
    pub unlit: bool,
}
impl Default for Material {
    fn default() -> Self {
        Self {
            color: [0.8, 0.8, 0.8, 1.],
            metallic: 0.,
            roughness: 0.5,
            emissive: [0.; 3],
            unlit: false,
        }
    }
}
impl Material {
    /// A dielectric of an sRGB colour (`0xRRGGBB`, as a design gives it).
    pub fn rgb(hex: u32) -> Self {
        let channel = |shift: u32| srgb_to_linear(((hex >> shift) & 0xff) as f32 / 255.);
        Self {
            color: [channel(16), channel(8), channel(0), 1.],
            ..Self::default()
        }
    }
    /// It with `metallic` and `roughness`.
    pub fn with(self, metallic: f32, roughness: f32) -> Self {
        Self {
            metallic,
            roughness,
            ..self
        }
    }
}
/// An sRGB channel (0 to 1) in linear light.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    /// Sunlight: parallel rays going `direction`, `intensity` in lux-like
    /// units (3 is a bright day for these materials).
    Directional {
        /// Which way the light travels.
        direction: Vec3,
        /// Its colour, linear.
        color: [f32; 3],
        /// How strong.
        intensity: f32,
    },
    /// A bulb at `position`, falling off with distance to nothing at `range`.
    Point {
        /// Where it is.
        position: Vec3,
        /// Its colour, linear.
        color: [f32; 3],
        /// How strong.
        intensity: f32,
        /// How far it reaches.
        range: f32,
    },
    /// Light from all around: `sky` from above, `ground` from below, mixed
    /// between by which way a surface faces.
    Hemisphere {
        /// From above, linear.
        sky: [f32; 3],
        /// From below, linear.
        ground: [f32; 3],
        /// How strong.
        intensity: f32,
    },
}
/// Point lights a frame shades with (the nearest the camera's target, past
/// this many).
pub const POINT_LIGHTS: usize = 4;

/// How the scene is seen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Where it is.
    pub position: Vec3,
    /// What it looks at.
    pub target: Vec3,
    /// Its field of view top to bottom, in radians (perspective), or how
    /// tall what it sees is (orthographic).
    pub fov: f32,
    /// The nearest and farthest it sees.
    pub near: f32,
    /// The farthest it sees.
    pub far: f32,
    /// Parallel rather than perspective.
    pub orthographic: bool,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            position: vec3(0., 1.5, 5.),
            target: Vec3::ZERO,
            fov: 45_f32.to_radians(),
            near: 0.05,
            far: 200.,
            orthographic: false,
        }
    }
}
impl Camera {
    /// At `position`, looking at `target`.
    pub fn looking_at(position: impl Into<Vec3>, target: impl Into<Vec3>) -> Self {
        Self {
            position: position.into(),
            target: target.into(),
            ..Self::default()
        }
    }
    /// Its view: the scene to its frame.
    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.position, self.target, Vec3::Y)
    }
    /// Its projection for a box `aspect` wide for 1 high.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 0. {
            aspect
        } else {
            1.
        };
        let near = self.near.max(1e-4);
        let far = self.far.max(near * 2.);
        if self.orthographic {
            Mat4::orthographic(self.fov.max(1e-3), aspect, near, far)
        } else {
            Mat4::perspective(self.fov.clamp(1e-3, 3.1), aspect, near, far)
        }
    }
}

/// An object of a world (`World::add`): stable while it lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    index: u32,
    generation: u32,
}
/// A light of a world (`World::add_light`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LightId {
    index: u32,
    generation: u32,
}

/// What an object is: a mesh of `geometry` in `material`, or nothing drawn
/// (a group's origin, for its children).
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Its shape; `None` draws nothing (a group).
    pub geometry: Option<Geometry>,
    /// How it takes light.
    pub material: Material,
    /// Where it is in its parent's frame.
    pub transform: Transform,
    /// Whether it (and its children) draw.
    pub visible: bool,
    /// Its parent; `None` is the world.
    pub parent: Option<ObjectId>,
}
impl Object {
    /// A mesh of `geometry` in `material`, at the origin.
    pub fn mesh(geometry: Geometry, material: Material) -> Self {
        Self {
            geometry: Some(geometry),
            material,
            transform: Transform::default(),
            visible: true,
            parent: None,
        }
    }
    /// A group: draws nothing, places its children.
    pub fn group() -> Self {
        Self {
            geometry: None,
            material: Material::default(),
            transform: Transform::default(),
            visible: true,
            parent: None,
        }
    }
    /// It placed by `transform`.
    pub fn transformed(self, transform: Transform) -> Self {
        Self { transform, ..self }
    }
    /// It at `position`.
    pub fn at(self, position: impl Into<Vec3>) -> Self {
        Self {
            transform: Transform {
                position: position.into(),
                ..self.transform
            },
            ..self
        }
    }
    /// It a child of `parent`.
    pub fn child_of(self, parent: ObjectId) -> Self {
        Self {
            parent: Some(parent),
            ..self
        }
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}
/// Put `value` in the first free slot (or a new one): its index and
/// generation.
fn claim<T>(slots: &mut Vec<Slot<T>>, value: T) -> (u32, u32) {
    let index = match slots.iter().position(|slot| slot.value.is_none()) {
        Some(free) => free,
        None => {
            slots.push(Slot {
                generation: 0,
                value: None,
            });
            slots.len() - 1
        }
    };
    let slot = &mut slots[index];
    slot.value = Some(value);
    (index as u32, slot.generation)
}
pub(crate) struct Inner {
    objects: Vec<Slot<Object>>,
    lights: Vec<Slot<Light>>,
    pub(crate) camera: Camera,
    /// Its clear colour, linear and premultiplied; `None`: transparent, the
    /// box's own background shows.
    pub(crate) background: Option<[f32; 4]>,
    /// Whether the sun casts shadows (`World::set_shadows`).
    pub(crate) shadows: bool,
    /// Objects' textures (`World::set_map`), by index and map, with the
    /// generation they were set for.
    textures: std::collections::HashMap<(u32, crate::Map), (u32, crate::Texture)>,
    /// Counts changes: a renderer redraws what changed since it last drew.
    pub(crate) revision: u64,
}
/// What a frame draws: each visible mesh with its world matrix.
pub(crate) struct Drawn<'a> {
    pub(crate) geometry: &'a Geometry,
    pub(crate) material: &'a Material,
    /// Its textures, by `Map::ALL`'s order.
    pub(crate) maps: [Option<&'a crate::Texture>; crate::texture::MAPS],
    pub(crate) matrix: Mat4,
}
impl Inner {
    pub(crate) fn lights(&self) -> impl Iterator<Item = &Light> {
        self.lights.iter().filter_map(|slot| slot.value.as_ref())
    }
    /// Every visible mesh with its world matrix, its ancestors' applied.
    fn world_of(&self, index: usize) -> Option<Mat4> {
        let mut memo = vec![None; self.objects.len()];
        world_matrix(self, index, &mut memo, 0)
    }
    pub(crate) fn drawn(&self) -> Vec<Drawn<'_>> {
        let mut matrices: Vec<Option<Option<Mat4>>> = vec![None; self.objects.len()];
        let mut out = Vec::new();
        for index in 0..self.objects.len() {
            if let Some(object) = &self.objects[index].value
                && let Some(geometry) = &object.geometry
                && let Some(matrix) = world_matrix(self, index, &mut matrices, 0)
            {
                let generation = self.objects[index].generation;
                out.push(Drawn {
                    geometry,
                    material: &object.material,
                    maps: crate::Map::ALL.map(|map| {
                        self.textures
                            .get(&(index as u32, map))
                            .filter(|(set_for, _)| *set_for == generation)
                            .map(|(_, texture)| texture)
                    }),
                    matrix,
                });
            }
        }
        out
    }
    /// The nearest visible mesh the world segment `from` to `to` meets, and
    /// where along it (0 to 1).
    fn nearest(&self, from: Vec3, to: Vec3) -> Option<(ObjectId, f32)> {
        let mut matrices: Vec<Option<Option<Mat4>>> = vec![None; self.objects.len()];
        let mut nearest: Option<(ObjectId, f32)> = None;
        for index in 0..self.objects.len() {
            let slot = &self.objects[index];
            if let Some(object) = &slot.value
                && let Some(geometry) = &object.geometry
                && let Some(matrix) = world_matrix(self, index, &mut matrices, 0)
                && let Some(t) = crate::pick::meet(geometry, matrix, from, to)
                && nearest.is_none_or(|(_, n)| t < n)
            {
                let id = ObjectId {
                    index: index as u32,
                    generation: slot.generation,
                };
                nearest = Some((id, t));
            }
        }
        nearest
    }
    fn live(&self, index: u32, generation: u32) -> Option<usize> {
        let slot = self.objects.get(index as usize)?;
        (slot.generation == generation && slot.value.is_some()).then_some(index as usize)
    }
}

// An object's world matrix, or `None` where it or an ancestor is
// hidden; each worked out once.
fn world_matrix(
    inner: &Inner,
    index: usize,
    memo: &mut [Option<Option<Mat4>>],
    depth: u32,
) -> Option<Mat4> {
    if let Some(known) = memo[index] {
        return known;
    }
    let object = inner.objects[index].value.as_ref()?;
    let local = object.visible.then(|| object.transform.matrix());
    let found = match (local, object.parent) {
        (None, _) => None,
        (Some(local), None) => Some(local),
        // A cycle (or one too deep to be meant) draws nothing.
        (Some(_), Some(_)) if depth > 64 => None,
        (Some(local), Some(parent)) => inner
            .live(parent.index, parent.generation)
            .and_then(|p| world_matrix(inner, p, memo, depth + 1))
            .map(|parent| parent * local),
    };
    memo[index] = Some(found);
    found
}

/// A 3D scene shown by a GPU view (`view`): change it and the view draws a
/// frame, before the window's next; leave it and it draws nothing. Clones
/// share it.
#[derive(Clone)]
pub struct World {
    inner: Rc<RefCell<Inner>>,
    view: zgx::gpu_view::GpuView,
}
impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.borrow();
        f.debug_struct("World")
            .field(
                "objects",
                &inner.objects.iter().filter(|s| s.value.is_some()).count(),
            )
            .field("lights", &inner.lights().count())
            .finish()
    }
}
impl World {
    /// An empty world: a camera 5 back and 1.5 up looking at the origin, no
    /// lights, a transparent background.
    pub fn new() -> Self {
        let inner = Rc::new(RefCell::new(Inner {
            objects: Vec::new(),
            lights: Vec::new(),
            camera: Camera::default(),
            background: None,
            shadows: false,
            textures: Default::default(),
            revision: 0,
        }));
        let view = zgx_gpu::viewport::view(crate::render::Renderer::new(Rc::downgrade(&inner)));
        Self { inner, view }
    }
    /// What draws it, to draw it outside a view.
    pub fn viewport(&self) -> impl zgx_gpu::viewport::Viewport + use<> {
        crate::render::Renderer::new(Rc::downgrade(&self.inner))
    }
    /// The GPU view drawing it: show it with `zgx::compose::gpu_view`.
    pub fn view(&self) -> &zgx::gpu_view::GpuView {
        &self.view
    }
    /// Change it with `f`; the view draws a frame if `f` says it changed.
    fn change(&self, f: impl FnOnce(&mut Inner) -> bool) {
        let changed = f(&mut self.inner.borrow_mut());
        if changed {
            let mut inner = self.inner.borrow_mut();
            inner.revision = inner.revision.wrapping_add(1);
            drop(inner);
            self.view.invalidate();
        }
    }
    /// Add `object`.
    pub fn add(&self, mut object: Object) -> ObjectId {
        object.transform = object.transform.sanitized();
        let mut id = None;
        self.change(|inner| {
            let (index, generation) = claim(&mut inner.objects, object);
            id = Some(ObjectId { index, generation });
            true
        });
        id.unwrap()
    }
    /// Remove `id` (its children stay, drawing nothing until reparented).
    pub fn remove(&self, id: ObjectId) {
        self.change(|inner| match inner.objects.get_mut(id.index as usize) {
            Some(slot) if slot.generation == id.generation && slot.value.is_some() => {
                slot.value = None;
                slot.generation = slot.generation.wrapping_add(1);
                inner.textures.retain(|(index, _), _| *index != id.index);
                true
            }
            _ => false,
        });
    }
    /// Object `id`'s texture: its colour multiplied by `texture` across its
    /// geometry's texture coordinates (`None`: its colour alone). A copy of
    /// the texture is shared, not made. Its base colour map (`set_map`).
    pub fn set_texture(&self, id: ObjectId, texture: Option<crate::Texture>) {
        self.set_map(id, crate::Map::BaseColour, texture);
    }
    /// Object `id`'s texture, if it has one (its base colour map).
    pub fn texture(&self, id: ObjectId) -> Option<crate::Texture> {
        self.map(id, crate::Map::BaseColour)
    }
    /// Object `id`'s `map`: what `texture` gives it across its geometry's
    /// texture coordinates (`None`: its material alone). A copy of the
    /// texture is shared, not made.
    pub fn set_map(&self, id: ObjectId, map: crate::Map, texture: Option<crate::Texture>) {
        self.change(|inner| {
            if inner.live(id.index, id.generation).is_none() {
                return false;
            }
            let key = (id.index, map);
            let before = inner.textures.get(&key).map(|(_, t)| t.id());
            match texture {
                Some(texture) => {
                    let changed = before != Some(texture.id());
                    inner.textures.insert(key, (id.generation, texture));
                    changed
                }
                None => inner.textures.remove(&key).is_some(),
            }
        });
    }
    /// Object `id`'s `map`, if it has one.
    pub fn map(&self, id: ObjectId, map: crate::Map) -> Option<crate::Texture> {
        let inner = self.inner.borrow();
        inner.live(id.index, id.generation)?;
        inner
            .textures
            .get(&(id.index, map))
            .filter(|(set_for, _)| *set_for == id.generation)
            .map(|(_, texture)| texture.clone())
    }
    /// Change object `id` with `f` (nothing if it is gone). The view draws a
    /// frame only if the object changed.
    pub fn update(&self, id: ObjectId, f: impl FnOnce(&mut Object)) {
        self.change(|inner| {
            let Some(index) = inner.live(id.index, id.generation) else {
                return false;
            };
            let object = inner.objects[index].value.as_mut().unwrap();
            let before = object.clone();
            f(object);
            object.transform = object.transform.sanitized();
            *object != before
        });
    }
    /// Place object `id`.
    pub fn set_transform(&self, id: ObjectId, transform: Transform) {
        self.update(id, |object| object.transform = transform);
    }
    /// Object `id`'s material.
    pub fn set_material(&self, id: ObjectId, material: Material) {
        self.update(id, |object| object.material = material);
    }
    /// A copy of object `id`, if it lives.
    pub fn object(&self, id: ObjectId) -> Option<Object> {
        let inner = self.inner.borrow();
        inner
            .live(id.index, id.generation)
            .map(|index| inner.objects[index].value.clone().unwrap())
    }
    /// Add `light`.
    pub fn add_light(&self, light: Light) -> LightId {
        let mut id = None;
        self.change(|inner| {
            let (index, generation) = claim(&mut inner.lights, light);
            id = Some(LightId { index, generation });
            true
        });
        id.unwrap()
    }
    /// Light `id` as `light`.
    pub fn set_light(&self, id: LightId, light: Light) {
        self.change(|inner| match inner.lights.get_mut(id.index as usize) {
            Some(slot) if slot.generation == id.generation && slot.value.is_some() => {
                let changed = slot.value != Some(light);
                slot.value = Some(light);
                changed
            }
            _ => false,
        });
    }
    /// Remove light `id`.
    pub fn remove_light(&self, id: LightId) {
        self.change(|inner| match inner.lights.get_mut(id.index as usize) {
            Some(slot) if slot.generation == id.generation && slot.value.is_some() => {
                slot.value = None;
                slot.generation = slot.generation.wrapping_add(1);
                true
            }
            _ => false,
        });
    }
    /// How it is seen.
    pub fn set_camera(&self, camera: Camera) {
        self.change(|inner| std::mem::replace(&mut inner.camera, camera) != camera);
    }
    /// The camera.
    pub fn camera(&self) -> Camera {
        self.inner.borrow().camera
    }
    /// Its background: an sRGB colour (`0xRRGGBB`), or `None` for transparent
    /// (the box's own shows).
    pub fn set_background(&self, rgb: Option<u32>) {
        let color = rgb.map(|hex| {
            let m = Material::rgb(hex).color;
            [m[0], m[1], m[2], 1.]
        });
        self.change(|inner| std::mem::replace(&mut inner.background, color) != color);
    }
    /// Whether the sun (its first directional light) casts shadows: opaque
    /// meshes darken what they hide from it, their own faces included.
    /// Off by default; on, each frame draws the opaque meshes once more,
    /// from the sun, into a 2048 by 2048 depth map (16 MB) kept while
    /// shadows are on.
    pub fn set_shadows(&self, on: bool) {
        self.change(|inner| std::mem::replace(&mut inner.shadows, on) != on);
    }
    /// Object `id`'s matrix in the world, its ancestors' applied; `None` if
    /// it is gone, hidden or under a hidden one.
    pub fn world_matrix(&self, id: ObjectId) -> Option<Mat4> {
        let inner = self.inner.borrow();
        let index = inner.live(id.index, id.generation)?;
        inner.world_of(index)
    }
    /// Its background as a colour in linear light (alpha 1: opaque), or
    /// `None` for transparent.
    pub fn set_background_linear(&self, color: Option<[f32; 4]>) {
        let color = color.filter(|c| c.iter().all(|v| v.is_finite()));
        self.change(|inner| std::mem::replace(&mut inner.background, color) != color);
    }
    #[cfg(test)]
    pub(crate) fn inner(&self) -> std::cell::Ref<'_, Inner> {
        self.inner.borrow()
    }
    /// What the view shows at `point` of its box (from its top left, in the
    /// box's points), the box `size` points: the nearest visible mesh there,
    /// and where on it. Found on the CPU from the meshes' triangles, both
    /// sides; nothing is drawn for it.
    pub fn pick(&self, point: (f32, f32), size: (f32, f32)) -> Option<crate::Hit> {
        let inner = self.inner.borrow();
        let camera = inner.camera;
        let view_projection = camera.projection(size.0 / size.1) * camera.view();
        let (from, to) = crate::pick::segment(view_projection, point, size)?;
        let (object, t) = inner.nearest(from, to)?;
        let point = from + (to - from) * t;
        Some(crate::Hit {
            object,
            point,
            distance: (point - camera.position).length(),
        })
    }
    /// Changes made so far: a frame is drawn for each run of them.
    pub fn revision(&self) -> u64 {
        self.inner.borrow().revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sun() -> Light {
        Light::Directional {
            direction: Vec3::Y,
            color: [1.; 3],
            intensity: 1.,
        }
    }

    #[test]
    fn removed_lights_free_their_slots() {
        let world = World::new();
        for _ in 0..4096 {
            let id = world.add_light(sun());
            world.remove_light(id);
        }
        assert!(world.inner.borrow().lights.len() <= 1);
    }

    #[test]
    fn a_removed_lights_id_does_not_reach_the_one_in_its_slot() {
        let world = World::new();
        let old = world.add_light(sun());
        world.remove_light(old);
        let new = world.add_light(sun());
        assert_eq!(new.index, old.index, "the slot is reused");
        let revision = world.revision();
        world.remove_light(old);
        world.set_light(
            old,
            Light::Directional {
                direction: crate::math::vec3(1., 0., 0.),
                color: [1.; 3],
                intensity: 2.,
            },
        );
        assert_eq!(world.revision(), revision);
        assert_eq!(world.inner.borrow().lights().count(), 1);
    }
}
