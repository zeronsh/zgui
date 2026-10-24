//! anime.js's API in Luau, animated by zgx's Rust engine.
//! Run: cargo run --release -p zgx-script --example anime_showcase
//! Edit `examples/anime_showcase/app.luau` and save to reload it.
use std::{path::Path, time::Duration};
use zgx_desktop::{Application, Glass, Titlebar, WindowOptions};
use zgx_script::ScriptHost;

const TITLE: &str = "zgx · anime showcase";
const WIDTH: f64 = 760.;
const HEIGHT: f64 = 720.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/anime_showcase/app.luau");
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((520., 420.)),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            // `ui.window.titlebar_area`: where the title bar leaves the script room.
            host.window_chrome(
                cx.window_info.titlebar_area.clone(),
                cx.window_info.controls.clone(),
            );
            cx.render(host.view());
        })
}
