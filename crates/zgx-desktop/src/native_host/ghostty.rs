//! Ghostty terminals behind `zgx::terminal::Terminal` controllers (the
//! `ghostty` feature): Ghostty's embedding API, as its own macOS app uses it.
//!
//! One Ghostty app per process holds the configuration (the user's Ghostty
//! config, then this host's overrides) and runs on the main thread: Ghostty
//! wakes it from any thread and it ticks on the main queue. Each terminal is
//! a `ZgxTerminalView` that Ghostty renders into (Metal, its own thread) and
//! that reads AppKit's keyboard, IME and mouse input the way Ghostty's
//! `SurfaceView_AppKit.swift` does. Ghostty's callbacks copy what they need
//! and reach the controller through `zgx::native_view::post`.
//!
//! App-level shortcuts (new tab, splits, close, tab and pane navigation) are
//! unbound in Ghostty so they fall through to the app's menus.
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class,
    msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, NSObjectProtocol, Sel},
};
use objc2_app_kit::{
    NSCursor, NSEvent, NSEventModifierFlags, NSEventPhase, NSEventType, NSPasteboard,
    NSPasteboardTypeFileURL, NSPasteboardTypeString, NSResponder, NSTextInputClient,
    NSTrackingArea, NSTrackingAreaOptions, NSView,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSNotFound, NSPoint, NSRange,
    NSRangePointer, NSRect, NSSize, NSString, NSUInteger, NSURL,
};
use std::{
    any::Any,
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
use zgx::terminal::{Terminal, TerminalBackend, TerminalCommand, TerminalEvent, WeakTerminal};
use zgx_ghostty_sys as g;

/// Overrides on top of the user's Ghostty configuration.
const CONFIG: &str = "\
# The app's menus own these shortcuts.
keybind = super+t=unbind
keybind = super+n=unbind
keybind = super+w=unbind
keybind = super+shift+w=unbind
keybind = super+alt+w=unbind
keybind = super+d=unbind
keybind = super+shift+d=unbind
keybind = super+q=unbind
keybind = super+comma=unbind
keybind = super+shift+comma=unbind
keybind = super+enter=unbind
keybind = super+shift+enter=unbind
keybind = super+ctrl+f=unbind
keybind = super+shift+p=unbind
keybind = super+bracket_left=unbind
keybind = super+bracket_right=unbind
keybind = super+shift+bracket_left=unbind
keybind = super+shift+bracket_right=unbind
keybind = super+alt+arrow_up=unbind
keybind = super+alt+arrow_down=unbind
keybind = super+alt+arrow_left=unbind
keybind = super+alt+arrow_right=unbind
keybind = super+ctrl+arrow_up=unbind
keybind = super+ctrl+arrow_down=unbind
keybind = super+ctrl+arrow_left=unbind
keybind = super+ctrl+arrow_right=unbind
keybind = super+ctrl+equal=unbind
keybind = super+one=unbind
keybind = super+two=unbind
keybind = super+three=unbind
keybind = super+four=unbind
keybind = super+five=unbind
keybind = super+six=unbind
keybind = super+seven=unbind
keybind = super+eight=unbind
keybind = super+nine=unbind
# The app draws each pane's background (its glass shows through).
background-opacity = 0
window-padding-x = 10
window-padding-y = 6
# Unbalanced: the grid stays put at the padding while a pane resizes (a
# balanced one re-centres on every point of width, sliding the text).
window-padding-balance = false
window-padding-color = background
confirm-close-surface = false
";

// ---------------------------------------------------------------------------
// Main-queue dispatch (Ghostty calls back from its own threads too).

#[repr(C)]
struct DispatchQueue {
    _private: [u8; 0],
}
// SAFETY: libdispatch's and libSystem's declarations (<dispatch/queue.h>,
// <dispatch/time.h>, <pthread.h>): `dispatch_function_t` is
// `void (*)(void *)`, `dispatch_time_t` a `uint64_t`, and the main queue the
// object `dispatch_get_main_queue()` returns the address of.
unsafe extern "C" {
    static _dispatch_main_q: DispatchQueue;
    fn dispatch_async_f(
        queue: *const DispatchQueue,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
    fn dispatch_after_f(
        when: u64,
        queue: *const DispatchQueue,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn pthread_main_np() -> i32;
}
fn main_queue() -> *const DispatchQueue {
    &raw const _dispatch_main_q
}
/// Run `work` on the main thread: now if this is it, else on the main queue.
fn on_main(work: impl FnOnce() + Send + 'static) {
    // SAFETY: takes no arguments; only reads which thread this is.
    if unsafe { pthread_main_np() } != 0 {
        work();
        return;
    }
    let boxed: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(work));
    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `context` is the `Box::into_raw` below, which dispatch
        // hands to this function exactly once.
        let work = unsafe { Box::from_raw(context as *mut Box<dyn FnOnce() + Send>) };
        work();
    }
    // SAFETY: the main queue lives for the process; `run` takes back the
    // box it is given. The closure is `Send`, so running it on the main
    // thread is sound.
    unsafe { dispatch_async_f(main_queue(), Box::into_raw(boxed) as *mut c_void, run) };
}
/// Run `work` on the main queue after `seconds`. The marker proves the
/// caller is on the main thread: `work` is not `Send`, so it must be made
/// on the thread it runs on.
fn after(_mtm: MainThreadMarker, seconds: f64, work: impl FnOnce() + 'static) {
    let boxed: Box<Box<dyn FnOnce()>> = Box::new(Box::new(work));
    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `context` is the `Box::into_raw` below, which dispatch
        // hands to this function exactly once.
        let work = unsafe { Box::from_raw(context as *mut Box<dyn FnOnce()>) };
        work();
    }
    // SAFETY: the main queue lives for the process; `run` takes back the
    // box it is given, on the main thread, where (`_mtm`) the closure was
    // made.
    unsafe {
        let when = dispatch_time(0, (seconds * 1e9) as i64);
        dispatch_after_f(when, main_queue(), Box::into_raw(boxed) as *mut c_void, run);
    }
}

// ---------------------------------------------------------------------------
// The app.

struct App {
    app: g::ghostty_app_t,
}
thread_local! {
    static APP: OnceCell<Option<App>> = const { OnceCell::new() };
    /// Live surfaces by address: callbacks find their view here.
    static SURFACES: RefCell<HashMap<usize, Retained<TerminalView>>> = RefCell::new(HashMap::new());
}
static TICK_PENDING: AtomicBool = AtomicBool::new(false);

fn app() -> Option<g::ghostty_app_t> {
    APP.with(|cell| cell.get_or_init(App::new).as_ref().map(|app| app.app))
}

impl App {
    fn new() -> Option<Self> {
        // SAFETY: on the main thread, as Ghostty's app must be (`app()`'s
        // callers are the main-queue tick and `MacTerminal::new`, which has
        // a `MainThreadMarker`), once (`APP`'s cell). `ghostty_init` gets no
        // arguments (argc 0).
        // `config` is checked non-null before use and every string passed
        // is a NUL-terminated `CString` alive for its call; the diagnostics'
        // messages are NUL-terminated strings the config owns, copied before
        // it goes. The callbacks match `ghostty_runtime_config_s`'s types and
        // never dereference their userdata. `ghostty_app_new` copies the
        // config (embedded.zig `App.init` clones it), so this one is freed.
        unsafe {
            if g::ghostty_init(0, std::ptr::null_mut()) != g::GHOSTTY_SUCCESS as i32 {
                eprintln!("zgx: ghostty_init failed");
                return None;
            }
            let config = g::ghostty_config_new();
            if config.is_null() {
                eprintln!("zgx: ghostty_config_new failed");
                return None;
            }
            g::ghostty_config_load_default_files(config);
            g::ghostty_config_load_recursive_files(config);
            let path =
                std::env::temp_dir().join(format!("zgx-ghostty-{}.conf", std::process::id()));
            if std::fs::write(&path, CONFIG).is_ok()
                && let Ok(path_c) = CString::new(path.to_string_lossy().as_bytes())
            {
                g::ghostty_config_load_file(config, path_c.as_ptr());
            }
            let _ = std::fs::remove_file(&path);
            g::ghostty_config_finalize(config);
            for index in 0..g::ghostty_config_diagnostics_count(config) {
                let diagnostic = g::ghostty_config_get_diagnostic(config, index);
                if !diagnostic.message.is_null() {
                    let message = CStr::from_ptr(diagnostic.message).to_string_lossy();
                    eprintln!("zgx: Ghostty config: {message}");
                }
            }
            let runtime = g::ghostty_runtime_config_s {
                userdata: std::ptr::null_mut(),
                supports_selection_clipboard: false,
                wakeup_cb: Some(wakeup),
                action_cb: Some(action),
                read_clipboard_cb: Some(read_clipboard),
                confirm_read_clipboard_cb: Some(confirm_read_clipboard),
                write_clipboard_cb: Some(write_clipboard),
                close_surface_cb: Some(close_surface),
            };
            let app = g::ghostty_app_new(&runtime, config);
            // The app keeps its own copy.
            g::ghostty_config_free(config);
            if app.is_null() {
                eprintln!("zgx: ghostty_app_new failed");
                return None;
            }
            g::ghostty_app_set_focus(app, true);
            Some(Self { app })
        }
    }
}

extern "C" fn wakeup(_userdata: *mut c_void) {
    if TICK_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    extern "C" fn tick(_context: *mut c_void) {
        TICK_PENDING.store(false, Ordering::Release);
        if let Some(app) = app() {
            // SAFETY: on the main queue, as ticks must be; the app is never
            // freed.
            unsafe { g::ghostty_app_tick(app) };
        }
    }
    // SAFETY: the main queue lives for the process; `tick` ignores its
    // (null) context. Ghostty wakes from any thread: nothing crosses but
    // the call.
    unsafe { dispatch_async_f(main_queue(), std::ptr::null_mut(), tick) };
}

/// The view for a surface, on the main thread.
fn view_of(surface: usize) -> Option<Retained<TerminalView>> {
    SURFACES.with(|surfaces| surfaces.borrow().get(&surface).cloned())
}
/// Deliver `change` to the surface's controller at zgx's next safe point.
fn deliver(surface: usize, change: impl FnOnce(&Terminal) + Send + 'static) {
    on_main(move || {
        let Some(view) = view_of(surface) else { return };
        let term = view.ivars().term.clone();
        zgx::native_view::post(move || {
            if let Some(term) = term.upgrade() {
                change(&term);
            }
        });
    });
}
/// A copy of a C string Ghostty passes a callback.
///
/// # Safety
///
/// `ptr` is null or a NUL-terminated string alive for the call (Ghostty's
/// are `[*:0]const u8`, valid until the callback returns).
unsafe fn string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        // SAFETY: non-null, so NUL-terminated and alive (the contract).
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

