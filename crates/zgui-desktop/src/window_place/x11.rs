//! Where a new window opens on X11 (see the module's notes): a point of its
//! content put under the pointer.
use super::Placed;

/// Place `window` (not shown yet): with content point `under_pointer`
/// (logical) under the pointer.
pub(crate) fn place(
    window: &winit::window::Window,
    under_pointer: Option<(f64, f64)>,
    _: bool,
    _: &crate::WindowOptions,
) -> Placed {
    if let Some(point) = under_pointer {
        under_pointer_at(window, point);
    }
    Placed::default()
}

/// Put content point `(x, y)` (logical) of `window` (not shown yet) under
/// the pointer.
fn under_pointer_at(window: &winit::window::Window, (x, y): (f64, f64)) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        properties::{WmSizeHints, WmSizeHintsSpecification},
        protocol::xproto::{ConfigureWindowAux, ConnectionExt as _},
        wrapper::ConnectionExt as _,
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let id = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return,
    };
    let Ok((connection, screen)) = x11rb::connect(None) else {
        return;
    };
    let root = connection.setup().roots[screen].root;
    let Some(pointer) = connection
        .query_pointer(root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return;
    };
    let frame = crate::x11_work_area::frame_extents(&connection, root, id).unwrap_or_default();
    let scale = window.scale_factor();
    // The frame's top-left: the content's, less the frame's left and top.
    let left = pointer.root_x as i32 - (x * scale).round() as i32 - frame.0 as i32;
    let top = pointer.root_y as i32 - (y * scale).round() as i32 - frame.2 as i32;
    let mut hints = WmSizeHints::get_normal_hints(&connection, id)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .flatten()
        .unwrap_or_default();
    hints.position = Some((WmSizeHintsSpecification::UserSpecified, left, top));
    let _ = hints.set_normal_hints(&connection, id);
    let _ = connection.configure_window(id, &ConfigureWindowAux::new().x(left).y(top));
    // A round trip, not a flush: winit's connection rewrites the hints (a
    // size request reads them back first) and maps the window, and the
    // server takes two connections' requests in no set order. Once this
    // returns, the hints and position are the server's, and the window
    // manager sees them as it maps the window.
    let _ = connection.sync();
}
