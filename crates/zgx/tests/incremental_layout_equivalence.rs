//! Incremental layout (measurement caches, layout boundaries) must place every
//! node exactly where a from-scratch layout of the same tree would.
use zgx::scene::{Align, Color, Insets, Justify, Layout, NodeId, NodeKind, Scene, Style};

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
    fn size(&mut self) -> f32 {
        4. + self.below(40) as f32
    }
}

fn text(value: &str) -> NodeKind {
    NodeKind::Text {
        text: value.into(),
        color: Color(255, 255, 255, 255),
        font_size: 14.,
    }
}

/// Handles to the nodes mutations target, in creation order so two scenes
/// built the same way correspond index by index.
struct Tree {
    all: Vec<NodeId>,
    /// Fixed-size containers: layout boundaries.
    boxes: Vec<NodeId>,
    /// A resizable leaf inside each box.
    cells: Vec<NodeId>,
    labels: Vec<NodeId>,
    fold: NodeId,
}

fn build() -> (Scene, Tree) {
    let mut scene = Scene::new(640., 480.);
    let root = scene.root();
    let mut all = Vec::new();
    let mut add = |scene: &mut Scene, parent, kind, style| {
        let id = scene.append(parent, kind, style);
        all.push(id);
        id
    };
    let page = add(
        &mut scene,
        root,
        NodeKind::Container(Layout::Column),
        Style {
            padding: 8.,
            gap: 6.,
            align: Align::Stretch,
            flex_grow: 1.,
            ..Style::default()
        },
    );
    let mut labels = vec![add(&mut scene, page, text("Header"), Style::default())];
    let strip = add(
        &mut scene,
        page,
        NodeKind::Container(Layout::Row),
        Style {
            gap: 4.,
            align: Align::Center,
            ..Style::default()
        },
    );
    let (mut boxes, mut cells) = (Vec::new(), Vec::new());
    for i in 0..4 {
        let fixed = add(
            &mut scene,
            strip,
            NodeKind::Container(Layout::Column),
            Style {
                width: Some(60.),
                height: Some(50.),
                padding: 3.,
                align: Align::Center,
                justify: Justify::Center,
                flex_shrink: if i % 2 == 0 { 1. } else { 0. },
                ..Style::default()
            },
        );
        labels.push(add(&mut scene, fixed, text("box"), Style::default()));
        cells.push(add(
            &mut scene,
            fixed,
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(10.),
                height: Some(10.),
                ..Style::default()
            },
        ));
        boxes.push(fixed);
    }
    let fold = add(
        &mut scene,
        page,
        NodeKind::Container(Layout::Column),
        Style {
            height: Some(0.),
            clip: true,
            ..Style::default()
        },
    );
    add(&mut scene, fold, text("folded"), Style::default());
    for i in 0..6 {
        let row = add(
            &mut scene,
            page,
            NodeKind::Container(Layout::Row),
            Style {
                gap: 3.,
                margin: Insets::all(1.),
                ..Style::default()
            },
        );
        labels.push(add(
            &mut scene,
            row,
            text(&format!("row {i}")),
            Style::default(),
        ));
        add(
            &mut scene,
            row,
            NodeKind::Rect(Color(9, 9, 9, 255)),
            Style {
                flex_grow: 1.,
                height: Some(8.),
                ..Style::default()
            },
        );
    }
    (
        scene,
        Tree {
            all,
            boxes,
            cells,
            labels,
            fold,
        },
    )
}

fn mutate(scene: &mut Scene, tree: &mut Tree, step: u64) {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (step + 1).wrapping_mul(0x2545_f491_4f6c_dd1d));
    for _ in 0..1 + rng.below(3) {
        match rng.below(7) {
            // Inside a boundary: must not disturb anything outside it.
            0 => {
                let cell = tree.cells[rng.below(tree.cells.len() as u64) as usize];
                let (w, h) = (rng.size(), rng.size());
                let mut style = scene.style(cell);
                style.width = Some(w);
                style.height = Some(h);
                scene.set_style(cell, style);
            }
            1 => {
                let label = tree.labels[rng.below(tree.labels.len() as u64) as usize];
                let words = "a bb ccc dddd eeeee"
                    .split(' ')
                    .take(1 + rng.below(5) as usize);
                scene.set_text(label, words.collect::<Vec<_>>().join(" "));
            }
            // The boundary's own size: its parent must re-lay out.
            2 => {
                let fixed = tree.boxes[rng.below(tree.boxes.len() as u64) as usize];
                let mut style = scene.style(fixed);
                style.width = Some(30. + rng.below(60) as f32);
                style.height = Some(20. + rng.below(50) as f32);
                scene.set_style(fixed, style);
            }
            3 => {
                let mut style = scene.style(tree.fold);
                style.height = Some(rng.below(40) as f32);
                scene.set_style(tree.fold, style);
            }
            // Structure changes inside a boundary.
            4 => {
                let fixed = tree.boxes[rng.below(tree.boxes.len() as u64) as usize];
                let id = scene.append(
                    fixed,
                    NodeKind::Rect(Color(4, 5, 6, 255)),
                    Style {
                        width: Some(rng.size()),
                        height: Some(4.),
                        ..Style::default()
                    },
                );
                tree.all.push(id);
            }
            5 => {
                let fixed = tree.boxes[rng.below(tree.boxes.len() as u64) as usize];
                let children = scene.children(fixed).to_vec();
                if children.len() > 2 {
                    let id = *children.last().unwrap();
                    scene.remove(id);
                    tree.all.retain(|node| *node != id);
                }
            }
            _ => {
                let fixed = tree.boxes[rng.below(tree.boxes.len() as u64) as usize];
                let mut style = scene.style(fixed);
                style.margin = Insets::all(rng.below(6) as f32);
                scene.set_style(fixed, style);
            }
        }
    }
}

#[test]
fn incremental_layout_matches_a_fresh_layout_after_every_mutation() {
    let (mut live, mut live_tree) = build();
    live.flush();
    for step in 0..120 {
        mutate(&mut live, &mut live_tree, step);
        live.flush();
        let (mut fresh, mut fresh_tree) = build();
        for replay in 0..=step {
            mutate(&mut fresh, &mut fresh_tree, replay);
        }
        fresh.flush();
        assert_eq!(live_tree.all.len(), fresh_tree.all.len());
        for (index, (a, b)) in live_tree.all.iter().zip(&fresh_tree.all).enumerate() {
            assert_eq!(
                live.bounds(*a),
                fresh.bounds(*b),
                "node {index} diverged after step {step}"
            );
        }
    }
}
