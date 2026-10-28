//! Native input-method probe; the application never synthesizes IME events.
//! `--password`: the first editor is a password field, which no input
//! method composes into (`scripts/native_ime_smoke.py --password`).
//! `--replace-while-composing`: the app sets the first editor's text to
//! "REPLACED" while an input method composes in it, which ends the
//! composition (`scripts/windows_ime_smoke.py --replace-while-composing`).
//! `--open-tsf-ime` (Windows, `ZGX_TSF=1`): the input method is opened
//! through the Text Services Framework's compartments, as `--open-ime`
//! opens it through IMM32 (`scripts/windows_tsf_japanese_smoke.py`).
//! `--input-modes`: the first editor takes an email address and the second
//! digits (`InputMode::Email`, `Numeric`): Windows' touch keyboard follows.
use std::time::Duration;
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent},
    scene::{Color, NodeKind},
};
use zgx_desktop::{Application, WindowOptions};

/// A composition has started (`--open-ime` stops opening the input method).
static COMPOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn observe_native_focus(tick: usize) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        let mtm = MainThreadMarker::new().expect("IME observation runs on UI thread");
        let app = NSApplication::sharedApplication(mtm);
        println!("NATIVE_FOCUS {tick} active={}", app.isActive());
        for window in app.windows() {
            println!(
                "NATIVE_WINDOW {tick} title={:?} key={}",
                window.title().to_string(),
                window.isKeyWindow()
            );
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = tick;
}

/// Windows (`--open-ime`): open the focused window's input method in its
/// native mode (hiragana for Japanese), as its on/off key would. Input
/// methods ignore that key when it is synthesized; the composition itself
/// still comes from real keystrokes.
#[cfg(windows)]
fn open_input_method() {
    use std::ffi::c_void;
    // SAFETY: winuser.h's and imm.h's declarations: HWND and HIMC are
    // pointers, a BOOL an i32, a DWORD a u32.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetFocus() -> *mut c_void;
    }
    // SAFETY: as above.
    #[link(name = "imm32")]
    unsafe extern "system" {
        fn ImmGetContext(window: *mut c_void) -> *mut c_void;
        fn ImmSetOpenStatus(context: *mut c_void, open: i32) -> i32;
        fn ImmGetConversionStatus(
            context: *mut c_void,
            conversion: *mut u32,
            sentence: *mut u32,
        ) -> i32;
        fn ImmSetConversionStatus(context: *mut c_void, conversion: u32, sentence: u32) -> i32;
        fn ImmReleaseContext(window: *mut c_void, context: *mut c_void) -> i32;
    }
    // SAFETY: the UI thread's own focus window and its input context,
    // released after use.
    unsafe {
        let window = GetFocus();
        let context = ImmGetContext(window);
        if context.is_null() {
            println!("IME_OPEN none");
            return;
        }
        let (mut conversion, mut sentence) = (0, 0);
        ImmGetConversionStatus(context, &mut conversion, &mut sentence);
        // IME_CMODE_NATIVE: hiragana, hangul, zhuyin.
        let open = ImmSetOpenStatus(context, 1) != 0
            && ImmSetConversionStatus(context, conversion | 0x1, sentence) != 0;
        println!("IME_OPEN {open}");
        ImmReleaseContext(window, context);
    }
}

