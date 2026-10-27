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
        let host = objc2_quartz_core::CALayer::new();
        let mut gpu =
            GpuRenderer::debug_for_layer(host.clone(), (SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32)
                .unwrap();
        gpu.set_scale_factor(2.);
        let mut ui = Ui::new(SIZE.0, SIZE.1);
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
    let renderer = |host: &objc2::rc::Retained<objc2_quartz_core::CALayer>| {
        let mut gpu =
            GpuRenderer::debug_for_layer(host.clone(), (SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32)
                .unwrap();
        gpu.set_scale_factor(2.);
        gpu
    };
    let host = objc2_quartz_core::CALayer::new();
    let mut gpu = renderer(&host);
    let mut ui = Ui::new(SIZE.0, SIZE.1);
    ui.scene.borrow_mut().set_pixel_scale(2.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let offset = ui.signal(0.);
    ui.mount(panel(offset.clone()));
    let mut copies = 0;
    for at in [
        0., 3.5, 9.25, 17., 30.5, 48., 71.75, 90., 120.5, 150., 200., 260., 200.5, 150., 95.5,
        61.25, 40., 22.5, 8.75, 2., 0.,
    ] {
        offset.set(at);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        copies += gpu
            .render(&ui.scene.borrow(), &damage)
            .unwrap()
            .scroll_copies;
        gpu.present().unwrap();
        let shown = gpu.shown_pixels().expect("a surface shown").2;
        // The same scene drawn whole, by a renderer of its own.
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
            let width = (SIZE.0 * 2.) as usize;
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
    // Each step moved by whole pixels: each copied, its corners repainted.
    assert!(copies > 0, "no scroll copied");
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
    }
}