extern "C" fn action(
    _app: g::ghostty_app_t,
    target: g::ghostty_target_s,
    action: g::ghostty_action_s,
) -> bool {
    // Each union read below is the member its tag names (ghostty.h's
    // `ghostty_action_u`); the members are plain C data (enums are integers
    // in the bindings), and their strings are Ghostty's, alive for the call.
    // The surface is only an address here: what runs later looks it up in
    // `SURFACES` before using it, as it may be freed by then.
    if target.tag != g::GHOSTTY_TARGET_SURFACE {
        return false;
    }
    // SAFETY: a surface target's member is `surface`.
    let surface = unsafe { target.target.surface } as usize;
    let payload = action.action;
    match action.tag {
        g::GHOSTTY_ACTION_SET_TITLE => {
            // SAFETY: SET_TITLE's member; a NUL-terminated title.
            let title = unsafe { string(payload.set_title.title) };
            deliver(surface, move |term| {
                term.update_state(|state| state.title = title.clone());
                term.emit(TerminalEvent::TitleChanged { title });
            });
            true
        }
        g::GHOSTTY_ACTION_PWD => {
            // SAFETY: PWD's member; a NUL-terminated path.
            let pwd = unsafe { string(payload.pwd.pwd) };
            deliver(surface, move |term| {
                term.update_state(|state| state.pwd = pwd.clone());
                term.emit(TerminalEvent::PwdChanged { pwd });
            });
            on_main(move || refresh_process(surface));
            true
        }
        g::GHOSTTY_ACTION_RING_BELL => {
            deliver(surface, |term| {
                if !term.focused() {
                    term.update_state(|state| state.bell = true);
                }
                term.emit(TerminalEvent::Bell);
            });
            true
        }
        g::GHOSTTY_ACTION_MOUSE_SHAPE => {
            // SAFETY: MOUSE_SHAPE's member, an integer.
            let shape = unsafe { payload.mouse_shape };
            on_main(move || {
                if let Some(view) = view_of(surface) {
                    view.set_cursor_shape(shape);
                }
            });
            true
        }
        g::GHOSTTY_ACTION_MOUSE_VISIBILITY => {
            // SAFETY: MOUSE_VISIBILITY's member, an integer.
            let hidden = unsafe { payload.mouse_visibility } == g::GHOSTTY_MOUSE_HIDDEN;
            on_main(move || NSCursor::setHiddenUntilMouseMoves(hidden));
            true
        }
        g::GHOSTTY_ACTION_PROGRESS_REPORT => {
            // SAFETY: PROGRESS_REPORT's member, plain integers.
            let report = unsafe { payload.progress_report };
            let progress = match report.state {
                g::GHOSTTY_PROGRESS_STATE_REMOVE => None,
                g::GHOSTTY_PROGRESS_STATE_INDETERMINATE => Some(-1.),
                _ if report.progress < 0 => Some(-1.),
                _ => Some(report.progress as f32 / 100.),
            };
            let failed = report.state == g::GHOSTTY_PROGRESS_STATE_ERROR;
            deliver(surface, move |term| {
                term.update_state(|state| {
                    state.progress = progress;
                    state.progress_error = failed;
                });
            });
            true
        }
        g::GHOSTTY_ACTION_COMMAND_FINISHED => {
            // SAFETY: COMMAND_FINISHED's member, plain integers.
            let finished = unsafe { payload.command_finished };
            let exit_code = (finished.exit_code >= 0).then_some(finished.exit_code as i32);
            let duration_ms = finished.duration / 1_000_000;
            deliver(surface, move |term| {
                term.emit(TerminalEvent::CommandFinished {
                    exit_code,
                    duration_ms,
                });
            });
            on_main(move || refresh_process(surface));
            true
        }
        g::GHOSTTY_ACTION_DESKTOP_NOTIFICATION => {
            // SAFETY: DESKTOP_NOTIFICATION's member: two NUL-terminated
            // strings.
            let (title, body) = unsafe {
                let note = payload.desktop_notification;
                (string(note.title), string(note.body))
            };
            deliver(surface, move |term| {
                term.emit(TerminalEvent::Notification { title, body });
            });
            true
        }
        g::GHOSTTY_ACTION_SHOW_CHILD_EXITED => {
            // SAFETY: SHOW_CHILD_EXITED's member, plain integers.
            let code = unsafe { payload.child_exited }.exit_code as i32;
            deliver(surface, move |term| {
                term.update_state(|state| state.exited = Some(code));
                term.emit(TerminalEvent::Exited { code });
            });
            // Ghostty's own "process exited" notice stays.
            false
        }
        g::GHOSTTY_ACTION_OPEN_URL => {
            // SAFETY: OPEN_URL's member.
            let open = unsafe { payload.open_url };
            if open.url.is_null() {
                return false;
            }
            // SAFETY: Ghostty's `[]const u8` URL as pointer and length,
            // alive for the call; copied before it returns.
            let bytes = unsafe { std::slice::from_raw_parts(open.url as *const u8, open.len) };
            let url = String::from_utf8_lossy(bytes).into_owned();
            on_main(move || {
                let _ = open::that_detached(url);
            });
            true
        }
        g::GHOSTTY_ACTION_RENDER => {
            // Ghostty asks for a frame drawn now (resizes, mostly).
            on_main(move || {
                if view_of(surface).is_some() {
                    // SAFETY: still in `SURFACES`, so not freed; main thread.
                    unsafe { g::ghostty_surface_draw(surface as g::ghostty_surface_t) };
                }
            });
            true
        }
        g::GHOSTTY_ACTION_CELL_SIZE => true,
        _ => false,
    }
}

extern "C" fn close_surface(userdata: *mut c_void, process_alive: bool) {
    let view = userdata as usize;
    on_main(move || {
        let found = SURFACES.with(|surfaces| {
            surfaces
                .borrow()
                .values()
                .find(|candidate| Retained::as_ptr(candidate) as usize == view)
                .cloned()
        });
        let Some(view) = found else { return };
        let term = view.ivars().term.clone();
        zgx::native_view::post(move || {
            if let Some(term) = term.upgrade() {
                term.emit(TerminalEvent::CloseRequested { process_alive });
            }
        });
    });
}

fn view_from_userdata(userdata: *mut c_void) -> Option<Retained<TerminalView>> {
    let address = userdata as usize;
    SURFACES.with(|surfaces| {
        surfaces
            .borrow()
            .values()
            .find(|view| Retained::as_ptr(view) as usize == address)
            .cloned()
    })
}

extern "C" fn read_clipboard(
    userdata: *mut c_void,
    _location: g::ghostty_clipboard_e,
    state: *mut c_void,
) -> bool {
    let Some(view) = view_from_userdata(userdata) else {
        return false;
    };
    let surface = view.ivars().surface.get();
    if surface.is_null() {
        return false;
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: reads AppKit's constant type name, an immutable static.
    let Some(text) = (unsafe { pasteboard.stringForType(NSPasteboardTypeString) }) else {
        return false;
    };
    let Ok(text) = CString::new(text.to_string()) else {
        return false;
    };
    // SAFETY: found in `SURFACES`, so the surface is live (and this is the
    // main thread, whose `SURFACES` it is). `state` is the request Ghostty
    // just started, completed once; `text` is NUL-terminated and outlives
    // the call (Ghostty copies it, or passes it straight to
    // `confirm_read_clipboard`, which copies it).
    unsafe { g::ghostty_surface_complete_clipboard_request(surface, text.as_ptr(), state, false) };
    true
}

extern "C" fn confirm_read_clipboard(
    userdata: *mut c_void,
    text: *const c_char,
    state: *mut c_void,
    request: g::ghostty_clipboard_request_e,
) {
    let Some(view) = view_from_userdata(userdata) else {
        return;
    };
    // SAFETY: Ghostty's NUL-terminated text, alive for the call.
    let text = unsafe { string(text) };
    let state = state as usize;
    // Asked from inside Ghostty: answer after it returns.
    let mtm = view.mtm();
    after(mtm, 0., move || {
        if view.ivars().surface.get().is_null() {
            return;
        }
        let (title, detail) = match request {
            g::GHOSTTY_CLIPBOARD_REQUEST_PASTE => (
                "Paste this text?",
                "It may run commands as soon as it is pasted.",
            ),
            _ => (
                "Let the program read your clipboard?",
                "A program in this terminal asked to read it.",
            ),
        };
        let preview: String = text.chars().take(400).collect();
        let allowed = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(title)
            .set_description(format!("{detail}\n\n{preview}"))
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                "Allow".into(),
                "Cancel".into(),
            ))
            .show()
            == rfd::MessageDialogResult::Custom("Allow".into());
        let reply = CString::new(if allowed { text } else { String::new() }).unwrap_or_default();
        // Read after the dialog: its modal loop runs the main queue, where
        // the terminal may have closed and freed its surface meanwhile.
        let surface = view.ivars().surface.get();
        if surface.is_null() {
            return;
        }
        // SAFETY: the view's surface is non-null, so not yet freed (`free`
        // nulls it first); main thread. `state` is the request Ghostty
        // kept for confirmation, completed only here, once; `reply` is
        // NUL-terminated and outlives the call.
        unsafe {
            g::ghostty_surface_complete_clipboard_request(
                surface,
                reply.as_ptr(),
                state as *mut c_void,
                true,
            )
        };
    });
}

extern "C" fn write_clipboard(
    _userdata: *mut c_void,
    _location: g::ghostty_clipboard_e,
    content: *const g::ghostty_clipboard_content_s,
    len: usize,
    confirm: bool,
) {
    if content.is_null() || len == 0 {
        return;
    }
    // SAFETY: Ghostty's array of `len` contents, alive for the call; each
    // one's mime and data are NUL-terminated strings, copied here.
    let items = unsafe { std::slice::from_raw_parts(content, len) };
    let Some(text) = items
        .iter()
        .find(|item| unsafe { string(item.mime) } == "text/plain")
        .map(|item| unsafe { string(item.data) })
    else {
        return;
    };
    on_main(move || {
        if confirm {
            let allowed = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Let the program set your clipboard?")
                .set_description(text.chars().take(400).collect::<String>())
                .set_buttons(rfd::MessageButtons::OkCancelCustom(
                    "Allow".into(),
                    "Cancel".into(),
                ))
                .show()
                == rfd::MessageDialogResult::Custom("Allow".into());
            if !allowed {
                return;
            }
        }
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        // SAFETY: reads AppKit's constant type name, an immutable static.
        unsafe { pasteboard.setString_forType(&NSString::from_str(&text), NSPasteboardTypeString) };
    });
}

