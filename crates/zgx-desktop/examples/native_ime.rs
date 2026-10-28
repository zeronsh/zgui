//! Native input-method probe; the application never synthesizes IME events.
//! `--password`: the first editor is a password field, which no input
//! method composes into (`scripts/native_ime_smoke.py --password`).
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

/// DIAGNOSTIC BUILD ONLY (branch tsf-idle-diag): every message the UI
/// thread takes (posted, sent), by message and window class, with
/// `ZGX_IDLE_DIAG=1`; reported with the store's calls every 5 s.
#[cfg(windows)]
mod idle_diag {
    use std::{collections::BTreeMap, sync::Mutex};
    use windows::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{DataExchange::GetClipboardFormatNameW, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            CWPSTRUCT, CallNextHookEx, GetClassNameW, MSG, SetWindowsHookExW, WH_CALLWNDPROC,
            WH_GETMESSAGE,
        },
    };
    static COUNTS: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());
    fn note(kind: &str, message: u32, wparam: usize, hwnd: HWND) {
        let mut class = [0u16; 64];
        // SAFETY: valid buffers; a window handle may be null.
        let class = unsafe {
            let n = GetClassNameW(hwnd, &mut class);
            String::from_utf16_lossy(&class[..n.max(0) as usize])
        };
        let name = if message >= 0xC000 {
            let mut buf = [0u16; 64];
            // SAFETY: a registered message's atom; a valid buffer.
            let n = unsafe { GetClipboardFormatNameW(message, &mut buf) };
            format!(
                "{:#x}={}",
                message,
                String::from_utf16_lossy(&buf[..n.max(0) as usize])
            )
        } else if message == 0x113 {
            format!("WM_TIMER(id={wparam:#x})")
        } else {
            format!("{message:#x}")
        };
        *COUNTS
            .lock()
            .unwrap()
            .entry(format!("{kind} {name} [{class}]"))
            .or_default() += 1;
    }
    unsafe extern "system" fn get_message(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 && w.0 == 1 {
            // SAFETY: WH_GETMESSAGE's lparam is the message taken.
            let m = unsafe { &*(l.0 as *const MSG) };
            note("posted", m.message, m.wParam.0, m.hwnd);
        }
        // SAFETY: passing the hook on.
        unsafe { CallNextHookEx(None, code, w, l) }
    }
    unsafe extern "system" fn call_wnd(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            // SAFETY: WH_CALLWNDPROC's lparam is the message sent.
            let c = unsafe { &*(l.0 as *const CWPSTRUCT) };
            note(
                if w.0 != 0 {
                    "sent-by-other-thread"
                } else {
                    "sent"
                },
                c.message,
                c.wParam.0,
                c.hwnd,
            );
        }
        // SAFETY: passing the hook on.
        unsafe { CallNextHookEx(None, code, w, l) }
    }
    pub fn install() {
        // SAFETY: hooks on this (the UI) thread, procedures in this module.
        unsafe {
            let thread = GetCurrentThreadId();
            let _ = SetWindowsHookExW(WH_GETMESSAGE, Some(get_message), None, thread);
            let _ = SetWindowsHookExW(WH_CALLWNDPROC, Some(call_wnd), None, thread);
        }
    }
    pub fn report(tick: usize) {
        for (key, n) in std::mem::take(&mut *COUNTS.lock().unwrap()) {
            println!("MSG_DIAG {tick} {n} {key}");
        }
        for (name, n, cycles) in zgx_desktop::tsf::diag::take() {
            println!("TSF_DIAG {tick} {name} calls={n} cycles={cycles}");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observe = std::env::args().any(|arg| arg == "--native-observe");
    let password = std::env::args().any(|arg| arg == "--password");
    #[cfg(windows)]
    let open_ime = std::env::args().any(|arg| arg == "--open-ime");
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
                    .child(editor("first", first.clone()).secure(password))
                    .child(text("Second editor").h(24.))
                    .child(editor("second", second.clone())),
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
            #[cfg(windows)]
            let idle_diag = std::env::var_os("ZGX_IDLE_DIAG").is_some_and(|v| v == "1");
            #[cfg(windows)]
            if idle_diag {
                idle_diag::install();
            }
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
                    #[cfg(windows)]
                    if idle_diag && tick % 10 == 9 {
                        idle_diag::report(tick);
                    }
                    // Until composition starts: the test may switch the window
                    // to the input method's language only after it shows.
                    #[cfg(windows)]
                    if open_ime && !COMPOSED.load(std::sync::atomic::Ordering::Relaxed) {
                        open_input_method();
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
                                _ => {}
                            }
                        }
                    }
                }
                window.close();
            });
        })
}
