//! Where an app's shortcuts go while a native view has the keyboard (macOS),
//! by AppKit key events posted to this app's own queue (`-[NSApplication
//! postEvent:atStart:]`, delivered as the keyboard's are; the user's
//! keyboard and other apps hear nothing):
//!
//! - ⌘U, a menu item's key nothing else uses;
//! - ⌘K, a menu item's key the page takes (`preventDefault`) and Ghostty
//!   binds (`clear_screen`);
//! - ⌘⇧L, a zgx key binding that is in no menu.
//!
//! Each is pressed with zgx's own box focused, then the page, then (with
//! `--features ghostty`) a terminal. It prints `ACTION <name> focus=<who>`
//! for each app action that fired, `PAGE <key> focus=<who>` for what the
//! page heard, `FIRST <class>` for AppKit's first responder as each part
//! starts and `KEY_ROUTING_DONE` at the end.
//! `scripts/macos_key_routing_probe.py` runs it and checks the lines.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers},
    web_view::{WebView, WebViewEvent, WebViewOptions, web_view},
};
use zgx_desktop::{AppMenu, AppMenuEntry, Application, MenuAccelerator, MenuAction, WindowOptions};

const PAGE: &str = r#"<!doctype html><body style="background:#223">
<script>
document.addEventListener('keydown', e => {
  window.webkit.messageHandlers.zgx.postMessage(e.key + (e.metaKey ? ' meta' : '') + (e.shiftKey ? ' shift' : ''));
  if (e.metaKey && e.key.toLowerCase() === 'k') e.preventDefault();
}, true);
</script>"#;

