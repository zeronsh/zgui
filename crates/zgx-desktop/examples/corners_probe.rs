//! Checks popups and windows at their corners (macOS), in this process's
//! own windows: a menu open as its window moves (it goes with it), as it
//! minimizes (it closes), on another display (it opens there), and the
//! Window menu with a dozen windows. `--fullscreen` also takes the window
//! into fullscreen with a menu open (it closes), which switches the screen
//! to the window's new Space: run it when the screen is yours to take.
use zgx::{compose::prelude::*, reactive::Signal};
use zgx_desktop::{Application, WindowBounds, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Corners".into(),
            width: 420.,
            height: 260.,
            bounds: Some(WindowBounds::new(840, 520).at(400, 300)),
            ..Default::default()
        })
        .run(move |cx| {
            let open: Signal<bool> = cx.ui.signal(false);
            let items = (1..=6).map(|n| menu_item(format!("Item {n}")).h(24.));
            cx.render(
                column().p(16.).child(
                    menu(
                        "File",
                        open.clone(),
                        button().h(28.).w(120.).child(text("File")),
                    )
                    .w(200.)
                    .p(5.)
                    .children(items),
                ),
            );
            #[cfg(target_os = "macos")]
            {
                let windows = cx.windows.clone();
                cx.tasks.spawn(async move {
                    macos::run(open, windows).await;
                    std::process::exit(0);
                });
            }
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{ClassType, MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSPanel, NSWindow};
    use objc2_foundation::NSObjectProtocol;
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgx::{compose::prelude::*, reactive::Signal};
    use zgx_desktop::{WindowFactory, WindowOptions};

    async fn sleep(ms: u64) {
        zgx::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn app() -> Retained<NSApplication> {
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap())
    }
    fn window(title: &str) -> Retained<NSWindow> {
        app()
            .windows()
            .iter()
            .find(|window| window.title().to_string() == title)
            .unwrap()
    }
    /// The visible popup panel, if one shows.
    fn panel() -> Option<Retained<NSWindow>> {
        app()
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.isKindOfClass(NSPanel::class()))
    }
    async fn open_menu(open: &Signal<bool>) -> Option<Retained<NSWindow>> {
        open.set(true);
        sleep(400).await;
        panel()
    }
    pub async fn run(open: Signal<bool>, windows: WindowFactory) {
        sleep(1500).await;
        let main = window("Corners");
        let fullscreen = std::env::args().any(|arg| arg == "--fullscreen");

        // Moved with a menu open: the menu goes with its window.
        let before = open_menu(&open).await.map(|panel| panel.frame().origin);
        let origin = main.frame().origin;
        main.setFrameOrigin(NSPoint::new(origin.x + 100., origin.y - 50.));
        sleep(300).await;
        let after = panel().map(|panel| panel.frame().origin);
        println!(
            "MOVED open={} panel_moved={:?}",
            open.get(),
            before.zip(after).map(|(a, b)| (b.x - a.x, b.y - a.y))
        );
        main.setFrameOrigin(origin);

        // Minimized with a menu open: it closes, as AppKit's do.
        sleep(300).await;
        let shown = open_menu(&open).await.is_some();
        main.miniaturize(None);
        sleep(1200).await;
        println!(
            "MINIMIZED shown_before={shown} open_after={} panel_after={}",
            open.get(),
            panel().is_some()
        );
        main.deminiaturize(None);
        sleep(1200).await;
        open.set(false);
        sleep(300).await;

        // On the other display: the menu opens on it.
        let screens = objc2_app_kit::NSScreen::screens(MainThreadMarker::new().unwrap());
        if let Some(other) = screens.iter().find(|screen| screen.frame().origin.x != 0.) {
            let frame = other.frame();
            main.setFrameOrigin(NSPoint::new(
                frame.origin.x + 200.,
                frame.origin.y + frame.size.height - 400.,
            ));
            sleep(500).await;
            let panel = open_menu(&open).await;
            let on_it = panel.as_ref().is_some_and(|panel| {
                let at = panel.frame().origin;
                (frame.origin.x..frame.origin.x + frame.size.width).contains(&at.x)
            });
            println!(
                "OTHER_DISPLAY panel={} on_it={on_it} scale={}",
                panel.is_some(),
                other.backingScaleFactor()
            );
            open.set(false);
            main.setFrameOrigin(origin);
            sleep(500).await;
        } else {
            println!("OTHER_DISPLAY none");
        }

        if fullscreen {
            let shown = open_menu(&open).await.is_some();
            main.toggleFullScreen(None);
            sleep(2500).await;
            println!(
                "FULLSCREEN shown_before={shown} open_after={} panel_after={}",
                open.get(),
                panel().is_some()
            );
            main.toggleFullScreen(None);
            sleep(2500).await;
        }

        // A dozen windows: the Window menu lists them all.
        for n in 1..=12 {
            windows.open(
                WindowOptions {
                    title: format!("Window {n}"),
                    width: 200.,
                    height: 120.,
                    ..Default::default()
                },
                move |cx| {
                    cx.render(text(format!("Window {n}")));
                },
            );
        }
        sleep(2000).await;
        let listed = app()
            .windowsMenu()
            .map(|menu| {
                menu.itemArray()
                    .iter()
                    .filter(|item| item.title().to_string().starts_with("Window "))
                    .count()
            })
            .unwrap_or(0);
        println!("WINDOW_MENU listed={listed} of 12");
    }
}
