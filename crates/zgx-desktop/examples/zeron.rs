//! A zgx client for a running Zeron: browse sessions, read live transcripts,
//! send messages, stop a turn and switch the session's model and reasoning.
//! It speaks the engine's local IPC (see `support/zeron_rpc.rs`), the same
//! protocol the Zeron window uses, so both stay in sync.
//! Run: cargo run --release -p zgx-desktop --example zeron
//! `ZERON_IPC_PORT` selects another engine (e.g. a sandboxed `zeron headless`);
//! `ZERON_SEND=<text>` and `ZERON_MODEL=<id>` drive the composer for tests;
//! `ZERON_OPEN=<title>` preselects a chat.
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};
use zgx::{compose::prelude::*, cursor::Cursor, reactive::Signal, timer::sleep};
use zgx_desktop::{Application, Glass, GlassMaterial, Titlebar, WindowOptions};

#[path = "support/zeron_composer.rs"]
mod composer;
#[path = "support/zeron_markdown.rs"]
mod markdown;
#[path = "support/zeron_model.rs"]
mod model;
#[path = "support/zeron_rpc.rs"]
mod rpc;
#[path = "support/zeron_sidebar.rs"]
mod sidebar;
#[path = "support/zeron_theme.rs"]
#[allow(dead_code)]
mod theme;
#[path = "support/zeron_tools.rs"]
mod tools;
#[path = "support/zeron_view.rs"]
mod view;

use model::{ChatRow, Status, Transcript};
use rpc::Rpc;
use view::{Anim, MUTED, TEXT, white};

const TITLE: &str = "Zeron · zgx";
const WIDTH: f64 = 1320.;
const HEIGHT: f64 = 880.;
/// The transcript column's text width (832 less 48 px gutters), for
/// estimating the heights of rows not yet mounted.
const COLUMN: f32 = 736.;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut application = Application::new();
    for font in theme::fonts() {
        application = application.font(font);
    }
    application
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((900., 600.)),
            titlebar: Titlebar::Inset,
            // Zeron's backdrop: the colourless under-window material,
            // untinted, unsaturated and blurred at 60 pt, under the shell's
            // own 80% tint.
            glass: Some(Glass::new(GlassMaterial::UnderWindowBackground).plain()),
            ..Default::default()
        })
        .run(move |cx| {
            // Clear hovers, and Zeron's scrollbar thumb: text at 30%.
            {
                use zgx::theme::{DARK, Theme, ThemeKind, tok};
                cx.ui.theme().register(
                    Theme::new("zeron", ThemeKind::Dark)
                        .extending(DARK)
                        .with(tok::HOVER, white(0))
                        .with(tok::BUTTON_HOVER, white(0))
                        .with(tok::MENU_HOVER, white(0))
                        .with(tok::SCROLLBAR_TRACK, white(0))
                        .with(tok::ACCENT, theme::alpha(theme::TEXT, 0.3)),
                );
                cx.ui.theme().set("zeron").unwrap();
            }
            // One connection at a time; the UI below remounts on each one.
            let generation = cx.ui.signal(0_u32);
            let offline = cx.ui.signal(Some("Connecting to Zeron…".to_owned()));
            let current: Rc<RefCell<Option<Rpc>>> = Rc::default();
            {
                let (generation, offline, current) =
                    (generation.clone(), offline.clone(), current.clone());
                cx.tasks.spawn(async move {
                    loop {
                        match rpc::connect().await {
                            Ok(client) => {
                                *current.borrow_mut() = Some(client.clone());
                                offline.set(None);
                                generation.update(|g| *g += 1);
                                let reason = client.pump().await;
                                current.borrow_mut().take();
                                offline.set(Some(format!("Lost Zeron: {reason}")));
                                generation.update(|g| *g += 1);
                            }
                            Err(error) => {
                                offline.set(Some(format!(
                                    "Waiting for Zeron at {} ({error})",
                                    rpc::engine_url()
                                )));
                            }
                        }
                        sleep(Duration::from_secs(2)).await;
                    }
                });
            }
            // ZERON_PREVIEW=<markdown file>: render it offline through the
            // transcript view, for checking markdown without a live session.
            if let Ok(path) = std::env::var("ZERON_PREVIEW") {
                cx.render(
                    switch(|| (), move |_, cx| preview(cx, &path))
                        .w_full()
                        .h_full(),
                );
                return;
            }
            cx.render(
                switch(
                    move || generation.get(),
                    move |_, cx| match current.borrow().clone() {
                        Some(client) => app(cx, client),
                        None => waiting(offline.clone()),
                    },
                )
                .w_full()
                .h_full(),
            );
        })
}