/// `--ime ja`'s page: a field, focused, reporting its composition and value.
const IME_PAGE: &str = r#"<!doctype html><body style="background:#223"><input id=f autofocus>
<script>
const post = (text) => window.webkit.messageHandlers.zgx.postMessage(text);
const field = document.getElementById('f');
field.addEventListener('compositionstart', () => post('composition start'));
field.addEventListener('compositionend', (e) => post('composition end ' + e.data));
field.addEventListener('input', () => post('value ' + field.value));
document.addEventListener('keydown', (e) => post(e.key + (e.metaKey ? ' meta' : '') + (e.isComposing ? ' composing' : '')), true);
</script>"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `--standard <h|m|q> --focus <zgx|web|terminal>`: one of AppKit's own
    // menu keys (Hide, Minimize, Quit), pressed with one part focused.
    let args: Vec<String> = std::env::args().collect();
    let after = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let standard = after("--standard");
    // `--ime ja`: a composition in the page with Japanese input (only if
    // already enabled: it is never enabled here), a ⌘ key in the middle of
    // it, then Return; the previous input source is selected again after.
    let ime = after("--ime").is_some();
    let part = after("--focus").unwrap_or_else(|| "web".into());
    let primary = |key: &str| MenuAccelerator::primary(Key::Character(key.into()));
    Application::new()
        .menus(vec![AppMenu::new(
            "Probe",
            [
                AppMenuEntry::item("Unused key", "unused").accelerator(primary("u")),
                AppMenuEntry::item("Key the view uses", "used").accelerator(primary("k")),
            ],
        )])?
        .window(WindowOptions {
            title: "Key routing".into(),
            width: 700.,
            height: 400.,
            ..Default::default()
        })
        .run(|cx| {
            let focus = Rc::new(RefCell::new("zgx"));
            let web = WebView::new(&cx.ui.runtime, WebViewOptions::default());
            web.load_html(if ime { IME_PAGE } else { PAGE }, None);
            let heard = focus.clone();
            let page = web.on_event(move |event| {
                if let WebViewEvent::Message { body, .. } = event {
                    println!("PAGE {body} focus={}", heard.borrow());
                }
            });
            #[cfg(feature = "ghostty")]
            let term = zgx::terminal::Terminal::new(&cx.ui.runtime, Default::default());
            let pane = row()
                .w_full()
                .grow()
                .gap(8.)
                .child(web_view(&web).w(340.).h_full());
            #[cfg(feature = "ghostty")]
            let pane = pane.child(zgx::terminal::terminal(&term).grow().h_full());
            let view = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .gap(8.)
                    .p(8.)
                    .child(
                        div()
                            .id("own")
                            .w(200.)
                            .h(30.)
                            .focusable(true)
                            .bg(rgba(0x80808040)),
                    )
                    .child(pane),
            );
            let root = cx.ui.root();
            let heard = focus.clone();
            cx.ui.on_event(root, false, move |event| {
                if event.phase != EventPhase::Capture
                    && let InputEvent::Action(action) = &event.event
                    && let Some(action) = action.downcast_ref::<MenuAction>()
                {
                    println!("ACTION {} focus={}", action.0, heard.borrow());
                    event.prevent_default();
                }
            });
            let command_shift = Modifiers {
                meta: true,
                shift: true,
                ..Default::default()
            };
            let binding = KeyBinding::new(
                [Keystroke::new(Key::Character("l".into()), command_shift)],
                Action::new(MenuAction("binding".into())),
            )
            .unwrap();
            let keymap = cx.ui.input.bind_keys(root, Keymap::new().bind(binding));
            let own = view.find("own").unwrap();
            let ui = cx.ui.downgrade();
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                let _keep = (page, keymap);
                let pause = |ms| zgx::timer::sleep(std::time::Duration::from_millis(ms));
                let press_all = || {
                    keys::press("u", 32, true, false);
                    keys::press("k", 40, true, false);
                    keys::press("l", 37, true, true);
                };
                pause(2500).await;
                if ime {
                    let Some(_restore) = keys::japanese() else {
                        println!("IME_SKIP no Japanese input source is enabled");
                        window.close();
                        return;
                    };
                    *focus.borrow_mut() = "web";
                    web.focus();
                    pause(800).await;
                    keys::first_responder();
                    for (characters, code) in [("k", 40), ("a", 0)] {
                        keys::press(characters, code, false, false);
                        pause(300).await;
                    }
                    // A menu key in the middle of the composition.
                    keys::press("u", 32, true, false);
                    pause(600).await;
                    keys::press("\r", 36, false, false);
                    pause(800).await;
                    println!("IME_DONE");
                    window.close();
                    return;
                }
                if let Some(key) = standard {
                    match part.as_str() {
                        "web" => web.focus(),
                        #[cfg(feature = "ghostty")]
                        "terminal" => term.focus(),
                        _ => {
                            if let Some(ui) = ui.upgrade() {
                                ui.input.focus(&ui.scene, Some(own));
                            }
                        }
                    }
                    pause(800).await;
                    keys::first_responder();
                    let code = match key.as_str() {
                        "h" => 4,
                        "m" => 46,
                        _ => 12,
                    };
                    keys::press(&key, code, true, false);
                    pause(1200).await;
                    keys::report_standard();
                    println!("STANDARD_DONE");
                    window.close();
                    return;
                }
                if let Some(ui) = ui.upgrade() {
                    ui.input.focus(&ui.scene, Some(own));
                }
                pause(300).await;
                keys::first_responder();
                press_all();
                pause(800).await;
                *focus.borrow_mut() = "web";
                web.focus();
                pause(800).await;
                keys::first_responder();
                press_all();
                pause(1500).await;
                #[cfg(feature = "ghostty")]
                {
                    *focus.borrow_mut() = "terminal";
                    term.focus();
                    pause(800).await;
                    keys::first_responder();
                    press_all();
                    pause(1500).await;
                }
                println!("KEY_ROUTING_DONE");
                window.close();
            });
        })
}

