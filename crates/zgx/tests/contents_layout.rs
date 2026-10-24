//! Contents nodes (`Scene::set_contents`, `View::contents`) lay out as their
//! last child would in their place: wrapping any node in one, or two, must
//! not move or resize anything, whatever its parent's layout and its own
//! sizing, and must keep doing so as styles and children change.
use std::sync::Arc;
use zgx::{
    compose::prelude::*,
    layout::{Display, FlexWrap, LayoutOptions, Length},
    scene::{Align, Color, Insets, Justify, Layout, NodeId, NodeKind, Scene, Style},
    widgets::Ui,
};

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
    fn chance(&mut self, in_ten: u64) -> bool {
        self.below(10) < in_ten
    }
}

fn label(value: &str) -> NodeKind {
    NodeKind::Text {
        text: value.into(),
        color: Color(255, 255, 255, 255),
        font_size: 13.,
    }
}

/// How a node sits in its parent: sizes, flex, margins, percentages,
/// absolute insets, self alignment.
fn item_style(rng: &mut Rng, mut style: Style) -> Style {
    let mut options = LayoutOptions::default();
    style.flex_grow = [0., 0., 1., 2.][rng.below(4) as usize];
    style.flex_shrink = [0., 1.][rng.below(2) as usize];
    match rng.below(5) {
        0 => style.width = Some(20. + rng.below(80) as f32),
        1 => style.width_percent = Some([0.25, 0.5, 1.][rng.below(3) as usize]),
        _ => {}
    }
    match rng.below(5) {
        0 => style.height = Some(10. + rng.below(60) as f32),
        1 => style.height_percent = Some([0.5, 1.][rng.below(2) as usize]),
        _ => {}
    }
    if rng.chance(3) {
        style.margin = Insets {
            top: rng.below(6) as f32,
            right: rng.below(6) as f32,
            bottom: rng.below(6) as f32,
            left: rng.below(6) as f32,
        };
    }
    if rng.chance(1) {
        style.absolute = true;
        options.inset.top = Some(Length::Px(rng.below(20) as f32));
        options.inset.left = Some(Length::Px(rng.below(20) as f32));
        if rng.chance(5) {
            options.inset.right = Some(Length::Px(0.));
            options.inset.bottom = Some(Length::Percent(0.1));
        }
    }
    if rng.chance(2) {
        options.align_self =
            Some([Align::Start, Align::Center, Align::End, Align::Stretch][rng.below(4) as usize]);
    }
    if rng.chance(1) {
        options.min_width = Some(Length::Percent(0.2));
    }
    if rng.chance(1) {
        options.basis = Some(Length::Px(rng.below(50) as f32));
    }
    if rng.below(40) == 0 {
        options.display = Some(Display::None);
    }
    style.layout_options = (!options.is_empty()).then(|| Arc::new(options));
    style
}

/// A container's own layout: alignment, gaps, padding, sometimes wrapping
/// (laid out by taffy).
fn container_style(rng: &mut Rng) -> Style {
    let mut style = Style {
        align: [Align::Start, Align::Center, Align::End, Align::Stretch][rng.below(4) as usize],
        justify: [Justify::Start, Justify::Center, Justify::SpaceBetween][rng.below(3) as usize],
        gap: rng.below(6) as f32,
        padding: rng.below(5) as f32,
        ..Style::default()
    };
    if rng.chance(2) {
        style.layout_options = Some(Arc::new(LayoutOptions {
            wrap: Some(FlexWrap::Wrap),
            gap_x: Some(Length::Px(3.)),
            ..Default::default()
        }));
    }
    style
}
fn merge(item: Style, own: Style) -> Style {
    let mut options = own.layout_options.as_deref().copied().unwrap_or_default();
    if let Some(i) = item.layout_options.as_deref() {
        options.merge(*i);
        options.inset.merge(i.inset);
    }
    Style {
        layout_options: (!options.is_empty()).then(|| Arc::new(options)),
        align: own.align,
        justify: own.justify,
        gap: own.gap,
        padding: own.padding,
        ..item
    }
}

