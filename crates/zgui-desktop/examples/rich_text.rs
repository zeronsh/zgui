//! Continuous inline typography; this is display text, not a rich-text editor.
use zgui::{compose::prelude::*, scene::Color};
use zgui_desktop::{Application, FontData, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .font(FontData::new(std::sync::Arc::<[u8]>::from(
            &include_bytes!("../../../assets/DejaVuSans.ttf")[..],
        ))?)
        .window(WindowOptions {
            title: "zgui rich text".into(),
            width: 640.,
            height: 440.,
            ..Default::default()
        })
        .run(move |cx| {
            let count = cx.ui.signal(0_u32);
            let read = count.clone();
            let write = count.clone();
            let link_write = count.clone();
            let view = cx.render(
                column()
                    .items_start()
                    .w_full()
                    .h_full()
                    .p(24.)
                    .gap(16.)
                    .text_size(20.)
                    .text_color(Color(230, 235, 245, 255))
                    .child(
                        rich_text()
                            .w_full()
                            .text_wrap(true)
                            .child(text_span("One paragraph, "))
                            .child(
                                text_span("mixed styles")
                                    .font_weight(700)
                                    .text_color(Color(90, 190, 255, 255))
                                    .background(Color(30, 50, 80, 255))
                                    .underline(Decoration::new(1.5)),
                            )
                            .child(text_span(" — العربية, 日本語 and "))
                            .child(
                                text_span("italic text.")
                                    .italic(true)
                                    .text_size(26.)
                                    .strikethrough(Decoration::new(1.)),
                            ),
                    )
                    .child(
                        rich_text().id("link").child(
                            text_span("Activate this inline link")
                                .text_color(Color(90, 190, 255, 255))
                                .on_click(move || {
                                    link_write.update(|v| *v += 1);
                                    println!("RICH_LINK count={}", link_write.get());
                                }),
                        ),
                    )
                    .child(rich_text_signal(move || {
                        vec![
                            text_span("Reactive inline count: "),
                            text_span(read.get().to_string())
                                .font_weight(700)
                                .text_color(Color(100, 230, 140, 255)),
                        ]
                    }))
                    .child(rich_text().id("clamp").w_full().text_wrap(true).line_clamp(2).text_overflow(TextOverflow::Ellipsis)
                        .child(text_span("This paragraph is limited to two visual lines. ").font_bold())
                        .child(text_span("The original accessible text stays complete while the displayed text is shaped with a real ellipsis. Unicode remains intact: العربية 日本語 👩‍💻. More content continues beyond the visible limit.")))
                    .child(button().p(10.).child("Increment").on_click(move || {
                        write.update(|v| *v += 1);
                    })),
            );
            if smoke {
                let window = cx.window.clone();
                let scene = cx.ui.scene.clone();
                let link = view.find("link").unwrap();
                let clamp = view.find("clamp").unwrap();
                cx.tasks.spawn(async move {
                    zgui::timer::sleep(std::time::Duration::from_secs(1)).await;
                    let bounds = scene.borrow().bounds(link);
                    println!("RICH_CLAMP height={}", scene.borrow().bounds(clamp).height);
                    let paragraph=scene.borrow().children(clamp)[0];
                    println!("RICH_CLAMP_PARAGRAPH {:?} {:?}", scene.borrow().bounds(paragraph), scene.borrow().kind(paragraph));
                    println!(
                        "RICH_BOUNDS x={} y={} width={} height={}",
                        bounds.x, bounds.y, bounds.width, bounds.height
                    );
                    zgui::timer::sleep(std::time::Duration::from_secs(7)).await;
                    window.close();
                });
            }
        })
}