// ---------------------------------------------------------------------------
// Foreground processes (what tab titles and icons name).

thread_local! {
    static POLLING: Cell<bool> = const { Cell::new(false) };
}
/// Whether a surface's foreground process must be polled: unless the app
/// resolves it and says when it changes (`terminal_processes_pushed`).
fn polled(view: &TerminalView) -> bool {
    !(crate::terminal_process::pushed() && view.ivars().resolved.get())
}
/// Recheck the polled surfaces' foreground processes twice a second, while
/// there are any.
fn poll_processes(mtm: MainThreadMarker) {
    if POLLING.replace(true) {
        return;
    }
    fn tick(mtm: MainThreadMarker) {
        after(mtm, 0.5, move || {
            let surfaces: Vec<Retained<TerminalView>> =
                SURFACES.with(|surfaces| surfaces.borrow().values().cloned().collect());
            let mut any = false;
            for view in surfaces.iter().filter(|view| polled(view)) {
                any = true;
                refresh_process(view.ivars().surface.get() as usize);
            }
            if any {
                tick(mtm);
            } else {
                POLLING.set(false);
            }
        });
    }
    tick(mtm);
}
/// Every surface asks again what is in front of it (the app's word that it
/// may have changed). From any thread.
pub(crate) fn processes_changed() {
    on_main(|| {
        let surfaces: Vec<usize> =
            SURFACES.with(|surfaces| surfaces.borrow().keys().copied().collect());
        for surface in surfaces {
            refresh_process(surface);
        }
    });
}
fn refresh_process(surface: usize) {
    let Some(view) = view_of(surface) else { return };
    // SAFETY: still in `SURFACES`, so not freed; main thread.
    let pid = unsafe { g::ghostty_surface_foreground_pid(surface as g::ghostty_surface_t) };
    // Seen through (an attach client to a session server): what is behind
    // it changes while the client stays in front, so ask every time.
    let resolved = if pid == 0 {
        None
    } else {
        crate::terminal_process::resolve(pid as u32)
    };
    view.ivars().resolved.set(resolved.is_some());
    if polled(&view) {
        poll_processes(view.mtm());
    }
    let (process, pid, status) = match resolved {
        Some(front) => {
            if view.ivars().front.borrow().as_ref() == Some(&front) {
                return;
            }
            *view.ivars().front.borrow_mut() = Some(front.clone());
            (front.name, u64::from(front.pid), front.status)
        }
        None => {
            if pid == view.ivars().pid.replace(pid) {
                return;
            }
            let process = if pid == 0 {
                String::new()
            } else {
                process_name(pid as i32)
            };
            (process, pid, None)
        }
    };
    let term = view.ivars().term.clone();
    zgx::native_view::post(move || {
        if let Some(term) = term.upgrade() {
            let changed = term
                .state()
                .with_untracked(|state| state.process != process || state.pid != pid as u32);
            term.update_state(|state| {
                state.process = process.clone();
                state.pid = pid as u32;
                state.status = status;
            });
            if changed {
                term.emit(TerminalEvent::ProcessChanged {
                    process,
                    pid: pid as u32,
                });
            }
        }
    });
}
/// A process's name as it presents itself: its `argv[0]` (which programs
/// like Claude Code set to their own name), else its executable's.
fn process_name(pid: i32) -> String {
    argv0(pid)
        .or_else(|| {
            let mut buffer = [0u8; 256];
            // SAFETY: writes at most `buffer.len()` bytes into `buffer`,
            // returning how many.
            let len = unsafe {
                libc::proc_name(pid, buffer.as_mut_ptr() as *mut c_void, buffer.len() as u32)
            };
            (len > 0).then(|| String::from_utf8_lossy(&buffer[..len as usize]).into_owned())
        })
        .map(|name| {
            let name = name.rsplit('/').next().unwrap_or(&name).to_owned();
            // Login shells: "-zsh".
            name.strip_prefix('-').map(str::to_owned).unwrap_or(name)
        })
        .unwrap_or_default()
}
fn argv0(pid: i32) -> Option<String> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 0;
    // SAFETY: `mib` holds the 3 names passed; the first call (no buffer)
    // only writes the size needed into `size`, the second writes at most
    // `size` bytes into a buffer that long and updates `size` to what it
    // wrote. No new value is set (null, 0).
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let mut buffer = vec![0u8; size];
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buffer.truncate(size);
        // argc, the executable path, NUL padding, then argv.
        let rest = buffer.get(4..)?;
        let exe_end = rest.iter().position(|b| *b == 0)?;
        let after_exe = &rest[exe_end..];
        let start = after_exe.iter().position(|b| *b != 0)?;
        let argv = &after_exe[start..];
        let end = argv.iter().position(|b| *b == 0).unwrap_or(argv.len());
        let first = String::from_utf8_lossy(&argv[..end]).trim().to_owned();
        // Some programs rewrite argv[0] as a whole command line.
        let first = first.split(' ').next().unwrap_or("").to_owned();
        (!first.is_empty()).then_some(first)
    }
}

// ---------------------------------------------------------------------------
// Input translation (Ghostty.Input.swift, NSEvent+Extension.swift).

const RIGHT_SHIFT: usize = 0x04;
const RIGHT_CONTROL: usize = 0x2000;
const RIGHT_ALT: usize = 0x40;
const RIGHT_COMMAND: usize = 0x10;

fn mods(flags: NSEventModifierFlags) -> g::ghostty_input_mods_e {
    let mut mods = g::GHOSTTY_MODS_NONE;
    if flags.contains(NSEventModifierFlags::Shift) {
        mods |= g::GHOSTTY_MODS_SHIFT;
    }
    if flags.contains(NSEventModifierFlags::Control) {
        mods |= g::GHOSTTY_MODS_CTRL;
    }
    if flags.contains(NSEventModifierFlags::Option) {
        mods |= g::GHOSTTY_MODS_ALT;
    }
    if flags.contains(NSEventModifierFlags::Command) {
        mods |= g::GHOSTTY_MODS_SUPER;
    }
    if flags.contains(NSEventModifierFlags::CapsLock) {
        mods |= g::GHOSTTY_MODS_CAPS;
    }
    let raw = flags.0;
    if raw & RIGHT_SHIFT != 0 {
        mods |= g::GHOSTTY_MODS_SHIFT_RIGHT;
    }
    if raw & RIGHT_CONTROL != 0 {
        mods |= g::GHOSTTY_MODS_CTRL_RIGHT;
    }
    if raw & RIGHT_ALT != 0 {
        mods |= g::GHOSTTY_MODS_ALT_RIGHT;
    }
    if raw & RIGHT_COMMAND != 0 {
        mods |= g::GHOSTTY_MODS_SUPER_RIGHT;
    }
    mods
}
fn flags_of(mods: g::ghostty_input_mods_e) -> NSEventModifierFlags {
    let mut flags = NSEventModifierFlags(0);
    if mods & g::GHOSTTY_MODS_SHIFT != 0 {
        flags |= NSEventModifierFlags::Shift;
    }
    if mods & g::GHOSTTY_MODS_CTRL != 0 {
        flags |= NSEventModifierFlags::Control;
    }
    if mods & g::GHOSTTY_MODS_ALT != 0 {
        flags |= NSEventModifierFlags::Option;
    }
    if mods & g::GHOSTTY_MODS_SUPER != 0 {
        flags |= NSEventModifierFlags::Command;
    }
    flags
}
/// A key event without text (callers set it, with a lifetime they control).
fn key_event(
    event: &NSEvent,
    action: g::ghostty_input_action_e,
    translation: Option<NSEventModifierFlags>,
) -> g::ghostty_input_key_s {
    let flags = event.modifierFlags();
    let consumed = translation
        .unwrap_or(flags)
        .difference(NSEventModifierFlags::Control | NSEventModifierFlags::Command);
    let mut unshifted = 0;
    let kind = event.r#type();
    if (kind == NSEventType::KeyDown || kind == NSEventType::KeyUp)
        && let Some(chars) = event.charactersByApplyingModifiers(NSEventModifierFlags(0))
        && let Some(first) = chars.to_string().chars().next()
    {
        unshifted = first as u32;
    }
    g::ghostty_input_key_s {
        action,
        mods: mods(flags),
        consumed_mods: mods(consumed),
        keycode: event.keyCode() as u32,
        text: std::ptr::null(),
        unshifted_codepoint: unshifted,
        composing: false,
    }
}
/// The text Ghostty encodes for a key: control characters go without
/// control (Ghostty encodes them), function keys' private-use text goes not at all.
fn ghostty_characters(event: &NSEvent) -> Option<String> {
    let chars = event.characters()?.to_string();
    let mut iter = chars.chars();
    if let (Some(first), None) = (iter.next(), iter.next()) {
        let value = first as u32;
        if value < 0x20 {
            let flags = event
                .modifierFlags()
                .difference(NSEventModifierFlags::Control);
            return event
                .charactersByApplyingModifiers(flags)
                .map(|s| s.to_string());
        }
        if (0xF700..=0xF8FF).contains(&value) {
            return None;
        }
    }
    Some(chars)
}
fn single_control(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if (c as u32) < 0x20)
}

// ---------------------------------------------------------------------------
// The view.

