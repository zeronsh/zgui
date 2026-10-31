//! Declared loops played by a layer (`loops.rs`): what leaves the window's
//! frames, what stays in them, and what draws a sheet again.
//!
//! Its own binary without the test harness: layer presentation runs on the
//! main thread, which the harness keeps for itself.
#[cfg(target_os = "macos")]
use std::{cell::Cell, rc::Rc};
#[cfg(target_os = "macos")]
use zgx::scene::*;
#[cfg(target_os = "macos")]
use zgx_gpu::GpuRenderer;

#[cfg(target_os = "macos")]
const RED: Color = Color(255, 0, 0, 255);
#[cfg(target_os = "macos")]
const BLUE: Color = Color(0, 0, 255, 255);

#[cfg(target_os = "macos")]
struct Window {
    scene: Scene,
    gpu: GpuRenderer,
    /// The looping node, and how many times its loop was refused.
    node: NodeId,
    refused: Rc<Cell<u32>>,
}
#[cfg(target_os = "macos")]
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}
#[cfg(target_os = "macos")]
impl Window {
    /// A 200-point window over blue; `holder` makes what holds the loop's
    /// 20-point red box at (`x`, `y`) of it.
    fn new(
        x: f32,
        y: f32,
        holder: impl FnOnce(&mut Scene, NodeId) -> NodeId,
        after: impl FnOnce(&mut Scene, NodeId),
    ) -> Self {
        let host = objc2_quartz_core::CALayer::new();
        let mut gpu = GpuRenderer::debug_for_layer(host, 400, 400).unwrap();
        gpu.set_scale_factor(2.);
        let mut scene = Scene::new(200., 200.);
        gpu.install_text(&mut scene);
        let root = scene.root();
        scene.set_kind(root, NodeKind::Container(Layout::Overlay));
        scene.append(root, NodeKind::Rect(BLUE), fixed(200., 200.));
        let parent = holder(&mut scene, root);
        let node = scene.append(parent, NodeKind::Rect(RED), fixed(20., 20.));
        scene.set_transform(node, Transform { x, y });
        after(&mut scene, root);
        let mut window = Self {
            scene,
            gpu,
            node,
            refused: Rc::default(),
        };
        // The first frame makes the layer the loops go on.
        window.show();
        window.bake();
        window.show();
        window
    }
    fn plain(x: f32, y: f32) -> Self {
        Self::new(x, y, |_, root| root, |_, _| {})
    }
    fn bake(&mut self) {
        let refused = self.refused.clone();
        let frames = (0..30).map(|at| (1. - at as f32 / 60., 0.)).collect();
        let baked = BakedLoop::new(
            frames,
            30.,
            Box::new(move || refused.set(refused.get() + 1)),
        );
        self.scene.set_baked_loop(self.node, Some(Rc::new(baked)));
    }
    fn show(&mut self) {
        let damage = self.scene.flush().damage;
        self.gpu.render(&self.scene, &damage).unwrap();
        self.gpu.present().unwrap();
    }
    /// The frame's pixel at (`x`, `y`) points, red and blue.
    fn pixel(&mut self, x: usize, y: usize) -> (u8, u8) {
        let (width, _, pixels) = self.gpu.debug_bare_pixels().expect("a surface shown");
        let at = (y * 2 * width as usize + x * 2) * 4;
        (pixels[at + 2], pixels[at])
    }
    fn played(&self) -> bool {
        !self.gpu.debug_loop_sheets().is_empty()
    }
}

#[cfg(target_os = "macos")]
fn a_loop_leaves_the_frames_and_nothing_presents_while_it_plays() {
    let mut window = Window::plain(40., 40.);
    assert!(window.played(), "no sheet");
    assert_eq!(window.refused.get(), 0);
    // The frames hold what's under it, for the layer over them.
    assert_eq!(window.pixel(50, 50), (0, 255));
    assert!(window.gpu.debug_loops_tagged_as_frames());
    // A hidden or covered window does as much: nothing asks for a frame.
    let drawn = window.gpu.debug_metal_frames();
    for _ in 0..5 {
        window.show();
    }
    assert_eq!(window.gpu.debug_metal_frames(), drawn);
}

