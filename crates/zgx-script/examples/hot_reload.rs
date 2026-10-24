//! A zgx window described by Luau scripts, reloaded as you edit them.
//! Run: cargo run --release -p zgx-script --example hot_reload
//!
//! Edit `examples/hot_reload/app.luau` or add a `.luau` file to
//! `examples/hot_reload/plugins/` and save: the window updates within a
//! quarter second and keeps its state. Script errors appear over the window
//! while the last good interface keeps running.
use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `ZGX_SCRIPT_DIR` runs another app.luau and plugins/ folder.
    let dir = zgx::env::var_os("ZGX_SCRIPT_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hot_reload"),
        Into::into,
    );
    Application::new()
        .window(WindowOptions {
            title: "zgx · Luau hot reload".into(),
            width: 820.,
            height: 560.,
            ..Default::default()
        })
        .run(move |cx| {
            let host =
                ScriptHost::new(&cx.ui.runtime, dir.join("app.luau")).plugins(dir.join("plugins"));

            // Rust owns the data; scripts bind to it by name.
            let size = host.shared("window", "");
            let viewport = cx.viewport.clone();
            let publish = cx.ui.runtime.effect(move || {
                let (w, h) = viewport.get();
                size.set(format!("{w:.0}×{h:.0}").as_str().into());
            });
            let window = cx.window.clone();
            host.action("quit", move |_| window.close());
            // Plugins reach only what the app exposes to them.
            host.expose_shared("window", None);
            host.expose_shared("count", None);
            host.expose_action("quit", None);

            // A failed first load shows its error; fixing the file recovers.
            let _ = host.reload();
            let watch = host.watch(Duration::from_millis(250));
            cx.tasks.spawn(async move {
                let _publish = publish;
                watch.await
            });
            cx.render(host.view());
        })
}
