//! Glass kitchen sink for manual platform testing: IME, text and displays,
//! drag and drop, cursors, accessible controls, scrolling, motion and
//! sleep/wake in one window.
//! Every interesting event lands in the on-screen log and on stdout.
//! Run: cargo run --release -p zgx-desktop --example kitchen_sink
use std::time::{Duration, Instant};
use zgx::{
    compose::prelude::*,
    cursor::Cursor,
    input::{EventPhase, InputEvent},
    reactive::Signal,
    scene::Color,
    text_layout::{FontFeatures, TextAlign},
    timer::sleep,
};
use zgx_desktop::{Application, Glass, Titlebar, WindowOptions};

#[path = "support/transcript.rs"]
mod transcript;

const TITLE: &str = "zgx kitchen sink";
const WIDTH: f64 = 1120.;
const HEIGHT: f64 = 740.;

const WHITE: u32 = 0xffffff;
const MUTED: u32 = 0xffffff8c;
const FAINT: u32 = 0xffffff14;
const HAIRLINE: u32 = 0xffffff1f;

const SECTIONS: [(&str, &str); 9] = [
    ("Input & IME", "✎"),
    ("Text & displays", "Aa"),
    ("Drag & drop", "⇄"),
    ("Cursors", "↖"),
    ("Controls", "◉"),
    ("Scrolling", "↕"),
    ("Motion", "✦"),
    ("Transcript", "☰"),
    ("Sleep & wake", "☾"),
];

/// Timestamped event log shown in the side panel and mirrored to stdout.
#[derive(Clone)]
struct Log {
    lines: Signal<Vec<String>>,
    start: Instant,
}
impl Log {
    fn push(&self, message: impl Into<String>) {
        let message = message.into();
        println!("{message}");
        let line = format!("{:>6.1}s  {message}", self.start.elapsed().as_secs_f32());
        self.lines.update(|lines| {
            lines.push(line);
            if lines.len() > 200 {
                lines.remove(0);
            }
        });
    }
}

/// Live window and display state for the display section.
#[derive(Clone)]
struct Displays {
    info: zgx_desktop::WindowInfo,
    viewport: Signal<(f32, f32)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((900., 620.)),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            // ZGX_MAX_FPS=<hz> caps the window's frame rate from the start.
            max_frame_rate: std::env::var("ZGX_MAX_FPS")
                .ok()
                .and_then(|v| v.parse().ok()),
            ..Default::default()
        })
        .run(move |cx| {
            // The default dark theme with clear hovers and a faint white
            // accent: overlay scrollbars paint their track and thumb from it.
            {
                use zgx::theme::{DARK, Theme, ThemeKind, tok};
                let clear = rgba(0xffffff00);
                cx.ui.theme().register(
                    Theme::new("kitchen-sink", ThemeKind::Dark)
                        .extending(DARK)
                        .with(tok::HOVER, clear)
                        .with(tok::BUTTON_HOVER, clear)
                        .with(tok::MENU_HOVER, clear)
                        .with(tok::SCROLLBAR_TRACK, clear)
                        .with(tok::ACCENT, rgba(0xffffff38)),
                );
                cx.ui.theme().set("kitchen-sink").unwrap();
            }
            let log = Log {
                lines: cx.ui.signal(Vec::new()),
                start: Instant::now(),
            };
            // ZGX_SECTION=<index> opens a section directly.
            let first = std::env::var("ZGX_SECTION")
                .ok()
                .and_then(|s| s.parse().ok());
            let section = cx
                .ui
                .signal(first.unwrap_or(0_usize).min(SECTIONS.len() - 1));
            let page = cx.ui.signal(0_f32);
            // ZGX_SCROLL=<points> scrolls the page once laid out, e.g. to put
            // Motion's reveal on screen for profiling.
            if let Some(offset) = std::env::var("ZGX_SCROLL")
                .ok()
                .and_then(|v| v.parse().ok())
            {
                let page = page.clone();
                cx.tasks.spawn(async move {
                    sleep(Duration::from_millis(300)).await;
                    page.set(offset);
                });
            }
            // ZGX_SCROLL_SPEED=<points per second> scrolls the page up and
            // down continuously, like a long trackpad fling, for profiling.
            if let Some(speed) = std::env::var("ZGX_SCROLL_SPEED")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                let page = page.clone();
                cx.tasks.spawn(async move {
                    let start = Instant::now();
                    loop {
                        sleep(Duration::from_millis(4)).await;
                        let travel = start.elapsed().as_secs_f32() * speed % 1200.;
                        page.set(if travel < 600. {
                            travel
                        } else {
                            1200. - travel
                        });
                    }
                });
            }
            // ZGX_CYCLE_MS=<ms> steps through every section on a timer: a
            // soak test for mounting and unmounting whole pages.
            if let Some(ms) = std::env::var("ZGX_CYCLE_MS")
                .ok()
                .and_then(|v| v.parse().ok())
            {
                let section = section.clone();
                cx.tasks.spawn(async move {
                    loop {
                        sleep(Duration::from_millis(ms)).await;
                        section.update(|s| *s = (*s + 1) % SECTIONS.len());
                    }
                });
            }
            // ZGX_TOUR=<section>:<seconds>,... visits sections in order once,
            // printing TOUR lines, e.g. "6:10,0:20" for a Motion round trip.
            if let Ok(tour) = std::env::var("ZGX_TOUR") {
                let steps: Vec<(usize, f64)> = tour
                    .split(',')
                    .filter_map(|step| {
                        let (index, seconds) = step.split_once(':')?;
                        Some((index.parse().ok()?, seconds.parse().ok()?))
                    })
                    .collect();
                let section = section.clone();
                cx.tasks.spawn(async move {
                    for (index, seconds) in steps {
                        section.set(index.min(SECTIONS.len() - 1));
                        println!("TOUR {index}");
                        sleep(Duration::from_secs_f64(seconds)).await;
                    }
                    println!("TOUR done");
                });
            }
            // ZGX_RESIZE_MS=<ms> resizes the window on a timer, like dragging
            // its edge: a soak test for per-size caches and textures.
            if let Some(ms) = std::env::var("ZGX_RESIZE_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
            {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for step in (0_u32..40).cycle() {
                        sleep(Duration::from_millis(ms)).await;
                        let grow = f64::from(step) * 6.;
                        window.set_inner_size(WIDTH + grow, HEIGHT + grow * 0.5);
                    }
                });
            }
            let displays = Displays {
                info: cx.window_info.clone(),
                viewport: cx.viewport.clone(),
            };
            let (uptime, gaps) = (cx.ui.signal(0_u64), cx.ui.signal(0_u32));
            spawn_heartbeat(&cx.tasks, log.clone(), uptime.clone(), gaps.clone());
            log.push("READY kitchen sink");

            let content = {
                let (log, displays, window) = (log.clone(), displays.clone(), cx.window.clone());
                let page = page.clone();
                switch(
                    {
                        let section = section.clone();
                        move || section.get()
                    },
                    move |index, cx| match index {
                        0 => ime_section(cx, &log),
                        1 => text_section(cx, &displays),
                        2 => drag_section(cx, &log),
                        3 => cursor_section(),
                        4 => controls_section(cx, &log),
                        5 => scrolling_section(cx, &log),
                        6 => motion_section(cx, &window),
                        7 => transcript::section(cx, &page),
                        _ => lifecycle_section(&uptime, &gaps),
                    },
                )
            };
            cx.render(
                row()
                    .w_full()
                    .h_full()
                    .items_stretch()
                    .bg(rgba(0x0a0d1440))
                    .text_color(rgb(WHITE))
                    .child(sidebar(&section, &page, &log))
                    .child(
                        // Stretched to the window height: `h_full` plus the
                        // vertical margins would overflow the bottom edge.
                        row()
                            .grow()
                            .min_w(0.)
                            .mt(8.)
                            .mr(8.)
                            .mb(8.)
                            .rounded(14.)
                            .bg(rgba(0x12161ea6))
                            .child(
                                scroll(page)
                                    .scrollbar(true)
                                    .grow()
                                    .min_w(0.)
                                    .h_full()
                                    .child(column().w_full().p(32.).child(content.w_full())),
                            )
                            .child(log_panel(&log, cx.ui.signal(0_f32))),
                    ),
            );
        })
}

