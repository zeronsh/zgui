//! winit's X11 backend queues an input method's spot
//! (`set_ime_cursor_area`) and applies it after it next handles an X event.
//! The last preedit of a word is often the last event for a while, so the
//! caret it moves sent its spot nowhere until the next key: IBus left its
//! candidates where a longer preedit had put the caret ("中哦" before
//! "中", one character to the right). A message to the window itself, on
//! winit's own connection, is that next event, at once.
use std::sync::OnceLock;
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
    window::Window,
};
use x11_dl::xlib;

/// Have winit handle what it queued for `window`'s X11 connection now.
pub(crate) fn nudge(window: &Window) {
    static XLIB: OnceLock<Option<xlib::Xlib>> = OnceLock::new();
    let Some(x) = XLIB.get_or_init(|| xlib::Xlib::open().ok()) else {
        return;
    };
    let (Ok(display), Ok(handle)) = (window.display_handle(), window.window_handle()) else {
        return;
    };
    let (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(handle)) =
        (display.as_raw(), handle.as_raw())
    else {
        return;
    };
    let Some(display) = display.display else {
        return;
    };
    let display = display.as_ptr().cast::<xlib::Display>();
    // SAFETY: winit's display, open while `window` (which holds winit's
    // connection) is borrowed, on the thread that runs its event loop
    // (winit made Xlib thread-safe besides, XInitThreads). The atom is
    // interned on it once: winit's one event loop has one display per
    // process. The event is a zeroed union with its client-message member
    // written, a plain value naming our own window, which winit ignores (an
    // atom it does not know).
    unsafe {
        static ATOM: OnceLock<xlib::Atom> = OnceLock::new();
        let atom = *ATOM.get_or_init(|| (x.XInternAtom)(display, c"ZGUI_NUDGE".as_ptr(), 0));
        let mut event: xlib::XEvent = std::mem::zeroed();
        event.client_message = xlib::XClientMessageEvent {
            type_: xlib::ClientMessage,
            serial: 0,
            send_event: 1,
            display,
            window: handle.window,
            message_type: atom,
            format: 32,
            data: xlib::ClientMessageData::new(),
        };
        (x.XSendEvent)(display, handle.window, 0, 0, &mut event);
        (x.XFlush)(display);
    }
}