#[cfg(target_os = "macos")]
fn a_read_of_the_frame_shows_the_loop_as_the_window_does() {
    let mut window = Window::plain(40., 40.);
    // Not in the frame itself; in what reads it, red over the blue.
    assert_eq!(window.pixel(50, 50), (0, 255));
    let (width, _, pixels) = window.gpu.shown_pixels().unwrap();
    let at = (100 * width as usize + 100) * 4;
    assert!(pixels[at + 2] > 120, "red {}", pixels[at + 2]);
    // Outside its box, the frame as it was.
    let beside = (100 * width as usize + 160) * 4;
    assert_eq!((pixels[beside + 2], pixels[beside]), (0, 255));
    // A region read from its middle finds it there too.
    let frame = window.gpu.shown_frame().unwrap();
    let (_, _, part) = frame.read(100, 100, 4, 4).unwrap();
    assert!(part[2] > 120, "red {}", part[2]);

    // The caret, shown or not.
    let caret = |shown| zgx_gpu::Caret {
        rect: (10., 10., 2., 20.),
        color: [0., 1., 0., 1.],
        shown,
    };
    window.gpu.set_caret(Some(caret(true)));
    let (width, _, pixels) = window.gpu.shown_pixels().unwrap();
    assert_eq!(pixels[(30 * width as usize + 22) * 4 + 1], 255, "the caret");
    window.gpu.set_caret(Some(caret(false)));
    let (width, _, pixels) = window.gpu.shown_pixels().unwrap();
    assert_eq!(pixels[(30 * width as usize + 22) * 4 + 1], 0, "hidden");
}

#[cfg(target_os = "macos")]
fn what_a_layer_would_show_wrongly_stays_in_the_frames() {
    // A clip cutting it.
    let clipped = |scene: &mut Scene, root: NodeId| {
        let clip = scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            Style {
                clip: true,
                ..fixed(50., 200.)
            },
        );
        scene.set_transform(clip, Transform { x: 0., y: 0. });
        clip
    };
    let mut window = Window::new(40., 40., clipped, |_, _| {});
    assert_eq!(window.refused.get(), 1, "cut by a clip");
    assert_eq!(window.pixel(45, 50), (255, 0), "drawn in the frames");

    // A fade's band reaching it; its interior doesn't.
    let faded = |scene: &mut Scene, root: NodeId| {
        scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            Style {
                clip: true,
                fade_edges: [60., 0.],
                ..fixed(200., 200.)
            },
        )
    };
    let window = Window::new(40., 40., faded, |_, _| {});
    assert_eq!(window.refused.get(), 1, "in a fade's band");
    let window = Window::new(40., 100., faded, |_, _| {});
    assert!(window.played(), "inside a fade, its bands clear of it");

    // Painted over by what follows it.
    let mut window = Window::new(
        40.,
        40.,
        |_, root| root,
        |scene, root| {
            let over = scene.append(root, NodeKind::Rect(BLUE), fixed(100., 100.));
            scene.set_transform(over, Transform { x: 0., y: 0. });
        },
    );
    assert_eq!(window.refused.get(), 1, "painted over");
    assert!(!window.played());
    assert_eq!(window.pixel(50, 50), (0, 255));

    // Holding something.
    let mut window = Window::plain(40., 40.);
    let panel = NodeKind::Panel {
        layout: Layout::Overlay,
        quad: QuadStyle {
            fill: RED,
            ..Default::default()
        },
    };
    window.scene.set_kind(window.node, panel);
    window.show();
    assert!(window.played(), "a panel plays");
    window
        .scene
        .append(window.node, NodeKind::Rect(BLUE), fixed(4., 4.));
    window.show();
    assert_eq!(window.refused.get(), 1, "holds a child");
}

