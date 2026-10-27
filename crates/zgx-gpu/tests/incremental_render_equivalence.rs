//! Damage-limited rendering (subtree culling, batched draws, deferred
//! submission) must produce exactly the pixels of a fresh full render.
use zgx::scene::{
    BoxShadow, Color, Effects, Layout, LocalTransform, NodeId, NodeKind, QuadStyle, Scene, Style,
    Transform,
};
use zgx_gpu::GpuRenderer;

const W: f32 = 320.;
const H: f32 = 240.;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn signed(&mut self, range: f32) -> f32 {
        (self.below(2001) as f32 / 1000. - 1.) * range
    }
}

fn quad(fill: Color, shadow: bool) -> NodeKind {
    NodeKind::Quad(QuadStyle {
        fill,
        radius: 6.,
        border_color: Color(255, 255, 255, 120),
        border_width: 1.,
        shadow: shadow.then_some(BoxShadow {
            color: Color(0, 0, 0, 160),
            offset: Transform { x: 3., y: 4. },
            blur_radius: 5.,
            spread: 0.,
        }),
        decoration: None,
        border_over: false,
        outline: None,
    })
}

struct Tree {
    /// Groups drawn as layers (`layered`): held still, inside the window.
    layers: bool,
    /// Every node, in creation order.
    all: Vec<NodeId>,
    /// Moving, recolouring and fading targets.
    movers: Vec<NodeId>,
    groups: Vec<NodeId>,
    labels: Vec<NodeId>,
}

fn build() -> (Scene, Tree) {
    let mut scene = Scene::new(W, H);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let mut tree = Tree {
        layers: false,
        all: Vec::new(),
        movers: Vec::new(),
        groups: Vec::new(),
        labels: Vec::new(),
    };
    for g in 0..4 {
        // Groups: some clip, some are plain containers whose children overflow.
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(140.),
                height: Some(100.),
                clip: g % 2 == 0,
                ..Style::default()
            },
        );
        scene.set_transform(
            group,
            Transform {
                x: 10. + (g % 2) as f32 * 160.,
                y: 10. + (g / 2) as f32 * 115.,
            },
        );
        tree.all.push(group);
        tree.groups.push(group);
        for i in 0..5 {
            let node = scene.append(
                group,
                quad(Color(40 * i as u8, 90, 200 - 30 * g as u8, 255), i % 2 == 0),
                Style {
                    width: Some(30.),
                    height: Some(22.),
                    ..Style::default()
                },
            );
            scene.set_transform(
                node,
                Transform {
                    x: 8. + i as f32 * 24.,
                    y: 8. + (i % 3) as f32 * 26.,
                },
            );
            tree.all.push(node);
            tree.movers.push(node);
        }
        let label = scene.append(
            group,
            NodeKind::Text {
                text: format!("group {g}").into(),
                color: Color(255, 255, 255, 255),
                font_size: 13.,
            },
            Style::default(),
        );
        scene.set_transform(label, Transform { x: 6., y: 76. });
        tree.all.push(label);
        tree.labels.push(label);
    }
    (scene, tree)
}

