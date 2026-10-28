//! Differential validation: every incremental frame must equal an independently
//! cleared and fully rasterized frame after exactly the same retained mutations.
use zgx::scene::{Color, Effects, Layout, NodeId, NodeKind, Rect, Scene, Style, Transform};
use zgx_desktop::raster::Raster;

const WIDTH: usize = 180;
const HEIGHT: usize = 140;

struct Random(u64);
impl Random {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn n(&mut self, bound: u32) -> u32 {
        self.next() % bound
    }
    fn coordinate(&mut self, bound: u32) -> f32 {
        self.n(bound * 4) as f32 / 4.0 - 15.0
    }
    fn color(&mut self) -> Color {
        Color(
            self.next() as u8,
            self.next() as u8,
            self.next() as u8,
            (40 + self.n(216)) as u8,
        )
    }
    fn kind(&mut self, text: bool) -> NodeKind {
        if text {
            NodeKind::Text {
                text: format!("{:x}\nstream", self.next()).into(),
                color: self.color(),
                font_size: 10.0 + self.n(8) as f32,
            }
        } else {
            NodeKind::Rect(self.color())
        }
    }
    fn style(&mut self) -> Style {
        Style {
            width: Some(10.0 + self.n(60) as f32),
            height: Some(10.0 + self.n(45) as f32),
            padding: 0.0,
            gap: self.n(4) as f32,
            clip: self.n(2) == 0,
            ..Style::default()
        }
    }
}

fn compare(scene: &mut Scene, incremental: &mut Raster, full: &mut Raster, label: &str) {
    let frame = scene.flush();
    incremental.render(scene, &frame.damage);
    full.render(scene, &[Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32)]);
    if let Some((index, (actual, expected))) = incremental
        .pixels
        .iter()
        .zip(&full.pixels)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        let count = incremental
            .pixels
            .iter()
            .zip(&full.pixels)
            .filter(|(a, b)| a != b)
            .count();
        panic!(
            "{label}: {count} mismatched pixels; first ({}, {}) actual={actual:06x} expected={expected:06x}; damage={:?}",
            index % WIDTH,
            index / WIDTH,
            frame.damage
        );
    }
    assert!(scene.flush().is_idle(), "{label}: flush failed to settle");
}

fn run(seed: u64, steps: usize, filters: bool) {
    let mut random = Random(seed);
    let mut scene = Scene::new(WIDTH as f32, HEIGHT as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let mut groups = Vec::new();
    for i in 0..3 {
        let id = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(110.0),
                height: Some(90.0),
                clip: i != 2,
                ..Style::default()
            },
        );
        scene.set_transform(
            id,
            Transform {
                x: i as f32 * 20.25,
                y: i as f32 * 14.75,
            },
        );
        groups.push(id);
    }
    let mut leaves: Vec<(NodeId, bool, usize)> = Vec::new();
    for i in 0..18 {
        let kind = random.kind(i % 3 == 0);
        let style = random.style();
        let id = scene.append(groups[i % 3], kind, style);
        scene.set_transform(
            id,
            Transform {
                x: random.coordinate(100),
                y: random.coordinate(80),
            },
        );
        leaves.push((id, i % 3 == 0, i % 3));
    }
    let mut incremental = Raster::new(WIDTH, HEIGHT);
    let mut full = Raster::new(WIDTH, HEIGHT);
    compare(&mut scene, &mut incremental, &mut full, "initial");
    for step in 0..steps {
        let index = random.n(leaves.len() as u32) as usize;
        let (id, is_text, group) = leaves[index];
        let op = random.n(if filters { 10 } else { 9 });
        match op {
            0 => scene.set_transform(
                id,
                Transform {
                    x: random.coordinate(170),
                    y: random.coordinate(130),
                },
            ),
            1 if is_text => scene.set_text(id, format!("stream {}\n{}", step, random.next())),
            1 | 7 => scene.set_kind(id, random.kind(is_text)),
            2 => scene.set_effects(
                id,
                Effects {
                    backdrop_material: zgx::scene::BackdropMaterial::Blur,
                    backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
                    backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
                    backdrop_fill_blend: zgx::scene::BlendMode::Normal,
                    opacity: random.n(101) as f32 / 100.0,
                    edge_fade: random.n(14) as f32,
                    blur_radius: 0.0,
                    filter_blur: 0.,
                    brightness: 1.,
                    contrast: 1.,
                    saturate: 1.,
                },
            ),
            3 => scene.set_style(id, random.style()),
            4 => {
                scene.remove(id);
                let is_text = !is_text;
                let kind = random.kind(is_text);
                let style = random.style();
                let new = scene.append(groups[group], kind, style);
                scene.set_transform(
                    new,
                    Transform {
                        x: random.coordinate(100),
                        y: random.coordinate(80),
                    },
                );
                leaves[index] = (new, is_text, group);
            }
            5 => scene.set_transform(
                groups[group],
                Transform {
                    x: random.coordinate(90),
                    y: random.coordinate(70),
                },
            ),
            6 => {
                let mut style = random.style();
                style.width = Some(20.0 + random.n(130) as f32);
                style.height = Some(20.0 + random.n(100) as f32);
                scene.set_style(groups[group], style);
            }
            8 => scene.set_kind(
                groups[group],
                NodeKind::Container(match random.n(3) {
                    0 => Layout::Row,
                    1 => Layout::Column,
                    _ => Layout::Overlay,
                }),
            ),
            9 => scene.set_effects(
                id,
                Effects {
                    backdrop_material: zgx::scene::BackdropMaterial::Blur,
                    backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
                    backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
                    backdrop_fill_blend: zgx::scene::BlendMode::Normal,
                    opacity: 0.7,
                    blur_radius: if random.n(3) == 0 {
                        0.0
                    } else {
                        2.0 + random.n(4) as f32
                    },
                    edge_fade: 4.0,
                    filter_blur: 0.,
                    brightness: 1.,
                    contrast: 1.,
                    saturate: 1.,
                },
            ),
            _ => unreachable!(),
        }
        compare(
            &mut scene,
            &mut incremental,
            &mut full,
            &format!("seed={seed:x} step={step} op={op} leaf={index} filters={filters}"),
        );
    }
}

