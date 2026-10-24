//! Typed retained drag sources, component previews and accepted destinations.
use zgx::{compose::prelude::*, scene::Color};
use zgx_desktop::{Application, WindowOptions};
#[derive(Clone)]
struct Card {
    name: &'static str,
    color: Color,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx typed drag".into(),
            width: 560.,
            height: 400.,
            ..Default::default()
        })
        .run(|window| {
            let message = window
                .ui
                .signal("Drag a card into the target. Escape cancels.".to_owned());
            let dropped = message.clone();
            let read = message.clone();
            let files = message.clone();
            window.render(
                div()
                    .w_full()
                    .h_full()
                    .bg(Color(22, 26, 34, 255))
                    .text_color(Color(245, 245, 250, 255))
                    .child(
                        text("Typed drag and drop")
                            .absolute()
                            .left(20.)
                            .top(20.)
                            .text_size(24.),
                    )
                    .child(
                        row().absolute().left(20.).top(70.).gap(20.).children(
                            [
                                Card {
                                    name: "Red",
                                    color: Color(155, 60, 70, 255),
                                },
                                Card {
                                    name: "Green",
                                    color: Color(50, 120, 90, 255),
                                },
                                Card {
                                    name: "Blue",
                                    color: Color(55, 85, 150, 255),
                                },
                            ]
                            .into_iter()
                            .map(|card| {
                                let payload = card.clone();
                                div()
                                    .size(100., 80.)
                                    .p(12.)
                                    .bg(card.color)
                                    .rounded(8.)
                                    .child(text(card.name))
                                    .on_drag(move || payload.clone())
                                    .drag_preview(|card: &Card| {
                                        div()
                                            .size(130., 60.)
                                            .p(12.)
                                            .bg(card.color)
                                            .rounded(8.)
                                            .opacity(0.85)
                                            .child(text(card.name))
                                    })
                                    .on_drag_end(|card: &Card, accepted| {
                                        println!("DRAG_END {} {accepted}", card.name)
                                    })
                            }),
                        ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(20.)
                            .top(190.)
                            .size(340., 100.)
                            .p(20.)
                            .bg(Color(38, 49, 64, 255))
                            .rounded(8.)
                            .child(text("Drop a typed card here"))
                            .on_event(|cx| {
                                if cx.phase != zgx::input::EventPhase::Capture
                                    && matches!(
                                        cx.event,
                                        zgx::input::InputEvent::FileHoverCancelled
                                    )
                                {
                                    println!("FILE_CANCEL");
                                }
                            })
                            .on_files_drop(move |paths, _| {
                                println!("FILE_DROP {:?}", paths);
                                files.set(format!("Dropped {} files", paths.len()));
                            })
                            .on_drop(move |card: &Card, _| {
                                println!("DRAG_DROP {}", card.name);
                                dropped.set(format!("Dropped: {}", card.name));
                            }),
                    )
                    .child(
                        text_signal(move || read.get())
                            .absolute()
                            .left(20.)
                            .top(320.),
                    ),
            );
        })?;
    Ok(())
}
