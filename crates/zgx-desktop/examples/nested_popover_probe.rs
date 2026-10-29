//! A popover opened from a row of another's panel, with each in a popup
//! window of its own (macOS): clicks posted to the outer popup's window close
//! the nested popover and leave the outer open, a click on the outer's field
//! focuses it as it closes the nested one, and a click in the main window
//! closes both. Synthetic AppKit events posted to this process's own windows,
//! each inside one (the user's pointer is never moved, nothing is dragged).
//! Prints one line per step for `scripts/macos_nested_popover_probe.py`.
use zgx::compose::{Placement, PlacementEdge, PlacementSide, prelude::*};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Nested popover probe".into(),
            width: 420.,
            height: 160.,
            bounds: Some(zgx_desktop::WindowBounds::new(840, 320).at(400, 300)),
            ..Default::default()
        })
        .run(|cx| {
            let outer = cx.ui.signal(false);
            let inner = cx.ui.signal(false);
            let query = cx.ui.signal(String::new());
            let row = {
                let inner = inner.clone();
                button()
                    .id("row")
                    .w(160.)
                    .h(24.)
                    .on_click(move || {
                        inner.set(!inner.get());
                    })
                    .child(text("Work in"))
            };
            let opener = {
                let outer = outer.clone();
                button()
                    .id("open")
                    .w(80.)
                    .h(24.)
                    .on_click(move || {
                        outer.set(true);
                    })
                    .child(text("Open"))
            };
            // The outer panel hangs past the window's bottom edge: in a
            // popup window of its own.
            let view = cx.render(
                column().w_full().h_full().p(16.).child(
                    popover("Outer", outer.clone(), opener)
                        .w(220.)
                        .h(240.)
                        .p(8.)
                        .gap(8.)
                        .child(
                            popover("Inner", inner.clone(), row)
                                .placement(Placement {
                                    side: PlacementSide::Right,
                                    align: PlacementEdge::Start,
                                    offset: 4.,
                                })
                                .w(140.)
                                .h(80.)
                                .child(text("Nested")),
                        )
                        .child(text_input("Search", query).id("search").w(180.).h(24.)),
                ),
            );
            #[cfg(target_os = "macos")]
            {
                let window = cx.window.clone();
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    macos::run(ui, view, outer, inner).await;
                    window.close();
                });
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (view, outer, inner);
                println!("macOS only");
                cx.window.close();
            }
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgx::{compose::ViewHandle, reactive::Signal, scene::Rect, widgets::WeakUi};
    use zgx_gpu::present_log::now_ns;

    async fn sleep(ms: u64) {
        zgx::timer::sleep(Duration::from_millis(ms)).await;
    }
    /// The app's windows: its own (titled) and the shown popups, by area
    /// (the outer popover's is the larger).
    fn windows() -> (Retained<NSWindow>, Vec<Retained<NSWindow>>) {
        let mtm = MainThreadMarker::new().unwrap();
        let mut main = None;
        let mut popups = Vec::new();
        for window in NSApplication::sharedApplication(mtm).windows().iter() {
            if !window.isVisible() {
                continue;
            }
            if window.title().to_string() == "Nested popover probe" {
                main = Some(window);
            } else {
                popups.push(window);
            }
        }
        popups.sort_by(|a, b| {
            let area = |w: &NSWindow| w.frame().size.width * w.frame().size.height;
            area(b).total_cmp(&area(a))
        });
        (main.expect("probe window"), popups)
    }
    /// Post a mouse event at `(x, y)` in `window`'s content (top-down).
    fn post(window: &NSWindow, kind: NSEventType, x: f64, y: f64) {
        let mtm = MainThreadMarker::new().unwrap();
        let height = window.contentView().unwrap().frame().size.height;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            NSPoint::new(x, height - y),
            NSEventModifierFlags::empty(),
            now_ns() as f64 / 1e9,
            window.windowNumber(),
            None,
            0,
            1,
            if kind == NSEventType::LeftMouseDown { 1. } else { 0. },
        )
        .unwrap();
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
    /// A click at `(x, y)` in the main window's coordinates, posted to
    /// whichever of `to`'s windows that is: a popup's own coordinates are
    /// the main window's less where it sits. Only inside `to`.
    async fn click(main: &NSWindow, to: &NSWindow, (x, y): (f64, f64)) {
        let parent = main.convertRectToScreen(main.contentView().unwrap().frame());
        // Content to content: the main window's frame has its title bar.
        let frame = to.convertRectToScreen(to.contentView().unwrap().frame());
        let (dx, dy) = (
            frame.origin.x - parent.origin.x,
            (parent.origin.y + parent.size.height) - (frame.origin.y + frame.size.height),
        );
        let (x, y) = (x - dx, y - dy);
        let content = to.contentView().unwrap().frame().size;
        assert!(
            x >= 0. && y >= 0. && x < content.width && y < content.height,
            "a click inside its window ({x}, {y})"
        );
        post(to, NSEventType::MouseMoved, x, y);
        sleep(150).await;
        post(to, NSEventType::LeftMouseDown, x, y);
        post(to, NSEventType::LeftMouseUp, x, y);
        sleep(300).await;
    }
    fn center(rect: Rect) -> (f64, f64) {
        (
            (rect.x + rect.width / 2.) as f64,
            (rect.y + rect.height / 2.) as f64,
        )
    }
    pub async fn run(ui: WeakUi, view: ViewHandle, outer: Signal<bool>, inner: Signal<bool>) {
        sleep(3000).await;
        let bounds = |id: &str| {
            let ui = ui.upgrade().unwrap();
            let node = view.find(id).unwrap();
            ui.scene.borrow().bounds(node)
        };
        let focused = |id: &str| {
            let ui = ui.upgrade().unwrap();
            let node = view.find(id).unwrap();
            let focus = ui.input.focused();
            let scene = ui.scene.borrow();
            focus.is_some_and(|f| f == node || scene.ancestors(f).any(|a| a == node))
        };
        let state = |step: &str| {
            println!(
                "{step} outer={} inner={} popups={}",
                outer.get(),
                inner.get(),
                windows().1.len()
            );
        };
        let (main, _) = windows();
        // Its own window key, as a click would make it (no other app's).
        main.makeKeyAndOrderFront(None);
        sleep(300).await;
        click(&main, &main, center(bounds("open"))).await;
        state("OPENED");
        let Some(outer_window) = windows().1.first().cloned() else {
            println!("NO_POPUP");
            return;
        };
        // Placed and shown before its own clicks.
        sleep(300).await;
        click(&main, &outer_window, center(bounds("row"))).await;
        state("NESTED");
        // Blank space in the outer panel, below the field: past the main
        // window's edge, in the outer popup's window.
        let search = bounds("search");
        let blank = (
            (search.x + 20.) as f64,
            (search.y + search.height + 60.) as f64,
        );
        click(&main, &outer_window, blank).await;
        state("BLANK");
        click(&main, &outer_window, center(bounds("row"))).await;
        click(&main, &outer_window, center(search)).await;
        println!(
            "FIELD outer={} inner={} focused={}",
            outer.get(),
            inner.get(),
            focused("search")
        );
        click(&main, &outer_window, center(bounds("row"))).await;
        // Outside both, in the main window, right of the outer panel.
        let content = main.contentView().unwrap().frame().size;
        click(&main, &main, (content.width - 20., 20.)).await;
        state("OUTSIDE");
    }
}
