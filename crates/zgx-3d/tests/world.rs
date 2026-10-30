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

#[test]
fn a_normal_map_tilts_a_surface_as_gltf_says() {
    // A square facing the camera, u across to the right, v down; lit from
    // the right (then from above), its normal map tilting it towards the
    // light or away: +x is towards u, +y up the texture (glTF's).
    let vertex = |x: f32, y: f32, u: f32, v: f32| Vertex {
        position: [x, y, 0.],
        normal: [0., 0., 1.],
        uv: [u, v],
    };
    let square = || {
        Geometry::new(
            vec![
                vertex(-1., -1., 0., 1.),
                vertex(1., -1., 1., 1.),
                vertex(1., 1., 1., 0.),
                vertex(-1., 1., 0., 0.),
            ],
            vec![0, 1, 2, 0, 2, 3],
        )
        .unwrap()
    };
    let level = |light: Vec3, texel: [u8; 4]| -> Option<u8> {
        let (world, id) = one(square(), Material::rgb(0xffffff).with(0., 1.));
        world.add_light(Light::Directional {
            direction: light,
            color: [1., 1., 1.],
            intensity: 2.,
        });
        world.set_map(
            id,
            Map::Normal,
            Some(Texture::from_rgba8(1, 1, texel.to_vec()).unwrap()),
        );
        let mut window = Window::new(&world)?;
        window.draw();
        Some(window.pixel(50, 50)[0])
    };
    // Light from the right, travelling left and in.
    let from_right = vec3(-1., 0., -1.);
    let (Some(towards), Some(flat), Some(away)) = (
        level(from_right, [230, 128, 180, 255]),
        level(from_right, [128, 128, 255, 255]),
        level(from_right, [26, 128, 180, 255]),
    ) else {
        return;
    };
    assert!(towards > flat && flat > away, "x: {towards} {flat} {away}");
    let from_above = vec3(0., -1., -1.);
    let (towards, away) = (
        level(from_above, [128, 230, 180, 255]).unwrap(),
        level(from_above, [128, 26, 180, 255]).unwrap(),
    );
    assert!(towards > away + 20, "y: {towards} {away}");
}

#[test]
fn a_metal_roughness_map_scales_the_materials_factors() {
    // A white sphere, fully metal and rough by its factors; its map's blue
    // (metal) 0 makes it a dielectric, lit diffusely where a rough metal
    // with no surroundings is dark.
    let shade = |texel: [u8; 4]| -> Option<u8> {
        let (world, id) = one(
            Geometry::sphere(1., 48),
            Material::rgb(0xffffff).with(1., 1.),
        );
        world.add_light(Light::Directional {
            direction: vec3(0., 0., -1.),
            color: [1., 1., 1.],
            intensity: 3.,
        });
        world.set_map(
            id,
            Map::MetalRoughness,
            Some(Texture::from_rgba8(1, 1, texel.to_vec()).unwrap()),
        );
        let mut window = Window::new(&world)?;
        window.draw();
        Some(window.pixel(50, 50)[0])
    };
    let (Some(metal), Some(dielectric)) = (shade([0, 255, 255, 255]), shade([0, 255, 0, 255]))
    else {
        return;
    };
    assert!(
        dielectric > metal + 30,
        "dielectric {dielectric}, metal {metal}"
    );
}

#[test]
fn a_decoded_texture_uploads_again_on_another_device() {
    // A renderer on a device of its own (as after a lost one) uploads the
    // texture again, its pixels decoded again from its file: the first
    // renderer's upload dropped them.
    const RED_BLUE: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 244, 34, 127, 138, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0,
        66, 255, 1, 15, 249, 3, 253, 133, 17, 153, 118, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
        130,
    ];
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
    world.set_texture(id, Some(Texture::decode(RED_BLUE).unwrap()));
    for _ in 0..2 {
        let Some(mut window) = Window::new(&world) else {
            return;
        };
        window.draw();
        let (left, right) = (window.pixel(30, 50), window.pixel(70, 50));
        assert!(left[0] > 200 && left[2] < 40, "red on the left: {left:?}");
        assert!(
            right[2] > 200 && right[0] < 40,
            "blue on the right: {right:?}"
        );
    }
}

