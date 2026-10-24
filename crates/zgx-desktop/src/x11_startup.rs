//! The end of another launch's X11 startup notification: a launch of the
//! app that a running instance served (a D-Bus `Activate` or `Open` with its
//! `desktop-startup-id`) is complete when the running one says so, and only
//! then does the launcher stop its busy cursor. winit says it for the launch
//! a window was made with, not for one handed over later.
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask},
};

/// Say that launch `id` is complete, from `window`.
pub(crate) fn complete(window: &Window, id: &str) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let from = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return,
    };
    let Ok((connection, screen)) = x11rb::connect(None) else {
        return;
    };
    let root = connection.setup().roots[screen].root;
    let atom = |name: &str| {
        Some(
            connection
                .intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    };
    let (Some(begin), Some(more)) = (atom("_NET_STARTUP_INFO_BEGIN"), atom("_NET_STARTUP_INFO"))
    else {
        return;
    };
    for (index, chunk) in message(id).chunks(20).enumerate() {
        let mut data = [0u8; 20];
        data[..chunk.len()].copy_from_slice(chunk);
        let kind = if index == 0 { begin } else { more };
        let event = ClientMessageEvent::new(8, from, kind, data);
        if connection
            .send_event(false, root, EventMask::PROPERTY_CHANGE, event)
            .is_err()
        {
            return;
        }
    }
    let _ = connection.flush();
}

/// The startup notification protocol's `remove` message for `id`, with its
/// terminating NUL: a value with a space, quote or backslash is quoted.
fn message(id: &str) -> Vec<u8> {
    let value = if id.contains([' ', '"', '\\']) {
        format!("\"{}\"", id.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        id.to_owned()
    };
    let mut bytes = format!("remove: ID={value}").into_bytes();
    bytes.push(0);
    bytes
}

#[cfg(test)]
mod tests {
    use super::message;

    #[test]
    fn a_remove_message_quotes_what_needs_it() {
        assert_eq!(message("zgx_TIME42"), b"remove: ID=zgx_TIME42\0");
        assert_eq!(message("a b\"c"), b"remove: ID=\"a b\\\"c\"\0");
    }
}
