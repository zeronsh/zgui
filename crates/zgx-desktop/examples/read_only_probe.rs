//! A read-only editor on macOS, by AppKit events posted to this process's
//! own window and calls on its own text input view (the user's keyboard,
//! input sources and pasteboard are never used):
//!
//! - `FOCUS`: a click focuses it.
//! - `TYPED`, `DELETE`, `RETURN`: a letter, Backspace and Return change
//!   nothing.
//! - `SELECT_ALL`, `ARROW`: Command-A selects the whole text and an arrow
//!   then moves the caret: selection and navigation still work.
//! - `MARKED`, `COMMIT`: an input method's marked text
//!   (`-setMarkedText:selectedRange:replacementRange:`) and its committed
//!   text (`-insertText:replacementRange:`), sent straight to the first
//!   responder, leave the editor no composition and no text. A real input
//!   method never gets that far: winit hands keys to input methods
//!   (`-interpretKeyEvents:`) only while IME is allowed, and a read-only
//!   editor disallows it. Called past that, winit's view keeps the marked
//!   text itself (`has_marked`), which reaches no editor.
//! - `FOCUS_AFTER`: it is still focused.
//!
//! Copying is not checked here: it would write the user's pasteboard.
//! `scripts/macos_read_only_probe.py` runs it and checks the lines.
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

/// The editor's text, which nothing here may change.
const TEXT: &str = "hello world";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "ReadOnly".into(),
            width: 420.,
            height: 220.,
            ..Default::default()
        })
        .run(|cx| {
            let value = cx.ui.signal(TEXT.to_owned());
            // At (20, 20), 360 by 120.
            cx.render(
                column().p(20.).child(
                    text_area("Notes", value.clone())
                        .id("notes")
                        .read_only(true)
                        .w(360.)
                        .h(120.),
                ),
            );
            #[cfg(target_os = "macos")]
            {
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    macos::run(ui, value).await;
                    std::process::exit(0);
                });
            }
            #[cfg(not(target_os = "macos"))]
            let _ = value;
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, msg_send, rc::Retained, runtime::AnyObject};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSWindow};
    use objc2_foundation::{NSPoint, NSRange, NSString};
    use std::time::Duration;
    use zgx::reactive::Signal;
    use zgx_gpu::present_log::now_ns;

    async fn sleep(ms: u64) {
        zgx::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn window() -> Retained<NSWindow> {
        let mtm = MainThreadMarker::new().unwrap();
        NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == "ReadOnly")
            .unwrap()
    }
    /// A point of the window's content, top-down, bottom-up in the window.
    fn local(window: &NSWindow, x: f64, y: f64) -> NSPoint {
        NSPoint::new(x, window.contentView().unwrap().frame().size.height - y)
    }
    fn mouse(window: &NSWindow, kind: NSEventType, x: f64, y: f64) {
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            local(window, x, y),
            NSEventModifierFlags::empty(),
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
    /// A key down then up, sent to the window (its first responder hears it).
    fn key(window: &NSWindow, code: u16, characters: &str, flags: NSEventModifierFlags) {
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let characters = NSString::from_str(characters);
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0., 0.),
                flags,
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
    /// What an input method hands the first responder: marked text, then
    /// its commit.
    fn marked(window: &NSWindow, text: &str) {
        let view: Retained<AnyObject> = window.firstResponder().unwrap().into();
        let text = NSString::from_str(text);
        let selected = NSRange::new(text.length(), 0);
        let replacement = NSRange::new(usize::MAX >> 1, 0);
        // SAFETY: the view is winit's, an NSTextInputClient; the arguments
        // are the protocol method's (an NSString, two NSRanges).
        unsafe {
            let _: () = msg_send![&view, setMarkedText: &*text, selectedRange: selected, replacementRange: replacement];
        }
    }
    fn has_marked(window: &NSWindow) -> bool {
        let view: Retained<AnyObject> = window.firstResponder().unwrap().into();
        // SAFETY: as `marked`: a protocol method returning BOOL.
        unsafe { msg_send![&view, hasMarkedText] }
    }
    fn commit(window: &NSWindow, text: &str) {
        let view: Retained<AnyObject> = window.firstResponder().unwrap().into();
        let text = NSString::from_str(text);
        let replacement = NSRange::new(usize::MAX >> 1, 0);
        // SAFETY: as `marked`: an NSString and an NSRange.
        unsafe {
            let _: () = msg_send![&view, insertText: &*text, replacementRange: replacement];
        }
    }

    pub async fn run(ui: zgx::widgets::WeakUi, value: Signal<String>) {
        sleep(2500).await;
        let window = window();
        let none = NSEventModifierFlags::empty();
        let state = || {
            let ui = ui.upgrade()?;
            let editor = ui.focused_editor()?;
            let preedit = editor.editor.borrow().preedit().is_some();
            Some((editor.selection(), preedit, editor.is_read_only()))
        };
        // Inside the editor, at its first line.
        mouse(&window, NSEventType::LeftMouseDown, 60., 34.);
        sleep(60).await;
        mouse(&window, NSEventType::LeftMouseUp, 60., 34.);
        sleep(200).await;
        let focused = state();
        println!(
            "FOCUS focused={} read_only={}",
            focused.is_some(),
            focused.is_some_and(|s| s.2)
        );
        key(&window, 7, "x", none);
        sleep(150).await;
        println!("TYPED value={:?}", value.get());
        key(&window, 51, "\u{7f}", none);
        sleep(150).await;
        println!("DELETE value={:?}", value.get());
        key(&window, 36, "\r", none);
        sleep(150).await;
        println!("RETURN value={:?}", value.get());
        key(&window, 0, "a", NSEventModifierFlags::Command);
        sleep(150).await;
        println!("SELECT_ALL selection={:?}", state().map(|s| s.0));
        key(&window, 124, "\u{F703}", none);
        sleep(150).await;
        println!("ARROW selection={:?}", state().map(|s| s.0));
        marked(&window, "に");
        sleep(150).await;
        println!(
            "MARKED has_marked={} preedit={:?} value={:?}",
            has_marked(&window),
            state().map(|s| s.1),
            value.get()
        );
        commit(&window, "日本");
        sleep(150).await;
        println!(
            "COMMIT preedit={:?} value={:?}",
            state().map(|s| s.1),
            value.get()
        );
        println!("FOCUS_AFTER focused={}", state().is_some());
        println!("DONE");
    }
}