pub(crate) struct ViewState {
    surface: Cell<g::ghostty_surface_t>,
    term: WeakTerminal,
    /// IME preedit text.
    marked: RefCell<String>,
    /// Text `interpretKeyEvents` commits during a key down.
    accumulator: RefCell<Option<Vec<String>>>,
    /// A command key equivalent passed on once, to encode if it comes back.
    last_perform_key: Cell<Option<f64>>,
    focused: Cell<bool>,
    cursor: RefCell<Option<Retained<NSCursor>>>,
    /// Whether the pointer was last over a part of the terminal that takes
    /// input (not under zgx content drawn over it).
    pointer_here: Cell<bool>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
    pid: Cell<u64>,
    /// The app's resolver named what is in front of it last time (and, when
    /// the app pushes changes, nothing need poll it).
    resolved: Cell<bool>,
    /// What a process resolver last saw in front (name, pid).
    front: RefCell<Option<crate::terminal_process::ForegroundProcess>>,
    /// Focus asked for before the view was in a window (a tab switching in).
    pending_focus: Cell<bool>,
    /// The size Ghostty was last given, in points.
    size: Cell<(f64, f64)>,
    /// When the view last took a new size, and the size it was asked for
    /// since, waiting (`set_frame_size`).
    resized_at: Cell<Option<std::time::Instant>>,
    wanted: Cell<Option<(f64, f64)>>,
    /// The window's zoom (`Ui::zoom`): Ghostty draws at the backing scale
    /// times it, so its text grows with the window's, crisply; its points
    /// are the window's divided by it.
    zoom: Cell<f64>,
}
/// How often a size an animation changes every frame (the sidebar easing,
/// panes easing to a new split) reaches the view: each one makes Ghostty
/// draw into new surfaces, about 1.6 ms of CPU. In between, the view keeps
/// its size where its pane placed it (the cut views clip it to the pane);
/// the last size always arrives. A size the user drags is not held back
/// (`dragged_size`).
const RESIZE_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

// SAFETY: NSView's superclass chain is NSResponder, NSObject, and NSView is
// main-thread only. Each method's selector and Rust signature match
// AppKit's declaration (NSView, NSResponder, NSDraggingDestination,
// NSTextInputClient): `BOOL` as `bool`, `NSDragOperation` as `NSUInteger`,
// object arguments AppKit never passes nil as references, and `method_id`
// returns as `Retained` (objc2 hands the caller the reference the method
// convention says).
define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZgxTerminalView"]
    #[ivars = ViewState]
    pub(crate) struct TerminalView;

    impl TerminalView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }
        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            // SAFETY: NSResponder's `- (BOOL)becomeFirstResponder`.
            let ok: bool = unsafe { msg_send![super(self), becomeFirstResponder] };
            if ok {
                self.focus_changed(true);
            }
            ok
        }
        #[unsafe(method(resignFirstResponder))]
        fn resign_first_responder(&self) -> bool {
            // SAFETY: NSResponder's `- (BOOL)resignFirstResponder`.
            let ok: bool = unsafe { msg_send![super(self), resignFirstResponder] };
            if ok {
                self.focus_changed(false);
            }
            ok
        }
        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            let queued = self.ivars().wanted.replace(Some((size.width, size.height))).is_some();
            if !queued {
                self.take_size();
            }
        }
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn did_change_backing(&self) {
            // SAFETY: NSView's `- (void)viewDidChangeBackingProperties`.
            let _: () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
            self.scale_changed();
        }
        #[unsafe(method(viewDidMoveToWindow))]
        fn did_move_to_window(&self) {
            // SAFETY: NSView's `- (void)viewDidMoveToWindow`.
            let _: () = unsafe { msg_send![super(self), viewDidMoveToWindow] };
            self.scale_changed();
            self.screen_changed();
            if let Some(window) = self.window()
                && self.ivars().pending_focus.take()
            {
                window.makeFirstResponder(Some(self));
            }
            self.sync_focus();
        }
        #[unsafe(method(viewDidMoveToSuperview))]
        fn did_move_to_superview(&self) {
            // SAFETY: NSView's `- (void)viewDidMoveToSuperview`.
            let _: () = unsafe { msg_send![super(self), viewDidMoveToSuperview] };
            self.sync_focus();
        }
        #[unsafe(method(viewDidHide))]
        fn did_hide(&self) {
            // SAFETY: NSView's `- (void)viewDidHide`.
            let _: () = unsafe { msg_send![super(self), viewDidHide] };
            // SAFETY: `with_surface` passes a live surface; main thread.
            self.with_surface(|surface| unsafe { g::ghostty_surface_set_occlusion(surface, false) });
        }
        #[unsafe(method(viewDidUnhide))]
        fn did_unhide(&self) {
            // SAFETY: NSView's `- (void)viewDidUnhide`.
            let _: () = unsafe { msg_send![super(self), viewDidUnhide] };
            // SAFETY: `with_surface` passes a live surface; main thread.
            self.with_surface(|surface| unsafe { g::ghostty_surface_set_occlusion(surface, true) });
        }
        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            if let Some(old) = self.ivars().tracking.borrow_mut().take() {
                self.removeTrackingArea(&old);
            }
            // SAFETY: the area does not retain its owner; the owner is this
            // view, which holds the area (and AppKit drops it with the view),
            // so the owner outlives it.
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    self.frame(),
                    NSTrackingAreaOptions::MouseEnteredAndExited
                        | NSTrackingAreaOptions::MouseMoved
                        | NSTrackingAreaOptions::InVisibleRect
                        | NSTrackingAreaOptions::ActiveAlways,
                    Some(self),
                    None,
                )
            };
            self.addTrackingArea(&area);
            *self.ivars().tracking.borrow_mut() = Some(area);
            // SAFETY: NSView's `- (void)updateTrackingAreas`.
            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };
        }
        // No cursor rects: they would win over zgx's cursor where zgx
        // draws over the terminal (a pane's grip). The cursor is set as the
        // pointer moves over the parts that are the terminal's instead.

        // Mouse.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.button(event, g::GHOSTTY_MOUSE_PRESS, g::GHOSTTY_MOUSE_LEFT);
        }
        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.button(event, g::GHOSTTY_MOUSE_RELEASE, g::GHOSTTY_MOUSE_LEFT);
            // SAFETY: `with_surface` passes a live surface; main thread.
            self.with_surface(|surface| unsafe { g::ghostty_surface_mouse_pressure(surface, 0, 0.) });
        }
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            // Unused by the program: the app's context menu.
            if !self.button(event, g::GHOSTTY_MOUSE_PRESS, g::GHOSTTY_MOUSE_RIGHT) {
                super::macos::forward_secondary_click(self, event);
            }
        }
        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.button(event, g::GHOSTTY_MOUSE_RELEASE, g::GHOSTTY_MOUSE_RIGHT);
        }
        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.button(event, g::GHOSTTY_MOUSE_PRESS, other_button(event.buttonNumber()));
        }
        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.button(event, g::GHOSTTY_MOUSE_RELEASE, other_button(event.buttonNumber()));
        }
        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.mouse_position(event);
            self.follow_cursor(event);
        }
        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.mouse_position(event);
        }
        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.mouse_position(event);
        }
        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.mouse_position(event);
        }
        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            self.mouse_position(event);
            self.follow_cursor(event);
        }
        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, event: &NSEvent) {
            // A drag still reports its position past the edge.
            if NSEvent::pressedMouseButtons() != 0 {
                return;
            }
            self.report_pointer(None);
            self.ivars().pointer_here.set(false);
            let flags = mods(event.modifierFlags());
            // SAFETY: `with_surface` passes a live surface; main thread.
            self.with_surface(|surface| unsafe { g::ghostty_surface_mouse_pos(surface, -1., -1., flags) });
        }
        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let (mut x, mut y) = (event.scrollingDeltaX(), event.scrollingDeltaY());
            let precise = event.hasPreciseScrollingDeltas();
            if precise {
                x *= 2.;
                y *= 2.;
            }
            let phase = event.momentumPhase();
            let momentum = if phase.contains(NSEventPhase::Began) {
                g::GHOSTTY_MOUSE_MOMENTUM_BEGAN
            } else if phase.contains(NSEventPhase::Stationary) {
                g::GHOSTTY_MOUSE_MOMENTUM_STATIONARY
            } else if phase.contains(NSEventPhase::Changed) {
                g::GHOSTTY_MOUSE_MOMENTUM_CHANGED
            } else if phase.contains(NSEventPhase::Ended) {
                g::GHOSTTY_MOUSE_MOMENTUM_ENDED
            } else if phase.contains(NSEventPhase::Cancelled) {
                g::GHOSTTY_MOUSE_MOMENTUM_CANCELLED
            } else if phase.contains(NSEventPhase::MayBegin) {
                g::GHOSTTY_MOUSE_MOMENTUM_MAY_BEGIN
            } else {
                g::GHOSTTY_MOUSE_MOMENTUM_NONE
            };
            let scroll_mods = (precise as i32) | ((momentum as i32) << 1);
            // SAFETY: `with_surface` passes a live surface; main thread.
            self.with_surface(|surface| unsafe { g::ghostty_surface_mouse_scroll(surface, x, y, scroll_mods) });
        }
        #[unsafe(method(pressureChangeWithEvent:))]
        fn pressure_change(&self, event: &NSEvent) {
            let (stage, pressure) = (event.stage(), event.pressure());
            // SAFETY: `with_surface` passes a live surface; main thread.
            self.with_surface(|surface| unsafe {
                g::ghostty_surface_mouse_pressure(surface, stage as u32, pressure as f64)
            });
        }

        // Keyboard.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            // A shortcut no menu took (AppKit offers them first) that is a
            // key binding of the app's: the app's, before the program's, up
            // the responder chain to zgx's view, which hears it as one.
            if event
                .modifierFlags()
                .intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control)
                && super::macos::app_binds_key(self, event)
            {
                // SAFETY: AppKit's own responder chain, on the main thread;
                // the next responder is a live view or the window.
                if let Some(next) = unsafe { self.nextResponder() } {
                    next.keyDown(event);
                }
                return;
            }
            self.handle_key_down(event);
        }
        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            self.key_action(g::GHOSTTY_ACTION_RELEASE, event, None, None, false);
        }
        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            self.handle_flags_changed(event);
        }
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            self.handle_key_equivalent(event)
        }
        // NSDraggingDestination's, which NSView declares informally: the
        // view's own methods, not NSTextInputClient's (a debug build checks
        // a protocol block's methods are the protocol's, and would panic).
        // Dropped files and text (as Ghostty's own terminal takes them):
        // pasted, files as their shell-escaped paths, so a program reading
        // a path (an agent taking an image) gets it whole.
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, sender: &AnyObject) -> NSUInteger {
            if dropped_text(sender).is_some() { DRAG_COPY } else { 0 }
        }
        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, sender: &AnyObject) -> NSUInteger {
            if dropped_text(sender).is_some() { DRAG_COPY } else { 0 }
        }
        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, sender: &AnyObject) -> bool {
            self.drop_text(sender)
        }
    }

    // SAFETY: an NSObject subclass, so it conforms to NSObjectProtocol.
    unsafe impl NSObjectProtocol for TerminalView {}

    // SAFETY: it implements NSTextInputClient's required methods, with the
    // protocol's signatures (a debug build checks them).
    unsafe impl NSTextInputClient for TerminalView {
        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            !self.ivars().marked.borrow().is_empty()
        }
        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            let len = self.ivars().marked.borrow().encode_utf16().count();
            if len == 0 {
                NSRange::new(NSNotFound as NSUInteger, 0)
            } else {
                NSRange::new(0, len)
            }
        }
        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            let surface = self.ivars().surface.get();
            if surface.is_null() {
                return NSRange::new(NSNotFound as NSUInteger, 0);
            }
            // SAFETY: non-null, so live (`free` nulls it first); main thread.
            // `ghostty_text_s` is plain C data, valid zeroed; filled when the
            // read succeeds, and only then freed, once.
            unsafe {
                let mut text: g::ghostty_text_s = std::mem::zeroed();
                if !g::ghostty_surface_read_selection(surface, &mut text) {
                    return NSRange::new(NSNotFound as NSUInteger, 0);
                }
                let range = NSRange::new(text.offset_start as usize, text.offset_len as usize);
                g::ghostty_surface_free_text(surface, &mut text);
                range
            }
        }
        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, string: &AnyObject, _selected: NSRange, _replacement: NSRange) {
            *self.ivars().marked.borrow_mut() = text_of(string);
            // Outside a key down (a layout switch mid-composition): show it now.
            if self.ivars().accumulator.borrow().is_none() {
                self.sync_preedit(true);
            }
        }
        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            if !self.ivars().marked.borrow().is_empty() {
                self.ivars().marked.borrow_mut().clear();
                self.sync_preedit(true);
            }
        }
        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }
        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(&self, range: NSRange, _actual: NSRangePointer) -> Option<Retained<NSAttributedString>> {
            self.selection_text(range)
                .map(|text| NSAttributedString::from_nsstring(&NSString::from_str(&text)))
        }
        #[unsafe(method(characterIndexForPoint:))]
        fn character_index(&self, _point: NSPoint) -> NSUInteger {
            0
        }
        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect(&self, range: NSRange, _actual: NSRangePointer) -> NSRect {
            self.ime_rect(range)
        }
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, _replacement: NSRange) {
            let app = objc2_app_kit::NSApplication::sharedApplication(self.mtm());
            if app.currentEvent().is_none() {
                return;
            }
            let chars = text_of(string);
            // Committed: the preedit is over.
            if !self.ivars().marked.borrow().is_empty() {
                self.ivars().marked.borrow_mut().clear();
                self.sync_preedit(true);
            }
            if let Some(accumulator) = self.ivars().accumulator.borrow_mut().as_mut() {
                accumulator.push(chars);
                return;
            }
            if !chars.is_empty() {
                self.committed_text(g::GHOSTTY_ACTION_PRESS, &chars);
            }
        }
        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: Sel) {
            self.redispatch_command();
        }
    }
);