fn spawn_heartbeat(
    tasks: &zgx_desktop::TaskSpawner,
    log: Log,
    uptime: Signal<u64>,
    gaps: Signal<u32>,
) {
    tasks.spawn(async move {
        let mut last = Instant::now();
        loop {
            sleep(Duration::from_secs(1)).await;
            let gap = last.elapsed();
            last = Instant::now();
            uptime.update(|n| *n += 1);
            // A one-second timer that fired late means the machine slept or
            // the event loop stalled; either is worth seeing in the log.
            if gap > Duration::from_millis(2500) {
                gaps.update(|n| *n += 1);
                log.push(format!("RESUMED after {:.1}s gap", gap.as_secs_f32()));
            }
        }
    });
}

fn sidebar(section: &Signal<usize>, page: &Signal<f32>, log: &Log) -> View {
    column()
        .w(220.)
        .h_full()
        .px(12.)
        .gap(2.)
        .child(div().w_full().h(52.).window_drag())
        .child(
            text("TESTS")
                .px(10.)
                .pb(6.)
                .text_size(10.5)
                .font_weight(600)
                .letter_spacing(1.6)
                .text_color(rgba(0xffffff66)),
        )
        .children(SECTIONS.iter().enumerate().map(|(index, (name, icon))| {
            let (selected, select) = (section.clone(), section.clone());
            let (page, log) = (page.clone(), log.clone());
            button()
                .w_full()
                .h(34.)
                .px(10.)
                .gap(10.)
                .rounded(8.)
                .items_center()
                .text_size(13.)
                .bg(rgba(0xffffff00))
                .hover(|s| s.bg(rgba(0xffffff10)))
                .active(|s| s.bg(rgba(0xffffff1c)))
                .focus(|s| s)
                .reactive_style(move || {
                    let on = selected.get() == index;
                    Styles::new()
                        .bg(rgba(if on { 0xffffff1f } else { 0xffffff00 }))
                        .text_color(rgba(if on { 0xffffffff } else { 0xffffffb8 }))
                })
                .child(
                    text(*icon)
                        .w(18.)
                        .text_size(13.)
                        .text_align(TextAlign::Center),
                )
                .child(text(*name))
                .on_click(move || {
                    if select.set(index) {
                        page.set(0.);
                        log.push(format!("SECTION {name}"));
                    }
                })
        }))
        .child(div().grow())
        .child(
            text("Everything logs to the right\nand to stdout.")
                .px(10.)
                .pb(18.)
                .text_size(11.)
                .text_color(rgba(0xffffff55)),
        )
}

fn log_panel(log: &Log, offset: Signal<f32>) -> View {
    let (lines, clear) = (log.lines.clone(), log.lines.clone());
    column()
        .w(300.)
        .h_full()
        .pt(20.)
        .gap(10.)
        .bg(rgba(0x0000001c))
        .rounded_corners(zgx::decoration::Corners {
            top_left: 0.,
            top_right: 14.,
            bottom_right: 14.,
            bottom_left: 0.,
        })
        .child(
            row()
                .w_full()
                .px(20.)
                .items_center()
                .justify_between()
                .child(eyebrow("EVENT LOG"))
                .child(pill_button("Clear").on_click(move || {
                    clear.set(Vec::new());
                })),
        )
        .child(
            scroll(offset)
                .scrollbar(true)
                .w_full()
                .grow()
                .min_h(0.)
                .child(switch(
                    move || lines.get(),
                    |lines: Vec<String>, _| {
                        column().w_full().px(20.).pb(20.).gap(5.).children(
                            lines.into_iter().rev().map(|line| {
                                text(line)
                                    .w_full()
                                    .truncate()
                                    .text_size(11.)
                                    .font_family("Menlo")
                                    .text_color(rgba(0xffffffb0))
                            }),
                        )
                    },
                )),
        )
}