/// A square facing the camera, u across to the right, v down.
fn facing_square() -> Geometry {
    let vertex = |x: f32, y: f32, u: f32, v: f32| Vertex {
        position: [x, y, 0.],
        normal: [0., 0., 1.],
        uv: [u, v],
    };
    Geometry::new(
        vec![
            vertex(-1., -1., 0., 1.),
            vertex(1., -1., 1., 1.),
            vertex(1., 1., 1., 0.),
            vertex(-1., 1., 0., 0.),
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
    .unwrap()
}

#[test]
fn an_emission_map_colours_the_light_a_mesh_gives_off() {
    // A black square, unlit by anything, glowing as its emission map says.
    let mut black = Material::rgb(0x000000);
    black.emissive = [1., 1., 1.];
    let (world, id) = one(facing_square(), black);
    let map = Texture::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    world.set_map(id, Map::Emission, Some(map));
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    let (left, right) = (window.pixel(30, 50), window.pixel(70, 50));
    // (A full glow is desaturated a little by the tone mapping.)
    assert!(left[0] > left[2] + 150, "red glow on the left: {left:?}");
    assert!(
        right[2] > right[0] + 150,
        "blue glow on the right: {right:?}"
    );
}

#[test]
fn an_occlusion_map_dims_the_ambient_light_alone() {
    // Lit by the hemisphere alone: where the map says no light reaches, dark.
    let level = |occlusion: u8| -> Option<u8> {
        let (world, id) = one(facing_square(), Material::rgb(0xffffff).with(0., 1.));
        world.add_light(Light::Hemisphere {
            sky: [1., 1., 1.],
            ground: [1., 1., 1.],
            intensity: 1.,
        });
        let map = Texture::from_rgba8(1, 1, vec![occlusion, 0, 0, 255]).unwrap();
        world.set_map(id, Map::Occlusion, Some(map));
        let mut window = Window::new(&world)?;
        window.draw();
        Some(window.pixel(50, 50)[0])
    };
    let (Some(open), Some(occluded)) = (level(255), level(0)) else {
        return;
    };
    assert!(
        open > 100 && occluded < 10,
        "open {open}, occluded {occluded}"
    );
}

#[test]
fn a_skinned_mesh_follows_its_joint_drawn_and_picked() {
    // A square bound wholly to one joint (a group); the joint moves right
    // and the square with it, where it's drawn and where it's picked.
    let square = facing_square()
        .with_skin(vec![[0; 4]; 4], vec![[1., 0., 0., 0.]; 4])
        .unwrap();
    let mut white = Material::rgb(0xffffff);
    white.unlit = true;
    let (world, id) = one(square, white);
    let joint = world.add(Object::group());
    world.set_skin(
        id,
        Some(Skin::new(vec![joint], vec![Mat4::IDENTITY]).unwrap()),
    );
    // The square (x -1 to 1) then at 0.6 to 2.6, clear of the middle.
    world.set_transform(joint, Transform::at([1.6, 0., 0.]));
    let middle = (50., 50.);
    let moved = seen_at(&world, vec3(1., 0., 0.));
    let hit = world.pick((moved.0 as f32, moved.1 as f32), (SIZE, SIZE));
    assert_eq!(hit.map(|h| h.object), Some(id), "picked where it's posed");
    assert_eq!(
        world.pick(middle, (SIZE, SIZE)),
        None,
        "not where it was bound"
    );
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    assert_eq!(
        window.pixel(50, 50),
        [0, 128, 0, 255],
        "gone from the middle"
    );
    let shown = window.pixel(moved.0, moved.1);
    assert!(
        shown[0] > 240 && shown[2] > 240,
        "drawn where it's posed: {shown:?}"
    );
    // Its skin taken away, it is drawn as bound.
    world.set_skin(id, None);
    window.draw();
    assert!(window.pixel(50, 50)[0] > 240);
}

/// An environment, 64 by 32 texels equirectangular, of `light(direction)`.
fn environment_of(light: impl Fn(Vec3) -> f32) -> Environment {
    let (w, h) = (64, 32);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            // The direction through the texel's centre: its centre column
            // faces −z, its top row up.
            let lon = ((x as f32 + 0.5) / w as f32 - 0.5) * 2. * PI;
            let lat = (y as f32 + 0.5) / h as f32 * PI;
            let d = vec3(lat.sin() * lon.sin(), lat.cos(), -lat.sin() * lon.cos());
            rgb.extend([light(d); 3]);
        }
    }
    Environment::from_equirect(w as u32, h as u32, &rgb).unwrap()
}