#[cfg(target_os = "macos")]
fn a_new_look_or_scale_draws_the_sheet_again() {
    let mut window = Window::plain(40., 40.);
    let first = window.gpu.debug_loop_sheets();
    window.show();
    assert_eq!(window.gpu.debug_loop_sheets(), first, "nothing changed");
    // Moving draws nothing again: its layer moves.
    window
        .scene
        .set_transform(window.node, Transform { x: 60., y: 40. });
    window.show();
    assert_eq!(window.gpu.debug_loop_sheets(), first, "a move");
    // Its look: a theme's colour, a hover's.
    window.scene.set_kind(window.node, NodeKind::Rect(BLUE));
    window.show();
    let second = window.gpu.debug_loop_sheets();
    assert_ne!(second, first, "a new look");
    // Its scale: a window moved to another display.
    window.gpu.set_scale_factor(1.);
    window
        .scene
        .set_transform(window.node, Transform { x: 61., y: 40. });
    window.show();
    assert_ne!(window.gpu.debug_loop_sheets(), second, "a new scale");
    assert!(window.gpu.debug_loops_tagged_as_frames());
}

#[cfg(target_os = "macos")]
fn a_sheet_goes_with_its_node_and_its_loop() {
    let mut window = Window::plain(40., 40.);
    window.scene.remove(window.node);
    window.show();
    assert!(!window.played(), "removed");

    // With the subtree holding it.
    let holder = |scene: &mut Scene, root: NodeId| {
        scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            fixed(200., 200.),
        )
    };
    let mut window = Window::new(40., 40., holder, |_, _| {});
    assert!(window.played());
    let parent = window.scene.parent(window.node).unwrap();
    window.scene.remove(parent);
    window.show();
    assert!(!window.played(), "its subtree removed");
    assert!(window.scene.baked_loops().is_empty());

    let mut window = Window::plain(40., 40.);
    window.scene.set_baked_loop(window.node, None);
    window.show();
    assert!(!window.played(), "ended");
    assert_eq!(window.pixel(50, 50), (255, 0), "back in the frames");
}

#[cfg(target_os = "macos")]
fn a_large_loop_stays_in_the_frames() {
    let mut window = Window::plain(40., 40.);
    window.scene.set_style(window.node, fixed(180., 150.));
    window.show();
    assert!(!window.played(), "past the sheet's limit");
    assert_eq!(window.refused.get(), 1);
}

#[cfg(target_os = "macos")]
fn loops_that_look_alike_share_a_sheet() {
    let mut window = Window::plain(40., 40.);
    let twin = window
        .scene
        .append(window.scene.root(), NodeKind::Rect(RED), fixed(20., 20.));
    window
        .scene
        .set_transform(twin, Transform { x: 100., y: 40. });
    let frames = (0..30).map(|at| (1. - at as f32 / 60., 0.)).collect();
    let baked = BakedLoop::new(frames, 30., Box::new(|| {}));
    window.scene.set_baked_loop(twin, Some(Rc::new(baked)));
    window.show();
    let sheets = window.gpu.debug_loop_sheets();
    assert_eq!(sheets.len(), 2);
    assert_eq!(sheets[0], sheets[1], "one surface");
    // One going leaves the other's.
    window.scene.remove(twin);
    window.show();
    assert_eq!(window.gpu.debug_loop_sheets(), sheets[..1]);
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        a_loop_leaves_the_frames_and_nothing_presents_while_it_plays();
        a_read_of_the_frame_shows_the_loop_as_the_window_does();
        what_a_layer_would_show_wrongly_stays_in_the_frames();
        a_new_look_or_scale_draws_the_sheet_again();
        a_sheet_goes_with_its_node_and_its_loop();
        a_large_loop_stays_in_the_frames();
        loops_that_look_alike_share_a_sheet();
    }
}
