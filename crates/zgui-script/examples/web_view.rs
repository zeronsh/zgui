//! A web page in a Luau interface, with a frosted popover over it.
//! Run: cargo run --release -p zgui-script --example script_web_view
//! Edit `examples/web_view/app.luau` and save to reload it.
use std::{path::Path, time::Duration};
use zgui_desktop::{Application, Glass, Titlebar, WindowOptions};
use zgui_script::ScriptHost;

const TITLE: &str = "zgui · web view";
const WIDTH: f64 = 1100.;
const HEIGHT: f64 = 760.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/web_view/app.luau");
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((560., 400.)),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            // The page's corners follow the window's (`ui.shared` in the script).
            let radius = host.shared("window_radius", 10.);
            let corner = cx.window_info.corner_radius.clone();
            let follow_radius = cx.ui.runtime.effect(move || {
                let r = corner.get();
                if r > 0. {
                    radius.set(r.into());
                }
            });
            cx.on_closed(move || drop(follow_radius));
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