fn preview(cx: &mut Context, path: &str) -> View {
    let source =
        std::fs::read_to_string(path).unwrap_or_else(|e| format!("Couldn't read {path}: {e}"));
    let transcript: model::Shared = Rc::new(RefCell::new(Transcript::new(cx.runtime())));
    let anim = Anim::new(cx);
    let tool = |id: &str, call: Value, output: &str, extra: Value| {
        let mut part = json!({ "kind": "tool", "id": id, "call": call, "resolved": true, "isError": false, "output": output });
        if let (Some(part), Some(extra)) = (part.as_object_mut(), extra.as_object()) {
            part.extend(extra.clone());
        }
        part
    };
    let frame = json!({ "reset": [
        { "id": "u", "role": "user", "createdAt": 0, "parts": [{ "kind": "text", "id": "t", "text": "Render this markdown preview." }] },
        { "id": "a", "role": "assistant", "createdAt": 0, "status": "complete", "parts": [
            { "kind": "reasoning", "id": "r", "text": "Reading the file, then checking tables, lists and code render like Zeron's." },
            tool("t1", json!({ "kind": "readFile", "path": path }), "", json!({})),
            tool("t2", json!({ "kind": "exec", "command": "cargo test -p zgx-desktop --example zeron" }), "running 1 test\ntest markdown::tests::runs_map_back_to_source ... ok\ntest result: ok. 1 passed", json!({})),
            tool("t3", json!({ "kind": "editFile", "path": "crates/zgx-desktop/examples/zeron.rs" }), "", json!({
                "diffStats": [{ "path": "zeron.rs", "additions": 3, "deletions": 1 }],
                "diff": { "path": "zeron.rs", "oldText": "fn main() {\n    run();\n}\n", "newText": "fn main() {\n    // Preview first.\n    preview();\n    run();\n}\n" }
            })),
            { "kind": "text", "id": "md", "text": source },
        ]}
    ]});
    transcript.borrow_mut().apply(&frame, anim.now());
    let ctx = view::Ctx::new(anim.clone(), None, transcript.clone(), cx.tasks());
    let blocks = transcript.borrow().blocks.clone();
    let body = {
        let (keys, previous) = (blocks.clone(), blocks.clone());
        keyed(
            move || keys.get(),
            move |block, cx| {
                let before = previous.with_untracked(|b| {
                    b.iter()
                        .position(|x| *x == block)
                        .and_then(|i| i.checked_sub(1))
                        .map(|i| b[i].clone())
                });
                view::block(cx, &ctx, &block, before)
            },
        )
        .w_full()
    };
    let offset = cx.state(0_f32);
    column()
        .w_full()
        .h_full()
        .bg(theme::GLASS)
        .font_family(theme::SANS)
        .text_color(TEXT)
        .child(div().w_full().h(38.))
        .child(
            scroll(offset)
                .scrollbar(true)
                .w_full()
                .grow()
                .flex_shrink(1.)
                .min_h(0.)
                .child(
                    column().w_full().items_center().child(
                        column()
                            .w_full()
                            .max_w(832.)
                            .px(48.)
                            .pt(20.)
                            .pb(60.)
                            .child(body),
                    ),
                ),
        )
}

fn drag_region(view: View) -> View {
    view.window_drag()
}

fn waiting(message: Signal<Option<String>>) -> View {
    column()
        .w_full()
        .h_full()
        .bg(theme::GLASS)
        .font_family(theme::SANS)
        .text_color(TEXT)
        .child(drag_region(div().w_full().h(52.)))
        .child(
            column()
                .w_full()
                .grow()
                .items_center()
                .justify_center()
                .gap(10.)
                .child(
                    text("Zeron")
                        .text_size(24.)
                        .font_weight(650)
                        .letter_spacing(-0.4),
                )
                .child(
                    text_signal(move || message.get().unwrap_or_default())
                        .text_size(12.5)
                        .text_color(MUTED),
                ),
        )
}

/// A harness's models as (id, label, reasoning levels).
type Models = Signal<Vec<(String, String, Vec<String>)>>;

/// The app's shared state for one connection.
#[derive(Clone)]
struct App {
    rpc: Rpc,
    rows: Signal<Vec<ChatRow>>,
    statuses: Signal<HashMap<String, Status>>,
    selected: Signal<Option<String>>,
    models: Rc<RefCell<HashMap<String, Models>>>,
    /// Ticks every 30 s so relative times stay fresh.
    now: Signal<i64>,
    /// App-lifetime tasks, for loads cached beyond the view that starts them.
    tasks: zgx::compose::Tasks,
    anim: Anim,
}

