//! Selectable display text, as on a web page: one `.selectable(true)` and
//! the pointer selects across headings, paragraphs, list items and inline
//! spans; double and triple presses select words and paragraphs, Shift
//! extends, ⌘A / Ctrl+A selects all and ⌘C / Ctrl+C copies, as does the
//! context menu on selected text. Links and buttons inside still click,
//! and a drag from one selects. A drag held past the transcript's edge
//! scrolls it, and the selection outlives the rows it recycles.
use zgui::{compose::prelude::*, scene::Color, theme::tok};
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgui selectable text".into(),
            width: 640.,
            height: 700.,
            ..Default::default()
        })
        .run(move |cx| {
            let clicks = cx.ui.signal(0_u32);
            let transcript = cx.ui.signal(0_f32);
            let (count, link) = (clicks.clone(), clicks.clone());
            let item = |text: &str| {
                row()
                    .gap(8.)
                    .child(text_span_label("•"))
                    .child(zgui::compose::text(text).flex_grow(1.).text_wrap(true))
            };
            let view = cx.render(
                column()
                    .id("document")
                    .w_full()
                    .h_full()
                    .p(28.)
                    .gap(14.)
                    .text_size(15.)
                    .line_height(22.)
                    .selectable(true)
                    .child(
                        text("Selectable text")
                            .id("heading")
                            .text_size(26.)
                            .line_height(32.)
                            .font_bold(),
                    )
                    .child(
                        rich_text()
                            .id("intro")
                            .w_full()
                            .text_wrap(true)
                            .child(text_span("Drag across this paragraph and the ones below: "))
                            .child(text_span("one selection").font_bold())
                            .child(text_span(" runs through every text node in document order, "))
                            .child(text_span("inline spans").italic(true))
                            .child(text_span(" included. "))
                            .child(
                                text_span("This link")
                                    .text_color(Color(90, 160, 255, 255))
                                    .on_click(move || {
                                        link.update(|n| *n += 1);
                                    }),
                            )
                            .child(text_span(" still clicks.")),
                    )
                    .child(
                        column()
                            .gap(4.)
                            .child(item("Double-press a word, triple-press a paragraph."))
                            .child(item("Keep dragging after either to extend by words or paragraphs."))
                            .child(item("Shift-press extends from the anchor; ⌘A / Ctrl+A selects all.")),
                    )
                    .child(
                        row()
                            .gap(12.)
                            .items_center()
                            .child(
                                button()
                                    .id("button")
                                    .child("Press me")
                                    .on_click(move || {
                                        count.update(|n| *n += 1);
                                    }),
                            )
                            .child(text_signal(move || format!("Pressed {} times", clicks.get()))),
                    )
                    .child(
                        text("Text in a part marked unselectable is left out of copies.")
                            .text_color(tok::TEXT_TERTIARY)
                            .selectable(false),
                    )
                    .child(
                        text("Copy (⌘C / Ctrl+C) puts plain text on the clipboard: a line break between lines, a blank line between paragraphs.")
                            .id("last")
                            .w_full()
                            .text_wrap(true),
                    )
                    .child(
                        virtual_list(
                            transcript,
                            22.,
                            4,
                            || 10_000,
                            |index| index,
                            |_, line, _| {
                                row()
                                    .gap(12.)
                                    .child(text(format!("{:05}", line + 1)).w(52.).text_color(tok::TEXT_TERTIARY))
                                    .child(text(format!("A transcript line in a virtual list of 10,000 (line {})", line + 1)))
                            },
                        )
                        .id("transcript")
                        .w_full()
                        .h(176.)
                        .scrollbar(true),
                    ),
            );
            // Start with part of it selected (for screenshots).
            if std::env::args().any(|arg| arg == "--preselect") {
                let ui = cx.ui.downgrade();
                let (intro, last) = (view.find("intro").unwrap(), view.find("last").unwrap());
                cx.tasks.spawn(async move {
                    zgui::timer::sleep(std::time::Duration::from_millis(300)).await;
                    if let Some(ui) = ui.upgrade() {
                        let paragraph = ui.scene.borrow().children(intro)[0];
                        let anchor = zgui::selection::TextPoint {
                            node: paragraph,
                            offset: 5,
                        };
                        let focus = zgui::selection::TextPoint {
                            node: last,
                            offset: 40,
                        };
                        ui.select_text(anchor, focus);
                    }
                });
            }
            if smoke {
                let ui = cx.ui.downgrade();
                let window = cx.window.clone();
                let find = |id: &str| view.find(id).unwrap();
                let (heading, intro, last) = (find("heading"), find("intro"), find("last"));
                let transcript = find("transcript");
                cx.tasks.spawn(async move {
                    for _ in 0..400 {
                        let Some(ui) = ui.upgrade() else { return };
                        {
                            let scene = ui.scene.borrow();
                            let bounds = |id| {
                                let b: zgui::scene::Rect = scene.bounds(id);
                                format!("{:.1},{:.1},{:.1},{:.1}", b.x, b.y, b.width, b.height)
                            };
                            println!(
                                "SELECTABLE heading={} intro={} last={} transcript={} text={:?}",
                                bounds(heading),
                                bounds(intro),
                                bounds(last),
                                bounds(transcript),
                                ui.selected_text(),
                            );
                        }
                        zgui::timer::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}

fn text_span_label(text: &str) -> View {
    zgui::compose::text(text).text_color(tok::TEXT_TERTIARY)
}
