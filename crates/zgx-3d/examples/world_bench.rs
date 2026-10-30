//! The CPU work of 3D frames, for the gate: a generated world drawn through
//! wgpu's no-op backend, so its count holds zgx-3d and wgpu's validation, not
//! a driver's or a software rasteriser's. Each frame moves what a real one
//! does: the whole world, and skins' joints and morph weights.
//!
//! `SCENE=textured` is meshes with a base colour map and nothing else, a sun
//! casting shadows: a feature here costs what it costs meshes not using it.
//! `SCENE=mixed` is those among skinned, morphing, double-sided cut-out and
//! placed ones. `MESHES` and `FRAMES` size it. Neither has an environment:
//! prefiltering one takes two thirds of a cell, and its threads move the count.
//! Run: cargo run --release -p zgx-3d --example world_bench
use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};
use zgx_3d::*;
use zgx_gpu::{
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const SIZE: u32 = 256;
/// Geometries and textures the meshes share, as a model's would.
const SHAPES: usize = 8;
const TEXTURES: usize = 16;

fn setting(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// A future of the no-op backend's, which never waits.
fn now<T>(future: impl Future<Output = T>) -> T {
    let mut context = Context::from_waker(Waker::noop());
    match pin!(future).poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the no-op backend waited"),
    }
}

fn device() -> (wgpu::Device, wgpu::Queue) {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop.enable = true;
    descriptor.flags = wgpu::InstanceFlags::empty();
    let adapter =
        now(wgpu::Instance::new(descriptor)
            .request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("the no-op adapter");
    now(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("the no-op device")
}

/// A texture of `index`, its colour its own, clear in a corner.
fn texture(index: usize) -> Texture {
    let shade = (index * 16) as u8;
    let pixels = (0..16)
        .flat_map(|i| [shade, 255 - shade, 128, if i == 0 { 0 } else { 255 }])
        .collect();
    Texture::from_rgba8(4, 4, pixels).expect("its size")
}

/// Which each mesh is in `SCENE=mixed`, in turn.
#[derive(Clone, Copy)]
enum Kind {
    Textured,
    Skinned,
    Morphing,
    CutOut,
    Placed,
}
const MIXED: [Kind; 5] = [
    Kind::Textured,
    Kind::Skinned,
    Kind::Morphing,
    Kind::CutOut,
    Kind::Placed,
];

/// The world, and what its frames move: its root, the joint its skinned
/// meshes share, and its morphing meshes.
fn world(mixed: bool, meshes: usize) -> (World, ObjectId, ObjectId, Vec<ObjectId>) {
    let world = World::new();
    world.set_shadows(true);
    world.add_light(Light::Directional {
        direction: vec3(-0.5, -1., -0.4),
        color: [1., 0.97, 0.92],
        intensity: 2.5,
    });
    world.set_camera(Camera::looking_at([0., 40., 80.], [0., 0., 0.]));
    let root = world.add(Object::group());
    let joint = world.add(Object::group().child_of(root));
    let shapes: Vec<Geometry> = (0..SHAPES)
        .map(|i| Geometry::cuboid(1., 1. + i as f32 * 0.1, 1.))
        .collect();
    let skinned: Vec<Geometry> = shapes
        .iter()
        .map(|shape| {
            let vertices = shape.vertices().len();
            shape
                .clone()
                .with_skin(vec![[0; 4]; vertices], vec![[1., 0., 0., 0.]; vertices])
                .expect("an entry a vertex")
        })
        .collect();
    let morphing: Vec<Geometry> = shapes
        .iter()
        .map(|shape| {
            let up = vec![[0., 0.5, 0.]; shape.vertices().len()];
            shape
                .clone()
                .with_morph_targets(vec![MorphTarget::new(up)])
                .expect("an entry a vertex")
        })
        .collect();
    let textures: Vec<Texture> = (0..TEXTURES).map(texture).collect();
    let side = (meshes as f32).sqrt().ceil() as usize;
    let mut morphs = Vec::new();
    for i in 0..meshes {
        let kind = if mixed {
            MIXED[i % MIXED.len()]
        } else {
            Kind::Textured
        };
        let shape = i % SHAPES;
        let geometry = match kind {
            Kind::Skinned => skinned[shape].clone(),
            Kind::Morphing => morphing[shape].clone(),
            _ => shapes[shape].clone(),
        };
        let material = Material::rgb(0xffffff).with(0.1, 0.6);
        let mesh = world.add(Object::mesh(geometry, material).child_of(root));
        let (x, z) = ((i % side) as f32, (i / side) as f32);
        let half = side as f32 / 2.;
        world.set_transform(mesh, Transform::at([(x - half) * 2., 0., (z - half) * 2.]));
        world.set_texture(mesh, Some(textures[i % TEXTURES].clone()));
        match kind {
            Kind::Textured => {}
            Kind::Skinned => {
                let skin = Skin::new(vec![joint], vec![Mat4::IDENTITY]).expect("a joint");
                world.set_skin(mesh, Some(skin));
            }
            Kind::Morphing => morphs.push(mesh),
            Kind::CutOut => {
                world.set_alpha_cutoff(mesh, Some(0.5));
                world.set_double_sided(mesh, true);
            }
            Kind::Placed => {
                let tiled = MapPlacement::new().scaled(2., 2.).offset(0.25, 0.);
                world.set_map_placement(mesh, Map::BaseColour, Some(tiled));
            }
        }
    }
    (world, root, joint, morphs)
}

fn main() {
    let mixed = std::env::var("SCENE").is_ok_and(|scene| scene == "mixed");
    let (meshes, frames) = (setting("MESHES", 2000), setting("FRAMES", 30));
    let (device, queue) = device();
    let (world, root, joint, morphs) = world(mixed, meshes);
    let mut painter = world.viewport();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("A frame"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[FORMAT.add_srgb_suffix()],
    });
    let view = target.create_view(&Default::default());
    let srgb = target.create_view(&wgpu::TextureViewDescriptor {
        format: Some(FORMAT.add_srgb_suffix()),
        ..Default::default()
    });
    for frame in 0..frames {
        let turned = frame as f32 * 0.01;
        world.set_transform(
            root,
            Transform {
                rotation: Quat::from_axis_angle(Vec3::Y, turned),
                ..Transform::default()
            },
        );
        world.set_transform(
            joint,
            Transform {
                rotation: Quat::from_axis_angle(vec3(1., 0., 0.), turned),
                ..Transform::default()
            },
        );
        for &mesh in &morphs {
            world.set_morph_weights(mesh, &[0.5 + turned.sin() * 0.5]);
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut drawing = ViewportFrame::new(
            &device,
            &queue,
            &mut encoder,
            &view,
            FORMAT,
            (SIZE, SIZE),
            1.,
        );
        drawing.target_srgb = Some(&srgb);
        painter.render(&mut drawing);
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::Poll).expect("the no-op device");
    }
}
