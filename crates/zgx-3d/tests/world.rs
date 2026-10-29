//! Worlds: their shapes face out, changes draw a frame only when they change
//! something, and frames show what the world holds (colours, lighting,
//! depth, a transparent background).
use std::f32::consts::PI;
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};
use zgx_3d::*;
use zgx_gpu::GpuRenderer;

/// Each triangle of a closed shape about the origin faces away from it.
fn faces_out(geometry: &Geometry) {
    let v = geometry.vertices();
    for t in geometry.indices().chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(v[i as usize].position));
        let area = (b - a).cross(c - a);
        if area.length() < 1e-7 {
            continue; // a pole's degenerate triangle
        }
        let centre = (a + b + c) * (1. / 3.);
        let normal = Vec3::from(v[t[0] as usize].normal);
        assert!(
            area.dot(normal) > 0.,
            "a triangle wound against its normal at {centre:?}"
        );
    }
    for vertex in v {
        let n = Vec3::from(vertex.normal);
        assert!(
            (n.length() - 1.).abs() < 1e-4,
            "a normal of length {}",
            n.length()
        );
    }
}

#[test]
fn shapes_are_wound_toward_their_normals() {
    for geometry in [
        Geometry::cuboid(1., 2., 3.),
        Geometry::sphere(1., 24),
        Geometry::plane(2., 2.),
        Geometry::cylinder(0.5, 1., 16),
        Geometry::cone(0.5, 1., 16),
        Geometry::torus(1., 0.25, 24),
    ] {
        faces_out(&geometry);
    }
    let (low, high) = Geometry::cuboid(1., 2., 3.).bounds();
    assert_eq!((low, high), (vec3(-0.5, -1., -1.5), vec3(0.5, 1., 1.5)));
    assert!(Geometry::new(vec![Vertex::default()], vec![0, 0]).is_err());
    assert!(Geometry::new(vec![Vertex::default()], vec![0, 0, 1]).is_err());
}

#[test]
fn a_change_counts_only_when_it_changes_something() {
    let world = World::new();
    let cube = world.add(Object::mesh(
        Geometry::cuboid(1., 1., 1.),
        Material::default(),
    ));
    let revision = world.revision();
    world.set_transform(cube, Transform::default());
    world.set_camera(world.camera());
    world.set_background(None);
    assert_eq!(world.revision(), revision, "nothing changed");
    world.set_transform(cube, Transform::at([1., 0., 0.]));
    assert_eq!(world.revision(), revision + 1);
    world.remove(cube);
    assert!(world.object(cube).is_none());
    let revision = world.revision();
    world.set_transform(cube, Transform::at([2., 0., 0.]));
    assert_eq!(
        world.revision(),
        revision,
        "a removed object changes nothing"
    );
    // Its slot reused, the old id stays gone.
    let other = world.add(Object::group());
    assert_ne!(other, cube);
    assert!(world.object(cube).is_none());
}

#[test]
fn children_are_placed_in_their_parents_frame() {
    let world = World::new();
    let parent = world.add(Object::group().at([1., 0., 0.]));
    let child = world.add(Object::group().at([0., 2., 0.]).child_of(parent));
    let m = world.world_matrix(child).unwrap();
    assert_eq!(&m.columns[3][..3], &[1., 2., 0.]);
    world.update(parent, |o| {
        o.transform.rotation = Quat::from_axis_angle(vec3(0., 0., 1.), PI / 2.)
    });
    let m = world.world_matrix(child).unwrap();
    assert!((m.columns[3][0] - -1.).abs() < 1e-5 && (m.columns[3][1] - 0.).abs() < 1e-5);
    world.update(parent, |o| o.visible = false);
    assert!(
        world.world_matrix(child).is_none(),
        "hidden with its parent"
    );
    // A cycle draws nothing, and does not hang.
    world.update(parent, |o| {
        o.visible = true;
        o.parent = Some(child);
    });
    assert!(world.world_matrix(child).is_none());
    // Non-finite placements are the identity's.
    world.set_transform(parent, Transform::at([f32::NAN, 0., 0.]));
    assert_eq!(world.object(parent).unwrap().transform.position, Vec3::ZERO);
}

struct Window {
    ui: Ui,
    gpu: GpuRenderer,
    other: zgx::reactive::Signal<bool>,
}
const SIZE: f32 = 100.;
impl Window {
    fn new(world: &World) -> Option<Self> {
        let gpu = GpuRenderer::new(SIZE as u32, SIZE as u32).ok()?;
        let mut ui = Ui::new(SIZE, SIZE);
        let other = ui.signal(false);
        let lit = other.clone();
        ui.mount(
            div()
                .size(SIZE, SIZE)
                .bg(Color(0, 0, 0, 255))
                .child(
                    gpu_view("World", world.view())
                        .absolute()
                        .size(SIZE, SIZE)
                        .bg(Color(0, 128, 0, 255)),
                )
                .child(div().absolute().size(4., 4.).reactive_style(move || {
                    Styles::new().bg(if lit.get() {
                        Color(255, 255, 255, 255)
                    } else {
                        Color(0, 0, 0, 255)
                    })
                })),
        );
        Some(Self { ui, gpu, other })
    }
    fn draw(&mut self) -> zgx_gpu::GpuStats {
        self.ui.prepare_frame();
        let report = self.ui.scene.borrow_mut().flush();
        self.gpu
            .render(&self.ui.scene.borrow(), &report.damage)
            .unwrap()
    }
    fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let pixels = self.gpu.readback().unwrap();
        let at = (y * SIZE as usize + x) * 4;
        pixels[at..at + 4].try_into().unwrap()
    }
}