// ---------------------------------------------------------------- sections

fn header(title: &str, steps: &[&str]) -> View {
    column()
        .w_full()
        .gap(10.)
        .child(
            text(title)
                .text_size(26.)
                .font_weight(600)
                .letter_spacing(-0.4),
        )
        .child(
            column()
                .w_full()
                .gap(4.)
                .children(steps.iter().enumerate().map(|(i, step)| {
                    text(format!("{}.  {step}", i + 1))
                        .w_full()
                        .text_size(12.5)
                        .text_color(rgba(MUTED))
                })),
        )
}

fn ime_section(cx: &mut Context, log: &Log) -> View {
    let editor = |label: &'static str, area: bool, log: Log| {
        let value = cx.state(String::new());
        let base = if area {
            text_area(label, value).h(120.).text_wrap(true)
        } else {
            text_input(label, value).h(44.)
        };
        column()
            .w_full()
            .gap(6.)
            .child(eyebrow(label))
            .child(glass_field(base).on_event(move |event| {
                if event.phase != EventPhase::Target {
                    return;
                }
                match &event.event {
                    InputEvent::ImePreedit { text, cursor } => {
                        log.push(format!("PREEDIT {label} {text:?} {cursor:?}"))
                    }
                    InputEvent::ImeCommit(text) => log.push(format!("COMMIT {label} {text:?}")),
                    InputEvent::Focus => log.push(format!("FOCUS {label}")),
                    InputEvent::Blur => log.push(format!("BLUR {label}")),
                    _ => {}
                }
            }))
    };
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Input & IME",
            &[
                if cfg!(windows) {
                    "Add Microsoft Pinyin in Settings › Time & language, switch with Win+Space."
                } else {
                    "Add Pinyin – Simplified in System Settings › Keyboard › Text Input, switch with 🌐."
                },
                "Type nihao + Space in the first field: candidates sit at the caret, one COMMIT.",
                "Type zhong + Escape: preedit clears, nothing commits.",
                if cfg!(windows) {
                    "Type shi, then click the second field (or Alt+Tab away and back): no stray text."
                } else {
                    "Type shi, then click the second field (or ⌘-Tab away and back): no stray text."
                },
                "Move the window and type again: the candidate window follows.",
            ],
        ))
        .child(editor("First field", false, log.clone()))
        .child(editor("Second field", false, log.clone()))
        .child(editor("Multi-line", true, log.clone()))
}

fn text_section(cx: &mut Context, displays: &Displays) -> View {
    let info = displays.info.clone();
    let viewport = displays.viewport.clone();
    let display = move || {
        let current = info.current_display.get();
        let screens = info.displays.get();
        let bounds = info.bounds.get();
        let (w, h) = viewport.get();
        let screen = current.and_then(|i| screens.iter().find(|d| d.index == i));
        format!(
            "Display {}  ·  scale {}  ·  {} attached\nWindow {}×{} pt at {}  ·  {} px",
            screen
                .and_then(|d| d.name.clone())
                .unwrap_or_else(|| "?".into()),
            screen.map_or("?".into(), |d| format!("{}×", d.scale_factor)),
            screens.len(),
            w,
            h,
            bounds
                .position
                .map_or("?".into(), |(x, y)| format!("({x}, {y})")),
            format_args!("{}×{}", bounds.size.0, bounds.size.1),
        )
    };
    let count = cx.state(0_i64);
    let (label, inc, dec) = (count.clone(), count.clone(), count.clone());
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Text & displays",
            &[
                "Switch one display to a 1× (low resolution) mode, then drag this window across.",
                "Text stays sharp at each scale; the numbers below update as you move.",
                "Unplug the display the window is on: it should move and keep drawing.",
                "Click + quickly for a while: digits must never show stray lines or boxes.",
            ],
        ))
        .child(
            glass_card().w_full().p(18.).child(
                text_signal(display)
                    .w_full()
                    .text_size(13.)
                    .line_height(22.)
                    .font_family("Menlo"),
            ),
        )
        .child(
            row()
                .w_full()
                .gap(20.)
                .items_center()
                .child(
                    text_signal(move || label.get().to_string())
                        .w(220.)
                        .text_size(96.)
                        .font_weight(200)
                        .letter_spacing(-3.)
                        .font_features(FontFeatures::new([(*b"tnum", 1)]))
                        .text_align(TextAlign::Center),
                )
                .child(
                    row()
                        .p(5.)
                        .gap(4.)
                        .items_center()
                        .rounded(30.)
                        .bg(rgba(0xffffff12))
                        .border(1.)
                        .border_color(rgba(0xffffff22))
                        .child(step_button(false).on_click(move || {
                            dec.update(|n| *n -= 1);
                        }))
                        .child(div().w(1.).h(22.).bg(rgba(0xffffff26)))
                        .child(step_button(true).on_click(move || {
                            inc.update(|n| *n += 1);
                        })),
                ),
        )
        .child(
            column()
                .w_full()
                .gap(8.)
                .children([11., 14., 18., 24.].map(|size| {
                    text(format!(
                        "{size}px  The quick brown fox jumps over the lazy dog"
                    ))
                    .w_full()
                    .truncate()
                    .text_size(size)
                }))
                .child(text("你好世界 · こんにちは · 안녕하세요 · العربية · 🙂✨").text_size(20.)),
        )
}

#[derive(Clone)]
struct Card {
    name: &'static str,
    tint: Color,
}