fn app(cx: &mut Context, rpc: Rpc) -> View {
    let state = App {
        rows: cx.state(Vec::new()),
        statuses: cx.state(HashMap::new()),
        selected: cx.state(None),
        models: Rc::default(),
        now: cx.state(model::now_seconds()),
        tasks: cx.tasks(),
        anim: Anim::new(cx),
        rpc: rpc.clone(),
    };
    // Chats are joined with project names from spaces.
    let raw_chats = Rc::new(RefCell::new(Value::Null));
    let spaces = Rc::new(RefCell::new(HashMap::new()));
    let refresh = {
        let (raw_chats, spaces, rows, selected) = (
            raw_chats.clone(),
            spaces.clone(),
            state.rows.clone(),
            state.selected.clone(),
        );
        move || {
            let joined = model::chat_rows(&raw_chats.borrow(), &spaces.borrow());
            // ZERON_OPEN=<title text> preselects a chat (for screenshots).
            let wanted = std::env::var("ZERON_OPEN").ok().map(|t| t.to_lowercase());
            let first = wanted
                .and_then(|wanted| {
                    joined
                        .iter()
                        .find(|row| row.title.to_lowercase().contains(&wanted))
                })
                .or(joined.first())
                .map(|row| row.id.clone());
            // Rows first: the chat view reads its row when it mounts.
            rows.set(joined);
            if selected.with_untracked(Option::is_none) && first.is_some() {
                selected.set(first);
            }
        }
    };
    let watch_chats = {
        let (raw_chats, refresh) = (raw_chats.clone(), refresh.clone());
        rpc.watch("WatchChats", json!({}), move |chats| {
            *raw_chats.borrow_mut() = chats;
            refresh();
        })
    };
    let watch_spaces = rpc.watch("WatchSpaces", json!({}), move |value| {
        *spaces.borrow_mut() = model::spaces(&value);
        refresh();
    });
    let watch_sessions = {
        let statuses = state.statuses.clone();
        rpc.watch("WatchSessions", json!({}), move |value| {
            statuses.set(model::statuses(&value));
        })
    };
    cx.retain((watch_chats, watch_spaces, watch_sessions));
    {
        let now = state.now.clone();
        cx.tasks().spawn(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                now.set(model::now_seconds());
            }
        });
    }
    let content = {
        let state = state.clone();
        switch(
            {
                let selected = state.selected.clone();
                move || selected.get()
            },
            move |selected, cx| match selected {
                Some(id) => chat_view(cx, &state, id),
                None => column()
                    .w_full()
                    .h_full()
                    .items_center()
                    .justify_center()
                    .child(text("No sessions yet").text_size(13.).text_color(MUTED)),
            },
        )
        .w_full()
        .h_full()
    };
    // As in Zeron the transcript sits on the window's glass; ours lets a
    // little more of the desktop through.
    row()
        .w_full()
        .h_full()
        .items_stretch()
        .bg(theme::GLASS)
        .font_family(theme::SANS)
        .text_size(14.)
        .text_color(TEXT)
        .child(sidebar::sidebar(cx, &state))
        // Shrinkable (zgx's flex items keep their content size by default):
        // the transcript's rows, laid out at the last width, would otherwise
        // hold the pane at it when the window narrows.
        .child(column().grow().flex_shrink(1.).min_w(0.).child(content))
}

// Chat -----------------------------------------------------------------------

