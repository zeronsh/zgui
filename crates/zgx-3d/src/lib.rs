//! Retained 3D scenes for zgx.
//!
//! A [`World`] holds meshes ([`Object`]: a [`Geometry`] in a [`Material`],
//! placed by a [`Transform`], grouped under parents), [`Light`]s and a
//! [`Camera`]. Its GPU view ([`World::view`]) shows it in any box
//! (`zgx::compose::gpu_view`), drawn with depth, 4x multisampling and
//! metallic-roughness shading. It draws a frame only when the world changes:
//! a still scene costs nothing, and its pipelines and buffers are made with
//! its first frame.
//!
//! With the `gltf` feature, `Model` reads glTF 2.0 files (`.glb`, or
//! `.gltf` and the buffers beside it) and `World::add_model` adds one as a
//! group to place, turn and scale as any object.
//!
//! ```no_run
//! use zgx::compose::prelude::*;
//! use zgx_3d::*;
//!
//! let world = World::new();
//! let cube = world.add(Object::mesh(Geometry::cuboid(1., 1., 1.), Material::rgb(0x4a90e2)));
//! world.add_light(Light::Directional {
//!     direction: vec3(-1., -2., -1.),
//!     color: [1., 1., 1.],
//!     intensity: 3.,
//! });
//! world.set_camera(Camera::looking_at([2., 1.5, 3.], [0., 0., 0.]));
//! let view = gpu_view("A cube", world.view()).w_full().h(320.);
//! // Turning it draws one frame:
//! world.update(cube, |o| o.transform.rotation = Quat::from_euler(0., 0.5, 0.));
//! ```
#![warn(missing_docs)]

#[cfg(feature = "gltf")]
mod animation;
mod environment;
mod geometry;
#[cfg(feature = "gltf")]
mod gltf;
mod math;
mod orbit;
mod pick;
mod render;
mod texture;
mod world;

#[cfg(feature = "gltf")]
pub use animation::{Clip, Placed};
pub use environment::Environment;
pub use geometry::{Geometry, Vertex};
#[cfg(feature = "gltf")]
pub use gltf::{Model, ModelError};
pub use math::{Mat4, Quat, Transform, Vec3, vec3};
pub use orbit::Orbit;
pub use pick::Hit;
pub use texture::{Map, Texture};
pub use world::{
    Camera, Light, LightId, MAX_JOINTS, Material, Object, ObjectId, POINT_LIGHTS, Skin, World,
    srgb_to_linear,
};