fn drag_section(cx: &mut Context, log: &Log) -> View {
    let cards = [
        Card {
            name: "Coral",
            tint: Color(255, 128, 110, 70),
        },
        Card {
            name: "Mint",
            tint: Color(110, 230, 180, 64),
        },
        Card {
            name: "Iris",
            tint: Color(140, 150, 255, 74),
        },
    ];
    let status = cx.state(
        if cfg!(windows) {
            "Drop a card or Explorer files here"
        } else {
            "Drop a card or Finder files here"
        }
        .to_owned(),
    );
    let hover = cx.state(false);
    let (shown, lit) = (status.clone(), hover.clone());
    let (drops, files, cancels, hovering) =
        (status.clone(), status.clone(), log.clone(), hover.clone());
    let (drop_log, file_log) = (log.clone(), log.clone());
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Drag & drop",
            &[
                "Drag a card by any corner: it lifts from exactly where you grabbed it.",
                "Drop it in the zone (accepted) and outside it (rejected); Escape mid-drag cancels.",
                if cfg!(windows) {
                    "Drag two files from Explorer into the zone, then drag one in and back out."
                } else {
                    "Drag two files from Finder into the zone, then drag one in and back out."
                },
            ],
        ))
        .child(row().gap(16.).children(cards.into_iter().map(|card| {
            let log = log.clone();
            let payload = card.clone();
            card_view(&card)
                .cursor(Cursor::Grab)
                .on_drag(move || payload.clone())
                .drag_preview(|card: &Card| card_view(card).opacity(0.9))
                .on_drag_end(move |card: &Card, accepted| {
                    log.push(format!("DRAG_END {} accepted={accepted}", card.name))
                })
        })))
        .child(
            column()
                .w_full()
                .h(170.)
                .items_center()
                .justify_center()
                .gap(8.)
                .rounded(16.)
                .border(1.5)
                .reactive_style(move || {
                    let on = lit.get();
                    Styles::new()
                        .bg(rgba(if on { 0x7cc4ff22 } else { FAINT }))
                        .border_color(rgba(if on { 0x9ad2ffcc } else { 0xffffff30 }))
                })
                .child(text("⇣").text_size(28.).text_color(rgba(MUTED)))
                .child(text_signal(move || shown.get()).text_size(14.))
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Capture {
                        return;
                    }
                    match &cx.event {
                        InputEvent::FileHover { .. } => {
                            hovering.set(true);
                        }
                        InputEvent::FileHoverCancelled => {
                            hovering.set(false);
                            cancels.push("FILE_CANCEL");
                        }
                        InputEvent::Drag(event) => {
                            use zgx::input::DragPhase;
                            match event.phase {
                                DragPhase::Over => {
                                    hovering.set(true);
                                }
                                DragPhase::Leave | DragPhase::Drop | DragPhase::End => {
                                    hovering.set(false);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                })
                .on_files_drop({
                    let hover = hover.clone();
                    move |paths, _| {
                        hover.set(false);
                        file_log.push(format!("FILE_DROP {} files {paths:?}", paths.len()));
                        files.set(format!("Dropped {} file(s)", paths.len()));
                    }
                })
                .on_drop(move |card: &Card, _| {
                    hover.set(false);
                    drop_log.push(format!("DRAG_DROP {}", card.name));
                    drops.set(format!("Dropped {}", card.name));
                }),
        )
}

fn card_view(card: &Card) -> View {
    column()
        .size(150., 96.)
        .p(16.)
        .gap(4.)
        .rounded(14.)
        .bg(card.tint)
        .border(1.)
        .border_color(rgba(0xffffff38))
        .child(text(card.name).text_size(16.).font_weight(600))
        .child(text("drag me").text_size(11.).text_color(rgba(MUTED)))
}

fn cursor_section() -> View {
    use Cursor::*;
    let cursors = [
        ("Default", Default),
        ("Pointer", Pointer),
        ("Text", Text),
        ("VerticalText", VerticalText),
        ("Crosshair", Crosshair),
        ("Move", Move),
        ("Grab", Grab),
        ("Grabbing", Grabbing),
        ("NotAllowed", NotAllowed),
        ("Wait", Wait),
        ("Progress", Progress),
        ("Help", Help),
        ("ContextMenu", ContextMenu),
        ("Copy", Copy),
        ("Alias", Alias),
        ("NoDrop", NoDrop),
        ("AllScroll", AllScroll),
        ("ColResize", ColResize),
        ("RowResize", RowResize),
        ("EwResize", EwResize),
        ("NsResize", NsResize),
        ("NeswResize", NeswResize),
        ("NwseResize", NwseResize),
        ("ZoomIn", ZoomIn),
        ("ZoomOut", ZoomOut),
    ];
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Cursors",
            &[
                "Hover each tile: the system cursor matches its name.",
                "Leave the tile: the cursor returns to the arrow immediately.",
            ],
        ))
        .child(
            // Explicit rows: a wrapping row is measured unconstrained here.
            column().gap(10.).children(cursors.chunks(4).map(|chunk| {
                row().gap(10.).children(chunk.iter().map(|&(name, cursor)| {
                    glass_card()
                        .size(118., 54.)
                        .items_center()
                        .justify_center()
                        .cursor(cursor)
                        .hover(|s| s.bg(rgba(0xffffff1e)))
                        .child(text(name).text_size(12.))
                }))
            })),
        )
}

