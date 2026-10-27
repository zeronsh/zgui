//! A window's zoom, as a browser's page zoom (Command or Control with plus,
//! minus and 0): wrapping text in a scroll view under a zoomable panel
//! (`View::zoomable`), the zoom, the panel's, the viewport in points and a
//! text's height in pixels printed as they change (`ZOOM`).
//! On macOS it sends its own window ⌘=, ⌘=, ⌘- and ⌘0, as the keyboard
//! would (`scripts/macos_zoom_probe.py`); `scripts/zoom_smoke.py` presses
//! the keys on X11. `--idle 1.5` zooms once and then does nothing, for
//! measuring what an idle zoomed window costs.
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx zoom".into(),
            width: 480.,
            height: 360.,
            ..Default::default()
        })
        .run(|cx| {
            let offset = cx.ui.signal(0_f32);
            let panel = cx.ui.signal(1_f32);
            let paragraphs: Vec<View> = (0..40)
                .map(|i| {
                    text(format!(
                        "Paragraph {i}: {}",
                        "words that wrap at the window's width ".repeat(4)
                    ))
                    .id(format!("p{i}"))
                    .text_wrap(true)
                })
                .collect();
            let view = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .child(
                        row()
                            .id("panel")
                            .zoomable(panel.clone())
                            .p(8.)
                            .child(text("A zoomable panel").shrink_0()),
                    )
                    .child(
                        scroll(offset)
                            .w_full()
                            .grow()
                            .child(column().w_full().p(12.).gap(8.).children(paragraphs)),
                    ),
            );
            // `--idle 1.5`: zoomed once, then nothing runs (what an idle
            // window costs at a zoom).
            let idle = std::env::args()
                .skip_while(|arg| arg != "--idle")
                .nth(1)
                .and_then(|zoom| zoom.parse::<f64>().ok());
            if let Some(zoom) = idle {
                cx.ui.set_zoom(zoom);
                return;
            }
            let ui = cx.ui.downgrade();
            let first = view.find("p0").unwrap();
            cx.tasks.spawn(async move {
                let mut last = String::new();
                loop {
                    let Some(ui) = ui.upgrade() else { return };
                    let line = {
                        let scene = ui.scene.borrow();
                        let root = scene.bounds(scene.root());
                        let size = match scene.kind(first) {
                            zgx::scene::NodeKind::Text { font_size, .. } => *font_size,
                            _ => 0.,
                        };
                        format!(
                            "ZOOM zoom={:.2} panel={:.2} viewport={:.0}x{:.0} text_points={size:.1}",
                            ui.zoom(),
                            panel.get(),
                            root.width,
                            root.height
                        )
                    };
                    if line != last {
                        println!("{line}");
                        last = line;
                    }
                    drop(ui);
                    zgx::timer::sleep(std::time::Duration::from_millis(50)).await;
                }
            });
            #[cfg(target_os = "macos")]
            if std::env::args().any(|arg| arg == "--drive") {
                cx.tasks.spawn(async {
                    use std::time::Duration;
                    zgx::timer::sleep(Duration::from_millis(1500)).await;
                    for (key, code) in [("=", 24), ("=", 24), ("-", 27), ("0", 29)] {
                        keys::command(key, code);
                        zgx::timer::sleep(Duration::from_millis(600)).await;
                    }
                    println!("ZOOM_DONE");
                    std::process::exit(0);
                });
            }
        })
}

#[cfg(target_os = "macos")]
mod keys {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSString};
    /// ⌘ with `characters` (key code `code`), pressed and released, sent to
    /// this app's key window as AppKit delivers a key: the user's keyboard
    /// and other apps hear nothing.
    pub fn command(characters: &str, code: u16) {
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        let Some(window) = app.keyWindow() else {
            return;
        };
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0., 0.),
                NSEventModifierFlags::Command,
                0.,
                window.windowNumber(),
                None,
                &NSString::from_str(characters),
                &NSString::from_str(characters),
                false,
                code,
            )
            .unwrap();
            app.sendEvent(&event);
        }
    }
}
