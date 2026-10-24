//! A streaming transcript written with zgx's Rust views. `stream_luau`
//! builds the same window from a Luau script; compare the two with
//! `examples/stream/measure.sh`.
//! Run: cargo run --release -p zgx-script --example stream_rust
#[path = "stream/shared.rs"]
mod shared;

use shared::*;
use std::rc::Rc;
use zgx::compose::prelude::*;
use zgx_desktop::Application;

const TITLE: &str = "zgx · stream · Rust";
/// The strip under the traffic lights that drags the window (h = 40 in
/// the script).
const TITLEBAR: f32 = 40.;
// The same colors as stream/app.luau: a translucent tint over the blurred
// desktop, surfaces as light washes over it.
const TINT: u32 = 0x0d0d0dcc;
const WASH: u32 = 0xffffff0a;
const LINE: u32 = 0xffffff1a;
const HOVER: u32 = 0xffffff1f;
const ACCENT: u32 = 0x8b7cf6ff;
const ACCENT_HOVER: u32 = 0x9d90f8ff;
const TEXT: u32 = 0xe8e8ea;
const MUTED: u32 = 0xa9a9ae;

fn button_style(view: View, bg: u32, hover: u32) -> View {
    view.px(14.)
        .py(7.)
        .rounded(8.)
        .bg(rgba(bg))
        .hover(move |s| s.bg(rgba(hover)))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new().window(window(TITLE)).run(|cx| {
        let running = cx.ui.signal(false);
        let tokens = cx.ui.signal(0_u64);
        let live = cx.ui.signal(String::new());
        let paragraphs = cx.ui.signal(Vec::<Rc<str>>::new());
        let offset = cx.ui.signal(0_f32);
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
            move || {
                if running.with_untracked(|r| *r) {
                    streamer.stop();
                    running.set(false);
                    return;
                }
                running.set(true);
                let (tokens, live, paragraphs, offset, runtime) = (
                    tokens.clone(),
                    live.clone(),
                    paragraphs.clone(),
                    offset.clone(),
                    runtime.clone(),
                );
                tasks.spawn(streamer.start(move |word, ends| {
                    runtime.batch(|| {
                        live.update(|text| {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(word);
                        });
                        tokens.update(|n| *n += 1);
                        if ends {
                            let done: Rc<str> = live.with_untracked(|text| text.as_str().into());
                            paragraphs.update(|list| list.push(done));
                            live.set(String::new());
                        }
                        // Follow the end; the scroll view clamps it.
                        offset.set(1e9);
                    })
                }));
            }
        };
        let clear = {
            let (tokens, live, paragraphs) = (tokens.clone(), live.clone(), paragraphs.clone());
            move || {
                tokens.set(0);
                live.set(String::new());
                paragraphs.set(Vec::new());
            }
        };
        if let Some(after) = autostart() {
            let toggle = toggle.clone();
            cx.tasks.spawn(async move {
                zgx::timer::sleep(after).await;
                toggle();
            });
        }

        let label = running.clone();
        let count = tokens.clone();
        let shown = paragraphs.clone();
        cx.render(
            column()
                .w_full()
                .h_full()
                .bg(rgba(TINT))
                .text_color(rgb(TEXT))
                // The titlebar: drags the window, under the native
                // traffic lights.
                .child(
                    row()
                        .w_full()
                        .h(TITLEBAR)
                        .items_center()
                        .justify_center()
                        .window_drag()
                        .child(
                            text("Streaming")
                                .text_size(13.)
                                .font_bold()
                                .text_color(rgb(MUTED)),
                        ),
                )
                .child(
                    column()
                        .grow()
                        .flex_shrink(1.)
                        .min_h(0.)
                        .w_full()
                        .px(20.)
                        .pb(20.)
                        .gap(14.)
                        .child(
                            row()
                                .w_full()
                                .gap(8.)
                                .items_center()
                                .child(
                                    button_style(button(), ACCENT, ACCENT_HOVER)
                                        .child(text_signal(move || {
                                            if label.get() { "Stop" } else { "Start stream" }.into()
                                        }))
                                        .on_click(toggle),
                                )
                                .child(
                                    button_style(button(), WASH, HOVER)
                                        .child(text("Clear"))
                                        .on_click(clear),
                                )
                                .child(div().grow())
                                .child(
                                    text_signal(move || format!("{} tokens", count.get()))
                                        .text_color(rgb(MUTED))
                                        .text_size(13.),
                                ),
                        )
                        .child(
                            scroll(offset)
                                .grow()
                                .flex_shrink(1.)
                                .min_h(0.)
                                .w_full()
                                .rounded(12.)
                                .bg(rgba(WASH))
                                .border(1.)
                                .border_color(rgba(LINE))
                                .fade_edges(18., 18.)
                                .child(
                                    column()
                                        .w_full()
                                        .p(18.)
                                        .gap(12.)
                                        .child(
                                            keyed(
                                                move || {
                                                    (0..paragraphs.with(Vec::len))
                                                        .collect::<Vec<_>>()
                                                },
                                                move |index, _| {
                                                    let paragraph = shown.with_untracked(|list| {
                                                        list[index].to_string()
                                                    });
                                                    text(paragraph).text_wrap(true).w_full()
                                                },
                                            )
                                            .w_full()
                                            .gap(12.),
                                        )
                                        .child(
                                            text_signal(move || live.get())
                                                .text_wrap(true)
                                                .w_full(),
                                        ),
                                ),
                        ),
                ),
        );
    })
}
