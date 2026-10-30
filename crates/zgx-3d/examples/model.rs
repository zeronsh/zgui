//! A glTF model in a zgx window: a `.glb`, or a `.gltf` and the files beside
//! it, under a sun (casting shadows) in the built-in studio's light,
//! playing its first animation if it has one. Drag to turn the camera about
//! it (right-drag to pan), scroll or pinch to move in and out; the arrow
//! keys turn it too, Home puts it back (`OrbitControls`).
//! Run: cargo run --release -p zgx-3d --features gltf --example model -- FILE
//!
//! `--environment=FILE.hdr` lights it with a Radiance image instead,
//! `--no-environment` with a plain sky; `--smoke-test` shows it a moment
//! and quits.
use std::{cell::RefCell, path::Path, rc::Rc};
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
    let environment = match std::env::args().find_map(|a| {
        a.strip_prefix("--environment=")
            .map(str::to_owned)
            .or_else(|| (a == "--no-environment").then(String::new))
    }) {
        Some(file) if file.is_empty() => None,
        found => {
            let started = std::time::Instant::now();
            let environment = match found {
                Some(file) => Environment::decode_hdr(&std::fs::read(&file)?)?,
                None => Environment::studio(),
            };
            eprintln!(
                "environment made in {:.1} ms",
                started.elapsed().as_secs_f64() * 1e3
            );
            Some(environment)
        }
    };
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
            let placed = world.place_model(&model);
            let id = placed.root;
            // Its first animation, if it has one, played round and round.
            if let Some(clip) = model.clips().first().cloned() {
                eprintln!(
                    "playing {} ({:.2} s) of {} clips",
                    clip.name().unwrap_or("its first clip"),
                    clip.duration(),
                    model.clips().len()
                );
                let (world, frames) = (world.clone(), cx.frames.clone());
                cx.tasks.spawn(async move {
                    let start = std::time::Instant::now();
                    loop {
                        frames.next().await;
                        let seconds = start.elapsed().as_secs_f32();
                        placed.pose(&world, &clip, seconds % clip.duration().max(1e-3));
                    }
                });
            }
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
            match environment.clone() {
                Some(environment) => world.set_environment(Some(environment)),
                None => {
                    world.add_light(Light::Hemisphere {
                        sky: [0.35, 0.45, 0.6],
                        ground: [0.12, 0.1, 0.08],
                        intensity: 0.8,
                    });
                }
            }
            let controls = Rc::new(RefCell::new(OrbitControls::new(Orbit {
                target: vec3(0., size.y * scale * 0.5, 0.),
                azimuth: 0.5,
                elevation: 0.3,
                distance: 4.,
                ..Orbit::default()
            })));
            world.set_camera(controls.borrow().orbit.camera(world.camera()));
            // A flick coasts on, a frame at a time, while it coasts.
            let (flicked, mut heard) = zgx::task::latest::<()>();
            {
                let (world, controls, frames) =
                    (world.clone(), controls.clone(), cx.frames.clone());
                cx.tasks.spawn(async move {
                    while heard.recv().await.is_some() {
                        let mut last = frames.next().await.time;
                        loop {
                            let frame = frames.next().await;
                            let seconds = frame.time.saturating_duration_since(last).as_secs_f32();
                            last = frame.time;
                            let mut controls = controls.borrow_mut();
                            if !controls.coast(seconds) {
                                break;
                            }
                            world.set_camera(controls.orbit.camera(world.camera()));
                        }
                    }
                });
            }
            cx.render(
                gpu_view("A model", world.view())
                    .w_full()
                    .h_full()
                    .bg(Color(16, 20, 27, 255))
                    .focusable(true)
                    .on_event({
                        let world = world.clone();
                        move |event| {
                            let modifiers = match &event.event {
                                InputEvent::KeyDown { modifiers, .. } => *modifiers,
                                _ => event.pointer_modifiers(),
                            };
                            let mut controls = controls.borrow_mut();
                            let answer =
                                controls.handle(&event.event, modifiers, std::time::Instant::now());
                            if answer.moved {
                                world.set_camera(controls.orbit.camera(world.camera()));
                            }
                            if answer.taken {
                                event.prevent_default();
                            }
                            if answer.coasting {
                                flicked.send(());
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
