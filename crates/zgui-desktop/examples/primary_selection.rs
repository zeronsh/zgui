//! X11's and Wayland's primary selection: what an editor selects becomes the
//! primary selection, and a middle click pastes it where it lands. It wears
//! the built-in themes, following the system's dark or light, accent, high
//! contrast (the Linux settings portal probe uses that).
//! `--smoke-test` reports the editors' text as it closes, after 14 s.
use std::time::Duration;
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

fn field(view: View) -> View {
    view.w(440.0).p(10.0).rounded(6.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgui primary selection".into(),
            width: 480.0,
            height: 260.0,
            ..Default::default()
        })
        .run(move |cx| {
            cx.ui
                .theme()
                .follow_system(Some((zgui::theme::DARK, zgui::theme::LIGHT)));
            let source = cx.ui.signal("select these words".to_owned());
            let target = cx.ui.signal(String::new());
            cx.render(
                column()
                    .p(20.0)
                    .gap(16.0)
                    .text_size(16.0)
                    .child(field(
                        text_input("Source", source.clone()).id("source").h(44.0),
                    ))
                    .child(field(
                        text_area("Target", target.clone()).id("target").h(96.0),
                    )),
            );
            cx.on_closed(move || {
                println!(
                    "PRIMARY source={:?} target={:?}",
                    source.get(),
                    target.get()
                );
            });
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgui::timer::sleep(Duration::from_secs(14)).await;
                    window.close();
                });
                println!("READY");
            }
        })
}
