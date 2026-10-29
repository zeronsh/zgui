//! An inbox in Luau, as a React app would make one: routing between the
//! list, a thread and settings (a modal route) with back, forward and deep
//! links (`mail://thread/42`), search debounced as you type with loading,
//! empty and error states, a virtualized list loaded page by page as you
//! scroll, folders dragged to reorder, and the signed-in user and theme as
//! context. Written to find where zgx-script was harder than React, and
//! built on what that found: ui.router, ui.paged with a list's on_end,
//! ui.debounced and ui.each's reorder.
//! Run: cargo run --release -p zgx-script --example inbox [mail://thread/42]
#[path = "support/inbox_server.rs"]
mod inbox_server;

use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/inbox/app.luau");
    let server = inbox_server::InboxServer::new();
    let links: Vec<String> = std::env::args().skip(1).collect();
    Application::new()
        .window(WindowOptions {
            title: "zgx · inbox".into(),
            width: 900.,
            height: 700.,
            min_size: Some((600., 400.)),
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            server.serve(&host, Duration::from_millis(150));
            let _ = host.reload();
            host.open(Vec::new(), links);
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