fn chat_view(cx: &mut Context, state: &App, id: String) -> View {
    let anim = state.anim.clone();
    let transcript: model::Shared = Rc::new(RefCell::new(Transcript::new(cx.runtime())));
    let (blocks, loaded, streaming) = {
        let t = transcript.borrow();
        (t.blocks.clone(), t.loaded.clone(), t.streaming.clone())
    };
    let page = cx.state(0_f32);
    // Resting at the end, the list stays there as rows are measured and
    // messages arrive.
    let heights = VariableHeights::new(&cx.runtime(), 0, 80.).anchor_end();
    let follow = Follow::new(cx, page.clone(), heights.clone());
    // The bottom stack's height (status strip, composer, footer): the
    // transcript scrolls on under it, fading over it, with its last row
    // padded clear of it (Zeron's `bottom_clearance`).
    let stack = cx.state(STACK);
    // ZERON_SCROLL_SPEED=<points/s>: sweep the transcript up and down, like a
    // long trackpad fling, for profiling.
    if let Some(speed) = std::env::var("ZERON_SCROLL_SPEED")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
    {
        let (page, frames) = (page.clone(), cx.frames());
        cx.tasks().spawn(async move {
            sleep(Duration::from_secs(2)).await;
            let start = std::time::Instant::now();
            loop {
                frames.next().await;
                let travel = start.elapsed().as_secs_f32() * speed % 4000.;
                page.set(if travel < 2000. {
                    2000. - travel
                } else {
                    travel - 2000.
                });
            }
        });
    }
    let ctx = view::Ctx::new(
        anim.clone(),
        Some(state.rpc.clone()),
        transcript.clone(),
        state.tasks.clone(),
    );
    // The transcript watch; a desync remounts it for a fresh snapshot.
    let resync = cx.state(0_u32);
    let subscription = {
        let (rpc, transcript, id, resync, follow, anim) = (
            state.rpc.clone(),
            transcript.clone(),
            id.clone(),
            resync.clone(),
            follow.clone(),
            anim.clone(),
        );
        switch(
            {
                let resync = resync.clone();
                move || resync.get()
            },
            move |_, cx| {
                let (transcript, resync, follow, anim) = (
                    transcript.clone(),
                    resync.clone(),
                    follow.clone(),
                    anim.clone(),
                );
                let runtime = cx.runtime();
                // `openingTail`: the newest 128 entries first, the full history
                // after, so long sessions paint at once (as Zeron opens them).
                let params = json!({ "chatId": id, "openingTail": true });
                let mounted = std::time::Instant::now();
                let (frames, tasks) = (cx.frames(), cx.tasks());
                let trace = std::env::var_os("ZERON_TRACE").is_some();
                // ZERON_REPLAY=<n>: re-stream this chat's last n entries
                // locally (read-only), for profiling streaming.
                let replay: Option<usize> = std::env::var("ZERON_REPLAY")
                    .ok()
                    .and_then(|n| n.parse().ok());
                let replaying = Rc::new(Cell::new(false));
                let watch = rpc.watch("WatchDocMessages", params, move |frame| {
                    if replaying.get() {
                        return;
                    }
                    let frame = match (replay, frame.get("reset").and_then(Value::as_array)) {
                        (Some(tail), Some(entries)) if frame.get("historyPending").is_none() => {
                            replaying.set(true);
                            let (reset, schedule) = replay_schedule(entries, tail);
                            let (transcript, runtime, anim, follow) = (
                                transcript.clone(),
                                runtime.clone(),
                                anim.clone(),
                                follow.clone(),
                            );
                            tasks.spawn(async move {
                                let start = std::time::Instant::now();
                                sleep(Duration::from_millis(600)).await;
                                for (at, frame) in schedule {
                                    let due = start + Duration::from_millis(600 + at);
                                    if let Some(wait) = due.checked_duration_since(std::time::Instant::now()) {
                                        sleep(wait).await;
                                    }
                                    let now = anim.now();
                                    runtime.batch(|| transcript.borrow_mut().apply(&frame, now));
                                    if frame.get("append").is_some() {
                                        anim.keep(0.45);
                                    }
                                    follow.schedule();
                                }
                                eprintln!("REPLAY done in {:?}", start.elapsed());
                            });
                            reset
                        }
                        _ => frame,
                    };
                    let first = frame.get("reset").is_some();
                    let now = anim.now();
                    let started = std::time::Instant::now();
                    // Batched: views built by these updates read the transcript.
                    let mut apply_time = std::time::Duration::ZERO;
                    let applied = runtime.batch(|| {
                        let applied = transcript.borrow_mut().apply(&frame, now);
                        apply_time = started.elapsed();
                        applied
                    });
                    if trace && first {
                        eprintln!(
                            "TRACE reset at {:?}: {} entries, model {:?}, views {:?} (tail preview: {})",
                            mounted.elapsed(),
                            frame["reset"].as_array().map_or(0, Vec::len),
                            apply_time,
                            started.elapsed() - apply_time,
                            frame.get("historyPending").is_some()
                        );
                        let (frames, mounted) = (frames.clone(), mounted);
                        tasks.spawn(async move {
                            frames.next().await;
                            frames.next().await;
                            eprintln!("TRACE painted by {:?}", mounted.elapsed());
                        });
                    }
                    if !applied {
                        resync.update(|n| *n += 1);
                        return;
                    }
                    if frame.get("append").is_some() {
                        // Keep the veil fading while text streams.
                        anim.keep(0.45);
                    }
                    if first {
                        follow.pin();
                    }
                    follow.schedule();
                });
                cx.retain(watch);
                div()
            },
        )
    };
    let chat = {
        let (rows, id) = (state.rows.clone(), id.clone());
        move || rows.with(|rows| rows.iter().find(|r| r.id == id).cloned())
    };
    let status = {
        let (statuses, id) = (state.statuses.clone(), id.clone());
        move || statuses.with(|s| s.get(&id).copied().unwrap_or(Status::Idle))
    };
    let working = {
        let (status, streaming) = (status.clone(), streaming.clone());
        move || status() == Status::Working || streaming.get()
    };
    // The transcript is virtualized: only rows near the viewport mount. The
    // rest take heights estimated from their text (prepared text measures at
    // any width by arithmetic) until they mount and are measured, so long
    // sessions open and scroll at the cost of what is on screen.
    #[derive(Clone, PartialEq, Eq, Hash)]
    enum Row {
        /// The top padding and, until the first snapshot, a skeleton.
        Head,
        Block(model::Block),
        /// The working indicator and the bottom padding.
        Tail,
    }
    let rows = {
        let blocks = blocks.clone();
        move || {
            let mut rows = vec![Row::Head];
            rows.extend(blocks.get().into_iter().map(Row::Block));
            rows.push(Row::Tail);
            rows
        }
    };
    // Heights of rows not yet mounted: text measured exactly, as laid out.
    let estimate = {
        let (ctx, measure, stack) = (ctx.clone(), cx.text_measure(), stack.clone());
        move |rows: &[Row], index: usize| match &rows[index] {
            Row::Head => 26.,
            Row::Tail => stack.with_untracked(|h| *h) + 32.,
            Row::Block(block) => {
                let previous = match index.checked_sub(1).map(|p| &rows[p]) {
                    Some(Row::Block(previous)) => Some(previous),
                    _ => None,
                };
                view::estimate(&ctx, &measure, block, previous, COLUMN)
            }
        }
    };
    let build = {
        let (ctx, previous, anim) = (ctx.clone(), blocks.clone(), anim.clone());
        let (loaded, working, stack) = (loaded.clone(), working.clone(), stack.clone());
        // Zeron picks the working word from the chat id.
        let seed = tools::flavour_seed(&id);
        move |_, row: Row, cx: &mut Context| {
            let content = match row {
                Row::Head => {
                    // A skeleton until the first snapshot lands.
                    let loaded = loaded.clone();
                    column().w_full().pt(26.).child(
                        column()
                            .w_full()
                            .gap(12.)
                            .reactive_style(move || {
                                if loaded.get() {
                                    Styles::new().hidden()
                                } else {
                                    Styles::new().flex()
                                }
                            })
                            .children(
                                [72_f32, 90., 55., 80.]
                                    .map(|w| div().w_percent(w).h(12.).rounded(6.).bg(white(12))),
                            ),
                    )
                }
                Row::Block(block) => {
                    let before = previous.with_untracked(|b| {
                        b.iter()
                            .position(|x| *x == block)
                            .and_then(|i| i.checked_sub(1))
                            .map(|i| b[i].clone())
                    });
                    view::block(cx, &ctx, &block, before)
                }
                // The working trailer, then clearance for the bottom stack
                // and its fade (Zeron: clearance + fade band + 8).
                Row::Tail => {
                    let stack = stack.clone();
                    column()
                        .w_full()
                        .reactive_style(move || Styles::new().pb(stack.get() + 32.))
                        .child(tools::trailer(cx, &anim, working.clone(), seed))
                }
            };
            column()
                .w_full()
                .items_center()
                .child(column().w_full().max_w(832.).px(48.).child(content))
        }
    };
    // Zeron's column: 736 wide with 48 px gutters, the first row 64 px from
    // the window top (38 of titlebar, then 26). The scroller runs on under
    // the bottom stack: text dissolves over 24 px under the titlebar and
    // across the composer (Zeron's `edge_faded`, squared ramp, per pixel),
    // so it reads through the frosted pill.
    // The transcript's scrollbar, drawn over everything at the end (see
    // `viewport`), reads where the list is from these.
    let (bar_offset, bar_heights) = (page.clone(), heights.clone());
    let scroller = {
        let stack = stack.clone();
        measured_rows(page.clone(), heights, 2, rows, estimate, build)
            .w_full()
            .h_full()
            // Messages select and copy as a chat's do on the web.
            .selectable(true)
            // Faded out over the soft edge (`edge`), down to the window's
            // bottom, as its blur grows.
            .reactive_style(move || Styles::new().fade_edges(24., stack.get() - 24. + EDGE_LEAD))
    };
    // Reports the scroller's height for the distance from the bottom.
    let probe = {
        let viewport = follow.viewport.clone();
        canvas(move |(_, height)| {
            viewport.set(height);
            zgx::canvas::Canvas::new()
        })
        .absolute()
        .left(0.)
        .top(0.)
        .w_full()
        .h_full()
    };
    let (dock, pill) = composer::dock(cx, state, &id, working, chat.clone());
    {
        // Status strip plus the composer dock, following the pill's morph.
        let stack = stack.clone();
        let effect = cx.runtime().effect(move || {
            stack.set(24. + composer::stack_height(pill.get()));
        });
        cx.retain(effect);
    }
    let bottom = column()
        .absolute()
        .left(0.)
        .bottom(0.)
        .w_full()
        .items_center()
        // Zeron's status strip: empty unless a send is in flight.
        .child(div().w_full().h(24.).shrink_0())
        .child(
            overlay()
                .w_full()
                .max_w(COMPOSER_WIDTH)
                .child(dock)
                .child(jump_pill(cx, &anim, &follow)),
        );
    // A soft scroll edge under the bottom stack: from EDGE_LEAD above the
    // composer down to the window's bottom, the transcript's blur grows from
    // nothing as the text fades out (`scroller`), so it dissolves gradually
    // on its way under the composer, as Tern's does; no tint, so no band
    // shows. The pills keep frosts of their own.
    let edge = {
        let reach = stack.clone();
        // Its fade spans the whole band; the other edges overhang the chat
        // pane by as much, clipped, so only the top's shows.
        let band = div()
            .absolute()
            .top(0.)
            .blur(EDGE_BLUR)
            .reactive_style(move || {
                let ramp = reach.get() - 24. + EDGE_LEAD;
                Styles::new()
                    .left(-ramp)
                    .right(-ramp)
                    .bottom(-ramp)
                    .edge_fade(ramp)
            });
        let stack = stack.clone();
        div()
            .absolute()
            .left(0.)
            .right(0.)
            .bottom(0.)
            .overflow_hidden()
            .reactive_style(move || Styles::new().h(stack.get() - 24. + EDGE_LEAD))
            .child(band)
    };
    // Last, so the frost and the pills never cover it.
    let bar = scrollbar(bar_offset, move || bar_heights.content_height())
        .absolute()
        .top(0.)
        .right(2.)
        .bottom(0.)
        .w(8.);
    let viewport = overlay()
        .w_full()
        .grow()
        .flex_shrink(1.)
        .min_h(0.)
        .child(scroller)
        .child(probe)
        .child(edge)
        .child(bottom)
        .child(bar);
    column()
        .w_full()
        .h_full()
        .child(subscription)
        .child(titlebar(chat.clone()))
        .child(viewport)
}