/// A world of one mesh in front of the camera, 3 away.
fn one(geometry: Geometry, material: Material) -> (World, ObjectId) {
    let world = World::new();
    let id = world.add(Object::mesh(geometry, material));
    world.set_camera(Camera::looking_at([0., 0., 3.], [0., 0., 0.]));
    (world, id)
}

#[test]
fn an_unlit_mesh_shows_its_colour() {
    let mut material = Material::rgb(0x336699);
    material.unlit = true;
    let (world, _) = one(Geometry::cuboid(1., 1., 1.), material);
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    let [r, g, b, a] = window.pixel(50, 50);
    assert!(
        r.abs_diff(0x33) <= 2 && g.abs_diff(0x66) <= 2 && b.abs_diff(0x99) <= 2 && a == 255,
        "{:?}",
        [r, g, b, a]
    );
    // Outside it, transparent: the box's own background.
    assert_eq!(window.pixel(10, 90), [0, 128, 0, 255]);
}

#[test]
fn a_sphere_is_lit_where_it_faces_the_light() {
    let (world, _) = one(Geometry::sphere(1., 48), Material::rgb(0xcc3322));
    world.add_light(Light::Directional {
        direction: vec3(0., 0., -1.),
        color: [1., 1., 1.],
        intensity: 3.,
    });
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    let centre = window.pixel(50, 50);
    let rim = window.pixel(50, 74);
    assert!(
        centre[0] > centre[1] && centre[0] > centre[2],
        "red: {centre:?}"
    );
    assert!(
        centre[0] > rim[0] + 20,
        "brighter facing the light: {centre:?} against {rim:?}"
    );
}

#[test]
fn nearer_meshes_hide_farther_ones() {
    let world = World::new();
    let mut red = Material::rgb(0xff0000);
    red.unlit = true;
    let mut blue = Material::rgb(0x0000ff);
    blue.unlit = true;
    let near = world.add(Object::mesh(Geometry::cuboid(1., 1., 1.), red).at([0., 0., 0.5]));
    world.add(Object::mesh(Geometry::cuboid(2., 2., 1.), blue).at([0., 0., -1.]));
    world.set_camera(Camera::looking_at([0., 0., 4.], [0., 0., 0.]));
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    assert_eq!(window.pixel(50, 50), [255, 0, 0, 255]);
    world.set_transform(near, Transform::at([0., 0., -3.]));
    window.draw();
    assert_eq!(window.pixel(50, 50), [0, 0, 255, 255]);
}

#[test]
fn a_still_world_draws_no_frame() {
    let (world, cube) = one(Geometry::cuboid(1., 1., 1.), Material::default());
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    assert_eq!(window.draw().image_uploads, 1, "its first frame");
    for lit in [true, false] {
        window.other.set(lit);
        assert_eq!(window.draw().image_uploads, 0, "a change elsewhere");
    }
    world.set_transform(cube, Transform::default());
    assert_eq!(window.draw().image_uploads, 0, "a change to what it was");
    world.update(cube, |o| {
        o.transform.rotation = Quat::from_euler(0.3, 0.2, 0.)
    });
    world.set_material(cube, Material::rgb(0x00ff00));
    assert_eq!(window.draw().image_uploads, 1, "two changes, one frame");
}

#[test]
fn translucent_meshes_are_drawn_back_to_front_whatever_their_order() {
    let mut red = Material::rgb(0xff0000);
    red.unlit = true;
    red.color[3] = 0.6;
    let mut blue = Material::rgb(0x0000ff);
    blue.unlit = true;
    blue.color[3] = 0.6;
    let near = || Object::mesh(Geometry::cuboid(1., 1., 0.1), red).at([0., 0., 0.5]);
    let far = || Object::mesh(Geometry::cuboid(1., 1., 0.1), blue).at([0., 0., -0.5]);
    for near_first in [true, false] {
        let world = World::new();
        world.set_camera(Camera::looking_at([0., 0., 4.], [0., 0., 0.]));
        if near_first {
            world.add(near());
            world.add(far());
        } else {
            world.add(far());
            world.add(near());
        }
        let Some(mut window) = Window::new(&world) else {
            return;
        };
        window.draw();
        let [r, _, b, _] = window.pixel(50, 50);
        assert!(
            r > b,
            "the nearer over the farther ({near_first}): {r} against {b}"
        );
    }
}

/// The pixel of the view (`SIZE` square) where the world shows point `p`.
fn seen_at(world: &World, p: Vec3) -> (usize, usize) {
    let camera = world.camera();
    let ndc = (camera.projection(1.) * camera.view()).project(p);
    let x = (ndc.x * 0.5 + 0.5) * SIZE;
    let y = (0.5 - ndc.y * 0.5) * SIZE;
    (x as usize, y as usize)
}

