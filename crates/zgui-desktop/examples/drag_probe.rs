//! Measures drags (macOS) by synthetic AppKit events posted to this
//! process alone (the user's pointer is never moved): a chip dragged along
//! its window, printing the retired instructions each pointer move costs,
//! and released where nothing takes it, its preview sliding back.
//!
//! It never drags out of the window: AppKit's session follows the real
//! pointer, and with no real button down it drops at once on whatever app
//! is under it (Finder makes a text clipping).
//!
//! winit hears moves outside a window only while a mouse button is down,
//! which it asks the hardware (`NSEvent.pressedMouseButtons`): the probe
//! answers that for its synthetic presses, in this process only.
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowBounds, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Drag".into(),
            width: 420.,
            height: 200.,
            bounds: Some(WindowBounds::new(840, 400).at(400, 300)),
            ..Default::default()
        })
        .run(move |cx| {
            let chip = div()
                .id("chip")
                .w(120.)
                .h(28.)
                .rounded(8.)
                .bg(rgba(0x3d7ef733))
                .items_center()
                .justify_center()
                .child(text("Some text"))
                .on_drag(|| String::from("Some text"))
                .drag_preview(|text: &String| {
                    div()
                        .w(120.)
                        .h(28.)
                        .rounded(8.)
                        .bg(rgb(0x3d7ef7))
                        .items_center()
                        .justify_center()
                        .child(zgui::compose::text(text))
                })
                .drag_export(|text: &String| vec![zgui::input::DragItem::Text(text.clone())])
                .on_drag_end(|_: &String, accepted| println!("DRAG_END accepted={accepted}"));
            cx.render(column().w_full().h_full().p(10.).child(chip));
            #[cfg(target_os = "macos")]
            cx.tasks.spawn(async move {
                macos::run().await;
                std::process::exit(0);
            });
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::NSPoint;
    use std::time::Duration;
    use zgui_gpu::present_log::now_ns;

    unsafe extern "C" {
        fn thread_selfcounts(kind: i32, buffer: *mut u64, size: usize) -> i32;
    }
    /// The main thread's retired instructions so far (the kernel's
    /// per-thread counters, as xnu's own tests read them): the work a move
    /// costs the app, apart from the system's threads in the process.
    fn instructions() -> u64 {
        let mut counts = [0u64; 2];
        // SAFETY: kind 1 fills instructions then cycles.
        unsafe { thread_selfcounts(1, counts.as_mut_ptr(), 16) };
        counts[0]
    }
    thread_local! {
        static PRESSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static POSTING: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    extern "C-unwind" fn pressed_buttons(
        _: *mut objc2::runtime::AnyObject,
        _: objc2::runtime::Sel,
    ) -> usize {
        PRESSED.with(|pressed| pressed.get())
    }
    /// `+[NSEvent pressedMouseButtons]` answers the probe's presses.
    fn hold_buttons() {
        let class = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        // SAFETY: a class method returning NSUInteger, replaced in this
        // process by one of the same signature.
        unsafe {
            let imp: objc2::runtime::Imp = std::mem::transmute(
                pressed_buttons
                    as extern "C-unwind" fn(
                        *mut objc2::runtime::AnyObject,
                        objc2::runtime::Sel,
                    ) -> usize,
            );
            objc2::ffi::class_replaceMethod(
                class.metaclass() as *const _ as *mut _,
                objc2::sel!(pressedMouseButtons),
                imp,
                c"Q@:".as_ptr(),
            );
        }
    }
    async fn sleep(ms: u64) {
        zgui::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn window(title: &str) -> Option<Retained<NSWindow>> {
        let mtm = MainThreadMarker::new().unwrap();
        NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == title)
    }
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
            if kind == NSEventType::LeftMouseUp { 0. } else { 1. },
        )
        .unwrap();
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
    /// Drag from `(x, y)` in `window` to `(to_x, to_y)` in `steps` moves,
    /// then release; returns the instructions of the moves from `measure_from`.
    async fn drag(
        window: &NSWindow,
        (x, y): (f64, f64),
        (to_x, to_y): (f64, f64),
        steps: usize,
        measure_from: usize,
    ) -> (u64, usize) {
        PRESSED.with(|pressed| pressed.set(1));
        post(window, NSEventType::LeftMouseDown, x, y);
        sleep(50).await;
        let mut start = 0;
        for step in 1..=steps {
            if step == measure_from {
                sleep(100).await;
                POSTING.with(|posting| posting.set(0));
                start = instructions();
            }
            let t = step as f64 / steps as f64;
            let before = instructions();
            post(
                window,
                NSEventType::LeftMouseDragged,
                x + (to_x - x) * t,
                y + (to_y - y) * t,
            );
            POSTING.with(|posting| posting.set(posting.get() + instructions() - before));
            sleep(8).await;
        }
        // Making and posting the synthetic events is the probe's own work.
        let spent = instructions() - start - POSTING.with(|posting| posting.replace(0));
        // As long again, the pointer held still: what the drag costs
        // anyway (frames, timers) is not the moves'.
        sleep(100).await;
        let still = instructions();
        for _ in measure_from..=steps {
            sleep(8).await;
        }
        let spent = spent.saturating_sub(instructions() - still);
        sleep(100).await;
        post(window, NSEventType::LeftMouseUp, to_x, to_y);
        sleep(50).await;
        PRESSED.with(|pressed| pressed.set(0));
        sleep(350).await;
        (spent, steps + 1 - measure_from)
    }
    pub async fn run() {
        sleep(3000).await;
        hold_buttons();
        let window = window("Drag").unwrap();
        // Hover first, as a real pointer does.
        post(&window, NSEventType::MouseMoved, 60., 24.);
        sleep(200).await;
        for _ in 0..3 {
            let (spent, moves) = drag(&window, (60., 24.), (330., 150.), 120, 40).await;
            println!(
                "SAME_WINDOW moves={moves} instructions_per_move={}",
                spent / moves as u64
            );
        }
        println!("DONE");
    }
}