#[test]
fn an_environment_lights_a_surface_by_how_it_faces_it() {
    // Light from +x alone: a rough white sphere is lit on its right, dark
    // on its left; turned half round, the other way.
    let bright_right = environment_of(|d| if d.x > 0. { 1. } else { 0. });
    let sides = |environment: Option<Environment>| -> Option<(u8, u8)> {
        let (world, _) = one(
            Geometry::sphere(1., 48),
            Material::rgb(0xffffff).with(0., 1.),
        );
        world.set_environment(environment);
        let mut window = Window::new(&world)?;
        window.draw();
        Some((window.pixel(25, 50)[0], window.pixel(75, 50)[0]))
    };
    let Some((left, right)) = sides(Some(bright_right.clone())) else {
        return;
    };
    assert!(right > left + 60, "left {left}, right {right}");
    let (left, right) = sides(Some(bright_right.rotated(PI))).unwrap();
    assert!(left > right + 60, "turned: left {left}, right {right}");
    // None: no light at all.
    assert_eq!(sides(None).unwrap(), (0, 0));
}

#[test]
fn a_mirror_reflects_its_environment_the_right_way_up_and_round() {
    // Behind the camera, bright above and brightest above to the right: a
    // polished metal square facing the camera shows its top right brightest,
    // its bottom dark.
    let environment = environment_of(|d| match (d.y > 0., d.x > 0.) {
        (true, true) => 4.,
        (true, false) => 0.3,
        _ => 0.,
    });
    let (world, _) = one(facing_square(), Material::rgb(0xffffff).with(1., 0.));
    world.set_environment(Some(environment));
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    let (top_left, top_right) = (window.pixel(38, 38)[0], window.pixel(62, 38)[0]);
    let bottom = window.pixel(62, 62)[0];
    assert!(
        top_right > top_left + 30 && top_left > bottom + 30,
        "top left {top_left}, top right {top_right}, bottom {bottom}"
    );
}

#[test]
fn the_studio_lights_a_model_with_no_lights_of_its_own() {
    let (world, _) = one(
        Geometry::sphere(1., 48),
        Material::rgb(0x808080).with(0., 0.5),
    );
    world.set_environment(Some(Environment::studio()));
    let revision = world.revision();
    world.set_environment(Some(Environment::studio()));
    assert_eq!(world.revision(), revision, "the same light: no change");
    world.set_environment(Some(Environment::studio().intensity(2.)));
    assert_eq!(world.revision(), revision + 1);
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    let [r, g, b, _] = window.pixel(50, 50);
    assert!(
        r > 60 && r.abs_diff(g) <= 6 && r.abs_diff(b) <= 8,
        "{:?}",
        [r, g, b]
    );
}

/// The red channel shown for a black square glowing `glow` (grey, linear),
/// tone mapped by `tone`, exposed `stops`.
fn shown(glow: f32, tone: ToneMapping, stops: f32) -> Option<u8> {
    let mut black = Material::rgb(0x000000);
    black.emissive = [glow; 3];
    let (world, _) = one(facing_square(), black);
    world.set_tone_mapping(tone);
    world.set_exposure(stops);
    let mut window = Window::new(&world)?;
    window.draw();
    Some(window.pixel(50, 50)[0])
}

#[test]
fn tone_mappings_bring_bright_light_into_range_each_its_own_way() {
    let Some(_) = shown(0.18, ToneMapping::Linear, 0.) else {
        return;
    };
    let curve = |tone| [0.05, 0.18, 1., 4.].map(|glow| shown(glow, tone, 0.).unwrap());
    let (linear, neutral, aces) = (
        curve(ToneMapping::Linear),
        curve(ToneMapping::Neutral),
        curve(ToneMapping::Aces),
    );
    for c in [linear, neutral, aces] {
        assert!(c.windows(2).all(|w| w[0] < w[1] || w[1] == 255), "{c:?}");
    }
    // Linear clips at 1; the others roll off below white.
    assert_eq!((linear[2], linear[3]), (255, 255), "{linear:?}");
    assert!(neutral[2] < 255 && aces[2] < 255, "{neutral:?} {aces:?}");
    // sRGB output: linear 0.18 is 118 of 255.
    assert!(linear[1].abs_diff(118) <= 2, "{linear:?}");
    // ACES lifts mid grey above neutral's (its 1/0.6 exposure).
    assert!(aces[1] > neutral[1], "{aces:?} {neutral:?}");
}

