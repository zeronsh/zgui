//! A streaming transcript described by a Luau script. `stream_rust` builds
//! the same window with Rust views; the stream itself is the same Rust code
//! in both. Compare them with `examples/stream/measure.sh`.
//! Run: cargo run --release -p zgx-script --example stream_luau
//! `ZGX_HOT_RELOAD=1` also reloads `examples/stream/app.luau` as you edit
//! it (off by default: polling the file would cost the comparison).
#[path = "stream/shared.rs"]
mod shared;

use shared::*;
use std::{path::Path, time::Duration};
use zgx_desktop::Application;
use zgx_script::{ScriptHost, Value};

const TITLE: &str = "zgx · stream · Luau";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/stream/app.luau");
    Application::new().window(window(TITLE)).run(move |cx| {
        let host = ScriptHost::new(&cx.ui.runtime, script);
        let running = host.shared("running", false);
        let tokens = host.shared("tokens", 0.);
        let live = host.shared("live", "");
        let paragraphs = host.shared("paragraphs", Value::List(Default::default()));
        let offset = host.shared("offset", 0.);
        let streamer = Streamer::default();

        let toggle = {
            let (running, tokens, live, paragraphs, offset) = (
                running.clone(),
                tokens.clone(),
                live.clone(),
                paragraphs.clone(),
                offset.clone(),
            );
            let (tasks, runtime, streamer) =
                (cx.tasks.clone(), cx.ui.runtime.clone(), streamer.clone());
            move |_: Value| {
                if running.with_untracked(Value::truthy) {
                    streamer.stop();
                    running.set(false.into());
                    return;
                }
                running.set(true.into());
                let (tokens, live, paragraphs, offset, runtime) = (
                    tokens.clone(),
                    live.clone(),
                    paragraphs.clone(),
                    offset.clone(),
                    runtime.clone(),
                );
                tasks.spawn(streamer.start(move |word, ends| {
                    runtime.batch(|| {
                        live.update(|value| {
                            let mut text = value.to_string();
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(word);
                            *value = Value::Str(text.into());
                        });
                        tokens.update(|n| *n = Value::Num(n.as_f64().unwrap_or(0.) + 1.));
                        if ends {
                            let done = live.with_untracked(Clone::clone);
                            paragraphs.update(|list| {
                                let mut items = list.list().to_vec();
                                items.push(done);
                                *list = Value::List(items.into());
                            });
                            live.set("".into());
                        }
                        // Follow the end; the scroll view clamps it.
                        offset.set(Value::Num(1e9));
                    })
                }));
            }
        };
        host.action("toggle", toggle.clone());
        host.action("clear", move |_| {
            tokens.set(Value::Num(0.));
            live.set("".into());
            paragraphs.set(Value::List(Default::default()));
        });
        if let Some(after) = autostart() {
            cx.tasks.spawn(async move {
                zgx::timer::sleep(after).await;
                toggle(Value::Nil);
            });
        }

        let _ = host.reload();
        if zgx::env::var_os("ZGX_HOT_RELOAD").is_some() {
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
        }
        cx.render(host.view());
    })
}
