//! A popup window's memory, for an outside sampler (`footprint`, `vmmap`):
//! a menu hanging past the window's bottom edge (so it shows in a popup
//! window) opens by itself, stays open, closes, and the window idles.
//! Nothing touches the user's pointer or keyboard.
//!
//! Prints `READY`, `OPEN`, `CLOSED` and `DONE`, each with the seconds since
//! launch: sample between them. `POPUP_MEMORY_HOLD` sets each span in
//! seconds (6 by default); `POPUP_MEMORY_POPOVER=1` opens a popover instead.
use std::time::{Duration, Instant};
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hold: f64 = std::env::var("POPUP_MEMORY_HOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6.);
    let started = Instant::now();
    Application::new()
        .window(WindowOptions {
            title: "Popup memory".into(),
            width: 420.,
            // Inside: tall enough for the menu below its button.
            height: if std::env::var_os("POPUP_MEMORY_INSIDE").is_some() {
                420.
            } else {
                220.
            },
            ..Default::default()
        })
        .run(move |cx| {
            let open = cx.ui.signal(false);
            let anchor = button().h(28.).w(120.).child(text("File"));
            let popup = if std::env::var_os("POPUP_MEMORY_POPOVER").is_some() {
                popover("Details", open.clone(), anchor)
                    .w(200.)
                    .h(180.)
                    .bg(rgb(0x4a3768))
            } else {
                menu("File", open.clone(), anchor)
                    .w(200.)
                    .p(5.)
                    .children((1..=8).map(|n| menu_item(format!("Item {n}")).h(24.)))
            };
            // `POPUP_MEMORY_INSIDE=1`: at the top, so the menu fits in the
            // window and shows there, with no popup window (to compare).
            let column = column().w_full().h_full().p(16.);
            let column = if std::env::var_os("POPUP_MEMORY_INSIDE").is_some() {
                column.justify_start()
            } else {
                column.justify_end()
            };
            // `POPUP_MEMORY_PAINT=1`: a box's colour changes instead of a
            // menu opening: a frame with no texture written (no new glyph).
            let tint = open.clone();
            let swatch = div().w(40.).h(40.).reactive_style(move || {
                Styles::new().bg(if tint.get() {
                    rgb(0x3d7ef7)
                } else {
                    rgb(0x202020)
                })
            });
            if std::env::var_os("POPUP_MEMORY_PAINT").is_some() {
                cx.render(column.child(swatch));
            } else {
                cx.render(column.child(popup));
            }
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                let at = || started.elapsed().as_secs_f64();
                let wait = |s: f64| zgx::timer::sleep(Duration::from_secs_f64(s));
                println!("READY {:.3}", at());
                wait(hold).await;
                open.set(true);
                wait(1.).await;
                println!("OPEN {:.3} windows={}", at(), windows());
                wait(hold).await;
                open.set(false);
                println!("CLOSED {:.3}", at());
                wait(hold).await;
                println!("DONE {:.3}", at());
                window.close();
            });
        })
}

/// How many windows the app shows (a popup window is one).
fn windows() -> usize {
    #[cfg(target_os = "macos")]
    {
        let mtm = objc2::MainThreadMarker::new().unwrap();
        objc2_app_kit::NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .filter(|w| w.isVisible())
            .count()
    }
    #[cfg(not(target_os = "macos"))]
    0
}
