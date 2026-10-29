//! A glTF model in a zgx window: a `.glb`, or a `.gltf` and the files beside
//! it, under a sun (casting shadows) and a sky. Drag to turn the camera
//! about it, scroll or pinch to move in and out.
//! Run: cargo run --release -p zgx-3d --features gltf --example model -- FILE
//!
//! `--smoke-test` shows it a moment and quits.
use std::{cell::Cell, path::Path, rc::Rc};
use zgx::{compose::prelude::*, input::InputEvent, scene::Color};
use zgx_3d::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let path = std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .ok_or("usage: model FILE.glb|FILE.gltf [--smoke-test]")?;
    let path = Path::new(&path).to_path_buf();
    let started = std::time::Instant::now();
    let bytes = std::fs::read(&path)?;
    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let model = Model::from_bytes_with(&bytes, |uri| std::fs::read(folder.join(uri)).ok())?;
    let (low, high) = model.bounds().ok_or("the model has no meshes")?;
    eprintln!(
        "read {} in {:.1} ms: {:?} to {:?}",
        path.display(),
        started.elapsed().as_secs_f64() * 1e3,
        low.to_array(),
        high.to_array()
    );
    Application::new()
        .window(WindowOptions {
            title: format!("zgx 3D · {}", path.display()),
            width: 720.,
            height: 560.,
            ..Default::default()
        })
        .run(move |cx| {
            let world = World::new();
            world.set_background(Some(0x10141b));
            world.set_shadows(true);
            // Scaled to 2 across, its base on a floor at the origin.
            let size = high - low;
            let scale = 2. / size.x.max(size.y).max(size.z).max(1e-6);
            let centre = (low + high) * 0.5;
            let id = world.add_model(&model);
            world.set_transform(
                id,
                Transform {
                    position: vec3(-centre.x, -low.y, -centre.z) * scale,
                    scale: Vec3::ONE * scale,
                    ..Transform::default()
                },
            );
            world.add(Object::mesh(
                Geometry::plane(6., 6.),
                Material::rgb(0x2a313c).with(0., 0.9),
            ));
            world.add_light(Light::Directional {
                direction: vec3(-0.5, -1., -0.4),
                color: [1., 0.97, 0.92],
                intensity: 2.5,
            });
            world.add_light(Light::Hemisphere {
                sky: [0.35, 0.45, 0.6],
                ground: [0.12, 0.1, 0.08],
                intensity: 0.8,
            });
            let orbit = Rc::new(Cell::new(Orbit {
                target: vec3(0., size.y * scale * 0.5, 0.),
                azimuth: 0.5,
                elevation: 0.3,
                distance: 4.,
                ..Orbit::default()
            }));
            world.set_camera(orbit.get().camera(world.camera()));
            let dragging = Rc::new(Cell::new(None::<(f32, f32)>));
            cx.render(
                gpu_view("A model", world.view())
                    .w_full()
                    .h_full()
                    .bg(Color(16, 20, 27, 255))
                    .on_event({
                        let (world, orbit) = (world.clone(), orbit.clone());
                        move |event| {
                            let mut moved = orbit.get();
                            match event.event {
                                InputEvent::PointerDown { x, y, .. } => dragging.set(Some((x, y))),
                                InputEvent::PointerMove { x, y } => {
                                    let Some((fx, fy)) = dragging.get() else {
                                        return;
                                    };
                                    dragging.set(Some((x, y)));
                                    moved.drag(x - fx, y - fy);
                                }
                                InputEvent::PointerUp { .. } | InputEvent::PointerCancel => {
                                    dragging.set(None)
                                }
                                InputEvent::Scroll { delta_y, .. } => {
                                    moved.zoom((delta_y * 0.004).exp())
                                }
                                InputEvent::Pinch { delta, .. } => moved.zoom(1. + delta),
                                _ => return,
                            }
                            if moved != orbit.get() {
                                orbit.set(moved);
                                world.set_camera(moved.camera(world.camera()));
                            }
                        }
                    }),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    window.close();
                });
            }
        })?;
    Ok(())
}
