//! Frames presented through a window's own surfaces (`LayerPresent`), each
//! brought up to date from the one shown around what a frame draws: what
//! they show is what drawing the scene whole shows. (The copies bringing a
//! surface up to date once read their rectangles 32 bytes apart from
//! 128-byte quads: stale patches in windows.)
//!
//! Its own binary without the test harness: layer presentation runs on the
//! main thread, which the harness keeps for itself.
#[cfg(target_os = "macos")]
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, widgets::Ui};
#[cfg(target_os = "macos")]
use zgx_gpu::GpuRenderer;

#[cfg(target_os = "macos")]
const SIZE: (f32, f32) = (640., 400.);

#[cfg(target_os = "macos")]
/// Cells all over the window, a few changing each tick: damage scattered
/// over most of it.
fn view(live: Signal<u32>) -> View {
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(14, 18, 24, 255))
        .children((0..48u32).map(move |cell| {
            let (bar, fill) = (live.clone(), live.clone());
            let (x, y) = ((cell % 8) as f32 * 80., (cell / 8) as f32 * 66.);
            div()
                .absolute()
                .ml(x + 4.)
                .mt(y + 4.)
                .size(72., 58.)
                .bg(Color(30, 40, 54, 255))
                .child(
                    div()
                        .absolute()
                        .ml(6.)
                        .w(10.)
                        .reactive_style(move || {
                            let on = (bar.get() + cell * 5) % 7 == 0;
                            let height = if on { 40. } else { 12. + (cell % 5) as f32 };
                            Styles::new().mt(52. - height).h(height)
                        })
                        .bg(Color(94, 177, 255, 255)),
                )
                .child(
                    div()
                        .absolute()
                        .ml(24.)
                        .mt(20.)
                        .size(30., 16.)
                        .reactive_style(move || {
                            let on = (fill.get() + cell * 3) % 11 == 0;
                            Styles::new().bg(if on {
                                Color(200, 80, 60, 255)
                            } else {
                                Color(40, 120, 80, 255)
                            })
                        }),
                )
        }))
}

