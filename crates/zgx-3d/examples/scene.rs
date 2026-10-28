//! A 3D world in a zgx window: a few shapes in metallic-roughness materials
//! on a floor, under a sun, a sky and a warm bulb. Drag to turn the camera
//! about them, scroll or pinch to move in and out; "Spin" turns the ring (a
//! world change each refresh). Left alone, nothing is drawn.
//! Run: cargo run --release -p zgx-3d --example scene
//!
//! `--smoke-test` spins for a moment, stops, and quits.
use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, input::InputEvent, scene::Color};
use zgx_3d::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx 3D".into(),
            width: 720.,
            height: 560.,
            ..Default::default()
        })
        .run(move |cx| {
            let world = World::new();
            world.set_background(Some(0x10141b));
            world.add(
                Object::mesh(
                    Geometry::plane(8., 8.),
                    Material::rgb(0x2a313c).with(0., 0.9),
                )
                .at([0., -0.5, 0.]),
            );
            world.add(
                Object::mesh(
                    Geometry::sphere(0.5, 48),
                    Material::rgb(0xd9a441).with(1., 0.3),
                )
                .at([-1.3, 0., 0.]),
            );
            world.add(
                Object::mesh(Geometry::cuboid(0.8, 0.8, 0.8), Material::rgb(0x4a90e2))
                    .at([0., -0.1, 0.]),
            );
            world.add(
                Object::mesh(
                    Geometry::cone(0.45, 1., 40),
                    Material::rgb(0xe5533d).with(0., 0.35),
                )
                .at([1.3, 0., 0.]),
            );
            let ring = world.add(
                Object::mesh(
                    Geometry::torus(0.5, 0.12, 64),
                    Material::rgb(0x9ad1c4).with(0.2, 0.2),
                )
                .at([0., 1., -0.4]),
            );
            world.add_light(Light::Directional {
                direction: vec3(-0.5, -1., -0.4),
                color: [1., 0.97, 0.92],
                intensity: 2.5,
            });
            world.add_light(Light::Hemisphere {
                sky: [0.35, 0.45, 0.6],
                ground: [0.12, 0.1, 0.08],
                intensity: 0.6,
            });
            world.add_light(Light::Point {
                position: vec3(1.5, 1.2, 1.5),
                color: [1., 0.6, 0.3],
                intensity: 6.,
                range: 6.,
            });
            let orbit = Rc::new(Cell::new(Orbit {
                azimuth: 0.5,
                elevation: 0.35,
                distance: 5.,
                ..Orbit::default()
            }));
            world.set_camera(orbit.get().camera(world.camera()));
            let spinning = cx.ui.signal(false);
            let dragging = Rc::new(Cell::new(None::<(f32, f32)>));
            // Spinning, a task turning the ring each refresh; stopped, none.
            let spin = Rc::new(std::cell::RefCell::new(None));
            let toggle = {
                let (world, spinning, frames, tasks) = (
                    world.clone(),
                    spinning.clone(),
                    cx.frames.clone(),
                    cx.tasks.clone(),
                );
                move || {
                    let mut spin = spin.borrow_mut();
                    if spin.take().is_none() {
                        let (world, frames) = (world.clone(), frames.clone());
                        *spin = Some(tasks.spawn_scoped(async move {
                            let mut angle = 0_f32;
                            loop {
                                let frame = frames.next().await;
                                angle += frame.interval.as_secs_f32() * 1.5;
                                world.update(ring, |o| {
                                    o.transform.rotation = Quat::from_euler(angle, angle * 0.6, 0.)
                                });
                            }
                        }));
                    }
                    spinning.set(spin.is_some());
                }
            };
            let label = spinning.clone();
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(16.)
                    .gap(12.)
                    .bg(rgb(0x0b0e13))
                    .text_color(rgb(0xe0eaf5))
                    .child(
                        row()
                            .gap(12.)
                            .items_center()
                            .child(text("zgx 3D").text_size(18.))
                            .child(
                                button()
                                    .px(12.)
                                    .py(6.)
                                    .rounded(6.)
                                    .bg(rgb(0x243447))
                                    .child(text_signal(move || {
                                        if label.get() { "Stop" } else { "Spin" }.to_string()
                                    }))
                                    .on_click(toggle.clone()),
                            ),
                    )
                    .child(
                        gpu_view("Shapes on a floor", world.view())
                            .w_full()
                            .grow()
                            .rounded(14.)
                            .overflow_hidden()
                            .bg(Color(16, 20, 27, 255))
                            .on_event({
                                let (world, orbit) = (world.clone(), orbit.clone());
                                move |event| {
                                    let mut moved = orbit.get();
                                    match event.event {
                                        InputEvent::PointerDown { x, y, .. } => {
                                            dragging.set(Some((x, y)))
                                        }
                                        InputEvent::PointerMove { x, y } => {
                                            let Some((fx, fy)) = dragging.get() else {
                                                return;
                                            };
                                            dragging.set(Some((x, y)));
                                            moved.drag(x - fx, y - fy);
                                        }
                                        InputEvent::PointerUp { .. }
                                        | InputEvent::PointerCancel => dragging.set(None),
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
                    ),
            );
            if smoke {
                let (window, world, toggle) = (cx.window.clone(), world.clone(), toggle.clone());
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(400)).await;
                    let still = world.revision();
                    toggle();
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    toggle();
                    let spun = world.revision() - still;
                    zgx::timer::sleep(std::time::Duration::from_millis(400)).await;
                    println!("SCENE spun={spun} idle={}", world.revision() - still - spun);
                    window.close();
                });
            }
        })
}
