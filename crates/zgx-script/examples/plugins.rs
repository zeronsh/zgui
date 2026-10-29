//! Tasks, extended by two plugins without slots: one adds a Focus button
//! beside a hook the app declares and filters the list; the other replaces
//! the empty state and, trusted with it, the app's row module, wrapping the
//! original with a badge from a package the app shares (`kit`). The host
//! keeps the tasks in Rust and decides what plugins see and may do
//! (`examples/support/plugins_host.rs`). See docs/plugins.md.
//! Run: cargo run --release -p zgx-script --example plugins
//! Edit the app or a plugin and save to reload it.
#[path = "support/plugins_host.rs"]
mod plugins_host;

use std::time::Duration;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx · plugins".into(),
            width: 560.,
            height: 480.,
            min_size: Some((360., 300.)),
            ..Default::default()
        })
        .run(move |cx| {
            let host = plugins_host::host(&cx.ui.runtime);
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            cx.render(host.view());
        })
}