/// `--open-tsf-ime`: through Windows' Text Services Framework
/// (`ZGX_TSF=1`), IMM32's context isn't the window's, so `--open-ime` can't
/// reach the input method. Open it as the framework's own switch does: its
/// thread's keyboard open-close compartment on, and the conversion mode
/// native and full-shape (hiragana, for the Japanese IME). No key is sent.
#[cfg(windows)]
fn open_input_method_tsf() {
    use windows::{
        Win32::{
            System::{
                Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
                Variant::{VARIANT, VT_I4},
            },
            UI::TextServices::{
                CLSID_TF_ThreadMgr, GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION,
                GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, ITfCompartmentMgr, ITfThreadMgr,
            },
        },
        core::{GUID, Interface},
    };
    // SAFETY: the UI thread's own thread manager (COM is initialized there,
    // and the app's text store activated it); each value a VT_I4 set whole.
    let open = unsafe {
        let thread: windows::core::Result<ITfThreadMgr> =
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER);
        thread.and_then(|thread| {
            let client = thread.Activate()?;
            let set = |compartments: &ITfCompartmentMgr, guid: &GUID, value: i32| {
                let mut variant = VARIANT::default();
                let inner = &mut *variant.Anonymous.Anonymous;
                inner.vt = VT_I4;
                inner.Anonymous.lVal = value;
                compartments
                    .GetCompartment(guid)?
                    .SetValue(client, &variant)
            };
            let done = thread.cast::<ITfCompartmentMgr>().and_then(|compartments| {
                set(&compartments, &GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, 1)?;
                // TF_CONVERSIONMODE_NATIVE | TF_CONVERSIONMODE_FULLSHAPE.
                set(
                    &compartments,
                    &GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION,
                    0x9,
                )
            });
            let _ = thread.Deactivate();
            done
        })
    };
    println!("IME_OPEN {} (text services)", open.is_ok());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observe = std::env::args().any(|arg| arg == "--native-observe");
    let password = std::env::args().any(|arg| arg == "--password");
    let replace = std::env::args().any(|arg| arg == "--replace-while-composing");
    let modes = std::env::args().any(|arg| arg == "--input-modes");
    let mode = |mode| {
        if modes {
            mode
        } else {
            zgx::text_edit::InputMode::Text
        }
    };
    #[cfg(windows)]
    let open_ime = std::env::args().any(|arg| arg == "--open-ime");
    #[cfg(windows)]
    let open_tsf_ime = std::env::args().any(|arg| arg == "--open-tsf-ime");
    Application::new()
        .window(WindowOptions {
            title: "zgx native IME".into(),
            width: 640.,
            height: 340.,
            ..Default::default()
        })
        .run(move |cx| {
            let first = cx.ui.signal(String::new());
            let second = cx.ui.signal(String::new());
            let editor = |label: &'static str, model| {
                text_input(label, model)
                    .id(label)
                    .size(560., 44.)
                    .p(10.)
                    .text_size(22.)
                    .bg(Color(38, 51, 72, 255))
                    .focus(|s| {
                        s.border(2.)
                            .border_over(true)
                            .border_color(Color(121, 184, 255, 255))
                    })
                    .on_event(move |event| {
                        if event.phase == EventPhase::Target {
                            match &event.event {
                                InputEvent::ImePreedit { text, cursor } => {
                                    COMPOSED.store(true, std::sync::atomic::Ordering::Relaxed);
                                    println!("PREEDIT {label} {text:?} {cursor:?}")
                                }
                                InputEvent::ImeCommit(text) => println!("COMMIT {label} {text:?}"),
                                InputEvent::Focus => println!("FOCUS {label}"),
                                InputEvent::Blur => println!("BLUR {label}"),
                                _ => {}
                            }
                        }
                    })
            };
            let handle = cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .text_color(Color(231, 236, 246, 255))
                    .child(text("Native input-method composition").h(24.))
                    .child(
                        editor("first", first.clone())
                            .secure(password)
                            .input_mode(mode(zgx::text_edit::InputMode::Email)),
                    )
                    .child(text("Second editor").h(24.))
                    .child(
                        editor("second", second.clone())
                            .input_mode(mode(zgx::text_edit::InputMode::Numeric)),
                    ),
            );
            let scene = cx.ui.scene.clone();
            let input = cx.ui.input.clone();
            let first_node = handle.find("first").unwrap();
            let second_node = handle.find("second").unwrap();
            let close_first = first.clone();
            let close_second = second.clone();
            cx.on_closed(move || {
                println!(
                    "FINAL first={:?} second={:?}",
                    close_first.get(),
                    close_second.get()
                )
            });
            let window = cx.window.clone();
            let first_editor = cx.ui.editor(first_node);
            let mut replaced = false;
            // The preedit at the last report: replaced once it holds still a
            // whole report, not between the keys composing it.
            let mut held: Option<String> = None;
            cx.tasks.spawn(async move {
                let ticks = zgx::env::var("ZGX_SECONDS")
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .map_or(40, |s| s * 2);
                for tick in 0..ticks {
                    zgx::timer::sleep(Duration::from_millis(500)).await;
                    if observe {
                        observe_native_focus(tick);
                    }
                    // Until composition starts: the test may switch the window
                    // to the input method's language only after it shows.
                    #[cfg(windows)]
                    if open_ime && !COMPOSED.load(std::sync::atomic::Ordering::Relaxed) {
                        open_input_method();
                    }
                    // Through the framework no preedit event comes: until the
                    // first field shows a composition.
                    #[cfg(windows)]
                    if open_tsf_ime
                        && first_editor
                            .as_ref()
                            .is_none_or(|editor| editor.editor.borrow().preedit().is_none())
                    {
                        open_input_method_tsf();
                    }
                    // The app's own edit while the input method composes.
                    if replace && !replaced {
                        let preedit = first_editor.as_ref().and_then(|editor| {
                            editor.editor.borrow().preedit().map(|p| p.text.clone())
                        });
                        if preedit.is_some() && preedit == held {
                            replaced = true;
                            first.set("REPLACED".to_owned());
                            println!(
                                "REPLACED while composing {:?}",
                                preedit.as_deref().unwrap_or_default()
                            );
                        }
                        held = preedit;
                    }
                    println!(
                        "MODEL {tick} first={:?} second={:?} focus={:?}",
                        first.get(),
                        second.get(),
                        input.focused()
                    );
                    let scene = scene.borrow();
                    for (label, node) in [("first", first_node), ("second", second_node)] {
                        let mut nodes = vec![node];
                        while let Some(node) = nodes.pop() {
                            nodes.extend(scene.children(node));
                            let rect = scene.bounds(node);
                            match scene.kind(node) {
                                NodeKind::Text { text, .. } => {
                                    println!("DISPLAY {label} {text:?} {rect:?}")
                                }
                                NodeKind::Rect(_) if rect.width <= 2. && rect.height > 5. => {
                                    println!("CARET {label} {rect:?}")
                                }
                                // The composition's underlines: thick under
                                // the clause being converted.
                                NodeKind::Rect(_) | NodeKind::Quad(_)
                                    if rect.width > 2. && rect.height <= 2. =>
                                {
                                    println!("UNDERLINE {label} {rect:?}")
                                }
                                _ => {}
                            }
                        }
                    }
                }
                window.close();
            });
        })
}
