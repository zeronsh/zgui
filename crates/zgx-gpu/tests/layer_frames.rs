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
    ui: Ui,
    gpu: GpuRenderer,
    live: Signal<u32>,
}
#[cfg(target_os = "macos")]
impl Window {
    fn new(live: u32) -> Self {
        let host = objc2_quartz_core::CALayer::new();
        let mut gpu =
            GpuRenderer::debug_for_layer(host, (SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32).unwrap();
        gpu.set_scale_factor(2.);
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let state = ui.signal(live);
        ui.mount(view(state.clone()));
        Window {
            ui,
            gpu,
            live: state,
        }
    }
    fn show(&mut self) -> Vec<u8> {
        self.ui.prepare_frame();
        let damage = self.ui.scene.borrow_mut().flush().damage;
        self.gpu.render(&self.ui.scene.borrow(), &damage).unwrap();
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
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        scattered_damage_presented_through_surfaces_matches_drawing_whole();
        println!("layer_frames: scattered damage through surfaces matches drawing whole");
    }
}
