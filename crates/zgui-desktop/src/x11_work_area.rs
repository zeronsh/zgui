//! A new window fitted to its monitor's work area on X11 (the screen less
//! its panels, `_NET_WORKAREA`): window managers show a window as large as
//! it asks, past the screen's edge (Openbox does; Mutter maximizes one near
//! the screen's size), where Chromium and GTK fit theirs first. A size asked
//! for in points can be more pixels than the screen has at a high scale. On
//! Wayland the compositor bounds a new window's size (`configure_bounds`)
//! and winit keeps to it.
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use x11rb::{
    connection::Connection,
    protocol::Event,
    protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, PropertyNotifyEvent,
    },
};

/// Shrink the hidden, never shown `window` to fit its monitor's work area,
/// its frame included (`decorated`), though not below `min` (logical).
pub(crate) fn fit(window: &Window, min: Option<(f64, f64)>, decorated: bool) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let id = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return,
    };
    let Some(monitor) = window
        .current_monitor()
        .or_else(|| window.primary_monitor())
    else {
        return;
    };
    let Ok((connection, screen)) = x11rb::connect(None) else {
        return;
    };
    let root = connection.setup().roots[screen].root;
    let Some(work) = work_area(&connection, root) else {
        return;
    };
    let (position, size) = (monitor.position(), monitor.size());
    let Some(work) = intersect(
        work,
        (
            position.x,
            position.y,
            size.width as i32,
            size.height as i32,
        ),
    ) else {
        return;
    };
    let inner = window.inner_size();
    // Room for any title bar: the window fits, nothing to ask the window
    // manager.
    if fitted((inner.width, inner.height), ROOMY, work, (1, 1)).is_none() {
        return;
    }
    let frame = if decorated {
        frame_extents(&connection, root, id).unwrap_or_default()
    } else {
        Frame::default()
    };
    let scale = window.scale_factor();
    let min = min.map_or((1, 1), |(w, h)| {
        ((w * scale).ceil() as u32, (h * scale).ceil() as u32)
    });
    if let Some((width, height)) = fitted((inner.width, inner.height), frame, work, min) {
        let _ = window.request_inner_size(PhysicalSize::new(width, height));
    }
}

/// A frame larger than window managers draw: a window that fits with it
/// fits with theirs.
const ROOMY: Frame = Frame(16, 16, 64, 16);

/// A window manager's frame around a window, in pixels: left, right, top,
/// bottom (`_NET_FRAME_EXTENTS`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Frame(pub u32, pub u32, pub u32, pub u32);

/// The inner size that fits `inner` and its `frame` in the `work` area
/// (x, y, width, height), though not below `min`; `None` if it fits.
pub(crate) fn fitted(
    inner: (u32, u32),
    Frame(left, right, top, bottom): Frame,
    work: (i32, i32, i32, i32),
    min: (u32, u32),
) -> Option<(u32, u32)> {
    let room = |work: i32, frame: u32| (work.max(0) as u32).saturating_sub(frame);
    let width = inner.0.min(room(work.2, left + right)).max(min.0);
    let height = inner.1.min(room(work.3, top + bottom)).max(min.1);
    ((width, height) != inner).then_some((width, height))
}