#[test]
fn incremental_matches_full_for_randomized_retained_mutations() {
    for seed in [0x1bad_cafe, 0xfeed_1234, 0x1234_abcd] {
        run(seed, 350, false);
    }
}

#[test]
fn incremental_matches_full_with_filter_changes_and_removal() {
    for seed in [0xbeef_cafe, 0x5678_abcd] {
        run(seed, 250, true);
    }
}

#[test]
fn removing_last_blur_restores_incremental_compositing_without_ghosts() {
    let mut scene = Scene::new(WIDTH as f32, HEIGHT as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    for index in 0..7 {
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(30 + index * 20, 180, 70, 150)),
            Style {
                width: Some(16.0),
                height: Some(100.0),
                ..Style::default()
            },
        );
        scene.set_transform(
            node,
            Transform {
                x: index as f32 * 22.25,
                y: 3.5,
            },
        );
    }
    let filter = scene.append(
        scene.root(),
        NodeKind::Rect(Color(220, 220, 255, 80)),
        Style {
            width: Some(110.0),
            height: Some(70.0),
            ..Style::default()
        },
    );
    scene.set_transform(filter, Transform { x: 10.5, y: 15.25 });
    scene.set_effects(
        filter,
        Effects {
            backdrop_material: zgx::scene::BackdropMaterial::Blur,
            backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill_blend: zgx::scene::BlendMode::Normal,
            blur_radius: 5.0,
            edge_fade: 8.0,
            opacity: 0.7,
            filter_blur: 0.,
            brightness: 1.,
            contrast: 1.,
            saturate: 1.,
        },
    );
    let mut incremental = Raster::new(WIDTH, HEIGHT);
    let mut full = Raster::new(WIDTH, HEIGHT);
    compare(&mut scene, &mut incremental, &mut full, "initial last blur");
    scene.set_effects(
        filter,
        Effects {
            backdrop_material: zgx::scene::BackdropMaterial::Blur,
            backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill_blend: zgx::scene::BlendMode::Normal,
            edge_fade: 4.0,
            opacity: 0.4,
            ..Effects::default()
        },
    );
    compare(&mut scene, &mut incremental, &mut full, "disable last blur");
    scene.set_effects(
        filter,
        Effects {
            backdrop_material: zgx::scene::BackdropMaterial::Blur,
            backdrop_tint: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill: zgx::scene::Color(0, 0, 0, 0),
            backdrop_fill_blend: zgx::scene::BlendMode::Normal,
            blur_radius: 3.0,
            ..Effects::default()
        },
    );
    compare(
        &mut scene,
        &mut incremental,
        &mut full,
        "reenable last blur",
    );
    scene.remove(filter);
    compare(
        &mut scene,
        &mut incremental,
        &mut full,
        "remove last blur node",
    );
}

#[test]
fn styled_fonts_change_software_pixels_and_damage_matches_full_render() {
    use zgx::text_layout::{FontFamily, FontStyle};
    let mut scene = Scene::new(300., 70.);
    let id = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "Typeface ffi".into(),
            color: Color(255, 255, 255, 255),
            font_size: 24.,
        },
        Style {
            width: Some(300.),
            height: Some(70.),
            ..Default::default()
        },
    );
    let mut raster = zgx_desktop::raster::Raster::new(300, 70);
    let damage = scene.flush().damage;
    raster.render(&scene, &damage);
    let regular = raster.pixels.clone();
    scene.set_font(
        id,
        FontStyle {
            family: FontFamily::Serif,
            weight: 700,
            italic: true,
            ..Default::default()
        },
    );
    let damage = scene.flush().damage;
    raster.render(&scene, &damage);
    assert_ne!(raster.pixels, regular);
    let partial = raster.pixels.clone();
    raster.render(&scene, &[Rect::new(0., 0., 300., 70.)]);
    assert_eq!(raster.pixels, partial);
    scene.set_font(id, Default::default());
    let damage = scene.flush().damage;
    raster.render(&scene, &damage);
    assert_eq!(raster.pixels, regular);
}