struct Tree {
    /// The nodes both trees share, in creation order.
    nodes: Vec<NodeId>,
    containers: Vec<NodeId>,
}

/// Wraps `parent`'s next child in `depth` contents nodes when `wrap` asks.
fn holder(scene: &mut Scene, parent: NodeId, wrap: &mut Option<Rng>) -> NodeId {
    let Some(rng) = wrap else { return parent };
    let mut at = parent;
    for _ in 0..rng.below(3) {
        let kind = [Layout::Column, Layout::Row, Layout::Overlay][rng.below(3) as usize];
        // Its own styles must not matter, except margins (none here).
        let own = Style {
            padding: 7.,
            gap: 9.,
            width: Some(3.),
            flex_grow: 5.,
            ..Style::default()
        };
        at = scene.append(at, NodeKind::Container(kind), own);
        scene.set_contents(at, true);
    }
    at
}

fn build(seed: u64, wrap: bool) -> (Scene, Tree) {
    let mut rng = Rng(seed);
    let mut wrap = wrap.then_some(Rng(seed ^ 0xdead_beef));
    let mut scene = Scene::new(500., 700.);
    let mut tree = Tree {
        nodes: Vec::new(),
        containers: Vec::new(),
    };
    let page = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Column),
        Style {
            align: Align::Stretch,
            flex_grow: 1.,
            padding: 4.,
            ..Style::default()
        },
    );
    tree.containers.push(page);
    let mut queue = vec![(page, 0)];
    while let Some((parent, depth)) = queue.pop() {
        for i in 0..2 + rng.below(3) {
            let at = holder(&mut scene, parent, &mut wrap);
            let id = if depth < 3 && rng.chance(4) {
                let layout = [Layout::Column, Layout::Row, Layout::Overlay][rng.below(3) as usize];
                let style = merge(
                    item_style(&mut rng, Style::default()),
                    container_style(&mut rng),
                );
                let id = scene.append(at, NodeKind::Container(layout), style);
                tree.containers.push(id);
                queue.push((id, depth + 1));
                id
            } else if rng.chance(5) {
                let style = item_style(
                    &mut rng,
                    Style {
                        text_wrap: true,
                        ..Style::default()
                    },
                );
                scene.append(at, label(&"word ".repeat(1 + i as usize * 3)), style)
            } else {
                let mut style = item_style(&mut rng, Style::default());
                style.width = style.width.or(Some(12.));
                style.height = style.height.or(Some(8.));
                scene.append(at, NodeKind::Rect(Color(1, 2, 3, 255)), style)
            };
            tree.nodes.push(id);
        }
    }
    (scene, tree)
}

fn mutate(scene: &mut Scene, tree: &mut Tree, step: u64, wrap: bool) {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (step + 1).wrapping_mul(0x2545_f491_4f6c_dd1d));
    let mut wrap = wrap.then_some(Rng(step ^ 0x5eed));
    match rng.below(4) {
        // Restyle a node: its contents ancestors follow.
        0 | 1 => {
            let id = tree.nodes[rng.below(tree.nodes.len() as u64) as usize];
            let base = scene.style(id);
            let own = Style {
                align: base.align,
                justify: base.justify,
                gap: base.gap,
                padding: base.padding,
                text_wrap: base.text_wrap,
                ..Style::default()
            };
            scene.set_style(id, item_style(&mut rng, own));
        }
        // A new last child somewhere (wrapped or not).
        2 => {
            let parent = tree.containers[rng.below(tree.containers.len() as u64) as usize];
            let at = holder(scene, parent, &mut wrap);
            let style = item_style(
                &mut rng,
                Style {
                    width: Some(15.),
                    height: Some(9.),
                    ..Style::default()
                },
            );
            let id = scene.append(at, NodeKind::Rect(Color(4, 5, 6, 255)), style);
            tree.nodes.push(id);
        }
        // Remove a leaf, with the contents nodes holding only it.
        _ => {
            let id = tree.nodes[rng.below(tree.nodes.len() as u64) as usize];
            if tree.containers.contains(&id) {
                return;
            }
            let mut top = id;
            while let Some(parent) = scene.parent(top)
                && scene.is_contents(parent)
                && scene.children(parent).len() == 1
            {
                top = parent;
            }
            scene.remove(top);
            tree.nodes.retain(|n| *n != id);
        }
    }
}

