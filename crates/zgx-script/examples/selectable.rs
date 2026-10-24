//! Selectable markdown in Luau: `selectable = true` on the document.
//! Run: cargo run --release -p zgx-script --example selectable
//! Edit `examples/selectable/app.luau` and save to reload it.
use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/selectable/app.luau");
    Application::new()
        .window(WindowOptions {
            title: "zgx · selectable text".into(),
            width: 680.,
            height: 720.,
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