fn controls_section(cx: &mut Context, log: &Log) -> View {
    let clicks = cx.state(0_u32);
    let (shown, count) = (clicks.clone(), clicks.clone());
    let checked = cx.state(true);
    let level = cx.state(0.4_f32);
    let menu_open = cx.state(false);
    let (click_log, check_log, menu_log) = (log.clone(), log.clone(), log.clone());
    let watched = checked.clone();
    let item = move |label: &'static str| {
        let log = menu_log.clone();
        menu_item(label)
            .h(30.)
            .px(10.)
            .rounded(6.)
            .text_size(13.)
            .bg(rgba(0xffffff00))
            .focus(|s| s.bg(rgba(0xffffff1c)))
            .hover(|s| s.bg(rgba(0xffffff14)))
            .on_click(move || log.push(format!("MENU {label}")))
    };
    let anchor = pill_button("Open menu ▾").on_click({
        let open = menu_open.clone();
        move || {
            open.set(true);
        }
    });
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Controls",
            &[
                if cfg!(windows) {
                    "Turn on Narrator (Win+Ctrl+Enter) and move with Caps Lock+→ through the controls."
                } else {
                    "Turn on VoiceOver (⌘F5) and move with Control-Option-→ through the controls."
                },
                "Each control announces its name and role; toggling announces the new value.",
                "Also try Tab / Shift-Tab and Space to operate everything by keyboard.",
            ],
        ))
        .child(
            glass_card()
                .w_full()
                .p(22.)
                .gap(18.)
                .child(
                    row()
                        .gap(12.)
                        .items_center()
                        .child(pill_button("Press me").on_click(move || {
                            count.update(|n| *n += 1);
                            click_log.push(format!("BUTTON clicks={}", count.get()));
                        }))
                        .child(pill_button("Disabled").disabled(true).opacity(0.4))
                        .child(
                            text_signal(move || format!("Pressed {} times", shown.get()))
                                .text_size(13.)
                                .text_color(rgba(MUTED)),
                        ),
                )
                .child(
                    checkbox("Enable notifications", checked)
                        .text_size(13.)
                        .focus(|s| s)
                        .on_click(move || check_log.push(format!("CHECKBOX {}", watched.get()))),
                )
                .child(
                    row()
                        .w_full()
                        .gap(16.)
                        .items_center()
                        .child(text("Level").w(60.).text_size(13.))
                        .child(slider("Level", level.clone(), 0.0..=1.0).w(200.).h(28.))
                        .child(
                            progress("Level progress", level)
                                .w(140.)
                                .h(6.)
                                .rounded(3.)
                                .bg(rgba(0xffffff1c))
                                .text_color(rgb(0x9ad2ff)),
                        ),
                )
                .child(
                    menu("Actions", menu_open, anchor)
                        .w(200.)
                        .p(6.)
                        .gap(2.)
                        .rounded(10.)
                        .bg(rgba(0x262b36f4))
                        .border(1.)
                        .border_color(rgba(HAIRLINE))
                        .children([item("Duplicate"), item("Rename"), item("Archive")]),
                ),
        )
}

fn scrolling_section(cx: &mut Context, log: &Log) -> View {
    let (list, gallery) = (cx.state(0_f32), cx.state(0_f32));
    let (list_at, gallery_at) = (list.clone(), gallery.clone());
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Scrolling",
            &[
                "Scroll this page, the list and the event log with a trackpad and a mouse wheel.",
                "Drag each overlay scrollbar: it follows the pointer and stays inside its track.",
                "Swipe the strip sideways (or Shift-scroll); vertical swipes over it scroll the page.",
                "Resize the window while scrolled: every viewport keeps its offset in range.",
            ],
        ))
        .child(
            text_signal(move || {
                format!(
                    "list {:.0} pt  ·  strip {:.0} pt",
                    list_at.get(),
                    gallery_at.get()
                )
            })
            .text_size(12.)
            .font_family("Menlo")
            .text_color(rgba(MUTED)),
        )
        .child(
            glass_card().w_full().p(6.).child(
                scroll(list)
                    .scrollbar(true)
                    .w_full()
                    .h(240.)
                    .children((1..=60).map(|i| {
                        let log = log.clone();
                        button()
                            .w_full()
                            .h(36.)
                            .px(12.)
                            .rounded(8.)
                            .items_center()
                            .text_size(13.)
                            .bg(rgba(0xffffff00))
                            .hover(|s| s.bg(rgba(0xffffff10)))
                            .active(|s| s.bg(rgba(0xffffff1c)))
                            .focus(|s| s)
                            .child(text(format!("Row {i}")))
                            .on_click(move || log.push(format!("ROW {i}")))
                    })),
            ),
        )
        .child(
            scroll_x(gallery)
                .scrollbar(true)
                .w_full()
                .h(110.)
                .gap(10.)
                .children((1..=24).map(|i| {
                    glass_card()
                        .size(140., 96.)
                        .p(16.)
                        .child(text(i.to_string()).text_size(22.).font_weight(300))
                })),
        )
}

// ---------------------------------------------------------------- motion

