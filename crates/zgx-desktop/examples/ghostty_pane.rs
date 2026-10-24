//! One Ghostty terminal in a frosted zgx window.
//! Run: cargo run --release -p zgx-desktop --features ghostty --example ghostty_pane
use zgx::{
    compose::prelude::*,
    terminal::{Terminal, TerminalOptions, terminal},
};
use zgx_desktop::{Application, Glass, Titlebar, WindowOptions};

const TITLE: &str = "zgx · ghostty pane";
const WIDTH: f64 = 900.;
const HEIGHT: f64 = 600.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let typed = std::env::args().nth(1);
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            let term = Terminal::new(&cx.ui.runtime, TerminalOptions::default());
            if let Some(typed) = &typed {
                term.input(format!("{typed}\r"));
            }
            term.focus();
            let title = term.clone();
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .items_stretch()
                    .bg(rgba(0x1214168c))
                    .child(
                        row().h(40.).pl(84.).items_center().child(
                            text_signal(move || {
                                let t = title.title();
                                if t.is_empty() { "Terminal".into() } else { t }
                            })
                            .text_size(13.)
                            .text_color(rgba(0xffffffa6)),
                        ),
                    )
                    .child(
                        terminal(&term)
                            .grow()
                            .mx(6.)
                            .mb(6.)
                            .rounded(10.)
                            .bg(rgba(0x0c0d10d9)),
                    ),
            );
        })
}