/// `NSDragOperationCopy`: the cursor shows a plus.
const DRAG_COPY: NSUInteger = 1;
/// A drop's contents as a terminal takes them: each file's path, escaped for
/// the shell, or the text; several joined by spaces. `None` for anything
/// else.
fn dropped_text(sender: &AnyObject) -> Option<String> {
    // SAFETY: `sender` is AppKit's NSDraggingInfo, whose
    // `- (NSPasteboard *)draggingPasteboard` returns a non-null pasteboard.
    let pasteboard: Retained<NSPasteboard> = unsafe { msg_send![sender, draggingPasteboard] };
    let items = pasteboard.pasteboardItems()?;
    let texts: Vec<String> = items
        .iter()
        .filter_map(|item| {
            // SAFETY: AppKit's constant type names (here and below),
            // immutable statics.
            if let Some(url) = item.stringForType(unsafe { NSPasteboardTypeFileURL }) {
                // The Finder's are file references (`file:///.file/id=…`).
                let url: Option<Retained<NSURL>> = NSURL::URLWithString(&url);
                let path = url?.filePathURL()?.path()?;
                Some(shell_escape(&path.to_string()))
            } else {
                item.stringForType(unsafe { NSPasteboardTypeString })
                    .map(|text| text.to_string())
            }
        })
        .collect();
    (!texts.is_empty()).then(|| texts.join(" "))
}
/// `path` with the characters a shell reads specially escaped by a
/// backslash (Ghostty's set).
fn shell_escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if "\\ ()[]{}<>\"'`!#$&;|*?\t".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn other_button(number: isize) -> g::ghostty_input_mouse_button_e {
    match number {
        0 => g::GHOSTTY_MOUSE_LEFT,
        1 => g::GHOSTTY_MOUSE_RIGHT,
        2 => g::GHOSTTY_MOUSE_MIDDLE,
        3 => g::GHOSTTY_MOUSE_EIGHT,
        4 => g::GHOSTTY_MOUSE_NINE,
        5 => g::GHOSTTY_MOUSE_SIX,
        6 => g::GHOSTTY_MOUSE_SEVEN,
        7 => g::GHOSTTY_MOUSE_FOUR,
        8 => g::GHOSTTY_MOUSE_FIVE,
        9 => g::GHOSTTY_MOUSE_TEN,
        10 => g::GHOSTTY_MOUSE_ELEVEN,
        _ => g::GHOSTTY_MOUSE_UNKNOWN,
    }
}
/// An `NSString` or `NSAttributedString`'s text.
fn text_of(object: &AnyObject) -> String {
    if let Some(attributed) = object.downcast_ref::<NSAttributedString>() {
        attributed.string().to_string()
    } else if let Some(string) = object.downcast_ref::<NSString>() {
        string.to_string()
    } else {
        String::new()
    }
}