fn motion_section(cx: &mut Context, window: &zgx_desktop::WindowHandle) -> View {
    // Display-paced clocks drive every animation on this page. The section's
    // tasks end when it unmounts, and frames pause while the window is hidden.
    let frames = cx.frames();
    let clock = cx.state(0_f32);
    // The Wave loader asks for at most 30 frames per second on its own.
    let wave = cx.state(0_f32);
    let display_hz = cx.state(0_u32);
    let (clock_hz, wave_hz) = (cx.state(0_u32), cx.state(0_u32));
    let cap = cx.state(None::<f64>);
    // The cap belongs to the window; lift it when this page goes away.
    struct Uncap(zgx_desktop::WindowHandle);
    impl Drop for Uncap {
        fn drop(&mut self) {
            self.0.set_max_frame_rate(None);
        }
    }
    cx.retain(Uncap(window.clone()));
    for (time, measured, limit) in [(&clock, &clock_hz, None), (&wave, &wave_hz, Some(30.))] {
        let (frames, time, measured, display) = (
            frames.clone(),
            time.clone(),
            measured.clone(),
            display_hz.clone(),
        );
        cx.tasks().spawn(async move {
            let next = || match limit {
                Some(hz) => frames.next().max_rate(hz),
                None => frames.next(),
            };
            let start = next().await.time;
            let (mut since, mut count) = (start, 0_u32);
            loop {
                let frame = next().await;
                time.set((frame.time - start).as_secs_f32());
                // Rate over whole frames each second: steady unless frames drop.
                count += 1;
                let elapsed = (frame.time - since).as_secs_f64();
                if elapsed >= 1. {
                    measured.set((count as f64 / elapsed).round() as u32);
                    display.set(frame.refresh_rate().round() as u32);
                    (since, count) = (frame.time, 0);
                }
            }
        });
    }
    let readout = {
        let (display, clock_hz, wave_hz) = (display_hz.clone(), clock_hz.clone(), wave_hz.clone());
        move || {
            format!(
                "display {} Hz  ·  animating {} Hz  ·  wave {} Hz",
                display.get(),
                clock_hz.get(),
                wave_hz.get()
            )
        }
    };
    let cap_button = |label: &'static str, hz: Option<f64>| {
        let (current, choose, window) = (cap.clone(), cap.clone(), window.clone());
        pill_button(label)
            .reactive_style(move || {
                Styles::new().bg(rgba(if current.get() == hz {
                    0xffffff3a
                } else {
                    0xffffff14
                }))
            })
            .on_click(move || {
                if choose.set(hz) {
                    window.set_max_frame_rate(hz);
                }
            })
    };
    let replayed = cx.state(0_f32);
    // Profiling aid: ZGX_REPLAY_MS=<ms> replays the reveal on a timer, like spamming Replay.
    if let Some(ms) = std::env::var("ZGX_REPLAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        let (clock, replayed) = (clock.clone(), replayed.clone());
        cx.tasks().spawn(async move {
            loop {
                sleep(Duration::from_millis(ms)).await;
                replayed.set(clock.get());
            }
        });
    }
    let folded = cx.state(Tween::settled(1.));
    let bumped = cx.state(-1_f32);
    let hovers = [(); 3].map(|_| cx.state(Tween::settled(0.)));
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Motion",
            &[
                "The readout shows your display's rate (120 Hz on ProMotion) and holds steady.",
                "Cap the window to 60 or 30: motion stays even, just with fewer frames.",
                "Replay: rows grow in one after another and the card sharpens from a blur.",
                "Toggle details quickly: the fold reverses from wherever it is.",
                "Hover the tiles in and out fast: each colour fade reverses without jumping.",
                "Resize the window while everything moves: nothing stalls or tears.",
            ],
        ))
        .child(
            row()
                .w_full()
                .gap(8.)
                .items_center()
                .child(eyebrow("FRAME RATE").mr(8.))
                .child(cap_button("Display", None))
                .child(cap_button("60 Hz", Some(60.)))
                .child(cap_button("30 Hz", Some(30.))),
        )
        .child(
            text_signal(readout)
                .w_full()
                .text_size(12.)
                .font_family("Menlo")
                .text_color(rgba(MUTED)),
        )
        .child(
            row()
                .w_full()
                .gap(12.)
                .child(stage("Matrix", matrix_spinner(&clock)))
                .child(stage("Glyph", glyph_spinner(&clock)))
                .child(stage("Wave · 30 Hz", wave_loader(&wave))),
        )
        .child(
            glass_card()
                .w_full()
                .p(20.)
                .gap(10.)
                .child(eyebrow("SHIMMER"))
                .child(shimmer("Reading workspace files", &clock)),
        )
        .child(reveal(&clock, &replayed))
        .child(fold(&clock, &folded))
        .child(
            row()
                .w_full()
                .gap(12.)
                .children(
                    ["Fade", "Blend", "Lift"]
                        .into_iter()
                        .zip(hovers)
                        .map(|(name, hover)| hover_tile(name, &clock, hover)),
                )
                .child(bounce_tile(&clock, &bumped)),
        )
}

/// The clock for an animation that ends at `end`. Once it has finished the
/// clock is read without subscribing, so settled views stop re-running on
/// every frame; the signal that starts the next run still wakes them.
fn clock_until(clock: &Signal<f32>, end: f32) -> f32 {
    let now = clock.with_untracked(|now| *now);
    if now >= end { now } else { clock.get() }
}

fn ease_out(x: f32) -> f32 {
    1. - (1. - x.clamp(0., 1.)).powi(3)
}

