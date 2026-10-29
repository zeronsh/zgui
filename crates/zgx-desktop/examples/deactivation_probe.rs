//! A window losing key status to another window and getting it back
//! (macOS), by AppKit events posted to this process's own windows: a helper
//! window of its own takes key status, as another app's window would, and
//! gives it back. No other app is activated and nothing outside the process
//! is touched (run with `ZGX_NATIVE_CARET=0`: the editor paints its caret).
//!
//! - `BEFORE`: a field focused with its caret drawn, the pointer over a
//!   button (hovered), Shift held.
//! - `AWAY`: with the helper key, no caret is drawn and nothing is hovered.
//! - `BACK`, `TYPED`: key again, the field has its focus and caret back, and
//!   a key types into it with no modifier held (Shift went with the window).
//! - `PRESSED`, `PRESS_AWAY`, `UP`: a press on the button is captured, let go
//!   as the window goes to the back, and released after coming back it
//!   clicks nothing (no `CLICK`).
//!
//! `scripts/macos_deactivation_probe.py` runs it and checks the lines.
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Deactivation".into(),
            width: 420.,
            height: 220.,
            ..Default::default()
        })
        .run(|cx| {
            let value = cx.ui.signal(String::new());
            // The field at (20, 20), 240 by 32; the button at (20, 70), 120 by 40.
            let view = cx.render(
                column()
                    .p(20.)
                    .gap(18.)
                    .items_start()
                    .on_event(|cx| {
                        if cx.phase == zgx::input::EventPhase::Bubble
                            && let zgx::input::InputEvent::KeyDown { key, modifiers, .. } =
                                &cx.event
                        {
                            println!("KEY {key:?} shift={}", modifiers.shift);
                        }
                    })
                    .child(text_input("Field", value.clone()).id("field").w(240.))
                    .child(
                        button()
                            .id("button")
                            .w(120.)
                            .h(40.)
                            .child(text("Button"))
                            .on_click(|| println!("CLICK")),
                    ),
            );
            #[cfg(target_os = "macos")]
            {
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    macos::run(ui, view, value).await;
                    std::process::exit(0);
                });
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (value, view);
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
    use objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSEvent, NSEventModifierFlags, NSEventType, NSWindow,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use std::time::Duration;
    use zgx::reactive::Signal;
    use zgx::scene::NodeKind;
    use zgx_gpu::present_log::now_ns;

    async fn sleep(ms: u64) {
        zgx::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn window() -> Retained<NSWindow> {
        let mtm = MainThreadMarker::new().unwrap();
        NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == "Deactivation")
            .unwrap()
    }
    /// A small titled window of this process's own, beside the probe's.
    fn helper(beside: &NSWindow) -> Retained<NSWindow> {
        let mtm = MainThreadMarker::new().unwrap();
        let frame = beside.frame();
        let rect = NSRect::new(
            NSPoint::new(frame.origin.x + frame.size.width + 20., frame.origin.y),
            NSSize::new(200., 120.),
        );
        // SAFETY: a plain window on the main thread, released by AppKit
        // when closed only if asked (`setReleasedWhenClosed:` false below).
        let helper = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect,
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { helper.setReleasedWhenClosed(false) };
        helper.setTitle(&NSString::from_str("Helper"));
        helper
    }
    /// A point of the window's content, top-down, bottom-up in the window.
    fn local(window: &NSWindow, x: f64, y: f64) -> NSPoint {
        NSPoint::new(x, window.contentView().unwrap().frame().size.height - y)
    }
    fn mouse(window: &NSWindow, kind: NSEventType, x: f64, y: f64, flags: NSEventModifierFlags) {
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            local(window, x, y),
            flags,
            now_ns() as f64 / 1e9,
            window.windowNumber(),
            None,
            0,
            1,
            if kind == NSEventType::LeftMouseUp { 0. } else { 1. },
        )
        .unwrap();
        window.sendEvent(&event);
    }
    /// Shift pressed (`down`) or let go, as AppKit reports a modifier.
    fn shift(window: &NSWindow, down: bool) {
        let flags = if down {
            NSEventModifierFlags::Shift
        } else {
            NSEventModifierFlags::empty()
        };
        let empty = NSString::from_str("");
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::FlagsChanged,
            NSPoint::new(0., 0.),
            flags,
            now_ns() as f64 / 1e9,
            window.windowNumber(),
            None,
            &empty,
            &empty,
            false,
            56,
        )
        .unwrap();
        window.sendEvent(&event);
    }
    /// A key down then up, sent to the window (its first responder hears it).
    fn key(window: &NSWindow, code: u16, characters: &str) {
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let characters = NSString::from_str(characters);
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0., 0.),
                NSEventModifierFlags::empty(),
                now_ns() as f64 / 1e9,
                window.windowNumber(),
                None,
                &characters,
                &characters,
                false,
                code,
            )
            .unwrap();
            window.sendEvent(&event);
        }
    }

    pub async fn run(
        ui: zgx::widgets::WeakUi,
        view: zgx::compose::ViewHandle,
        value: Signal<String>,
    ) {
        sleep(2500).await;
        let window = window();
        let none = NSEventModifierFlags::empty();
        let button = view.find("button").unwrap();
        // What the probe reads: the field focused, its caret drawn, whether
        // what is hovered and captured is the button (or inside it).
        let state = || {
            let ui = ui.upgrade().unwrap();
            let scene = ui.scene.borrow();
            let id_of = |node: Option<zgx::scene::NodeId>| {
                let Some(mut node) = node else {
                    return "none";
                };
                loop {
                    if node == button {
                        return "button";
                    }
                    match scene.parent(node) {
                        Some(parent) => node = parent,
                        None => return "other",
                    }
                }
            };
            let field = ui.focused_editor();
            let caret = field.as_ref().is_some_and(|editor| {
                let caret = editor.caret;
                let drawn = matches!(scene.kind(caret), NodeKind::Rect(color) if color.3 > 0);
                drawn
                    && scene.effects(caret).opacity > 0.
                    && scene.visible_bounds(caret).is_some_and(|b| b.width > 0.)
            });
            format!(
                "focused={} caret={} hovered={} captured={}",
                field.is_some(),
                caret,
                id_of(ui.input.hovered()),
                id_of(ui.input.captured()),
            )
        };
        let helper = helper(&window);
        // A: the field focused, Shift held, the pointer over the button.
        mouse(&window, NSEventType::LeftMouseDown, 140., 36., none);
        mouse(&window, NSEventType::LeftMouseUp, 140., 36., none);
        sleep(200).await;
        mouse(&window, NSEventType::MouseMoved, 80., 90., none);
        shift(&window, true);
        sleep(200).await;
        println!("BEFORE {}", state());
        // Another window takes key status, then gives it back.
        helper.makeKeyAndOrderFront(None);
        sleep(500).await;
        println!("AWAY {} main_key={}", state(), window.isKeyWindow());
        window.makeKeyAndOrderFront(None);
        sleep(500).await;
        println!("BACK {} main_key={}", state(), window.isKeyWindow());
        key(&window, 7, "x");
        sleep(200).await;
        println!("TYPED value={:?}", value.get());
        // B: the button pressed, then the window to the back and back, then
        // the press let go over it.
        mouse(&window, NSEventType::LeftMouseDown, 80., 90., none);
        sleep(200).await;
        println!("PRESSED {}", state());
        helper.makeKeyAndOrderFront(None);
        sleep(500).await;
        println!("PRESS_AWAY {}", state());
        window.makeKeyAndOrderFront(None);
        sleep(500).await;
        mouse(&window, NSEventType::LeftMouseUp, 80., 90., none);
        sleep(300).await;
        println!("UP done");
        helper.close();
        println!("DONE");
    }
}
