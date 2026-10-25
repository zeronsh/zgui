//! A window whose leaves alone change (bars resizing, values changing) is
//! drawn by patching those items into its last whole frame (`render_kept`):
//! each such frame is pixel for pixel what drawing the scene afresh draws.
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, widgets::Ui};
use zgx_gpu::{GpuRenderer, text::FontData};

const GEIST: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");
const SIZE: (f32, f32) = (480., 260.);

fn view(live: Signal<u32>, mounted: Signal<u32>) -> View {
    let rows = live.clone();
    let card = move |index: u32| {
        let (value, bars, fill) = (live.clone(), live.clone(), live.clone());
        div()
            .absolute()
            .ml(8. + 116. * index as f32)
            .mt(8.)
            .size(108., 120.)
            .rounded(6.)
            .border(1.)
            .border_color(Color(40, 50, 64, 255))
            .bg(Color(22, 28, 38, 255))
            .child(
                text_signal(move || format!("{:.1} ms", 3.7 * (value.get() + index) as f32))
                    .text_size(15.)
                    .absolute()
                    .ml(8.)
                    .mt(6.)
                    .size(92., 20.),
            )
            .child(
                div()
                    .absolute()
                    .ml(8.)
                    .mt(32.)
                    .h(4.)
                    .rounded(2.)
                    .bg(Color(94, 177, 255, 255))
                    .reactive_style(move || {
                        Styles::new().w(8. + ((fill.get() * 7 + index) % 13) as f32 * 6.3)
                    }),
            )
            .children((0..12).map(move |bar| {
                let bars = bars.clone();
                div()
                    .absolute()
                    .ml(8. + 7.5 * bar as f32)
                    .w(5.)
                    .bg(Color(94, 177, 255, 255))
                    .reactive_style(move || {
                        let height = 2. + ((bars.get() + bar + index) % 9) as f32 * 7.25;
                        Styles::new().mt(110. - height).h(height)
                    })
            }))
    };
    let row = move |row: u32| {
        let (status, label, latency) = (rows.clone(), rows.clone(), rows.clone());
        div()
            .absolute()
            .ml(8.)
            .mt(140. + 22. * row as f32)
            .size(464., 22.)
            .bg(if row.is_multiple_of(2) {
                Color(22, 28, 38, 255)
            } else {
                Color(25, 32, 43, 255)
            })
            .child(
                text(format!("worker-{row:03}"))
                    .text_size(12.)
                    .absolute()
                    .ml(6.)
                    .mt(3.)
                    .size(120., 16.),
            )
            .child(
                div()
                    .absolute()
                    .ml(140.)
                    .mt(3.)
                    .size(60., 16.)
                    .rounded(8.)
                    .reactive_style(move || {
                        let down = (status.get() / 4 + row).is_multiple_of(3);
                        Styles::new().bg(if down {
                            Color(138, 34, 48, 255)
                        } else {
                            Color(29, 107, 74, 255)
                        })
                    })
                    .child(
                        text_signal(move || {
                            ["down", "healthy", "healthy"][((label.get() / 4 + row) % 3) as usize]
                                .into()
                        })
                        .text_size(10.)
                        .absolute()
                        .ml(8.)
                        .mt(1.)
                        .size(48., 14.),
                    ),
            )
            .child(
                text_signal(move || format!("{} ms", (latency.get() * 13 + row * 7) % 997))
                    .text_size(12.)
                    .absolute()
                    .ml(220.)
                    .mt(3.)
                    .size(80., 16.),
            )
    };
    div()
        .size(SIZE.0, SIZE.1)
        .bg(Color(14, 18, 24, 255))
        .text_color(Color(229, 237, 247, 255))
        .font_family("Geist")
        .children((0..4).map(card))
        .children((0..5).map(row))
        // Mounted again whenever `mounted` changes: a change no patch makes.
        .child(switch(
            move || mounted.get(),
            |which, _| {
                div()
                    .absolute()
                    .ml(8. + 40. * which as f32)
                    .mt(252.)
                    .size(100., 6.)
                    .bg(Color(255, 196, 61, 255))
            },
        ))
}

struct Window {
    ui: Ui,
    gpu: GpuRenderer,
    live: Signal<u32>,
    mounted: Signal<u32>,
}
impl Window {
    fn new(live: u32) -> Self {
        Self::mounted(live, 0)
    }
    fn mounted(live: u32, mounted: u32) -> Self {
        let gpu = GpuRenderer::new((SIZE.0 * 2.) as u32, (SIZE.1 * 2.) as u32).unwrap();
        let mut gpu = gpu;
        gpu.set_scale_factor(2.);
        FontData::new(GEIST.to_vec())
            .unwrap()
            .install(&mut gpu.text_system().borrow_mut());
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let state = ui.signal(live);
        let which = ui.signal(mounted);
        ui.mount(view(state.clone(), which.clone()));
        Window {
            ui,
            gpu,
            live: state,
            mounted: which,
        }
    }
    fn draw(&mut self) -> zgx_gpu::GpuStats {
        self.ui.prepare_frame();
        let report = self.ui.scene.borrow_mut().flush();
        self.gpu
            .render(&self.ui.scene.borrow(), &report.damage)
            .unwrap()
    }
}

#[test]
fn a_frame_patched_into_the_kept_window_is_the_frame_drawn_afresh() {
    let mut window = Window::new(1);
    window.draw();
    let mut patched = 0;
    for live in 2..14 {
        window.live.set(live);
        let stats = window.draw();
        patched += usize::from(stats.patched_items > 0);
        let mut fresh = Window::new(live);
        fresh.draw();
        assert!(
            window.gpu.readback().unwrap() == fresh.gpu.readback().unwrap(),
            "frame {live} differs from the scene drawn afresh (patched {})",
            stats.patched_items
        );
    }
    // Every frame but those where a pill (a box holding a text) changes
    // colour patches; drawn whole then, the next patches again.
    assert!(patched >= 8, "{patched} frames patched");
}

/// A window that lost its kept frame to a change no patch makes (a node
/// mounted) before patching from it keeps one again once a few frames in a
/// row repaint leaves alone: patching goes on, each frame as drawn afresh.
#[test]
fn a_window_keeps_a_frame_again_after_a_change_no_patch_makes() {
    let mut window = Window::new(1);
    window.draw();
    window.mounted.set(1);
    window.live.set(2);
    window.draw();
    let mut patched = 0;
    for live in 3..14 {
        window.live.set(live);
        let stats = window.draw();
        patched += usize::from(stats.patched_items > 0);
        let mut fresh = Window::mounted(live, 1);
        fresh.draw();
        assert!(
            window.gpu.readback().unwrap() == fresh.gpu.readback().unwrap(),
            "frame {live} differs from the scene drawn afresh (patched {})",
            stats.patched_items
        );
    }
    assert!(patched >= 5, "{patched} frames patched");
}