fn ease_out_expo(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    if x >= 1. {
        1.
    } else {
        1. - 2_f32.powf(-10. * x)
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

/// Bright, fade to a floor, hold, snap back: one 750 ms loader cycle.
fn blink(phase: f32) -> f32 {
    if phase < 0.45 {
        1. - 0.9 * ease_out(phase / 0.45)
    } else if phase < 0.92 {
        0.1
    } else {
        1.
    }
}

fn stage(label: &str, loader: View) -> View {
    glass_card()
        .grow()
        .min_w(0.)
        .h(150.)
        .p(16.)
        .gap(10.)
        .child(eyebrow(label))
        .child(
            column()
                .w_full()
                .grow()
                .items_center()
                .justify_center()
                .child(loader),
        )
}

/// 3×3 dots whose brightness wave climbs toward the top-centre dot.
fn matrix_spinner(clock: &Signal<f32>) -> View {
    const ROWS: [u32; 3] = [0x6ea8ff, 0xffc56b, 0xff7eb6];
    column().gap(6.).children((0..3).map(|r| {
        row().gap(6.).children((0..3).map(move |c| {
            let clock = clock.clone();
            let delay = ((2 - r) as f32 + (1. - (c as f32 - 1.).abs()) * 0.5) * 0.12;
            div()
                .size(10., 10.)
                .rounded(5.)
                .bg(rgb(ROWS[r]))
                .reactive_style(move || {
                    Styles::new().opacity(blink((clock.get() / 0.75 - delay).rem_euclid(1.)))
                })
        }))
    }))
}

/// 2×3 dots with a bright spot chasing clockwise around the edge.
fn glyph_spinner(clock: &Signal<f32>) -> View {
    const RING: [(usize, usize); 6] = [(0, 0), (0, 1), (1, 1), (2, 1), (2, 0), (1, 0)];
    column().gap(5.).children((0..3).map(|r| {
        row().gap(5.).children((0..2).map(move |c| {
            let clock = clock.clone();
            let k = RING.iter().position(|&cell| cell == (r, c)).unwrap() as f32;
            div()
                .size(8., 8.)
                .rounded(2.)
                .bg(rgb(WHITE))
                .reactive_style(move || {
                    Styles::new().opacity(blink((clock.get() / 0.75 - k / 6.).rem_euclid(1.)))
                })
        }))
    }))
}

/// Five cells breathing on a cosine wave, each 150 ms behind the last.
fn wave_loader(clock: &Signal<f32>) -> View {
    row().gap(4.).children((0..5).map(|i| {
        let clock = clock.clone();
        // A fixed slot keeps the row still while each cell scales inside it.
        column()
            .size(16., 16.)
            .items_center()
            .justify_center()
            .child(div().rounded(4.).bg(rgb(0x9ad2ff)).reactive_style(move || {
                let t = (clock.get() - i as f32 * 0.15) / 2.4;
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * t).cos();
                let side = 14. * (0.9 + 0.1 * w);
                Styles::new().size(side, side).opacity(0.08 + 0.92 * w)
            }))
    }))
}

/// A light band sweeping across muted text every 3.4 s.
fn shimmer(label: &str, clock: &Signal<f32>) -> View {
    let chars: Vec<char> = label.chars().collect();
    let last = (chars.len() - 1).max(1) as f32;
    row().children(chars.into_iter().enumerate().map(|(i, ch)| {
        let clock = clock.clone();
        text(ch.to_string())
            .text_size(18.)
            .font_weight(500)
            .reactive_style(move || {
                let centre = -0.4 + 1.8 * (clock.get() / 3.4).rem_euclid(1.);
                let glow = smoothstep(1. - (i as f32 / last - centre).abs() / 0.36);
                Styles::new().text_color(Color(255, 255, 255, (90. + 165. * glow) as u8))
            })
    }))
}

/// Rows that grow in with a stagger, and a card that sharpens from a blur.
fn reveal(clock: &Signal<f32>, replayed: &Signal<f32>) -> View {
    const ROWS: [&str; 5] = [
        "Read crates/zgx/src/scene.rs",
        "Search “fn translate”",
        "Edit crates/zgx/src/style.rs",
        "Run cargo test -p zgx",
        "Read crates/zgx-gpu/src/lib.rs",
    ];
    let replay = {
        let (clock, replayed) = (clock.clone(), replayed.clone());
        move || {
            replayed.set(clock.get());
        }
    };
    let (card_clock, card_at) = (clock.clone(), replayed.clone());
    glass_card()
        .w_full()
        .p(20.)
        .gap(12.)
        .child(
            row()
                .w_full()
                .items_center()
                .justify_between()
                .child(eyebrow("REVEAL"))
                .child(pill_button("Replay").on_click(replay)),
        )
        .child(
            column()
                .w_full()
                .children(ROWS.iter().enumerate().map(|(i, label)| {
                    let (clock, at) = (clock.clone(), replayed.clone());
                    row()
                        .w_full()
                        .overflow_hidden()
                        .items_center()
                        .gap(10.)
                        .text_size(13.)
                        .reactive_style(move || {
                            let start = at.get() + 0.09 + 0.065 * i as f32;
                            let p =
                                ease_out_expo((clock_until(&clock, start + 0.36) - start) / 0.36);
                            Styles::new()
                                .h(30. * p)
                                .opacity(p)
                                .translate(0., (1. - p) * 4.)
                        })
                        .child(div().size(6., 6.).rounded(3.).bg(rgba(0xffffff66)))
                        .child(text(*label).text_color(rgba(0xffffffc8)))
                })),
        )
        .child(
            column()
                .w_full()
                .p(16.)
                .gap(4.)
                .rounded(12.)
                .bg(rgba(0xffffff0c))
                .isolated(true)
                .reactive_style(move || {
                    let start = card_at.get() + 0.4;
                    let p = ease_out_expo((clock_until(&card_clock, start + 0.5) - start) / 0.5);
                    Styles::new()
                        .opacity(p)
                        .blur((1. - p) * 12.)
                        .translate(0., (1. - p) * 6.)
                })
                .child(text("Done").text_size(14.).font_weight(600))
                .child(
                    text("Opacity, blur and offset ease out together over 500 ms.")
                        .text_size(12.)
                        .text_color(rgba(MUTED)),
                ),
        )
}

/// A tween toward `target` that retargets from its current value, so an
/// interrupted transition reverses smoothly instead of jumping.
#[derive(Clone, Copy, PartialEq)]
struct Tween {
    from: f32,
    target: f32,
    at: f32,
}
impl Tween {
    fn settled(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            at: f32::NEG_INFINITY,
        }
    }
    fn value(self, now: f32, seconds: f32, ease: fn(f32) -> f32) -> f32 {
        self.from + (self.target - self.from) * ease((now - self.at) / seconds)
    }
    fn toward(self, target: f32, now: f32, seconds: f32, ease: fn(f32) -> f32) -> Self {
        Self {
            from: self.value(now, seconds, ease),
            target,
            at: now,
        }
    }
}

/// A 180 ms fold of height, opacity and spacing.
fn fold(clock: &Signal<f32>, folded: &Signal<Tween>) -> View {
    let toggle = {
        let (clock, folded) = (clock.clone(), folded.clone());
        move || {
            let tween = folded.get();
            folded.set(tween.toward(1. - tween.target, clock.get(), 0.18, ease_out));
        }
    };
    let (chevron, body, now) = (folded.clone(), folded.clone(), clock.clone());
    glass_card()
        .w_full()
        .p(20.)
        .child(
            row()
                .w_full()
                .items_center()
                .justify_between()
                .child(eyebrow("FOLD"))
                .child(
                    pill_button("Toggle details")
                        .gap(6.)
                        .child(text_signal(move || {
                            if chevron.get().target > 0.5 {
                                "▾"
                            } else {
                                "▸"
                            }
                            .to_owned()
                        }))
                        .on_click(toggle),
                ),
        )
        .child(
            column()
                .w_full()
                .overflow_hidden()
                .gap(6.)
                .reactive_style(move || {
                    let tween = body.get();
                    let v = tween.value(clock_until(&now, tween.at + 0.18), 0.18, ease_out);
                    Styles::new().h(84. * v).opacity(v).mt(14. * v)
                })
                .children(
                    [
                        "Height, opacity and spacing tween together.",
                        "Content is clipped while the fold is in motion.",
                        "Rapid toggles reverse smoothly from mid-flight.",
                    ]
                    .map(|line| text(line).h(24.).text_size(13.).text_color(rgba(MUTED))),
                ),
        )
}

