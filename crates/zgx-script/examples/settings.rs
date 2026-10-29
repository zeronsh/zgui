//! Settings in Luau, as a React app would make them: a profile loaded
//! from an API, a form checked as it is typed and saved with an Undo
//! toast, and a notifications switch that saves at once, going back if
//! the server refuses. Written to find where zgx-script is harder than
//! React (docs/ROADMAP.md, "Luau app ergonomics").
//! Run: cargo run --release -p zgx-script --example settings
//! `SETTINGS_FAIL=n` fails the first `n` requests.
#[path = "support/settings_server.rs"]
mod settings_server;

use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/settings/app.luau");
    let server = settings_server::SettingsServer::new();
    if let Some(n) = zgx::env::var("SETTINGS_FAIL")
        .ok()
        .and_then(|n| n.parse().ok())
    {
        server.fail(n);
    }
    Application::new()
        .window(WindowOptions {
            title: "zgx · settings".into(),
            width: 520.,
            height: 560.,
            min_size: Some((420., 400.)),
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            server.serve(&host, Duration::from_millis(400));
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