fn assert_same(plain: &Scene, a: &Tree, wrapped: &Scene, b: &Tree, when: &str) {
    assert_eq!(a.nodes.len(), b.nodes.len());
    let hidden = |mut n: NodeId| loop {
        let style = plain.style(n);
        if style
            .layout_options
            .is_some_and(|o| o.display == Some(Display::None))
        {
            return true;
        }
        match plain.parent(n) {
            Some(parent) => n = parent,
            None => return false,
        }
    };
    for (index, (x, y)) in a.nodes.iter().zip(&b.nodes).enumerate() {
        // Where hidden content would be is nothing anyone sees.
        if hidden(*x) {
            continue;
        }
        let (p, q) = (plain.bounds(*x), wrapped.bounds(*y));
        let close = |u: f32, v: f32| (u - v).abs() < 0.01;
        assert!(
            close(p.x, q.x)
                && close(p.y, q.y)
                && close(p.width, q.width)
                && close(p.height, q.height),
            "node {index} {when}: {p:?} plain, {q:?} wrapped\n{:?} {:?}\nparent {:?} {:?}",
            plain.kind(*x),
            plain.style(*x),
            plain.kind(plain.parent(*x).unwrap()),
            plain.style(plain.parent(*x).unwrap()),
        );
    }
}

#[test]
fn contents_wrappers_lay_out_as_if_absent() {
    let mut wrappers = 0;
    for seed in 1..=40_u64 {
        let seed = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let (mut plain, mut a) = build(seed, false);
        let (mut wrapped, mut b) = build(seed, true);
        plain.flush();
        wrapped.flush();
        wrappers += wrapped.len() - plain.len();
        assert_same(&plain, &a, &wrapped, &b, &format!("seed {seed:x}"));
        for step in 0..25 {
            mutate(&mut plain, &mut a, step, false);
            mutate(&mut wrapped, &mut b, step, true);
            plain.flush();
            wrapped.flush();
            assert_same(
                &plain,
                &a,
                &wrapped,
                &b,
                &format!("seed {seed:x}, step {step}"),
            );
        }
    }
    assert!(wrappers > 200, "{wrappers} wrappers tested");
}

#[test]
fn a_contents_node_adds_its_own_margins_and_keeps_its_paint() {
    let mut scene = Scene::new(300., 300.);
    let column = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Column),
        Style {
            width: Some(300.),
            align: Align::Stretch,
            ..Style::default()
        },
    );
    let wrapper = scene.append(
        column,
        NodeKind::Container(Layout::Column),
        Style::default(),
    );
    scene.set_contents(wrapper, true);
    let inner = scene.append(
        wrapper,
        NodeKind::Rect(Color(1, 1, 1, 255)),
        Style {
            height: Some(40.),
            margin: Insets {
                top: 5.,
                ..Insets::default()
            },
            ..Style::default()
        },
    );
    let below = scene.append(
        column,
        NodeKind::Rect(Color(1, 1, 1, 255)),
        Style {
            height: Some(10.),
            ..Style::default()
        },
    );
    scene.flush();
    assert_eq!(
        scene.bounds(inner),
        zgx::scene::Rect::new(0., 5., 300., 40.)
    );
    assert_eq!(scene.bounds(wrapper), scene.bounds(inner));
    assert_eq!(scene.bounds(below).y, 45.);
    // A negative bottom margin of its own (a layout animation's slack)
    // pulls what follows up; the content keeps its size.
    let mut own = scene.style(wrapper);
    own.margin.bottom = -15.;
    own.clip = true;
    scene.set_style(wrapper, own.clone());
    scene.flush();
    assert_eq!(scene.style(wrapper), own, "its own style reads back");
    assert_eq!(scene.bounds(inner).height, 40.);
    assert_eq!(scene.bounds(below).y, 30.);
    // Static: nothing to do from then on.
    assert_eq!(scene.flush().layout_nodes, 0);
    // Back to an ordinary box: its own style again.
    scene.set_contents(wrapper, false);
    scene.flush();
    assert_eq!(scene.bounds(wrapper).height, 45.);
    assert_eq!(scene.bounds(below).y, 30., "its own margin stays");
}

