//! Selectable markdown in Luau: `selectable = true` on the document.
//! Run: cargo run --release -p zgui-script --example selectable
//! Edit `examples/selectable/app.luau` and save to reload it.
use std::{path::Path, time::Duration};
use zgui_desktop::{Application, WindowOptions};
use zgui_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/selectable/app.luau");
    Application::new()
        .window(WindowOptions {
            title: "zgui · selectable text".into(),
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