/// The soft scroll edge's blur, reached at the window's bottom, and how far
/// above the composer's top it starts.
const EDGE_BLUR: f32 = 16.;
const EDGE_LEAD: f32 = 24.;
/// The docked composer's width: the 736 column and 16 px each side.
const COMPOSER_WIDTH: f32 = 768.;
/// The bottom stack before the composer reports: status strip 24, a
/// single-line pill and its dock.
const STACK: f32 = 24. + 49. + 40.;

/// Zeron's "Scroll to bottom" pill: 6 px above the composer, frosted (radius
/// 15, blur 16) over the popup tint with a hairline border and a hover wash.
/// It enters with `dialog_in` (180 ms, rising 2 px) and leaves at once.
fn jump_pill(cx: &mut Context, anim: &Anim, follow: &Rc<Follow>) -> View {
    let hover = theme::Hover::new(cx, anim);
    let entered = cx.state(theme::Tween::settled(0.));
    {
        let (shown, entered, anim) = (follow.shown.clone(), entered.clone(), anim.clone());
        let effect = cx.runtime().effect(move || {
            if shown.get() {
                anim.retarget(&entered, 1., theme::DIALOG_IN);
            } else {
                entered.set(theme::Tween::settled(0.));
            }
        });
        cx.retain(effect);
    }
    let pill = {
        let (follow, hover2, anim) = (follow.clone(), hover.clone(), anim.clone());
        row()
            .h(30.)
            .pl(11.)
            .pr(13.)
            .gap(6.)
            .items_center()
            .rounded(15.)
            .blur(16.)
            .border(1.)
            .border_color(theme::BORDER)
            .cursor(Cursor::Pointer)
            .text_size(13.)
            .reactive_style(move || {
                let t = anim.tween(&entered, theme::DIALOG_IN);
                Styles::new()
                    .bg(theme::mix(
                        theme::SURFACE_TINT,
                        theme::GLASS_HOVER,
                        hover2.get(),
                    ))
                    .opacity(t)
                    .translate(0., 2. * (1. - t))
            })
            .on_click(move || follow.jump())
            .child(text("↓").text_color(theme::TEXT_MUTED))
            .child(text("Scroll to bottom").text_color(TEXT))
    };
    let shown = follow.shown.clone();
    row()
        .absolute()
        .left(0.)
        .top(-36.)
        .w_full()
        .pr(10.)
        .justify_center()
        .reactive_style(move || {
            if shown.get() {
                Styles::new().flex()
            } else {
                Styles::new().hidden()
            }
        })
        .child(hover.track(pill))
}

