//! A 3D scene in Luau (`ui.scene3d`): shapes, lights, an orbiting camera and
//! a spring-driven ring.
//! Run: cargo run --release -p zgx-script --features scene3d --example scene3d
//! Edit `examples/scene3d/app.luau` and save to reload it.
use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scene3d/app.luau");
    Application::new()
        .window(WindowOptions {
            title: "zgx · 3D".into(),
            width: 760.,
            height: 560.,
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