fn mutate(scene: &mut Scene, tree: &Tree, step: u64) {
    let mut rng = Rng(0x243f_6a88_85a3_08d3 ^ (step + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    for _ in 0..1 + rng.below(3) {
        let mover = tree.movers[rng.below(tree.movers.len() as u64) as usize];
        match rng.below(6) {
            0 => {
                let t = scene.transform(mover);
                scene.set_transform(
                    mover,
                    Transform {
                        x: t.x + rng.signed(30.),
                        y: t.y + rng.signed(30.),
                    },
                );
            }
            1 => scene.set_kind(
                mover,
                quad(
                    Color(rng.below(256) as u8, rng.below(256) as u8, 128, 255),
                    rng.below(2) == 0,
                ),
            ),
            2 => scene.set_effects(
                mover,
                Effects {
                    // Includes fully transparent (hidden) and back.
                    opacity: rng.below(5) as f32 / 4.,
                    ..Effects::default()
                },
            ),
            3 => {
                let group = tree.groups[rng.below(tree.groups.len() as u64) as usize];
                let t = scene.transform(group);
                // A layer moved part of a pixel shows its texture resampled,
                // where a fresh render rasterizes it there: alike, not equal.
                if tree.layers && scene.is_layer(group) {
                    continue;
                }
                scene.set_transform(
                    group,
                    Transform {
                        x: (t.x + rng.signed(20.)).clamp(-40., W),
                        y: (t.y + rng.signed(20.)).clamp(-40., H),
                    },
                );
            }
            4 => {
                let label = tree.labels[rng.below(tree.labels.len() as u64) as usize];
                scene.set_text(label, format!("step {step} {}", rng.below(1000)));
            }
            _ => {
                let group = tree.groups[rng.below(tree.groups.len() as u64) as usize];
                let effects = scene.effects(group);
                scene.set_effects(
                    group,
                    Effects {
                        opacity: if effects.opacity > 0. { 0. } else { 1. },
                        ..effects
                    },
                );
            }
        }
    }
}

/// Groups drawn as layers: one zoomed, one isolated, one rotated, placed
/// within the window. A change inside one repaints its layer only where the
/// window's damage reaches it (`GpuStats::layer_partial_repaints`).
fn layered(scene: &mut Scene, tree: &mut Tree) {
    tree.layers = true;
    scene.set_transform(tree.groups[1], Transform { x: 100., y: 10. });
    scene.set_transform(tree.groups[2], Transform { x: 10., y: 130. });
    scene.set_transform(tree.groups[3], Transform { x: 160., y: 70. });
    scene.set_zoom(tree.groups[1], 1.5);
    scene.set_isolated(tree.groups[2], true);
    scene.set_local_transform(
        tree.groups[3],
        LocalTransform {
            rotate: 8.,
            ..LocalTransform::IDENTITY
        },
    );
}

#[test]
fn damage_limited_rendering_matches_a_fresh_render_after_every_mutation() {
    matches_a_fresh_render(false);
}

#[test]
fn layers_repainted_where_damaged_match_a_fresh_render_after_every_mutation() {
    matches_a_fresh_render(true);
}

fn matches_a_fresh_render(layers: bool) {
    let (mut live, mut tree) = build();
    if layers {
        layered(&mut live, &mut tree);
    }
    let mut renderer = GpuRenderer::new(W as u32, H as u32).expect("GPU adapter");
    let context = renderer.context();
    // The same scene, changed alike, its frames each told the whole window
    // changed: its layers repaint whole, its history otherwise the same (a
    // scene keeps what its renderer drew, so each has its own).
    let (mut again, mut again_tree) = build();
    if layers {
        layered(&mut again, &mut again_tree);
    }
    let mut whole = GpuRenderer::new_with_context(W as u32, H as u32, &context).unwrap();
    let window = [zgx::scene::Rect::new(0., 0., W, H)];
    let report = live.flush();
    renderer.render(&live, &report.damage).unwrap();
    again.flush();
    whole.render(&again, &window).unwrap();
    let mut partial = 0;
    // Layered, past step 84: a child of the zoomed group goes past its
    // box's left edge, its layer drawn from another corner (and every
    // pixel of it to be shown again, not only the child's).
    let steps = if layers { 130 } else { 60 };
    for step in 0..steps {
        mutate(&mut live, &tree, step);
        let report = live.flush();
        partial += renderer
            .render(&live, &report.damage)
            .unwrap()
            .layer_partial_repaints;
        let incremental = renderer.readback().unwrap();
        if layers {
            mutate(&mut again, &again_tree, step);
            again.flush();
            whole.render(&again, &window).unwrap();
            let repainted = whole.readback().unwrap();
            // A level apart at most: overlapping layers composite through
            // textures drawn at other times (stale pixels are far more).
            let differing = incremental
                .chunks(4)
                .zip(repainted.chunks(4))
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 1))
                .count();
            assert_eq!(
                differing, 0,
                "{differing} pixels differ from whole repaints after step {step}"
            );
        }

        let (mut fresh, mut fresh_tree) = build();
        if layers {
            layered(&mut fresh, &mut fresh_tree);
        }
        for replay in 0..=step {
            mutate(&mut fresh, &fresh_tree, replay);
        }
        let report = fresh.flush();
        let mut full = GpuRenderer::new_with_context(W as u32, H as u32, &context).unwrap();
        full.render(&fresh, &report.damage).unwrap();
        let expected = full.readback().unwrap();

        // A layer's texture, reused at a size a fresh render doesn't pick,
        // places its pixels through other coordinates: a level off at most.
        let tolerance = if layers { 2 } else { 0 };
        let differing = incremental
            .chunks(4)
            .zip(expected.chunks(4))
            .filter(|(a, b)| {
                a.iter()
                    .zip(b.iter())
                    .any(|(a, b)| a.abs_diff(*b) > tolerance)
            })
            .count();
        assert_eq!(differing, 0, "{differing} pixels differ after step {step}");
    }
    // The layers' changes went the partial way, not only the whole one.
    assert!(!layers || partial > 0, "no layer repainted where damaged");
}

/// Plain frames encoded with Metal directly (macOS) draw exactly the pixels
/// of the same frames encoded through wgpu.
#[test]
fn direct_metal_frames_match_wgpu_frames() {
    let (mut scene, tree) = build();
    let mut direct = GpuRenderer::new(W as u32, H as u32).expect("GPU adapter");
    let context = direct.context();
    let mut wgpu = GpuRenderer::new_with_context(W as u32, H as u32, &context).unwrap();
    wgpu.set_direct_metal(false);
    for step in 0..40 {
        let report = scene.flush();
        direct.render(&scene, &report.damage).unwrap();
        wgpu.render(&scene, &report.damage).unwrap();
        assert!(
            direct.readback().unwrap() == wgpu.readback().unwrap(),
            "frame {step} differs"
        );
        mutate(&mut scene, &tree, step);
    }
}