impl TerminalView {
    fn new(term: &Terminal, mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(ViewState {
            surface: Cell::new(std::ptr::null_mut()),
            term: term.downgrade(),
            marked: RefCell::new(String::new()),
            accumulator: RefCell::new(None),
            last_perform_key: Cell::new(None),
            focused: Cell::new(false),
            cursor: RefCell::new(None),
            pointer_here: Cell::new(false),
            tracking: RefCell::new(None),
            pid: Cell::new(0),
            resolved: Cell::new(false),
            front: RefCell::new(None),
            pending_focus: Cell::new(false),
            size: Cell::new((0., 0.)),
            resized_at: Cell::new(None),
            wanted: Cell::new(None),
            zoom: Cell::new(1.),
        });
        // A non-zero first size: Ghostty sizes its first frame from it.
        let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(800., 600.));
        // SAFETY: NSView's designated `- (instancetype)initWithFrame:(NSRect)`
        // on a fresh allocation of this class, its ivars set.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // It takes dropped files and text (`performDragOperation:`).
        // SAFETY: reads AppKit's constant type names, immutable statics.
        let types = unsafe { [NSPasteboardTypeFileURL, NSPasteboardTypeString] };
        view.registerForDraggedTypes(&NSArray::from_slice(&types));
        view
    }
    /// Paste what is dropped on it; whether it took it.
    fn drop_text(&self, sender: &AnyObject) -> bool {
        let Some(text) = dropped_text(sender) else {
            return false;
        };
        if self.ivars().surface.get().is_null() {
            return false;
        }
        // What is dropped on a pane is for it: it takes the keyboard.
        if let Some(window) = self.window() {
            window.makeFirstResponder(Some(self));
        }
        // SAFETY: `with_surface` passes a live surface; main thread. Ghostty
        // reads `len` bytes of `text` (no NUL needed), alive for the call.
        self.with_surface(|surface| unsafe {
            g::ghostty_surface_text(surface, text.as_ptr().cast(), text.len())
        });
        true
    }
    /// Re-dispatch a command key equivalent passed on once (see
    /// `handle_key_equivalent`) so it encodes; anything else: no beep.
    fn redispatch_command(&self) {
        let app = objc2_app_kit::NSApplication::sharedApplication(self.mtm());
        if let (Some(last), Some(current)) =
            (self.ivars().last_perform_key.get(), app.currentEvent())
            && last == current.timestamp()
        {
            app.sendEvent(&current);
        }
    }
    /// The selected text, for input methods and services asking for `range`.
    fn selection_text(&self, range: NSRange) -> Option<String> {
        let surface = self.ivars().surface.get();
        if surface.is_null() || range.length == 0 {
            return None;
        }
        // SAFETY: non-null, so live (`free` nulls it first); main thread.
        // `ghostty_text_s` is plain C data, valid zeroed; filled when the
        // read succeeds, its text (null or NUL-terminated) copied before it
        // is freed, once.
        unsafe {
            let mut text: g::ghostty_text_s = std::mem::zeroed();
            if !g::ghostty_surface_read_selection(surface, &mut text) {
                return None;
            }
            let selected = string(text.text);
            g::ghostty_surface_free_text(surface, &mut text);
            Some(selected)
        }
    }
    /// Run `work` with the view's surface, if it has one. The surface is live
    /// while `work` runs: `MacTerminal::free` nulls it before freeing it, and
    /// both run on the main thread (this view is main-thread only).
    fn with_surface<R>(&self, work: impl FnOnce(g::ghostty_surface_t) -> R) -> Option<R> {
        let surface = self.ivars().surface.get();
        (!surface.is_null()).then(|| work(surface))
    }
    /// Focus as AppKit has it. A first responder removed from its window
    /// (moved to another split, its tab hidden) is never asked to resign.
    fn sync_focus(&self) {
        let first = self
            .window()
            .and_then(|window| window.firstResponder())
            .is_some_and(|responder| {
                std::ptr::eq(
                    Retained::as_ptr(&responder) as *const c_void,
                    self as *const Self as *const c_void,
                )
            });
        self.focus_changed(first);
    }
    fn focus_changed(&self, focused: bool) {
        if self.ivars().focused.replace(focused) == focused {
            return;
        }
        // SAFETY: `with_surface` passes a live surface; main thread.
        self.with_surface(|surface| unsafe { g::ghostty_surface_set_focus(surface, focused) });
        let term = self.ivars().term.clone();
        zgx::native_view::post(move || {
            if let Some(term) = term.upgrade() {
                term.update_state(|state| {
                    state.focused = focused;
                    if focused {
                        state.bell = false;
                    }
                });
            }
        });
    }
    fn backing_scale(&self) -> f64 {
        self.window()
            .map_or(2., |window| window.backingScaleFactor())
    }
    /// Take the size asked for (`set_frame_size`), or wait out the rest of
    /// `RESIZE_EVERY` since the last and take the latest then.
    fn take_size(&self) {
        let Some((width, height)) = self.ivars().wanted.get() else {
            return;
        };
        let now = std::time::Instant::now();
        if let Some(at) = self.ivars().resized_at.get()
            && now - at < RESIZE_EVERY
            && !self.dragged_size()
        {
            let view = self.retain();
            after(
                self.mtm(),
                (RESIZE_EVERY - (now - at)).as_secs_f64(),
                move || view.take_size(),
            );
            return;
        }
        self.ivars().wanted.set(None);
        if self.frame().size == NSSize::new(width, height) {
            return;
        }
        self.ivars().resized_at.set(Some(now));
        let size = NSSize::new(width, height);
        // SAFETY: NSView's `- (void)setFrameSize:(NSSize)`, on the main thread
        // (`self` is main-thread only).
        let _: () = unsafe { msg_send![super(self), setFrameSize: size] };
        self.size_changed();
    }
    /// Whether the user is dragging the size (the window's edge, a divider):
    /// it follows every frame then, as in Ghostty's own window. Ghostty
    /// draws the new size in the same transaction and batches the reflow
    /// (and the program's SIGWINCH) itself.
    fn dragged_size(&self) -> bool {
        self.inLiveResize() || NSEvent::pressedMouseButtons() & 1 != 0
    }
    fn size_changed(&self) {
        let size = self.frame().size;
        if self.ivars().size.replace((size.width, size.height)) == (size.width, size.height) {
            return;
        }
        let scale = self.backing_scale();
        let (width, height) = (
            (size.width * scale).round().max(1.) as u32,
            (size.height * scale).round().max(1.) as u32,
        );
        // SAFETY: `with_surface` passes a live surface; main thread.
        let Some(grid) = self.with_surface(|surface| unsafe {
            g::ghostty_surface_set_size(surface, width, height);
            g::ghostty_surface_size(surface)
        }) else {
            return;
        };
        let term = self.ivars().term.clone();
        zgx::native_view::post(move || {
            if let Some(term) = term.upgrade() {
                term.update_state(|state| {
                    state.columns = grid.columns;
                    state.rows = grid.rows;
                });
            }
        });
    }
    /// Follow the window's zoom (`Ui::zoom`): text and cells grow with it.
    pub(crate) fn set_zoom(&self, zoom: f64) {
        if self.ivars().zoom.replace(zoom) != zoom {
            self.scale_changed();
        }
    }
    fn scale_changed(&self) {
        let scale = self.backing_scale();
        if let Some(layer) = self.layer() {
            let _transaction = super::macos::Transaction::begin();
            layer.setContentsScale(scale);
        }
        // Its pixels stay the backing's; its points are the zoom's.
        let content = scale * self.ivars().zoom.get();
        // SAFETY: `with_surface` passes a live surface; main thread.
        self.with_surface(|surface| unsafe {
            g::ghostty_surface_set_content_scale(surface, content, content)
        });
        self.ivars().size.set((0., 0.));
        self.size_changed();
    }
    fn screen_changed(&self) {
        let Some(screen) = self.window().and_then(|window| window.screen()) else {
            return;
        };
        let description = screen.deviceDescription();
        let key = NSString::from_str("NSScreenNumber");
        let Some(number) = description.objectForKey(&key) else {
            return;
        };
        // SAFETY: AppKit documents NSScreenNumber as an NSNumber, whose
        // `- (unsigned int)unsignedIntValue` this is.
        let id: u32 = unsafe { msg_send![&*number, unsignedIntValue] };
        // SAFETY: `with_surface` passes a live surface; main thread.
        self.with_surface(|surface| unsafe { g::ghostty_surface_set_display_id(surface, id) });
    }
    // The resize cursors' replacements need macOS 15.
    #[allow(deprecated)]
    fn set_cursor_shape(&self, shape: g::ghostty_action_mouse_shape_e) {
        let cursor = match shape {
            g::GHOSTTY_MOUSE_SHAPE_DEFAULT => NSCursor::arrowCursor(),
            g::GHOSTTY_MOUSE_SHAPE_TEXT => NSCursor::IBeamCursor(),
            g::GHOSTTY_MOUSE_SHAPE_VERTICAL_TEXT => NSCursor::IBeamCursorForVerticalLayout(),
            g::GHOSTTY_MOUSE_SHAPE_POINTER => NSCursor::pointingHandCursor(),
            g::GHOSTTY_MOUSE_SHAPE_CROSSHAIR | g::GHOSTTY_MOUSE_SHAPE_CELL => {
                NSCursor::crosshairCursor()
            }
            g::GHOSTTY_MOUSE_SHAPE_GRAB => NSCursor::openHandCursor(),
            g::GHOSTTY_MOUSE_SHAPE_GRABBING | g::GHOSTTY_MOUSE_SHAPE_MOVE => {
                NSCursor::closedHandCursor()
            }
            g::GHOSTTY_MOUSE_SHAPE_CONTEXT_MENU => NSCursor::contextualMenuCursor(),
            g::GHOSTTY_MOUSE_SHAPE_COPY => NSCursor::dragCopyCursor(),
            g::GHOSTTY_MOUSE_SHAPE_ALIAS => NSCursor::dragLinkCursor(),
            g::GHOSTTY_MOUSE_SHAPE_NOT_ALLOWED | g::GHOSTTY_MOUSE_SHAPE_NO_DROP => {
                NSCursor::operationNotAllowedCursor()
            }
            g::GHOSTTY_MOUSE_SHAPE_COL_RESIZE
            | g::GHOSTTY_MOUSE_SHAPE_EW_RESIZE
            | g::GHOSTTY_MOUSE_SHAPE_E_RESIZE
            | g::GHOSTTY_MOUSE_SHAPE_W_RESIZE => NSCursor::resizeLeftRightCursor(),
            g::GHOSTTY_MOUSE_SHAPE_ROW_RESIZE
            | g::GHOSTTY_MOUSE_SHAPE_NS_RESIZE
            | g::GHOSTTY_MOUSE_SHAPE_N_RESIZE
            | g::GHOSTTY_MOUSE_SHAPE_S_RESIZE => NSCursor::resizeUpDownCursor(),
            _ => NSCursor::IBeamCursor(),
        };
        *self.ivars().cursor.borrow_mut() = Some(cursor.clone());
        if self.ivars().pointer_here.get() {
            cursor.set();
        }
    }
    /// Whether input at `event`'s point goes to this view: zgx takes it
    /// where it draws, or has something interactive, over the terminal.
    fn takes(&self, event: &NSEvent) -> bool {
        let Some(content) = self.window().and_then(|window| window.contentView()) else {
            return false;
        };
        // SAFETY: a getter on the main thread; the superview comes back
        // retained.
        let Some(frame) = (unsafe { content.superview() }) else {
            return false;
        };
        let point = frame.convertPoint_fromView(event.locationInWindow(), None);
        content
            .hitTest(point)
            .is_some_and(|hit| hit.isDescendantOf(self))
    }
    /// Show the terminal's cursor while the pointer is over it, and zgx's
    /// over what zgx draws on it (AppKit does not always bring zgx's back
    /// when the pointer crosses onto it).
    fn follow_cursor(&self, event: &NSEvent) {
        let here = self.takes(event);
        self.ivars().pointer_here.set(here);
        if here {
            self.ivars()
                .cursor
                .borrow()
                .clone()
                .unwrap_or_else(NSCursor::IBeamCursor)
                .set();
        } else if let Some(cursor) = super::macos::zgx_cursor_at(self, event) {
            ns_cursor(cursor).set();
        }
    }
    /// Report a mouse button; whether Ghostty consumed it.
    fn button(
        &self,
        event: &NSEvent,
        state: g::ghostty_input_mouse_state_e,
        button: g::ghostty_input_mouse_button_e,
    ) -> bool {
        let flags = mods(event.modifierFlags());
        // SAFETY: `with_surface` passes a live surface; main thread.
        self.with_surface(|surface| unsafe {
            g::ghostty_surface_mouse_button(surface, state, button, flags)
        })
        .unwrap_or(false)
    }
    fn mouse_position(&self, event: &NSEvent) {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        let height = self.frame().size.height;
        let flags = mods(event.modifierFlags());
        // Ghostty's points and zgx's are the window's over its zoom.
        let zoom = self.ivars().zoom.get();
        let (x, y) = (point.x / zoom, (height - point.y) / zoom);
        // SAFETY: `with_surface` passes a live surface; main thread.
        self.with_surface(|surface| unsafe { g::ghostty_surface_mouse_pos(surface, x, y, flags) });
        self.report_pointer(Some((x as f32, y as f32)));
    }
    /// Tell the controller where the pointer is (scripts show affordances
    /// near an edge from it).
    fn report_pointer(&self, pointer: Option<(f32, f32)>) {
        let term = self.ivars().term.clone();
        zgx::native_view::post(move || {
            if let Some(term) = term.upgrade() {
                term.set_pointer(pointer);
            }
        });
    }

    fn handle_key_down(&self, event: &NSEvent) {
        let surface = self.ivars().surface.get();
        if surface.is_null() {
            self.interpretKeyEvents(&NSArray::from_slice(&[event]));
            return;
        }
        // Option-as-alt and the like: the modifiers text translation uses.
        // SAFETY: non-null, so live (`free` nulls it first); main thread.
        let translated = flags_of(unsafe {
            g::ghostty_surface_key_translation_mods(surface, mods(event.modifierFlags()))
        });
        let mut translation = event.modifierFlags();
        for flag in [
            NSEventModifierFlags::Shift,
            NSEventModifierFlags::Control,
            NSEventModifierFlags::Option,
            NSEventModifierFlags::Command,
        ] {
            if translated.contains(flag) {
                translation |= flag;
            } else {
                translation = translation.difference(flag);
            }
        }
        // The same event when nothing changed: input methods (Korean) need it.
        let translation_event: Retained<NSEvent> = if translation == event.modifierFlags() {
            event.retain()
        } else {
            let chars = event
                .charactersByApplyingModifiers(translation)
                .unwrap_or_else(|| NSString::from_str(""));
            let ignoring = event
                .charactersIgnoringModifiers()
                .unwrap_or_else(|| NSString::from_str(""));
            NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                event.r#type(),
                event.locationInWindow(),
                translation,
                event.timestamp(),
                event.windowNumber(),
                None,
                &chars,
                &ignoring,
                event.isARepeat(),
                event.keyCode(),
            )
            .unwrap_or_else(|| event.retain())
        };
        let action = if event.isARepeat() {
            g::GHOSTTY_ACTION_REPEAT
        } else {
            g::GHOSTTY_ACTION_PRESS
        };
        *self.ivars().accumulator.borrow_mut() = Some(Vec::new());
        let marked_before = !self.ivars().marked.borrow().is_empty();
        self.ivars().last_perform_key.set(None);
        self.interpretKeyEvents(&NSArray::from_slice(&[&*translation_event]));
        let accumulated = self
            .ivars()
            .accumulator
            .borrow_mut()
            .take()
            .unwrap_or_default();

        self.sync_preedit(marked_before);
        let composing = !self.ivars().marked.borrow().is_empty() || marked_before;

        if marked_before && !accumulated.is_empty() {
            // The input method committed (part of) its preedit.
            for text in &accumulated {
                if composing && single_control(text) {
                    continue;
                }
                self.committed_text(action, text);
            }
            if replays_after_commit(&translation_event) {
                self.key_action(action, event, Some(&translation_event), None, false);
            }
            return;
        }
        if !accumulated.is_empty() {
            for text in &accumulated {
                if composing && single_control(text) {
                    continue;
                }
                self.key_action(action, event, Some(&translation_event), Some(text), false);
            }
        } else {
            if composing
                && event
                    .characters()
                    .is_some_and(|chars| single_control(&chars.to_string()))
            {
                return;
            }
            let text = ghostty_characters(&translation_event);
            self.key_action(
                action,
                event,
                Some(&translation_event),
                text.as_deref(),
                composing,
            );
        }
    }
    fn key_action(
        &self,
        action: g::ghostty_input_action_e,
        event: &NSEvent,
        translation: Option<&NSEvent>,
        text: Option<&str>,
        composing: bool,
    ) -> bool {
        let surface = self.ivars().surface.get();
        if surface.is_null() {
            return false;
        }
        let mut key = key_event(event, action, translation.map(|t| t.modifierFlags()));
        key.composing = composing;
        // Ghostty encodes control characters itself (keeping the physical
        // key for protocols like the Kitty keyboard protocol).
        // SAFETY: for both calls, non-null so live (`free` nulls it first);
        // main thread. `key.text` is null or a NUL-terminated `CString`
        // alive past the call (Ghostty reads it during the call only).
        match text.filter(|text| !text.is_empty() && !text.starts_with(|c: char| (c as u32) < 0x20))
        {
            Some(text) => {
                let text = CString::new(text).unwrap_or_default();
                key.text = text.as_ptr();
                unsafe { g::ghostty_surface_key(surface, key) }
            }
            None => unsafe { g::ghostty_surface_key(surface, key) },
        }
    }
    /// Committed IME text (or dictation): typed input, never a paste.
    fn committed_text(&self, action: g::ghostty_input_action_e, text: &str) {
        let surface = self.ivars().surface.get();
        if surface.is_null() {
            return;
        }
        let text = CString::new(text).unwrap_or_default();
        let key = g::ghostty_input_key_s {
            action,
            mods: g::GHOSTTY_MODS_NONE,
            consumed_mods: g::GHOSTTY_MODS_NONE,
            keycode: 0,
            text: text.as_ptr(),
            unshifted_codepoint: 0,
            composing: false,
        };
        // SAFETY: non-null, so live (`free` nulls it first); main thread.
        // `key.text` is a NUL-terminated `CString` alive past the call.
        unsafe { g::ghostty_surface_key(surface, key) };
    }
    fn sync_preedit(&self, clear_if_needed: bool) {
        let surface = self.ivars().surface.get();
        if surface.is_null() {
            return;
        }
        let marked = self.ivars().marked.borrow().clone();
        // SAFETY: for both calls, non-null so live (`free` nulls it first);
        // main thread. Ghostty reads `len` bytes from the pointer (none to
        // clear), which is never null: Zig's `[*]const u8` may not be.
        if !marked.is_empty() {
            let text = CString::new(marked).unwrap_or_default();
            let len = text.as_bytes().len();
            unsafe { g::ghostty_surface_preedit(surface, text.as_ptr(), len) };
        } else if clear_if_needed {
            unsafe { g::ghostty_surface_preedit(surface, c"".as_ptr(), 0) };
        }
    }
    fn handle_flags_changed(&self, event: &NSEvent) {
        let modifier = match event.keyCode() {
            0x39 => g::GHOSTTY_MODS_CAPS,
            0x38 | 0x3C => g::GHOSTTY_MODS_SHIFT,
            0x3B | 0x3E => g::GHOSTTY_MODS_CTRL,
            0x3A | 0x3D => g::GHOSTTY_MODS_ALT,
            0x37 | 0x36 => g::GHOSTTY_MODS_SUPER,
            _ => return,
        };
        if !self.ivars().marked.borrow().is_empty() {
            return;
        }
        let flags = event.modifierFlags();
        let mut action = g::GHOSTTY_ACTION_RELEASE;
        if mods(flags) & modifier != 0 {
            let side = match event.keyCode() {
                0x3C => flags.0 & RIGHT_SHIFT != 0,
                0x3E => flags.0 & RIGHT_CONTROL != 0,
                0x3D => flags.0 & RIGHT_ALT != 0,
                0x36 => flags.0 & RIGHT_COMMAND != 0,
                _ => true,
            };
            if side {
                action = g::GHOSTTY_ACTION_PRESS;
            }
        }
        self.key_action(action, event, None, None, false);
    }
    fn handle_key_equivalent(&self, event: &NSEvent) -> bool {
        if event.r#type() != NSEventType::KeyDown || !self.ivars().focused.get() {
            return false;
        }
        let surface = self.ivars().surface.get();
        if surface.is_null() {
            return false;
        }
        // A shortcut the app keeps: the app's, before Ghostty's own binding.
        if super::macos::take_reserved_key(self, event) {
            return true;
        }
        // A Ghostty binding (copy, paste, font size, ...): Ghostty's.
        let mut key = key_event(event, g::GHOSTTY_ACTION_PRESS, None);
        let chars = event
            .characters()
            .map(|chars| chars.to_string())
            .unwrap_or_default();
        let chars = CString::new(chars).unwrap_or_default();
        key.text = chars.as_ptr();
        let mut flags: g::ghostty_binding_flags_e = 0;
        // SAFETY: non-null, so live (`free` nulls it first); main thread.
        // `key.text` is a NUL-terminated `CString` alive past the call;
        // `flags` is a writable local.
        if unsafe { g::ghostty_surface_key_is_binding(surface, key, &mut flags) } {
            self.handle_key_down(event);
            return true;
        }
        let ignoring = event
            .charactersIgnoringModifiers()
            .map(|chars| chars.to_string())
            .unwrap_or_default();
        let flags = event.modifierFlags();
        let equivalent = match ignoring.as_str() {
            // Pass control-return through verbatim.
            "\r" if flags.contains(NSEventModifierFlags::Control) => "\r".to_owned(),
            "\r" => return false,
            // Control-slash as control-underscore (no beep).
            "/" if flags.contains(NSEventModifierFlags::Control)
                && !flags.intersects(
                    NSEventModifierFlags::Shift
                        | NSEventModifierFlags::Command
                        | NSEventModifierFlags::Option,
                ) =>
            {
                "_".to_owned()
            }
            "/" => return false,
            _ => {
                if event.timestamp() == 0. {
                    return false;
                }
                if !flags.intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control)
                {
                    self.ivars().last_perform_key.set(None);
                    return false;
                }
                // Not a binding: let the menus try it first. If nothing
                // takes it, it comes back (doCommandBySelector:) to encode.
                if self.ivars().last_perform_key.take() == Some(event.timestamp()) {
                    event
                        .characters()
                        .map(|c| c.to_string())
                        .unwrap_or_default()
                } else {
                    self.ivars().last_perform_key.set(Some(event.timestamp()));
                    return false;
                }
            }
        };
        let text = NSString::from_str(&equivalent);
        let Some(synthetic) = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            event.locationInWindow(),
            flags,
            event.timestamp(),
            event.windowNumber(),
            None,
            &text,
            &text,
            event.isARepeat(),
            event.keyCode(),
        ) else {
            return false;
        };
        self.handle_key_down(&synthetic);
        true
    }
    fn ime_rect(&self, range: NSRange) -> NSRect {
        let surface = self.ivars().surface.get();
        let frame = self.frame();
        if surface.is_null() {
            return NSRect::new(frame.origin, NSSize::new(0., 0.));
        }
        let (mut x, mut y, mut width, mut height) = (0., 0., 0., 0.);
        // SAFETY: non-null, so live (`free` nulls it first); main thread.
        // It writes one `f64` through each pointer, all writable locals.
        unsafe { g::ghostty_surface_ime_point(surface, &mut x, &mut y, &mut width, &mut height) };
        // In Ghostty's points: the window's are the zoom's times them.
        let zoom = self.ivars().zoom.get();
        let (mut x, y, mut width, height) = (x * zoom, y * zoom, width * zoom, height * zoom);
        if range.length == 0 && width > 0. {
            x += width * (range.location + range.length) as f64;
            width = 0.;
        }
        let rect = NSRect::new(
            NSPoint::new(x, frame.size.height - y),
            NSSize::new(width, height),
        );
        let in_window = self.convertRect_toView(rect, None);
        match self.window() {
            Some(window) => window.convertRectToScreen(in_window),
            None => in_window,
        }
    }
}
/// AppKit's cursor for a zgx one.
// The resize cursors' replacements need macOS 15.
#[allow(deprecated)]
fn ns_cursor(cursor: zgx::cursor::Cursor) -> Retained<NSCursor> {
    use zgx::cursor::Cursor;
    match cursor {
        Cursor::Pointer => NSCursor::pointingHandCursor(),
        Cursor::Text => NSCursor::IBeamCursor(),
        Cursor::VerticalText => NSCursor::IBeamCursorForVerticalLayout(),
        Cursor::Crosshair => NSCursor::crosshairCursor(),
        Cursor::Grab | Cursor::Move | Cursor::AllScroll => NSCursor::openHandCursor(),
        Cursor::Grabbing => NSCursor::closedHandCursor(),
        Cursor::NotAllowed | Cursor::NoDrop => NSCursor::operationNotAllowedCursor(),
        Cursor::ContextMenu => NSCursor::contextualMenuCursor(),
        Cursor::Copy => NSCursor::dragCopyCursor(),
        Cursor::Alias => NSCursor::dragLinkCursor(),
        Cursor::ColResize | Cursor::EwResize => NSCursor::resizeLeftRightCursor(),
        Cursor::RowResize | Cursor::NsResize => NSCursor::resizeUpDownCursor(),
        _ => NSCursor::arrowCursor(),
    }
}