pub(crate) fn intersect(
    a: (i32, i32, i32, i32),
    b: (i32, i32, i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let (x, y) = (a.0.max(b.0), a.1.max(b.1));
    let (right, bottom) = ((a.0 + a.2).min(b.0 + b.2), (a.1 + a.3).min(b.1 + b.3));
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
}

fn atom(connection: &impl Connection, name: &str) -> Option<u32> {
    Some(
        connection
            .intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()?
            .atom,
    )
}

fn cardinals(connection: &impl Connection, window: u32, property: u32) -> Option<Vec<u32>> {
    let reply = connection
        .get_property(false, window, property, AtomEnum::CARDINAL, 0, 64)
        .ok()?
        .reply()
        .ok()?;
    Some(reply.value32()?.collect())
}

/// The current desktop's work area, on a connection kept for it: what
/// popups may cover (`popup_window`), asked as the window moves.
pub(crate) fn current() -> Option<(i32, i32, i32, i32)> {
    thread_local! {
        static CONNECTION: Option<(x11rb::rust_connection::RustConnection, u32)> =
            x11rb::connect(None).ok().map(|(connection, screen)| {
                let root = connection.setup().roots[screen].root;
                (connection, root)
            });
    }
    CONNECTION.with(|connection| {
        let (connection, root) = connection.as_ref()?;
        work_area(connection, *root)
    })
}

/// The work area of the current desktop (`_NET_WORKAREA`, x, y, width,
/// height per desktop).
fn work_area(connection: &impl Connection, root: u32) -> Option<(i32, i32, i32, i32)> {
    let areas = cardinals(connection, root, atom(connection, "_NET_WORKAREA")?)?;
    let desktop = atom(connection, "_NET_CURRENT_DESKTOP")
        .and_then(|current| cardinals(connection, root, current))
        .and_then(|d| d.first().copied())
        .unwrap_or(0) as usize;
    let (areas, _) = areas.as_chunks::<4>();
    let area = areas.get(desktop).or(areas.first())?;
    Some((
        area[0] as i32,
        area[1] as i32,
        area[2] as i32,
        area[3] as i32,
    ))
}

/// The frame the window manager will put around `window` before it is
/// shown: asked for with `_NET_REQUEST_FRAME_EXTENTS`, which the window
/// manager answers by setting `_NET_FRAME_EXTENTS` on it (EWMH). `None`
/// when the window manager does not say it answers (`_NET_SUPPORTED`), and
/// after a moment's wait if it does not.
pub(crate) fn frame_extents(connection: &impl Connection, root: u32, window: u32) -> Option<Frame> {
    let extents = atom(connection, "_NET_FRAME_EXTENTS")?;
    let request = atom(connection, "_NET_REQUEST_FRAME_EXTENTS")?;
    let supported = connection
        .get_property(
            false,
            root,
            atom(connection, "_NET_SUPPORTED")?,
            AtomEnum::ATOM,
            0,
            4096,
        )
        .ok()?
        .reply()
        .ok()?;
    if !supported.value32()?.any(|atom| atom == request) {
        return None;
    }
    connection
        .change_window_attributes(
            window,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                .event_mask(EventMask::PROPERTY_CHANGE),
        )
        .ok()?;
    let message = ClientMessageEvent::new(32, window, request, [0u32; 5]);
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        )
        .ok()?;
    connection.flush().ok()?;
    let read = || {
        let values = cardinals(connection, window, extents)?;
        let [l, r, t, b] = values[..] else {
            return None;
        };
        Some(Frame(l, r, t, b))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        if let Some(frame) = read() {
            return Some(frame);
        }
        // Until the window manager sets it, or the moment passes.
        match connection.poll_for_event().ok()? {
            Some(Event::PropertyNotify(PropertyNotifyEvent { atom, .. })) if atom == extents => {}
            Some(_) => {}
            None if std::time::Instant::now() > deadline => return None,
            None => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, fitted, intersect};

    #[test]
    fn a_window_past_its_work_area_shrinks_to_it_but_not_below_its_minimum() {
        let work = (0, 0, 1024, 640);
        assert_eq!(fitted((800, 600), Frame::default(), work, (1, 1)), None);
        assert_eq!(
            fitted((1200, 800), Frame::default(), work, (1, 1)),
            Some((1024, 640))
        );
        // Its title bar and borders fit too.
        assert_eq!(
            fitted((1200, 800), Frame(1, 1, 24, 1), work, (1, 1)),
            Some((1022, 615))
        );
        assert_eq!(
            fitted((1000, 630), Frame(1, 1, 24, 1), work, (1, 1)),
            Some((1000, 615))
        );
        assert_eq!(
            fitted((1200, 800), Frame::default(), work, (1100, 700)),
            Some((1100, 700))
        );
    }

    #[test]
    fn the_work_area_is_the_monitors_less_the_panels() {
        // _NET_WORKAREA spans both monitors, less a 32-pixel top panel.
        let work = (0, 32, 3840, 1048);
        assert_eq!(
            intersect(work, (1920, 0, 1920, 1080)),
            Some((1920, 32, 1920, 1048))
        );
        assert_eq!(intersect(work, (5000, 0, 100, 100)), None);
    }
}
