//! The zgx frame profiler design (pen: `zgx · Frame profiler` and its
//! popovers), built from Luau. Interactive, with sample data.
//! Run: cargo run --release -p zgx-script --example frame_profiler
//! Edit `examples/frame_profiler/*.luau` and save to reload it.
use std::{path::Path, sync::Arc, time::Duration};
use zgx_desktop::{Application, FontData, Titlebar, WindowOptions};
use zgx_script::ScriptHost;

const TITLE: &str = "zgx · Frame profiler";
const WIDTH: f64 = 1132.;
const HEIGHT: f64 = 1034.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/frame_profiler/app.luau");
    let mut app = Application::new();
    for bytes in [
        &include_bytes!("../../zgx-desktop/examples/support/fonts/GeistVariable.ttf")[..],
        include_bytes!("../../zgx-desktop/examples/support/fonts/GeistMonoVariable.ttf"),
    ] {
        app = app.font(FontData::new(Arc::<[u8]>::from(bytes))?);
    }
    app.window(WindowOptions {
        title: TITLE.into(),
        width: WIDTH,
        height: HEIGHT,
        transparent: true,
        titlebar: Titlebar::Inset,
        decorations: cfg!(target_os = "macos"),
        min_size: Some((980., 560.)),
        ..Default::default()
    })
    .run(move |cx| {
        let host = ScriptHost::new(&cx.ui.runtime, script);
        let window = cx.window.clone();
        host.action("drag_window", move |_| window.drag_window());
        if let Err(error) = host.reload() {
            eprintln!("frame_profiler: {error}");
        }
        cx.tasks.spawn(host.watch(Duration::from_millis(250)));
        // `ui.window.titlebar_area`: where the title bar leaves the script room.
        host.window_chrome(
            cx.window_info.titlebar_area.clone(),
            cx.window_info.controls.clone(),
        );
        cx.render(host.view());
    })
}