#[cfg(target_os = "macos")]
mod keys {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSProcessInfo, NSString};
    /// `characters` (key code `code`) pressed and released, with Command
    /// and Shift as asked, posted to this app's own queue.
    pub fn press(characters: &str, code: u16, command: bool, shift: bool) {
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        let Some(window) = app.keyWindow() else {
            println!("NO_KEY_WINDOW");
            return;
        };
        let mut flags = NSEventModifierFlags::empty();
        if command {
            flags |= NSEventModifierFlags::Command;
        }
        if shift {
            flags |= NSEventModifierFlags::Shift;
        }
        let shown = if shift {
            characters.to_uppercase()
        } else {
            characters.to_owned()
        };
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0., 0.),
                flags,
                // A real time: Ghostty takes no key equivalent at 0.
                NSProcessInfo::processInfo().systemUptime(),
                window.windowNumber(),
                None,
                &NSString::from_str(&shown),
                &NSString::from_str(characters),
                false,
                code,
            )
            .unwrap();
            // Through the queue, as the keyboard's arrive: AppKit sends it
            // with `currentEvent` set, as WebKit and Ghostty expect.
            app.postEvent_atStart(&event, false);
        }
    }
    /// Whether the app hid and its window minimized (what ⌘H and ⌘M do),
    /// then the app shown and the window restored again.
    pub fn report_standard() {
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        let windows = app.windows();
        let minimized = windows.iter().any(|window| window.isMiniaturized());
        println!("HIDDEN {} MINIMIZED {minimized}", app.isHidden());
        app.unhide(None);
        for window in windows.iter() {
            window.deminiaturize(None);
        }
    }
    /// Selects an enabled Japanese input method for the probe, if there is
    /// one (none is ever enabled here): the previous input source is
    /// selected again when the returned guard goes.
    pub fn japanese() -> Option<Restore> {
        use objc2::{rc::Retained, runtime::AnyObject};
        use objc2_foundation::NSArray;
        // SAFETY: Text Input Sources' C API with its declared types, on the
        // main thread. The list and the current source are owned (Create,
        // Copy) and released by `Retained`; properties are unowned
        // CFString and CFBoolean values of a source the list keeps alive.
        unsafe {
            let current =
                Retained::from_raw(TISCopyCurrentKeyboardInputSource() as *mut AnyObject)?;
            let list = Retained::from_raw(
                TISCreateInputSourceList(std::ptr::null(), 0) as *mut NSArray<AnyObject>
            )?;
            let japanese = list.iter().find(|source| {
                let id = TISGetInputSourceProperty(
                    std::ptr::from_ref(&**source).cast(),
                    kTISPropertyInputSourceID,
                )
                .cast::<NSString>()
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default();
                let selectable = TISGetInputSourceProperty(
                    std::ptr::from_ref(&**source).cast(),
                    kTISPropertyInputSourceIsSelectCapable,
                ) == kCFBooleanTrue;
                selectable && id.contains("Kotoeri") && id.ends_with("Japanese")
            })?;
            if TISSelectInputSource(std::ptr::from_ref(&*japanese).cast()) != 0 {
                return None;
            }
            Some(Restore(current))
        }
    }
    /// The input source to select again (`japanese`).
    pub struct Restore(objc2::rc::Retained<objc2::runtime::AnyObject>);
    impl Drop for Restore {
        fn drop(&mut self) {
            // SAFETY: a source this guard owns, on the main thread.
            unsafe { TISSelectInputSource(std::ptr::from_ref(&*self.0).cast()) };
        }
    }
    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn TISCreateInputSourceList(
            properties: *const std::ffi::c_void,
            include_all: u8,
        ) -> *const std::ffi::c_void;
        fn TISCopyCurrentKeyboardInputSource() -> *const std::ffi::c_void;
        fn TISSelectInputSource(source: *const std::ffi::c_void) -> i32;
        fn TISGetInputSourceProperty(
            source: *const std::ffi::c_void,
            key: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        static kTISPropertyInputSourceID: *const std::ffi::c_void;
        static kTISPropertyInputSourceIsSelectCapable: *const std::ffi::c_void;
        static kCFBooleanTrue: *const std::ffi::c_void;
    }
    /// AppKit's first responder in the key window.
    pub fn first_responder() {
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        let class = app
            .keyWindow()
            .and_then(|window| window.firstResponder())
            .map(|responder| responder.class().name().to_string_lossy().into_owned());
        println!("FIRST {class:?}");
    }
}
#[cfg(not(target_os = "macos"))]
mod keys {
    pub fn press(_: &str, _: u16, _: bool, _: bool) {}
    pub fn first_responder() {}
    pub fn report_standard() {}
    pub fn japanese() -> Option<()> {
        None
    }
}
