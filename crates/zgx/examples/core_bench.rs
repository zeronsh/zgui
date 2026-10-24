//! CPU-only invalidation microbenchmarks; not equivalent to rendered framework comparisons.
use std::{hint::black_box, time::Instant};
use zgx::{
    scene::{Color, Layout, LocalTransform, NodeKind, Scene, Style, Transform},
    virtual_list::VirtualList,
};

fn measure(name: &str, iterations: usize, mut operation: impl FnMut(usize)) {
    let start = Instant::now();
    for index in 0..iterations {
        operation(index);
    }
    let elapsed = start.elapsed();
    println!(
        "{name}: {iterations} iterations, {:.1} ns/iteration",
        elapsed.as_nanos() as f64 / iterations as f64
    );
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|s| s.parse::<usize>().expect("positive iteration count"))
        .unwrap_or(100_000)
        .max(1);
    let mut scene = Scene::new(1000.0, 700.0);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let stream = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "ready".into(),
            color: Color(255, 255, 255, 255),
            font_size: 16.0,
        },
        Style {
            width: Some(400.0),
            height: Some(100.0),
            ..Style::default()
        },
    );
    // Retain siblings to ensure text writes do not accidentally invalidate the tree.
    for _ in 0..1000 {
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(50, 50, 50, 255)),
            Style {
                width: Some(10.0),
                height: Some(10.0),
                ..Style::default()
            },
        );
    }
    scene.flush();
    measure("idle (1002 retained nodes)", iterations, |_| {
        assert!(black_box(scene.flush()).is_idle());
    });
    let mut layouts = 0;
    let mut paints = 0;
    measure("fixed text update + flush", iterations, |i| {
        scene.set_text(stream, if i % 2 == 0 { "stream A" } else { "stream B" });
        let frame = black_box(scene.flush());
        layouts += frame.layout_nodes;
        paints += frame.paint_nodes;
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 1);
        assert_eq!(frame.damage.len(), 1);
    });
    println!("  layout_nodes={layouts}, paint_nodes={paints}; expected 0 and {iterations}");
    let mut composites = 0;
    measure("leaf translation + flush", iterations, |i| {
        scene.set_transform(
            stream,
            Transform {
                x: (i % 2 + 1) as f32,
                y: 0.0,
            },
        );
        let frame = black_box(scene.flush());
        composites += frame.composite_nodes;
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 0);
        assert_eq!(frame.composite_nodes, 1);
    });
    println!("  composite_nodes={composites}; expected {iterations}");
    // An animation moving every card of a grid each frame: by translation,
    // then by rotation (each card a cached layer under a new matrix).
    let mut grid = Scene::new(1000.0, 700.0);
    grid.set_kind(grid.root(), NodeKind::Container(Layout::Overlay));
    let cards: Vec<_> = (0..1000)
        .map(|index| {
            let card = grid.append(
                grid.root(),
                NodeKind::Container(Layout::Overlay),
                Style {
                    width: Some(30.0),
                    height: Some(16.0),
                    ..Style::default()
                },
            );
            grid.append(
                card,
                NodeKind::Text {
                    text: format!("{index}").into(),
                    color: Color(255, 255, 255, 255),
                    font_size: 12.0,
                },
                Style {
                    width: Some(30.0),
                    height: Some(16.0),
                    ..Style::default()
                },
            );
            let (column, row) = (index % 40, index / 40);
            grid.set_transform(
                card,
                Transform {
                    x: column as f32 * 25.0,
                    y: row as f32 * 28.0,
                },
            );
            card
        })
        .collect();
    grid.flush();
    let frames = (iterations / 100).max(10);
    measure("1000 cards translating + flush", frames, |i| {
        for (index, card) in cards.iter().enumerate() {
            let (column, row) = (index % 40, index / 40);
            grid.set_transform(
                *card,
                Transform {
                    x: column as f32 * 25.0 + (i % 2 + 1) as f32,
                    y: row as f32 * 28.0,
                },
            );
        }
        let frame = black_box(grid.flush());
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert_eq!(frame.composite_nodes, 1000);
    });
    measure("hit test among 1000 cards", iterations, |i| {
        black_box(grid.hit_test((i % 1000) as f32, (i % 700) as f32));
    });
    measure("1000 cards rotating + flush", frames, |i| {
        for card in &cards {
            grid.set_local_transform(
                *card,
                LocalTransform {
                    rotate: 0.01 * (i + 1) as f32,
                    ..LocalTransform::IDENTITY
                },
            );
        }
        let frame = black_box(grid.flush());
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert_eq!(frame.composite_nodes, 1000);
    });
    measure("hit test among 1000 rotated cards", iterations, |i| {
        black_box(grid.hit_test((i % 1000) as f32, (i % 700) as f32));
    });
    let virtual_list = VirtualList::new(1_000_000, 24.0, 3);
    let mut maximum = 0;
    measure("million-row virtual range", iterations, |i| {
        let range = black_box(virtual_list.visible_range((i as f32 * 11.0) % 24_000_000.0, 480.0));
        maximum = maximum.max(range.len());
        assert!(range.len() <= 27);
    });
    println!(
        "  maximum_mounted_rows={maximum}; expected <=27; range computation retains no row data"
    );
}
