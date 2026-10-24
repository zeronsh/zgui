//! Theme switches on the GPU: a token's new value repaints only what uses
//! it, the incremental frame matches a full repaint, and a scoped subtree
//! keeps its own theme; on the wgpu path and with Metal directly.
use zgx::{
    compose::prelude::*,
    scene::{Color, Rect},
    theme::{DARK, LIGHT, ThemeOverride, tok},
    widgets::Ui,
};
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 120;
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
fn rgba(Color(r, g, b, a): Color) -> [u8; 4] {
    [r, g, b, a]
}
/// Render what changed, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, ui: &Ui) -> (zgx_gpu::GpuStats, Vec<u8>) {
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    let stats = gpu.render(&ui.scene.borrow(), &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    let full = [Rect::new(0., 0., SIZE as f32, SIZE as f32)];
    gpu.render(&ui.scene.borrow(), &full).unwrap();
    assert_eq!(
        pixels,
        gpu.readback().unwrap(),
        "incremental frame differs from full"
    );
    (stats, pixels)
}

#[test]
fn theme_switches_repaint_token_nodes_on_both_paths() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        let mut ui = Ui::new(SIZE as f32, SIZE as f32);
        gpu.install_text(&mut ui.scene.borrow_mut());
        ui.mount(
            column()
                .w(SIZE as f32)
                .h(SIZE as f32)
                .bg(rgb(0x000000))
                .child(
                    div()
                        .size(40., 40.)
                        .bg(tok::SURFACE_RAISED)
                        .rounded(tok::RADIUS_SM),
                )
                .child(div().size(40., 20.).bg(rgb(0x204060)))
                .child(theme_scope(
                    ThemeOverride::new().theme(LIGHT),
                    div().size(40., 40.).bg(tok::SURFACE_RAISED),
                )),
        );
        let (_, dark) = render(&mut gpu, &ui);
        assert_eq!(
            pixel(&dark, 20, 20),
            rgba(Color(35, 46, 64, 255)),
            "direct {direct}"
        );
        assert_eq!(pixel(&dark, 20, 50), [0x20, 0x40, 0x60, 255]);
        assert_eq!(
            pixel(&dark, 20, 80),
            [255, 255, 255, 255],
            "the light scope"
        );
        ui.theme().set(LIGHT).unwrap();
        let (stats, light) = render(&mut gpu, &ui);
        assert_eq!(
            pixel(&light, 20, 20),
            [255, 255, 255, 255],
            "direct {direct}"
        );
        assert_eq!(pixel(&light, 20, 50), [0x20, 0x40, 0x60, 255], "untouched");
        assert!(
            stats.damaged_pixels <= 40 * 40,
            "only the switched box repaints: {} pixels",
            stats.damaged_pixels
        );
        ui.theme().set(DARK).unwrap();
        let (_, back) = render(&mut gpu, &ui);
        assert_eq!(back, dark, "the same frame as before");
    }
}