#[test]
fn exposure_doubles_the_light_a_stop() {
    let (Some(up), Some(double)) = (
        shown(0.1, ToneMapping::Linear, 1.),
        shown(0.2, ToneMapping::Linear, 0.),
    ) else {
        return;
    };
    assert!(up.abs_diff(double) <= 1, "{up} against {double}");
    let down = shown(0.2, ToneMapping::Linear, -1.).unwrap();
    let half = shown(0.1, ToneMapping::Linear, 0.).unwrap();
    assert!(down.abs_diff(half) <= 1, "{down} against {half}");
}

#[test]
fn four_samples_smooth_a_slanted_edge_and_one_draws_it_hard() {
    // A white unlit square turned a little: along a row across its edge,
    // pixels part white and part the box's green show only when sampled
    // more than once a pixel.
    let blended = |samples: u32| -> Option<usize> {
        let mut white = Material::rgb(0xffffff);
        white.unlit = true;
        let (world, id) = one(facing_square(), white);
        world.update(id, |o| o.transform.rotation = Quat::from_euler(0., 0., 0.3));
        world.set_samples(samples);
        let mut window = Window::new(&world)?;
        window.draw();
        let pixels = window.gpu.readback().unwrap();
        Some(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 20 && p[0] < 235)
                .count(),
        )
    };
    let (Some(smooth), Some(hard)) = (blended(4), blended(1)) else {
        return;
    };
    assert!(smooth > 40 && hard == 0, "smooth {smooth}, hard {hard}");
}

/// A square facing the camera with one morph target moving it `by` along x.
fn morphing_square(by: f32) -> Geometry {
    facing_square()
        .with_morph_targets(vec![MorphTarget::new(vec![[by, 0., 0.]; 4])])
        .unwrap()
}

#[test]
fn a_morph_target_moves_a_mesh_by_its_weight_drawn_and_picked() {
    let mut white = Material::rgb(0xffffff);
    white.unlit = true;
    let (world, id) = one(morphing_square(1.6), white);
    let middle = (50., 50.);
    assert!(
        world.pick(middle, (SIZE, SIZE)).is_some(),
        "at rest, as made"
    );
    // At full weight it's at 0.6 to 2.6, clear of the middle.
    world.set_morph_weights(id, &[1.]);
    let moved = seen_at(&world, vec3(1., 0., 0.));
    let hit = world.pick((moved.0 as f32, moved.1 as f32), (SIZE, SIZE));
    assert_eq!(hit.map(|h| h.object), Some(id), "picked where it's blended");
    assert_eq!(world.pick(middle, (SIZE, SIZE)), None, "not where it was");
    let revision = world.revision();
    world.set_morph_weights(id, &[1.]);
    assert_eq!(world.revision(), revision, "the same weights: no frame");
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    assert_eq!(
        window.pixel(50, 50),
        [0, 128, 0, 255],
        "gone from the middle"
    );
    let shown = window.pixel(moved.0, moved.1);
    assert!(shown[0] > 240, "drawn where it's blended: {shown:?}");
    // Half way: -0.2 to 1.8, so the middle again and not the far left.
    world.set_morph_weight(id, 0, 0.5);
    window.draw();
    assert!(window.pixel(50, 50)[0] > 240, "half way, over the middle");
    let left = seen_at(&world, vec3(-0.7, 0., 0.));
    assert_eq!(window.pixel(left.0, left.1), [0, 128, 0, 255]);
    // Back to rest: as made, through the plain pipeline.
    world.set_morph_weights(id, &[0.]);
    window.draw();
    assert!(window.pixel(left.0, left.1)[0] > 240, "at rest again");
}