/// A reset holding all but the last `tail` entries, then those entries
/// streamed back: each appears empty, gains its parts one by one, and text
/// arrives six characters every 16 ms (about 375 characters a second).
fn replay_schedule(entries: &[Value], tail: usize) -> (Value, Vec<(u64, Value)>) {
    let split = entries.len().saturating_sub(tail);
    let reset = json!({ "reset": entries[..split] });
    let mut schedule = Vec::new();
    let mut at = 0_u64;
    let mut after = entries[..split].last().and_then(|e| e.get("id")).cloned();
    for entry in &entries[split..] {
        let id = entry.get("id").cloned().unwrap_or(Value::Null);
        let parts = entry
            .get("parts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut shown: Vec<Value> = Vec::new();
        let with_parts = |shown: &[Value]| {
            let mut entry = entry.clone();
            entry["parts"] = json!(shown);
            entry
        };
        schedule.push((
            at,
            json!({ "upsert": [{ "entry": with_parts(&shown), "after": after }] }),
        ));
        at += 250;
        for part in parts {
            let text = part.get("text").and_then(Value::as_str).map(str::to_owned);
            let mut empty = part.clone();
            if text.is_some() {
                empty["text"] = json!("");
            }
            shown.push(empty);
            schedule.push((at, json!({ "upsert": [{ "entry": with_parts(&shown) }] })));
            at += if text.is_some() { 60 } else { 350 };
            if let Some(text) = text {
                let chars: Vec<char> = text.chars().collect();
                for chunk in chars.chunks(6) {
                    let chunk: String = chunk.iter().collect();
                    schedule.push((
                        at,
                        json!({ "append": [{ "entry": id, "part": part.get("id"), "text": chunk }] }),
                    ));
                    at += 16;
                }
                shown.last_mut().unwrap()["text"] = json!(text);
            }
        }
        after = Some(id);
    }
    (reset, schedule)
}

/// Zeron's session title row (`render_session_title_bar`): 38 px with the
/// row centred on the traffic lights, 16 px in from the sidebar, the
/// harness mark and the session title. The rest of the strip drags.
fn titlebar(chat: impl Fn() -> Option<ChatRow> + Clone + 'static) -> View {
    let (title_of, harness_of) = (chat.clone(), chat);
    let mark = switch(
        move || harness_of().map(|c| c.harness()),
        |harness, _| match harness {
            Some(harness) => theme::harness_icon(&harness, 14., theme::TEXT_MUTED),
            None => div(),
        },
    );
    drag_region(
        row()
            .w_full()
            .h(38.)
            .pt(4.)
            .px(16.)
            .gap(6.)
            .shrink_0()
            .items_center()
            .child(mark)
            .child(
                text_signal(move || title_of().map(|c| c.title).unwrap_or_default())
                    .truncate()
                    .min_w(0.)
                    .text_size(12.)
                    .font_weight(500)
                    .text_color(theme::alpha(TEXT, 0.85)),
            ),
    )
}

/// Zeron's stick-to-bottom (`transcript.rs`): pinned readers follow new
/// content; scrolling away from the bottom escapes, and scrolling back
/// within 70 px (or to it) pins again. The jump pill shows past 320 px and
/// hides at the bottom. Jumps wait a frame: offsets clamp to laid-out
/// content, so a jump made before new content is laid out would stop short.
struct Follow {
    offset: Signal<f32>,
    heights: VariableHeights,
    /// The scroller's height, reported by a probe in its overlay.
    viewport: Signal<f32>,
    pinned: Signal<bool>,
    /// The jump pill, with Zeron's hysteresis.
    shown: Signal<bool>,
    /// The last distance from the bottom, for the escape / re-pin rules.
    previous: Cell<f32>,
    scheduled: Cell<bool>,
    gliding: Cell<bool>,
    tasks: zgx::compose::Tasks,
    frames: zgx::frame::FrameClock,
}

/// Zeron's `STICK_THRESHOLD_PX`, `SCROLL_BUTTON_THRESHOLD_PX`, `AT_BOTTOM_PX`.
const STICK: f32 = 70.;
const JUMP_SHOWS: f32 = 320.;
const AT_BOTTOM: f32 = 2.;

impl Follow {
    fn new(cx: &mut Context, offset: Signal<f32>, heights: VariableHeights) -> Rc<Self> {
        let follow = Rc::new(Self {
            offset: offset.clone(),
            heights,
            viewport: cx.state(0.),
            pinned: cx.state(true),
            shown: cx.state(false),
            previous: Cell::new(0.),
            scheduled: Cell::new(false),
            gliding: Cell::new(false),
            tasks: cx.tasks(),
            frames: cx.frames(),
        });
        // Reader scrolls (offset changes) escape or re-pin; growth alone
        // never does, so the content is read untracked.
        let weak = Rc::downgrade(&follow);
        let runtime = cx.runtime();
        let effect = cx.runtime().effect(move || {
            let current = offset.get();
            // The jump's sentinel, before layout clamps it to the bottom.
            if current >= 1e8 {
                return;
            }
            let Some(follow) = weak.upgrade() else {
                return;
            };
            let distance = runtime.untracked(|| follow.distance());
            let previous = follow.previous.replace(distance);
            if follow.gliding.get() {
                return;
            }
            if distance > previous + 1. && distance > AT_BOTTOM {
                follow.pinned.set(false);
            } else if distance <= AT_BOTTOM || (distance <= STICK && distance < previous) {
                follow.pinned.set(true);
            }
        });
        cx.retain(effect);
        let weak = Rc::downgrade(&follow);
        let effect = cx.runtime().effect(move || {
            let Some(follow) = weak.upgrade() else {
                return;
            };
            follow.offset.get();
            let distance = follow.distance();
            let pinned = follow.pinned.get();
            let was = follow.shown.with_untracked(|s| *s);
            let show = !pinned
                && if was {
                    distance > AT_BOTTOM
                } else {
                    distance > JUMP_SHOWS
                };
            follow.shown.set(show);
        });
        cx.retain(effect);
        follow
    }
    /// Pixels between the viewport's bottom and the content's.
    fn distance(&self) -> f32 {
        let offset = self.offset.get();
        if offset >= 1e8 {
            return 0.;
        }
        (self.heights.content_height() - self.viewport.get() - offset).max(0.)
    }
    fn pin(&self) {
        self.pinned.set(true);
    }
    /// After the next layout, jump to the bottom if pinned.
    fn schedule(self: &Rc<Self>) {
        if self.scheduled.replace(true) {
            return;
        }
        let follow = self.clone();
        self.tasks.spawn(async move {
            follow.frames.next().await;
            follow.scheduled.set(false);
            if follow.pinned.with_untracked(|p| *p) && !follow.gliding.get() {
                follow.offset.set(1e9);
            }
        });
    }
    /// The jump pill: pin, teleport to within 2.5 viewports, then Zeron's
    /// critically damped `StickSpring` the rest of the way (it never
    /// overshoots), landing glued to the end.
    fn jump(self: &Rc<Self>) {
        self.pinned.set(true);
        if self.gliding.replace(true) {
            return;
        }
        let follow = self.clone();
        self.tasks.spawn(async move {
            let mut velocity = 0_f32;
            let mut last = None::<std::time::Instant>;
            loop {
                let frame = follow.frames.next().await;
                let (content, viewport, mut position) = (
                    follow.heights.content_height(),
                    follow.viewport.with_untracked(|v| *v),
                    follow.offset.with_untracked(|o| *o),
                );
                let target = (content - viewport).max(0.);
                position = position.min(target);
                if target - position > 2.5 * viewport {
                    position = target - 2.5 * viewport;
                }
                let frames = last.map_or(1., |last: std::time::Instant| {
                    (frame.time.saturating_duration_since(last).as_secs_f32() * 60.).min(8.)
                });
                last = Some(frame.time);
                let mut left = frames;
                while left > 0. {
                    let h = left.min(1.);
                    let diff = (target - position).max(0.);
                    velocity += h * ((0.7 * velocity + 0.05 * diff) / 1.25 - velocity);
                    position = (position + velocity * h).min(target);
                    left -= h;
                }
                if target - position <= 0.5 {
                    follow.gliding.set(false);
                    follow.previous.set(0.);
                    follow.offset.set(1e9);
                    break;
                }
                follow.offset.set(position);
            }
        });
    }
}