#[test]
fn earlier_children_lie_over_the_last_at_its_origin() {
    let mut scene = Scene::new(300., 300.);
    let row = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Row),
        Style::default(),
    );
    scene.append(
        row,
        NodeKind::Rect(Color(1, 1, 1, 255)),
        Style {
            width: Some(20.),
            height: Some(20.),
            ..Style::default()
        },
    );
    let wrapper = scene.append(row, NodeKind::Container(Layout::Column), Style::default());
    scene.set_contents(wrapper, true);
    let leaving = scene.append(
        wrapper,
        NodeKind::Rect(Color(1, 1, 1, 255)),
        Style {
            absolute: true,
            width: Some(70.),
            height: Some(30.),
            ..Style::default()
        },
    );
    let current = scene.append(
        wrapper,
        NodeKind::Rect(Color(1, 1, 1, 255)),
        Style {
            width: Some(50.),
            height: Some(10.),
            ..Style::default()
        },
    );
    scene.flush();
    assert_eq!(
        scene.bounds(current),
        zgx::scene::Rect::new(20., 0., 50., 10.)
    );
    assert_eq!(
        scene.bounds(leaving),
        zgx::scene::Rect::new(20., 0., 70., 30.)
    );
    // The last child leaving: the one before it leads.
    scene.remove(current);
    scene.flush();
    assert_eq!(
        scene.bounds(leaving),
        zgx::scene::Rect::new(0., 0., 70., 30.)
    );
}

/// The case a script hit: an editor body under a conditional region must
/// fill the window's height (`grow`), and swapping what it shows keeps
/// the region invisible to layout.
#[test]
fn a_switch_marked_contents_passes_its_childs_flex_through() {
    let mut ui = Ui::new(400., 300.);
    let editing = ui.signal(true);
    let read = editing.clone();
    let view = ui.mount(
        column()
            .w_full()
            .h_full()
            .items_stretch()
            .child(div().h(40.))
            .child(
                switch(
                    move || read.get(),
                    |editing, _| match editing {
                        true => column().id("body").grow().child(text("Body")),
                        false => div().id("empty").w(120.).h(20.).mt(6.),
                    },
                )
                .contents(),
            )
            .child(div().id("footer").h(20.)),
    );
    ui.prepare_frame();
    let bounds = |ui: &Ui, id: &str| ui.scene.borrow().bounds(view.find(id).unwrap());
    assert_eq!(
        bounds(&ui, "body"),
        zgx::scene::Rect::new(0., 40., 400., 240.)
    );
    assert_eq!(bounds(&ui, "footer").y, 280.);
    editing.set(false);
    ui.prepare_frame();
    assert_eq!(
        bounds(&ui, "empty"),
        zgx::scene::Rect::new(0., 46., 120., 20.)
    );
    assert_eq!(bounds(&ui, "footer").y, 66.);
}

#[test]
fn contents_wrappers_lay_out_as_if_absent_right_to_left_too() {
    use zgx::scene::Direction;
    for seed in 1..=40_u64 {
        let seed = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let (mut plain, mut a) = build(seed, false);
        let (mut wrapped, mut b) = build(seed, true);
        plain.set_direction(a.containers[0], Some(Direction::Rtl));
        wrapped.set_direction(b.containers[0], Some(Direction::Rtl));
        plain.flush();
        wrapped.flush();
        assert_same(&plain, &a, &wrapped, &b, &format!("rtl seed {seed:x}"));
        for step in 0..25 {
            mutate(&mut plain, &mut a, step, false);
            mutate(&mut wrapped, &mut b, step, true);
            plain.flush();
            wrapped.flush();
            assert_same(
                &plain,
                &a,
                &wrapped,
                &b,
                &format!("rtl seed {seed:x}, step {step}"),
            );
        }
    }
}