#[test]
fn the_sun_casts_shadows_only_when_asked() {
    // A cube on a floor, the sun going down and to the right: its shadow
    // falls on the floor to its right.
    let world = World::new();
    world.add(Object::mesh(
        Geometry::plane(8., 8.),
        Material::rgb(0xcccccc),
    ));
    let cube = world.add(
        Object::mesh(Geometry::cuboid(1., 1., 1.), Material::rgb(0xcccccc)).at([-1., 0.5, 0.]),
    );
    world.add_light(Light::Directional {
        direction: vec3(1., -1., 0.),
        color: [1., 1., 1.],
        intensity: 3.,
    });
    world.set_camera(Camera::looking_at([0., 7., 2.], [0., 0., 0.]));
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    // On the floor: in the cube's shadow, and well away from it.
    let shaded = seen_at(&world, vec3(0.1, 0., 0.));
    let lit = seen_at(&world, vec3(2.5, 0., 0.));
    let level = |window: &Window, (x, y): (usize, usize)| window.pixel(x, y)[0];
    window.draw();
    let (off_shaded, off_lit) = (level(&window, shaded), level(&window, lit));
    assert!(
        off_shaded.abs_diff(off_lit) <= 6,
        "without shadows the floor is even: {off_shaded} against {off_lit}"
    );
    world.set_shadows(true);
    window.draw();
    let (on_shaded, on_lit) = (level(&window, shaded), level(&window, lit));
    assert!(
        (on_shaded as f32) < on_lit as f32 * 0.6,
        "the cube's shadow: {on_shaded} against {on_lit}"
    );
    assert!(
        on_lit.abs_diff(off_lit) <= 3,
        "lit floor unchanged (no self-shadowing): {on_lit} against {off_lit}"
    );
    // The camera moved alone: the map drawn before still holds, and shows.
    world.set_camera(Camera::looking_at([0.3, 7., 2.2], [0., 0., 0.]));
    window.draw();
    let (moved_shaded, moved_lit) = (
        level(&window, seen_at(&world, vec3(0.1, 0., 0.))),
        level(&window, seen_at(&world, vec3(2.5, 0., 0.))),
    );
    assert!(
        (moved_shaded as f32) < moved_lit as f32 * 0.6,
        "the shadow, the camera moved: {moved_shaded} against {moved_lit}"
    );
    // The cube moved away: the map is drawn again, and that floor is lit.
    world.set_camera(Camera::looking_at([0., 7., 2.], [0., 0., 0.]));
    world.set_transform(cube, Transform::at([-3., 0.5, 0.]));
    window.draw();
    assert!(
        level(&window, shaded).abs_diff(on_lit) <= 3,
        "its old shadow lit: {} against {on_lit}",
        level(&window, shaded)
    );
    // Off again, as before.
    world.set_shadows(false);
    window.draw();
    assert!(level(&window, shaded).abs_diff(off_shaded) <= 2);
}

#[test]
fn a_texture_colours_a_mesh_across_its_coordinates() {
    // A square facing the camera, its texture red on the left, blue on the
    // right.
    let vertex = |x: f32, y: f32, u: f32, v: f32| Vertex {
        position: [x, y, 0.],
        normal: [0., 0., 1.],
        uv: [u, v],
    };
    let square = Geometry::new(
        vec![
            vertex(-1., -1., 0., 1.),
            vertex(1., -1., 1., 1.),
            vertex(1., 1., 1., 0.),
            vertex(-1., 1., 0., 0.),
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
    .unwrap();
    let mut white = Material::rgb(0xffffff);
    white.unlit = true;
    let (world, id) = one(square, white);
    let texture = Texture::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    world.set_texture(id, Some(texture.clone()));
    assert_eq!(world.texture(id), Some(texture.clone()));
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    // The texels' centres (u 0.25 and 0.75): x ±0.5, 30 and 70 across.
    let (left, right) = (window.pixel(30, 50), window.pixel(70, 50));
    assert!(left[0] > 200 && left[2] < 40, "red on the left: {left:?}");
    assert!(
        right[2] > 200 && right[0] < 40,
        "blue on the right: {right:?}"
    );
    // The same texture again changes nothing; none, the colour alone.
    let revision = world.revision();
    world.set_texture(id, Some(texture));
    assert_eq!(world.revision(), revision);
    world.set_texture(id, None);
    window.draw();
    let plain = window.pixel(30, 50);
    assert!(
        plain[0] > 240 && plain[1] > 240 && plain[2] > 240,
        "{plain:?}"
    );
}

#[test]
fn textures_are_checked_and_mipmapped_down_to_one_texel() {
    assert!(Texture::from_rgba8(0, 1, vec![]).is_err());
    assert!(Texture::from_rgba8(2, 2, vec![0; 15]).is_err());
    assert!(Texture::from_rgba8(9000, 1, vec![0; 36000]).is_err());
    let t = Texture::from_rgba8(3, 2, vec![10; 24]).unwrap();
    assert_eq!((t.width(), t.height()), (3, 2));
}
