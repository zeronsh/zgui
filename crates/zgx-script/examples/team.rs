//! A team directory in Luau, against an API served in memory: a list loaded
//! with `ui.resource` (loading, a failure and its retry), a modal form
//! checked as it is typed (`build` makes it new each time it opens), and a
//! confirmation before removing someone.
//! Run: cargo run --release -p zgx-script --example team
//! Edit `examples/team/app.luau` and save to reload it. `TEAM_FAIL=n` makes
//! the first `n` requests fail, to see the errors.
#[path = "support/team_server.rs"]
mod team_server;

use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/team/app.luau");
    let server = team_server::TeamServer::new(&[
        ("Ada Lovelace", "ada@example.com", "Engineer"),
        ("Grace Hopper", "grace@example.com", "Manager"),
        ("Edsger Dijkstra", "edsger@example.com", "Engineer"),
        ("Susan Kare", "susan@example.com", "Designer"),
    ]);
    if let Some(n) = zgx::env::var("TEAM_FAIL").ok().and_then(|n| n.parse().ok()) {
        server.fail(n);
    }
    Application::new()
        .window(WindowOptions {
            title: "zgx · team".into(),
            width: 720.,
            height: 640.,
            min_size: Some((420., 360.)),
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            // A server a little slow to answer, so loading shows.
            server.serve(&host, Duration::from_millis(400));
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