/// 150 ms colour and lift fade on hover, reversing from its current value.
fn hover_tile(name: &str, clock: &Signal<f32>, hover: Signal<Tween>) -> View {
    let (now, shown, target) = (clock.clone(), hover.clone(), hover);
    let events = now.clone();
    glass_card()
        .grow()
        .min_w(0.)
        .h(76.)
        .items_center()
        .justify_center()
        .reactive_style(move || {
            let tween = shown.get();
            let v = tween.value(clock_until(&now, tween.at + 0.15), 0.15, smoothstep);
            Styles::new()
                .bg(Color(255, 255, 255, (15. + 26. * v) as u8))
                .border_color(Color(255, 255, 255, (26. + 40. * v) as u8))
                .translate(0., -3. * v)
        })
        .on_event(move |event| {
            let goal = match event.event {
                InputEvent::PointerEnter => 1.,
                InputEvent::PointerLeave => 0.,
                _ => return,
            };
            let tween = target.get();
            if tween.target != goal {
                target.set(tween.toward(goal, events.get(), 0.15, smoothstep));
            }
        })
        .child(text(name).text_size(13.))
}

/// Overshoots 5 pt and settles back over 220 ms, like a pane hitting its limit.
fn bounce_tile(clock: &Signal<f32>, bumped: &Signal<f32>) -> View {
    let (now, at) = (clock.clone(), bumped.clone());
    let (tap, hit) = (clock.clone(), bumped.clone());
    glass_card()
        .grow()
        .min_w(0.)
        .h(76.)
        .items_center()
        .justify_center()
        .cursor(Cursor::Pointer)
        .reactive_style(move || {
            let at = at.get();
            let p = (clock_until(&now, at + 0.22) - at) / 0.22;
            let x = if p < 0.32 {
                smoothstep(p / 0.32)
            } else {
                1. - smoothstep((p - 0.32) / 0.68)
            };
            Styles::new().translate(5. * x, 0.)
        })
        .on_click(move || {
            hit.set(tap.get());
        })
        .child(text("Click to bump").text_size(13.))
}

fn lifecycle_section(uptime: &Signal<u64>, gaps: &Signal<u32>) -> View {
    let (up, gap) = (uptime.clone(), gaps.clone());
    let pulse = uptime.clone();
    column()
        .w_full()
        .gap(22.)
        .child(header(
            "Sleep & wake",
            &[
                "Note the uptime, then choose Apple menu › Sleep and wake the Mac.",
                "The log shows RESUMED after the gap and the counters keep ticking.",
                "The window redraws correctly and still responds to clicks and typing.",
            ],
        ))
        .child(
            row()
                .gap(16.)
                .child(stat("UPTIME", move || format!("{}s", up.get())))
                .child(stat("RESUMES", move || gap.get().to_string())),
        )
        .child(row().gap(8.).children((0..12).map(move |i| {
            let pulse = pulse.clone();
            div().size(14., 14.).rounded(7.).reactive_style(move || {
                let on = pulse.get() % 12 == i;
                Styles::new().bg(rgba(if on { 0x9ad2ffff } else { 0xffffff1c }))
            })
        })))
}

// ---------------------------------------------------------------- pieces

fn eyebrow(label: &str) -> View {
    text(label.to_uppercase())
        .text_size(10.5)
        .font_weight(600)
        .letter_spacing(1.4)
        .text_color(rgba(0xffffff70))
}

fn glass_card() -> View {
    column()
        .rounded(14.)
        .bg(rgba(FAINT))
        .border(1.)
        .border_color(rgba(HAIRLINE))
}

fn glass_field(editor: View) -> View {
    editor
        .w_full()
        .px(12.)
        .py(10.)
        .rounded(10.)
        .text_size(16.)
        .bg(rgba(0x0000002e))
        .border(1.)
        .border_color(rgba(HAIRLINE))
        .focus(|s| s.border_color(rgba(0x9ad2ffb0)).bg(rgba(0x00000040)))
}

fn pill_button(label: &str) -> View {
    button()
        .px(14.)
        .h(30.)
        .rounded(15.)
        .items_center()
        .text_size(12.5)
        .font_weight(500)
        .bg(rgba(0xffffff14))
        .border(1.)
        .border_color(rgba(0xffffff1c))
        .hover(|s| s.bg(rgba(0xffffff22)))
        .active(|s| s.bg(rgba(0xffffff30)))
        .focus(|s| s)
        .child(text(label))
}

fn stat(label: &str, value: impl FnMut() -> String + 'static) -> View {
    glass_card()
        .w(200.)
        .p(20.)
        .gap(6.)
        .child(eyebrow(label))
        .child(
            text_signal(value)
                .text_size(40.)
                .font_weight(250)
                .font_features(FontFeatures::new([(*b"tnum", 1)])),
        )
}

fn step_button(plus: bool) -> View {
    let bar = |w: f32, h: f32| div().absolute().size(w, h).rounded(1.).bg(rgb(WHITE));
    let mut icon = overlay().size(16., 16.).child(bar(16., 2.).mt(7.));
    if plus {
        icon = icon.child(bar(2., 16.).ml(7.));
    }
    button()
        .size(64., 44.)
        .rounded(22.)
        .items_center()
        .justify_center()
        .bg(rgba(0xffffff00))
        .hover(|s| s.bg(rgba(0xffffff18)))
        .active(|s| s.bg(rgba(0xffffff2c)))
        .focus(|s| s)
        .child(icon)
}
