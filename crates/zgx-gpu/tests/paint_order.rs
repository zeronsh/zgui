//! Tree order is paint order: an absolutely positioned subtree after
//! another is drawn over it, whatever the earlier one holds (opaque panels,
//! images, transformed, blurred or masked layers), on a fresh frame and
//! after damage inside the earlier one.
use std::sync::Arc;
use zgx::{compose::prelude::*, layout::Length, scene::Color, widgets::Ui};
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
const PANEL: Color = Color(0x1c, 0x1c, 0x1a, 255);
const POPOVER: Color = Color(0, 200, 0, 255);
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
fn solid(color: [u8; 4], size: u32) -> Arc<zgx::image::ImageData> {
    let pixels: Vec<u8> = (0..size * size).flat_map(|_| color).collect();
    Arc::new(zgx::image::ImageData::new(size, size, pixels).unwrap())
}
/// What the card holds besides its opaque panel.
#[derive(Clone, Copy, Debug)]
enum Holds {
    Panel,
    Image,
    Rotated,
    Blurred,
    Masked,
}
/// A relative column holding a card (an opaque panel, its shade a signal,
/// and `holds`), then an absolute column → absolute group above it →
/// absolute popover reaching down into the card, as the composer's
/// attachment preview sits over its dock. `promoted`: the card holds what
/// `holds` asks; otherwise a plain panel of the same colour (the reference).
fn scene(
    holds: Holds,
    promoted: bool,
    shade: zgx::reactive::Signal<u8>,
    rise: zgx::reactive::Signal<f32>,
) -> View {
    let fill = Color(120, 0, 0, 255);
    let plain = || div().w(160.).h(100.).bg(fill);
    let inner = match (holds, promoted) {
        (_, false) | (Holds::Panel, true) => plain(),
        (Holds::Image, true) => image("photo", solid([120, 0, 0, 255], 8)).w(160.).h(100.),
        (Holds::Rotated, true) => plain().rotate(0.0001),
        (Holds::Blurred, true) => plain().filter_blur(0.01),
        (Holds::Masked, true) => plain().mask(solid([255; 4], 1), (160., 100.)),
    };
    let card = column()
        .p(10.)
        .bg(PANEL)
        .reactive_style(move || Styles::new().bg(Color(shade.get(), 28, 26, 255)))
        .child(inner);
    let popover = div().absolute().top(40.).w(80.).h(40.).bg(POPOVER);
    // Rising in as the composer's popovers do: scaled, lifted and faded
    // from `rise` 0 to rest at 1.
    let group = column()
        .absolute()
        .bottom(Length::Percent(1.))
        .w(100.)
        .reactive_style(move || {
            let t = rise.get();
            Styles::new()
                .opacity(0.5 + t / 2.)
                .scale(0.97 + 0.03 * t)
                .translate(0., 4. * (1. - t))
        })
        .child(popover);
    let holder = column().absolute().top(0.).left(0.).right(0.).child(group);
    column().relative().w(180.).child(card).child(holder)
}
/// Frames as the panel's shade changes: the first whole, the rest from
/// their damage.
fn frames(holds: Holds, promoted: bool, direct: bool) -> Vec<Vec<u8>> {
    let mut ui = Ui::new(SIZE as f32, SIZE as f32);
    let shade = ui.signal(28_u8);
    let rise = ui.signal(0.5_f32);
    let _view = ui.mount(scene(holds, promoted, shade.clone(), rise.clone()));
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    gpu.set_direct_metal(direct);
    let mut out = Vec::new();
    // Risen, then at rest while the card changes under it.
    for (value, risen) in [(28, 0.5), (40, 1.), (60, 1.), (90, 1.)] {
        shade.set(value);
        rise.set(risen);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        out.push(gpu.readback().unwrap());
    }
    out
}

#[test]
fn later_absolute_subtrees_paint_over_earlier_layers() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let reference = frames(Holds::Panel, false, direct);
        for holds in [
            Holds::Panel,
            Holds::Image,
            Holds::Rotated,
            Holds::Blurred,
            Holds::Masked,
        ] {
            for (frame, pixels) in frames(holds, true, direct).iter().enumerate().skip(1) {
                let case = format!("{holds:?}, direct {direct}, frame {frame}");
                // The popover over the card's content and over its panel.
                assert_eq!(pixel(pixels, 40, 60), [0, 200, 0, 255], "{case}");
                assert_eq!(pixel(pixels, 5, 60), [0, 200, 0, 255], "{case}");
                // Elsewhere, the card as the reference draws it.
                assert_eq!(
                    pixel(pixels, 120, 60),
                    pixel(&reference[frame], 120, 60),
                    "{case}"
                );
                assert_eq!(
                    pixel(pixels, 170, 60),
                    pixel(&reference[frame], 170, 60),
                    "{case}"
                );
            }
        }
    }
}
