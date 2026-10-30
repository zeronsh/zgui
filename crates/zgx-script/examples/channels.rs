//! A team's chat in Luau (examples/channels), against a server in memory:
//! a WebSocket that reconnects on its own, messages sent at once and
//! confirmed or refused, unread counts and presence, a list that stays at
//! its end and loads earlier pages at its start, mentions at the caret,
//! emoji, uploads with progress, and notifications in the background.
//! Run: cargo run --release -p zgx-script --example channels
//! Edit `examples/channels/*.luau` and save to reload. `CHANNELS_FAIL=n`
//! refuses the first `n` sends; `CHANNELS_DROP=s` drops the connection
//! every `s` seconds, to see the banner and the reconnect.
#[path = "support/channels_server.rs"]
mod channels_server;

use channels_server::ChannelsServer;
use std::{path::Path, time::Duration};
use zgx_desktop::{Application, WindowOptions};
use zgx_script::ScriptHost;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/channels/app.luau");
    let server = ChannelsServer::new();
    if let Some(n) = zgx::env::var("CHANNELS_FAIL")
        .ok()
        .and_then(|n| n.parse().ok())
    {
        server.fail(n);
    }
    let drop_every = zgx::env::var("CHANNELS_DROP")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    Application::new()
        .window(WindowOptions {
            title: "zgx · channels".into(),
            width: 1100.,
            height: 760.,
            min_size: Some((560., 480.)),
            ..Default::default()
        })
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script);
            server.serve(&host);
            // `ui.window.active` and `ui.window.shown`.
            host.window_state(&cx.ui);
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            // The rest of the team: someone types, then says something,
            // every few seconds; someone comes and goes now and then.
            let team = server.clone();
            cx.tasks.spawn(async move {
                let people = ["u-ada", "u-grace", "u-alan", "u-susan"];
                let channels = ["general", "general", "design", "releases"];
                let mut n = 0_usize;
                loop {
                    n += 1;
                    zgx::timer::sleep(Duration::from_millis(3500 + (n % 5) as u64 * 900)).await;
                    let (who, channel) =
                        (people[n % people.len()], channels[(n / 2) % channels.len()]);
                    team.typing(who, channel);
                    zgx::timer::sleep(Duration::from_millis(1500)).await;
                    team.say(who, channel, ChannelsServer::line(n * 7 + 3));
                    if n % 6 == 5 {
                        team.set_online("u-alan", n % 12 == 5);
                    }
                }
            });
            if let Some(seconds) = drop_every {
                let dropping = server.clone();
                cx.tasks.spawn(async move {
                    loop {
                        zgx::timer::sleep(Duration::from_secs(seconds)).await;
                        dropping.drop_all();
                    }
                });
            }
            cx.render(host.view());
        })
}