#[test]
fn a_morph_target_turns_normals_as_well() {
    // A square lit from its right: turning its normals that way lights it.
    let turned = facing_square()
        .with_morph_targets(vec![MorphTarget::new(vec![[0.; 3]; 4]).with_normals(vec![
            [
                3., 0., 0.
            ];
            4
        ])])
        .unwrap();
    let shown = |weight: f32| -> Option<u8> {
        let (world, id) = one(turned.clone(), Material::rgb(0xffffff).with(0., 1.));
        world.add_light(Light::Directional {
            direction: vec3(-1., 0., 0.),
            color: [1., 1., 1.],
            intensity: 3.,
        });
        world.set_morph_weights(id, &[weight]);
        let mut window = Window::new(&world)?;
        window.draw();
        Some(window.pixel(50, 50)[0])
    };
    let (Some(rest), Some(lit)) = (shown(0.), shown(1.)) else {
        return;
    };
    assert!(lit > rest + 80, "at rest {rest}, turned {lit}");
}

#[test]
fn a_morph_and_a_skin_move_a_mesh_together() {
    // Blended 1.6 along x, and moved up 1 by its joint: both show, joints
    // read past the weights in the frame's buffer.
    let square = morphing_square(1.6)
        .with_skin(vec![[0; 4]; 4], vec![[1., 0., 0., 0.]; 4])
        .unwrap();
    let mut white = Material::rgb(0xffffff);
    white.unlit = true;
    let (world, id) = one(square, white);
    let joint = world.add(Object::group());
    world.set_skin(
        id,
        Some(Skin::new(vec![joint], vec![Mat4::IDENTITY]).unwrap()),
    );
    world.set_transform(joint, Transform::at([0., 1., 0.]));
    world.set_morph_weights(id, &[1.]);
    let there = seen_at(&world, vec3(1., 1., 0.));
    let hit = world.pick((there.0 as f32, there.1 as f32), (SIZE, SIZE));
    assert_eq!(hit.map(|h| h.object), Some(id), "picked in its pose");
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    assert!(window.pixel(there.0, there.1)[0] > 240, "drawn in its pose");
    let unmoved = seen_at(&world, vec3(1., -0.5, 0.));
    assert_eq!(window.pixel(unmoved.0, unmoved.1), [0, 128, 0, 255]);
}

#[test]
fn a_morph_target_has_an_entry_a_vertex() {
    let too_few = MorphTarget::new(vec![[0.; 3]; 3]);
    assert!(facing_square().with_morph_targets(vec![too_few]).is_err());
    let bad_normals = MorphTarget::new(vec![[0.; 3]; 4]).with_normals(vec![[0.; 3]; 2]);
    assert!(
        facing_square()
            .with_morph_targets(vec![bad_normals])
            .is_err()
    );
}

#[test]
fn a_morphing_mesh_casts_its_shadow_where_it_is_blended() {
    // A small flat square 1 over a floor, under a sun slanting toward -x:
    // its shadow falls 1 to the -x side of it. Blended 1 along +x, the
    // shadow is under the origin, and not at -1, where it was at rest.
    let vertex = |x: f32, z: f32| Vertex {
        position: [x, 0., z],
        normal: [0., 1., 0.],
        uv: [0., 0.],
    };
    let flat = Geometry::new(
        vec![
            vertex(-0.3, 0.3),
            vertex(0.3, 0.3),
            vertex(0.3, -0.3),
            vertex(-0.3, -0.3),
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
    .unwrap()
    .with_morph_targets(vec![MorphTarget::new(vec![[1., 0., 0.]; 4])])
    .unwrap();
    let world = World::new();
    world.set_shadows(true);
    world.add_light(Light::Directional {
        direction: vec3(-1., -1., 0.),
        color: [1., 1., 1.],
        intensity: 3.,
    });
    let caster = world.add(Object::mesh(flat, Material::rgb(0xffffff)));
    world.set_transform(caster, Transform::at([0., 1., 0.]));
    world.add(Object::mesh(
        Geometry::plane(6., 6.),
        Material::rgb(0xffffff).with(0., 1.),
    ));
    world.set_camera(Camera::looking_at([0., 6., 0.01], [0., 0., 0.]));
    world.set_morph_weights(caster, &[1.]);
    let Some(mut window) = Window::new(&world) else {
        return;
    };
    window.draw();
    let shaded = seen_at(&world, vec3(0., 0., 0.));
    let open = seen_at(&world, vec3(-1., 0., 0.));
    let (shaded, open) = (
        window.pixel(shaded.0, shaded.1)[0],
        window.pixel(open.0, open.1)[0],
    );
    assert!(open > shaded + 40, "shadow {shaded}, open floor {open}");
}