#[cfg(target_os = "macos")]
struct Window {
    /// The layer the renderer presents into (`Layer`'s host).
    host: objc2::rc::Retained<objc2_quartz_core::CALayer>,
    ui: Ui,
    gpu: GpuRenderer,
    live: Signal<u32>,
    /// Frames patched into the window as kept (`render_kept`).
    patched: usize,
}
#[cfg(target_os = "macos")]
impl Window {
    fn new(live: u32) -> Self {
        Self::sized(live, SIZE)
    }
    /// A window of `size` points at scale 2.
    fn sized(live: u32, size: (f32, f32)) -> Self {
        let host = objc2_quartz_core::CALayer::new();
        let mut gpu =
            GpuRenderer::debug_for_layer(host.clone(), (size.0 * 2.) as u32, (size.1 * 2.) as u32)
                .unwrap();
        gpu.set_scale_factor(2.);
        let mut ui = Ui::new(size.0, size.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let state = ui.signal(live);
        ui.mount(view(state.clone()));
        Window {
            host,
            ui,
            gpu,
            live: state,
            patched: 0,
        }
    }
    fn show(&mut self) -> Vec<u8> {
        self.ui.prepare_frame();
        let damage = self.ui.scene.borrow_mut().flush().damage;
        let stats = self.gpu.render(&self.ui.scene.borrow(), &damage).unwrap();
        self.patched += usize::from(stats.patched_items > 0);
        self.gpu.present().unwrap();
        self.gpu.shown_pixels().expect("a surface shown").2
    }
}

#[cfg(target_os = "macos")]
fn scattered_damage_presented_through_surfaces_matches_drawing_whole() {
    let mut window = Window::new(1);
    assert!(
        window.show() == Window::new(1).show(),
        "the first frame differs"
    );
    for live in 2..16 {
        window.live.set(live);
        assert!(
            window.show() == Window::new(live).show(),
            "frame {live}: the surface shown differs from the scene drawn whole"
        );
    }
    // Only leaves change: frames patch the window as kept, each surface
    // then brought up to date around them.
    assert!(window.patched > 0, "no frame patched");
}

/// A rounded panel (overflow hidden) whose scroll view, filled opaque and
/// square, sits beside a divider, as an app's sidebar: scrolled down and
/// back up in a wheel's uneven steps, presented through the window's
/// surfaces, each step a copy of the shown frame's pixels. The panel's
/// corners stay where they are: a copy must not carry their edge, or what
/// lies outside it, down the view (a user's report: a trail of corners).
#[cfg(target_os = "macos")]
fn panel(offset: Signal<f32>) -> View {
    let mut cards = column().gap(6.).p(6.);
    for i in 0..40 {
        cards = cards.child(div().w(300.).h(36.).bg(if i % 2 == 0 {
            Color(34, 51, 68, 255)
        } else {
            Color(42, 58, 74, 255)
        }));
    }
    overlay()
        .size(SIZE.0, SIZE.1)
        .bg(Color(47, 107, 58, 255))
        .child(
            row()
                .w(600.)
                .h(380.)
                .left(20.)
                .top(10.)
                .rounded(16.)
                .bg(Color(17, 17, 17, 255))
                .overflow_hidden()
                // Its own opaque fill, square: the panel's corners round it.
                .child(
                    scroll(offset)
                        .w(330.)
                        .h(380.)
                        .bg(Color(17, 17, 17, 255))
                        .child(cards),
                )
                .child(div().w(1.).h(380.).bg(Color(51, 51, 51, 255)))
                .child(div().w(260.).h(380.).bg(Color(22, 22, 22, 255))),
        )
}
#[cfg(target_os = "macos")]
fn rounded_panel_scrolled_through_surfaces_matches_drawing_whole() {
    let copies = scrolled_matches_drawing_whole(panel, STEPS);
    // Each step moved by whole pixels: each copied, its corners repainted.
    assert!(copies > 0, "no scroll copied");
}

/// A scroll view's content under something that stays where it is: a
/// translucent veil, or a turned card (a layer).
#[cfg(target_os = "macos")]
fn under_a_still(offset: Signal<f32>, turned: bool) -> View {
    let mut cards = column().gap(6.).p(6.);
    for i in 0..40 {
        cards = cards.child(div().w(300.).h(36.).bg(if i % 2 == 0 {
            Color(34, 51, 68, 255)
        } else {
            Color(42, 58, 74, 255)
        }));
    }
    let still = div().left(60.).top(300.).size(160., 40.);
    let still = match turned {
        true => still.rounded(6.).bg(Color(90, 60, 200, 255)).rotate(0.2),
        false => still.bg(Color(250, 250, 250, 255)).opacity(0.4),
    };
    overlay()
        .size(SIZE.0, SIZE.1)
        .bg(Color(47, 107, 58, 255))
        .child(
            scroll(offset)
                .w(330.)
                .h(380.)
                .left(20.)
                .top(10.)
                .bg(Color(17, 17, 17, 255))
                .child(cards),
        )
        .child(still)
}
/// Scrolled far enough that the strip scrolled in covers what lies over
/// the content: drawn there afresh, its pixels in the frame before must not
/// move with the copy (a ghost of it up the view).
#[cfg(target_os = "macos")]
fn still_over_a_scroll_repainted_where_it_is_leaves_no_ghost() {
    let steps = [0., 120., 240., 120., 360., 0.];
    let copies = scrolled_matches_drawing_whole(|offset| under_a_still(offset, false), &steps);
    let turned = scrolled_matches_drawing_whole(|offset| under_a_still(offset, true), &steps);
    assert!(
        copies > 0 && turned > 0,
        "no scroll copied: {copies}, {turned}"
    );
}

#[cfg(target_os = "macos")]
struct Rng(u64);
#[cfg(target_os = "macos")]
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// A random scene of scroll views (plain, in rounded panels, nested) under
/// and among moving, fading and turning things, for `seed`: `tick` changes
/// what moves, `offsets` scroll.
#[cfg(target_os = "macos")]
fn scrolling_scene(seed: u64, tick: Signal<u32>, offsets: &[Signal<f32>]) -> View {
    let mut rng = Rng(seed);
    let rows = |rng: &mut Rng, count: u32, width: f32, inner: Option<Signal<f32>>| {
        let mut list = column()
            .gap(rng.below(3) as f32 * 3.)
            .p(rng.below(2) as f32 * 4.);
        for i in 0..count {
            let fill = Color(30 + (i * 17 % 60) as u8, 50, 70 + (i * 29 % 80) as u8, 255);
            let mut row = div().w(width).h(24. + rng.below(3) as f32 * 9.).bg(fill);
            match rng.below(8) {
                0 => row = row.rounded(8.),
                1 => row = row.bg(Color(fill.0, fill.1, fill.2, 140)),
                2 => {
                    row = row.shadow(zgx::scene::BoxShadow {
                        color: Color(0, 0, 0, 150),
                        offset: zgx::scene::Transform { x: 2., y: 3. },
                        blur_radius: 4.,
                        spread: 0.,
                    })
                }
                3 => row = row.border(1.).border_color(Color(220, 220, 220, 255)),
                _ => {}
            }
            if rng.chance(30) {
                row = row.child(
                    text(format!("Row {i} of {seed}")).text_color(Color(230, 230, 230, 255)),
                );
            }
            if rng.chance(15) {
                let (tick, phase) = (tick.clone(), i);
                row = row.reactive_style(move || {
                    let on = (tick.get() + phase) % 3 == 0;
                    Styles::new()
                        .bg(if on { Color(200, 90, 50, 255) } else { fill })
                        .translate(if on { 6. } else { 0. }, 0.)
                });
            }
            list = list.child(row);
        }
        if let Some(offset) = inner {
            list = list.child(
                scroll(offset)
                    .w(width - 20.)
                    .h(90.)
                    .bg(Color(12, 12, 16, 255))
                    .child(rows_plain(width - 30., 12)),
            );
        }
        list
    };
    let mut root = overlay().size(SIZE.0, SIZE.1).bg(Color(47, 107, 58, 255));
    for (n, offset) in offsets.iter().enumerate() {
        let (x, y) = (10. + n as f32 * 210., 8. + rng.below(4) as f32 * 7.);
        let inner = (n == 0).then(|| offsets[offsets.len() - 1].clone());
        let mut view = scroll(offset.clone())
            .w(190.)
            .h(300. + rng.below(3) as f32 * 20.)
            .child(rows(&mut rng, 24, 180., inner));
        if rng.chance(60) {
            view = view.bg(Color(17, 17, 17, 255));
        }
        if rng.chance(30) {
            view = view.edge_fade(12.);
        }
        let holder = if rng.chance(50) {
            div()
                .left(x)
                .top(y)
                .rounded(4. + rng.below(3) as f32 * 6.)
                .bg(Color(17, 17, 17, 255))
                .overflow_hidden()
                .child(view)
        } else {
            div().left(x).top(y).child(view)
        };
        root = root.child(holder);
        if n == offsets.len() - 2 {
            break;
        }
    }
    // Over and among them: a badge that moves, a veil that fades, a card
    // that turns.
    if rng.chance(70) {
        let tick = tick.clone();
        let size = (60. + rng.below(40) as f32, 30.);
        root = root.child(
            div()
                .size(size.0, size.1)
                .bg(Color(200, 60, 40, 255))
                .reactive_style(move || {
                    let t = tick.get();
                    Styles::new()
                        .left(40. + (t % 5) as f32 * 37.)
                        .top(60. + (t % 3) as f32 * 41.)
                }),
        );
    }
    if rng.chance(50) {
        let tick = tick.clone();
        root = root.child(
            div()
                .left(150.)
                .top(200.)
                .size(160., 60.)
                .bg(Color(250, 250, 250, 255))
                .reactive_style(move || Styles::new().opacity(0.2 + (tick.get() % 4) as f32 * 0.2)),
        );
    }
    if rng.chance(40) {
        root = root.child(
            div()
                .left(320.)
                .top(120.)
                .size(80., 50.)
                .rounded(6.)
                .bg(Color(90, 60, 200, 255))
                .rotate(0.2),
        );
    }
    root
}
#[cfg(target_os = "macos")]
fn rows_plain(width: f32, count: u32) -> View {
    column().children((0..count).map(move |i| {
        div()
            .w(width)
            .h(20.)
            .bg(Color(60 + (i * 13 % 90) as u8, 40, 40, 255))
    }))
}
/// How far antialiased edges may differ from drawing whole, per channel.
#[cfg(target_os = "macos")]
const EDGE_LEVELS: u8 = 6;
/// Random scenes of scroll views among moving things, scrolled and changed
/// at random through a window's own surfaces: each frame shows what
/// drawing it whole shows (scroll copies, kept frames and damage together).
#[cfg(target_os = "macos")]
fn random_scrolling_scenes_through_surfaces_match_drawing_whole() {
    let renderer = |host: &objc2::rc::Retained<objc2_quartz_core::CALayer>| {
        let mut gpu =
            GpuRenderer::debug_for_layer(host.clone(), (SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32)
                .unwrap();
        gpu.set_scale_factor(2.);
        gpu
    };
    let seeds: u64 = std::env::var("ZGX_SCROLL_SEEDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(6);
    let mut copies = 0;
    for seed in 1..=seeds {
        let seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let host = objc2_quartz_core::CALayer::new();
        let mut gpu = renderer(&host);
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        ui.scene.borrow_mut().set_pixel_scale(2.);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let tick = ui.signal(0u32);
        let offsets: Vec<Signal<f32>> = (0..4).map(|_| ui.signal(0.)).collect();
        ui.mount(scrolling_scene(seed, tick.clone(), &offsets));
        // Drawn whole each step: one renderer a seed (a fresh one a step
        // makes the GPU driver run out of contexts it has yet to free).
        let whole_host = objc2_quartz_core::CALayer::new();
        let mut whole = renderer(&whole_host);
        let mut rng = Rng(seed ^ 0xD1B5_4A32_D192_ED03);
        for step in 0..30 {
            let mut did = Vec::new();
            for (n, offset) in offsets.iter().enumerate() {
                if rng.chance(50) {
                    // Mostly whole device pixels (copies), some between
                    // (repaints).
                    let at = rng.below(400) as f32 * 0.5 + if rng.chance(20) { 0.25 } else { 0. };
                    offset.set(at);
                    did.push(format!("offset{n}={at}"));
                }
            }
            if rng.chance(40) {
                tick.set(tick.with_untracked(|t| t + 1));
                did.push("tick".into());
            }
            ui.prepare_frame();
            let damage = ui.scene.borrow_mut().flush().damage;
            copies += gpu
                .render(&ui.scene.borrow(), &damage)
                .unwrap()
                .scroll_copies;
            gpu.present().unwrap();
            let shown = gpu.shown_pixels().expect("a surface shown").2;
            whole
                .render(
                    &ui.scene.borrow(),
                    &[zgx::scene::Rect::new(0., 0., SIZE.0, SIZE.1)],
                )
                .unwrap();
            whole.present().unwrap();
            let expected = whole.shown_pixels().expect("a surface shown").2;
            // Curved and text edges may differ by a few levels: a copy by an
            // odd number of device pixels (docs/performance-feature-audit.md),
            // and a region repainted (rounded corners by a level, glyphs by a
            // few). Anything drawn in the wrong place differs by far more.
            let off = |(a, b): (&[u8], &[u8])| {
                a.iter()
                    .zip(b)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0)
            };
            let worst = shown
                .chunks(4)
                .zip(expected.chunks(4))
                .map(off)
                .max()
                .unwrap_or(0);
            if worst > EDGE_LEVELS {
                let width = (SIZE.0 * 2.) as usize;
                let spots: Vec<(usize, usize)> = shown
                    .chunks(4)
                    .zip(expected.chunks(4))
                    .enumerate()
                    .filter(|(_, pair)| off(*pair) > EDGE_LEVELS)
                    .map(|(i, _)| (i % width, i / width))
                    .collect();
                let (xs, ys) = (
                    (
                        spots.iter().map(|s| s.0).min(),
                        spots.iter().map(|s| s.0).max(),
                    ),
                    (
                        spots.iter().map(|s| s.1).min(),
                        spots.iter().map(|s| s.1).max(),
                    ),
                );
                panic!(
                    "seed {seed} step {step} ({}): {} pixels differ from drawing whole (by up to {worst}), x {xs:?} y {ys:?}",
                    did.join(" "),
                    spots.len()
                );
            }
        }
    }
    assert!(copies > 0, "no scroll copied");
}

/// Offsets there and back, whole device pixels and between.
#[cfg(target_os = "macos")]
const STEPS: &[f32] = &[
    0., 3.5, 9.25, 17., 30.5, 48., 71.75, 90., 120.5, 150., 200., 260., 200.5, 150., 95.5, 61.25,
    40., 22.5, 8.75, 2., 0.,
];
/// Scroll `view`'s offset through `steps` through a window's own surfaces, each
/// frame compared with the same scene drawn whole by a renderer of its own;
/// returns how many scrolls copied.
#[cfg(target_os = "macos")]
fn scrolled_matches_drawing_whole(view: fn(Signal<f32>) -> View, steps: &[f32]) -> usize {
    scrolled_matches_drawing_whole_at(view, steps, 2.)
}
/// `scrolled_matches_drawing_whole` at a device `scale`.
#[cfg(target_os = "macos")]
fn scrolled_matches_drawing_whole_at(
    view: fn(Signal<f32>) -> View,
    steps: &[f32],
    scale: f32,
) -> usize {
    let renderer = |host: &objc2::rc::Retained<objc2_quartz_core::CALayer>| {
        let mut gpu = GpuRenderer::debug_for_layer(
            host.clone(),
            (SIZE.0 * scale) as u32,
            (SIZE.1 * scale) as u32,
        )
        .unwrap();
        gpu.set_scale_factor(scale);
        gpu
    };
    let host = objc2_quartz_core::CALayer::new();
    let mut gpu = renderer(&host);
    let mut ui = Ui::new(SIZE.0, SIZE.1);
    ui.scene.borrow_mut().set_pixel_scale(scale);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let offset = ui.signal(0.);
    ui.mount(view(offset.clone()));
    let mut copies = 0;
    for &at in steps {
        offset.set(at);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        copies += gpu
            .render(&ui.scene.borrow(), &damage)
            .unwrap()
            .scroll_copies;
        gpu.present().unwrap();
        let shown = gpu.shown_pixels().expect("a surface shown").2;
        let whole_host = objc2_quartz_core::CALayer::new();
        let mut whole = renderer(&whole_host);
        whole
            .render(
                &ui.scene.borrow(),
                &[zgx::scene::Rect::new(0., 0., SIZE.0, SIZE.1)],
            )
            .unwrap();
        whole.present().unwrap();
        let expected = whole.shown_pixels().expect("a surface shown").2;
        if shown != expected {
            let width = (SIZE.0 * scale) as usize;
            let spots: Vec<(usize, usize)> = shown
                .chunks(4)
                .zip(expected.chunks(4))
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| (i % width, i / width))
                .collect();
            let xs = (
                spots.iter().map(|s| s.0).min(),
                spots.iter().map(|s| s.0).max(),
            );
            let ys = (
                spots.iter().map(|s| s.1).min(),
                spots.iter().map(|s| s.1).max(),
            );
            eprintln!("DIFF scroll {at}: {} px, x {xs:?} y {ys:?}", spots.len());
        }
        assert!(
            shown == expected,
            "scroll {at}: the surface shown differs from the scene drawn whole"
        );
    }
    copies
}

/// The plain layer's tiles (one per surface shown, `Layer::tiles`): each
/// one's contents, as an address, and whether it is hidden.
#[cfg(target_os = "macos")]
fn tiles(host: &objc2_quartz_core::CALayer) -> Vec<(usize, bool)> {
    use objc2::{msg_send, rc::Retained, runtime::AnyObject};
    // SAFETY: the layers' live sublayer arrays, read on the main thread.
    let plain = unsafe { host.sublayers() }
        .and_then(|subs| subs.iter().find(|sub| unsafe { sub.sublayers() }.is_some()))
        .expect("a plain layer holding tiles");
    unsafe { plain.sublayers() }
        .expect("tiles")
        .iter()
        .map(|tile| {
            // SAFETY: `contents` returns a nullable object (`id`), retained here.
            let contents: Option<Retained<AnyObject>> = unsafe { msg_send![&*tile, contents] };
            let at = contents.map_or(0, |c| Retained::as_ptr(&c) as usize);
            (at, tile.isHidden())
        })
        .collect()
}

/// Frames shown on the plain layer flip which tile shows: a tile's contents
/// are set once (new contents each frame make the window server take the
/// surface in again, at about 4 points of a core more in the heavy
/// dashboard), one tile shows at a time, and a surface the renderer lets go
/// takes its tile with it.
#[cfg(target_os = "macos")]
fn plain_frames_flip_tiles_whose_contents_are_set_once() {
    let mut window = Window::new(1);
    window.show();
    let mut seen = std::collections::BTreeSet::new();
    for live in 2..12 {
        window.live.set(live);
        window.show();
        let tiles = tiles(&window.host);
        assert_eq!(
            tiles.iter().filter(|(_, hidden)| !hidden).count(),
            1,
            "frame {live}: one tile shows ({tiles:?})"
        );
        assert!(
            tiles.iter().all(|(at, _)| *at != 0),
            "a tile without contents: {tiles:?}"
        );
        seen.extend(tiles.iter().map(|(at, _)| *at));
        assert!(
            tiles.len() <= 3,
            "frame {live}: a tile per surface, {} tiles",
            tiles.len()
        );
    }
    // As many contents as surfaces ever shown: none set again.
    assert!(
        seen.len() <= 3,
        "tiles' contents changed: {} seen",
        seen.len()
    );
    // At rest the renderer keeps one surface, and the plain layer one tile.
    window.gpu.trim();
    let tiles = tiles(&window.host);
    assert_eq!(tiles.len(), 1, "at rest: one tile ({tiles:?})");
    assert!(!tiles[0].1, "at rest the kept tile shows");
}

/// The host's bounds, the plain layer's, and each tile's frame.
#[cfg(target_os = "macos")]
fn geometry(
    host: &objc2_quartz_core::CALayer,
) -> (
    objc2_core_foundation::CGRect,
    objc2_core_foundation::CGRect,
    Vec<objc2_core_foundation::CGRect>,
) {
    // SAFETY: the layers' live sublayer arrays, read on the main thread.
    let plain = unsafe { host.sublayers() }
        .and_then(|subs| subs.iter().find(|sub| unsafe { sub.sublayers() }.is_some()))
        .expect("a plain layer holding tiles");
    let tiles = unsafe { plain.sublayers() }
        .expect("tiles")
        .iter()
        .map(|tile| tile.frame())
        .collect();
    (host.bounds(), plain.bounds(), tiles)
}

/// Every tile is exactly the plain layer's box, and the plain layer the
/// host's, after every show: a host that starts empty (an overlay plane's)
/// and grows to a fractional width (1168.5 points at scale 2) once left the
/// first tile at the whole points autoresizing gave it, its surface stretched
/// a pixel wider than the other's as they took turns (the layout shifted a
/// pixel while a caret blinked).
#[cfg(target_os = "macos")]
fn tiles_fill_the_plain_layer_exactly_after_the_host_grows() {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_quartz_core::CATransaction;
    let mut window = Window::new(1);
    // The first tile, made while the host is empty.
    window.show();
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    window.host.setFrame(CGRect::new(
        CGPoint::new(0., 0.),
        CGSize::new(1168.5, 400.25),
    ));
    CATransaction::commit();
    for live in 2..10 {
        window.live.set(live);
        window.show();
        let (host, plain, tiles) = geometry(&window.host);
        assert_eq!(
            plain, host,
            "frame {live}: the plain layer is not the host's box"
        );
        for tile in &tiles {
            assert_eq!(
                *tile, plain,
                "frame {live}: a tile is not the plain layer's box ({tiles:?})"
            );
        }
    }
}

/// A region of the shown frame (`ShownFrame::read`) is those rows and
/// columns of the whole frame (`shown_pixels`), clipped to it.
#[cfg(target_os = "macos")]
fn shown_frame_regions_are_the_whole_frames_rows() {
    let mut window = Window::new(3);
    let (width, height, whole) = {
        window.show();
        window.gpu.shown_pixels().expect("a surface shown")
    };
    let frame = window.gpu.shown_frame().expect("a frame shown");
    let (x, y, w, h) = (37, 11, 64, 9);
    let (rw, rh, region) = frame.read(x, y, w, h).expect("a region");
    assert_eq!((rw, rh), (w, h));
    for row in 0..h as usize {
        let at = ((y as usize + row) * width as usize + x as usize) * 4;
        assert_eq!(
            &region[row * w as usize * 4..][..w as usize * 4],
            &whole[at..at + w as usize * 4],
            "row {row}"
        );
    }
    // Past the right and bottom edges: clipped.
    let (cw, ch, _) = frame
        .read(width - 5, height - 3, 64, 64)
        .expect("a clipped region");
    assert_eq!((cw, ch), (5, 3));
    assert!(
        frame.read(width, 0, 4, 4).is_none(),
        "a region outside the frame"
    );
}

/// At the display's full rate frames are tiles too: no video layer (its
/// CoreMedia thread never starts), one tile shown at a time, and at rest one
/// tile left.
#[cfg(target_os = "macos")]
fn full_rate_frames_show_as_tiles_without_a_video_layer() {
    let mut window = Window::new(1);
    window.gpu.set_full_rate(true);
    for live in 2..12 {
        window.live.set(live);
        window.show();
        // SAFETY: the host's live sublayer array, read on the main thread.
        let classes: Vec<String> = unsafe { window.host.sublayers() }
            .map(|subs| {
                subs.iter()
                    .map(|sub| sub.class().name().to_str().unwrap_or("?").to_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            classes
                .iter()
                .all(|class| !class.contains("AVSampleBufferDisplayLayer")),
            "frame {live}: a video layer at full rate ({classes:?})"
        );
        let tiles = tiles(&window.host);
        assert_eq!(
            tiles.iter().filter(|(_, hidden)| !hidden).count(),
            1,
            "frame {live}: one tile shows ({tiles:?})"
        );
        assert!(tiles.len() <= 3, "frame {live}: {} tiles", tiles.len());
    }
    window.gpu.set_full_rate(false);
    window.gpu.trim();
    let tiles = tiles(&window.host);
    assert_eq!(tiles.len(), 1, "at rest: one tile ({tiles:?})");
}

/// The host's sublayers' class names (the plain layer, a drawable layer).
#[cfg(target_os = "macos")]
fn layer_classes(host: &objc2_quartz_core::CALayer) -> Vec<String> {
    // SAFETY: the host's live sublayer array, read on the main thread.
    unsafe { host.sublayers() }
        .map(|subs| {
            subs.iter()
                .map(|sub| sub.class().name().to_str().unwrap_or("?").to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Full-rate frames drawn into `CAMetalLayer` drawables show what drawing
/// the scene whole shows, each drawable brought up to date from the frame
/// before around what changed; out of full rate frames are tiles again, and
/// at rest on a tile the drawables go.
#[cfg(target_os = "macos")]
fn full_rate_frames_through_drawables_match_drawing_whole() {
    let mut window = Window::new(1);
    window.gpu.debug_present_drawable(true);
    window.show();
    // Frames show on the plain layer while a resize settles (500 ms).
    std::thread::sleep(std::time::Duration::from_millis(600));
    window.gpu.set_full_rate(true);
    for live in 2..14 {
        window.live.set(live);
        assert!(
            window.show() == Window::new(live).show(),
            "drawable frame {live}: the frame shown differs from the scene drawn whole"
        );
    }
    assert!(
        layer_classes(&window.host)
            .iter()
            .any(|class| class.contains("CAMetalLayer")),
        "no drawable layer at full rate ({:?})",
        layer_classes(&window.host)
    );
    // The renderer's own surfaces went (the drawables hold the frames), and
    // their tiles with them.
    // SAFETY: the layers' live sublayer arrays, read on the main thread.
    let shown_tiles = unsafe { window.host.sublayers() }
        .into_iter()
        .flat_map(|subs| subs.to_vec())
        .filter_map(|sub| unsafe { sub.sublayers() })
        .flat_map(|tiles| tiles.to_vec())
        .filter(|tile| !tile.isHidden())
        .count();
    assert_eq!(shown_tiles, 0, "a tile shows under the drawables");
    window.gpu.set_full_rate(false);
    for live in 14..17 {
        window.live.set(live);
        assert!(
            window.show() == Window::new(live).show(),
            "tile frame {live} after drawables differs from the scene drawn whole"
        );
    }
    assert_eq!(
        tiles(&window.host)
            .iter()
            .filter(|(_, hidden)| !hidden)
            .count(),
        1,
        "one tile shows again"
    );
    window.gpu.trim();
    assert!(
        layer_classes(&window.host)
            .iter()
            .all(|class| !class.contains("CAMetalLayer")),
        "the drawables outlast rest ({:?})",
        layer_classes(&window.host)
    );
}

/// Switching between tiles and drawables, either way, shows the frame the
/// scene draws whole at every switch: the first frame on the other path
/// starts from a surface (or drawable) holding an older frame, brought up to
/// date around what it draws. So does a switch after the trim released the
/// surfaces, a trim while drawables show, and a resize mid-animation (whose
/// settle shows frames on tiles, then drawables again).
/// Whether a drawable layer (a CAMetalLayer among the host's sublayers)
/// shows.
#[cfg(target_os = "macos")]
fn drawables_show(host: &objc2_quartz_core::CALayer) -> bool {
    // SAFETY: the host's live sublayer array, read on the main thread.
    unsafe { host.sublayers() }.is_some_and(|subs| {
        subs.iter().any(|sub| {
            let class = sub.class().name().to_str().unwrap_or("").to_owned();
            class.contains("CAMetalLayer") && !sub.isHidden()
        })
    })
}

#[cfg(target_os = "macos")]
fn switches_between_tiles_and_drawables_match_drawing_whole() {
    let mut window = Window::new(1);
    window.gpu.debug_present_drawable(true);
    window.show();
    // Frames show on the plain layer while a resize settles (500 ms).
    std::thread::sleep(std::time::Duration::from_millis(600));
    let mut live = 2;
    let mut check = |window: &mut Window, size: (f32, f32), what: &str| {
        window.live.set(live);
        assert!(
            window.show() == Window::sized(live, size).show(),
            "{what}, frame {live}: the frame shown differs from the scene drawn whole"
        );
        live += 1;
    };
    for round in 0..3 {
        window.gpu.set_full_rate(true);
        for frame in 0..3 {
            check(
                &mut window,
                SIZE,
                &format!("round {round}, drawable {frame}"),
            );
            assert!(
                drawables_show(&window.host),
                "round {round}: no drawables at full rate"
            );
        }
        window.gpu.set_full_rate(false);
        for frame in 0..3 {
            check(&mut window, SIZE, &format!("round {round}, tile {frame}"));
            assert!(
                !drawables_show(&window.host),
                "round {round}: drawables over the tiles"
            );
        }
    }
    // At rest on a tile the trim drops the surfaces and the drawables: the
    // next drawables start with nothing to bring up to date from.
    window.gpu.trim();
    window.gpu.set_full_rate(true);
    for frame in 0..3 {
        check(
            &mut window,
            SIZE,
            &format!("drawable {frame} after the trim"),
        );
        assert!(drawables_show(&window.host), "no drawables after the trim");
    }
    // A trim while drawables show.
    window.gpu.trim();
    for frame in 0..2 {
        check(
            &mut window,
            SIZE,
            &format!("drawable {frame} after a trim on drawables"),
        );
    }
    window.gpu.set_full_rate(false);
    check(&mut window, SIZE, "tile after a trim on drawables");
    // A resize mid-animation: frames go to tiles while it settles, then back
    // to drawables.
    window.gpu.set_full_rate(true);
    check(&mut window, SIZE, "drawable before the resize");
    let grown = (SIZE.0 + 160., SIZE.1 + 100.);
    {
        use objc2_core_foundation::{CGPoint, CGRect, CGSize};
        use objc2_quartz_core::CATransaction;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        window.host.setFrame(CGRect::new(
            CGPoint::new(0., 0.),
            CGSize::new(grown.0 as f64, grown.1 as f64),
        ));
        CATransaction::commit();
    }
    window
        .gpu
        .resize((grown.0 * 2.) as u32, (grown.1 * 2.) as u32);
    window.ui.scene.borrow_mut().resize(grown.0, grown.1);
    let settle = std::time::Instant::now();
    let mut frame = 0;
    while settle.elapsed() < std::time::Duration::from_millis(800) {
        check(
            &mut window,
            grown,
            &format!("frame {frame} after the resize"),
        );
        frame += 1;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Input after a window idled (`trim`) wakes the GPU once (`warm_up`): not
/// before it idles, not for later input, and again after the next idle; the
/// frames drawn around it show as before.
#[cfg(target_os = "macos")]
fn input_after_idle_wakes_the_gpu_once() {
    let mut window = Window::new(1);
    window.show();
    window.gpu.warm_up();
    assert_eq!(window.gpu.debug_warms(), 0, "warmed before idling");
    window.gpu.trim();
    window.gpu.warm_up();
    window.gpu.warm_up();
    assert_eq!(window.gpu.debug_warms(), 1, "one input after idling");
    window.live.set(2);
    assert!(
        window.show() == Window::new(2).show(),
        "the frame after a warm-up differs from the scene drawn whole"
    );
    window.gpu.warm_up();
    assert_eq!(window.gpu.debug_warms(), 1, "warmed while drawing");
    window.gpu.trim();
    window.gpu.warm_up();
    assert_eq!(window.gpu.debug_warms(), 2, "the next idle");
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        scattered_damage_presented_through_surfaces_matches_drawing_whole();
        println!("layer_frames: scattered damage through surfaces matches drawing whole");
        plain_frames_flip_tiles_whose_contents_are_set_once();
        println!("layer_frames: plain frames flip tiles whose contents are set once");
        tiles_fill_the_plain_layer_exactly_after_the_host_grows();
        println!("layer_frames: tiles fill the plain layer exactly after the host grows");
        shown_frame_regions_are_the_whole_frames_rows();
        println!("layer_frames: shown frame regions are the whole frame's rows");
        full_rate_frames_show_as_tiles_without_a_video_layer();
        println!("layer_frames: full-rate frames show as tiles without a video layer");
        input_after_idle_wakes_the_gpu_once();
        println!("layer_frames: input after idle wakes the GPU once");
        rounded_panel_scrolled_through_surfaces_matches_drawing_whole();
        println!("layer_frames: a rounded panel scrolled through surfaces matches drawing whole");
        still_over_a_scroll_repainted_where_it_is_leaves_no_ghost();
        println!("layer_frames: a still over a scroll repainted where it is leaves no ghost");
        random_scrolling_scenes_through_surfaces_match_drawing_whole();
        println!("layer_frames: random scrolling scenes through surfaces match drawing whole");
        full_rate_frames_through_drawables_match_drawing_whole();
        println!("layer_frames: full-rate frames through drawables match drawing whole");
        switches_between_tiles_and_drawables_match_drawing_whole();
        println!("layer_frames: switches between tiles and drawables match drawing whole");
        faded_scrolls_copy_their_interior_and_match_drawing_whole();
        println!("layer_frames: faded scrolls copy their interior and match drawing whole");
        rounded_corners_scrolled_by_a_copy_match_drawing_whole();
        println!("layer_frames: rounded corners scrolled by a copy match drawing whole");
    }
}

/// A scroll view fading 40 px at top and bottom over cards (light paper,
/// dark art over it: its bands drawn as one) or rows (an icon and a bar on
/// nothing: faded item by item).
#[cfg(target_os = "macos")]
fn faded(offset: Signal<f32>, cards: bool) -> View {
    let mut list = column().gap(8.).p(8.);
    for i in 0..40u8 {
        let shade = Color(40 + i % 5 * 20, 40, 90, 255);
        list = list.child(match cards {
            true => div()
                .w(300.)
                .h(60.)
                .rounded(10.)
                .bg(Color(240, 238, 232, 255))
                .child(div().w(120.).h(60.).bg(shade)),
            false => row()
                .gap(8.)
                .h(28.)
                .child(div().size(20., 20.).bg(shade))
                .child(div().w(200.).h(10.).bg(Color(200, 60, 40, 255))),
        });
    }
    overlay()
        .size(SIZE.0, SIZE.1)
        .bg(Color(47, 107, 58, 255))
        .child(
            scroll(offset)
                .w(330.)
                .h(380.)
                .left(20.)
                .top(10.)
                .fade_edges(40., 40.)
                .child(list),
        )
}
/// Rounded cards in a scroll view, unfaded.
#[cfg(target_os = "macos")]
fn rounded_cards(offset: Signal<f32>) -> View {
    let mut list = column().gap(8.).p(8.);
    for i in 0..40u8 {
        list = list.child(
            div()
                .w(300.)
                .h(60.)
                .rounded(10.)
                .border(2.)
                .border_color(Color(90, 90, 110, 255))
                .bg(Color(240, 238, 232, 255 - i % 3 * 40)),
        );
    }
    overlay()
        .size(SIZE.0, SIZE.1)
        .bg(Color(47, 107, 58, 255))
        .child(
            scroll(offset)
                .w(330.)
                .h(380.)
                .left(20.)
                .top(10.)
                .child(list),
        )
}
/// Rounded corners (and their borders) copied by a scroll, moved an odd
/// number of pixels, as drawn where they went: their antialiasing is each
/// pixel's own, not its 2×2 block's.
#[cfg(target_os = "macos")]
fn rounded_corners_scrolled_by_a_copy_match_drawing_whole() {
    let steps = [0., 3., 7., 30., 31., 90., 89., 0.];
    for scale in [1., 1.5] {
        let copies = scrolled_matches_drawing_whole_at(rounded_cards, &steps, scale);
        assert!(copies > 0, "scale {scale}: no copy");
    }
}
/// A faded scroll copies its interior and repaints its bands and what it
/// scrolls in: each frame as the scene drawn whole, at scale 1 and 1.5.
#[cfg(target_os = "macos")]
fn faded_scrolls_copy_their_interior_and_match_drawing_whole() {
    let steps = [0., 3., 7., 30., 31., 90., 400., 380., 0.];
    for scale in [1., 1.5] {
        let cards = scrolled_matches_drawing_whole_at(|o| faded(o, true), &steps, scale);
        let rows = scrolled_matches_drawing_whole_at(|o| faded(o, false), &steps, scale);
        assert!(
            cards > 0 && rows > 0,
            "scale {scale}: no copy, {cards} {rows}"
        );
    }
}