/// Arrow keys an input method's commit should still move the cursor with.
fn replays_after_commit(event: &NSEvent) -> bool {
    match event.keyCode() {
        // Right, down, up.
        0x7C..=0x7E => true,
        // Left only with a modifier (Korean IMEs leave the caret after commit).
        0x7B => event.modifierFlags().intersects(
            NSEventModifierFlags::Shift
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option
                | NSEventModifierFlags::Command,
        ),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// The backend.

/// A Ghostty surface in its view: the platform side of a `Terminal`.
pub(crate) struct MacTerminal {
    view: Retained<TerminalView>,
}
impl MacTerminal {
    fn new(term: &Terminal, mtm: MainThreadMarker) -> Option<Self> {
        let app = app()?;
        let view = TerminalView::new(term, mtm);
        let options = term.options();
        let command = options
            .command
            .as_deref()
            .and_then(|c| CString::new(c).ok());
        let cwd = options
            .working_directory
            .as_ref()
            .and_then(|dir| CString::new(dir.to_string_lossy().as_bytes()).ok());
        let mut env: Vec<(CString, CString)> = options
            .env
            .iter()
            .filter_map(|(k, v)| {
                Some((
                    CString::new(k.as_str()).ok()?,
                    CString::new(v.as_str()).ok()?,
                ))
            })
            .collect();
        // No "Last login" banner (see zgx-ghostty-sys's patch).
        env.push((c"GHOSTTY_QUIET_LOGIN".into(), c"1".into()));
        let mut env_vars: Vec<g::ghostty_env_var_s> = env
            .iter()
            .map(|(key, value)| g::ghostty_env_var_s {
                key: key.as_ptr(),
                value: value.as_ptr(),
            })
            .collect();
        // SAFETY: main thread; `app` is live (never freed). The config's
        // strings and `env_vars` (pointing into `env`) outlive the call, and
        // Ghostty copies what it keeps (its own app passes them scoped to
        // the call, too). `nsview` is the view the surface renders into:
        // `MacTerminal` keeps it alive until after `ghostty_surface_free`.
        // `userdata` is only compared as an address by the callbacks, never
        // dereferenced.
        let surface = unsafe {
            let mut config = g::ghostty_surface_config_new();
            config.platform_tag = g::GHOSTTY_PLATFORM_MACOS;
            config.platform = g::ghostty_platform_u {
                macos: g::ghostty_platform_macos_s {
                    nsview: Retained::as_ptr(&view) as *mut c_void,
                },
            };
            config.userdata = Retained::as_ptr(&view) as *mut c_void;
            config.scale_factor = 2.;
            config.font_size = options.font_size.unwrap_or(0.);
            config.working_directory = cwd.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
            config.command = command.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
            config.env_vars = env_vars.as_mut_ptr();
            config.env_var_count = env_vars.len();
            config.context = g::GHOSTTY_SURFACE_CONTEXT_WINDOW;
            g::ghostty_surface_new(app, &config)
        };
        if surface.is_null() {
            eprintln!("zgx: ghostty_surface_new failed");
            return None;
        }
        view.ivars().surface.set(surface);
        SURFACES.with(|surfaces| surfaces.borrow_mut().insert(surface as usize, view.clone()));
        // SAFETY: just made, live; main thread.
        unsafe { g::ghostty_surface_set_focus(surface, false) };
        refresh_process(surface as usize);
        view.scale_changed();
        Some(Self { view })
    }
    pub fn view(&self) -> &NSView {
        &self.view
    }
    /// Follow the window's zoom (see `TerminalView::set_zoom`).
    pub(crate) fn set_zoom(&self, zoom: f64) {
        self.view.set_zoom(zoom);
    }
    fn free(&self) {
        let surface = self.view.ivars().surface.replace(std::ptr::null_mut());
        if surface.is_null() {
            return;
        }
        SURFACES.with(|surfaces| surfaces.borrow_mut().remove(&(surface as usize)));
        self.view.removeFromSuperview();
        // SAFETY: Ghostty's surface, freed once: the view's pointer is null
        // and the address out of `SURFACES` first, so nothing reaches it
        // after (callbacks queued for it find nothing). The view it renders
        // into is still alive (`self.view`). Main thread.
        unsafe { g::ghostty_surface_free(surface) };
    }
}
impl TerminalBackend for MacTerminal {
    fn snapshot(&self, max_width: u32) -> Option<std::sync::Arc<zgx::image::ImageData>> {
        // Ghostty's renderer makes the view's layer an IOSurface layer, its
        // contents the IOSurface it last drew (BGRA, premultiplied).
        let layer = self.view.layer()?;
        // SAFETY: `contents` is a CALayer property, an object or nil.
        let contents: *mut AnyObject = unsafe { msg_send![&*layer, contents] };
        if contents.is_null() {
            return None;
        }
        // SAFETY: `contents` is a live CF object (the layer holds it); its
        // type is compared with IOSurface's before it is used as one.
        let surface = unsafe {
            if iosurface::CFGetTypeID(contents.cast()) != iosurface::IOSurfaceGetTypeID() {
                return None;
            }
            contents.cast::<c_void>()
        };
        iosurface::thumbnail(surface, max_width)
    }
    fn run(&self, command: TerminalCommand) {
        let surface = self.view.ivars().surface.get();
        match command {
            TerminalCommand::Focus(focus) => {
                let Some(window) = self.view.window() else {
                    self.view.ivars().pending_focus.set(focus);
                    return;
                };
                let first = window.firstResponder().is_some_and(|responder| {
                    std::ptr::eq(
                        Retained::as_ptr(&responder) as *const c_void,
                        Retained::as_ptr(&self.view) as *const c_void,
                    )
                });
                if focus && !first {
                    window.makeFirstResponder(Some(&self.view));
                } else if !focus && first {
                    window.makeFirstResponder(
                        window
                            .contentView()
                            .as_deref()
                            .map(|view| -> &NSResponder { view }),
                    );
                }
            }
            TerminalCommand::Input(text) if !surface.is_null() => {
                // As typed: one committed-text key event per line ending.
                self.view.committed_text(g::GHOSTTY_ACTION_PRESS, &text);
            }
            TerminalCommand::Paste(text) if !surface.is_null() => {
                // SAFETY: non-null, so live (`free` nulls it first); main
                // thread. Ghostty reads `len` bytes of `text` (no NUL
                // needed), alive for the call.
                unsafe { g::ghostty_surface_text(surface, text.as_ptr().cast(), text.len()) };
            }
            TerminalCommand::Binding(action) if !surface.is_null() => {
                // SAFETY: as above; Ghostty parses `len` bytes of `action`.
                unsafe {
                    g::ghostty_surface_binding_action(
                        surface,
                        action.as_ptr() as *const c_char,
                        action.len(),
                    )
                };
            }
            TerminalCommand::Close => self.free(),
            _ => {}
        }
    }
    fn needs_confirm_close(&self) -> bool {
        let surface = self.view.ivars().surface.get();
        if surface.is_null() {
            return false;
        }
        // SAFETY: for both calls, non-null so live (`free` nulls it first);
        // main thread (the backend is the main thread's `Rc`).
        let pid = unsafe { g::ghostty_surface_foreground_pid(surface) };
        if pid != 0
            && let Some(front) = crate::terminal_process::resolve(pid as u32)
        {
            return !front.idle;
        }
        unsafe { g::ghostty_surface_needs_confirm_quit(surface) }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Drop for MacTerminal {
    fn drop(&mut self) {
        self.free();
    }
}

/// The terminal's platform backend, made on first mount. `None` once closed
/// or if Ghostty could not start.
pub(crate) fn backend(term: &Terminal, mtm: MainThreadMarker) -> Option<Rc<dyn TerminalBackend>> {
    if let Some(backend) = term.backend()
        && backend.as_any().is::<MacTerminal>()
    {
        return Some(backend);
    }
    if term.is_closed() {
        return None;
    }
    let backend: Rc<dyn TerminalBackend> = Rc::new(MacTerminal::new(term, mtm)?);
    term.attach(backend.clone());
    Some(backend)
}

#[cfg(test)]
mod tests {
    use super::shell_escape;

    #[test]
    fn dropped_paths_are_escaped_for_the_shell() {
        assert_eq!(
            shell_escape("/Users/me/Screen Shot (2).png"),
            r"/Users/me/Screen\ Shot\ \(2\).png"
        );
        assert_eq!(shell_escape("/tmp/a'b&c$d"), r"/tmp/a\'b\&c\$d");
        assert_eq!(shell_escape("/plain/path.jpg"), "/plain/path.jpg");
    }
}

/// Reading a terminal's last frame (an IOSurface of Ghostty's) into a
/// thumbnail.
mod iosurface {
    use std::ffi::c_void;

    #[link(name = "IOSurface", kind = "framework")]
    unsafe extern "C" {
        pub fn IOSurfaceGetTypeID() -> usize;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub fn CFGetTypeID(object: *const c_void) -> usize;
    }

    /// `surface` (live, BGRA with premultiplied alpha) scaled down by a whole
    /// factor to at most `max_width` pixels wide, each pixel the average of
    /// those it covers, as straight-alpha RGBA.
    pub fn thumbnail(
        surface: *const c_void,
        max_width: u32,
    ) -> Option<std::sync::Arc<zgx::image::ImageData>> {
        // SAFETY: `surface` is a live IOSurface of four bytes a pixel (the
        // caller checked its type, and its layer holds it).
        unsafe {
            zgx_gpu::iosurface::with_pixels(surface, |bytes, width, height, stride| {
                crate::thumbnail::image(bytes, width, height, stride, max_width as usize)
            })
            .flatten()
        }
    }
}
