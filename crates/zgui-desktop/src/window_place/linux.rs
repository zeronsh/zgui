//! Where a new window opens on Linux (see the module's notes): a point of
//! its content put under the pointer, a saved frame put back, or a step on
//! from the app's front window (X11); on Wayland, a saved frame's size and
//! whether it was maximized, as the compositor places windows.
use super::Placed;
use std::path::{Path, PathBuf};
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use x11rb::{
    connection::Connection,
    properties::{WmSizeHints, WmSizeHintsSpecification},
    protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt as _, MapState, PropMode},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// A title bar's height, in pixels at 1x: what of a saved frame a monitor
/// must still show for it to reopen there, and the cascade's step when the
/// front window has no frame.
const TITLE: f64 = 28.;

/// A saved frame: its top-left on the root (X11; none on Wayland, where the
/// app cannot know it), its content's size, in pixels, and whether it was
/// left maximized. The frame is the one before it was maximized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    position: Option<(i32, i32)>,
    size: (u32, u32),
    maximized: bool,
}
impl Frame {
    fn encode(self) -> String {
        let (width, height) = self.size;
        let maximized = u8::from(self.maximized);
        match self.position {
            Some((x, y)) => format!("{x} {y} {width} {height} {maximized}"),
            None => format!("- - {width} {height} {maximized}"),
        }
    }
    fn decode(text: &str) -> Option<Self> {
        let mut words = text.split_whitespace();
        let (x, y) = (words.next()?, words.next()?);
        let position = match (x, y) {
            ("-", "-") => None,
            (x, y) => Some((x.parse().ok()?, y.parse().ok()?)),
        };
        let size = (words.next()?.parse().ok()?, words.next()?.parse().ok()?);
        let maximized = words.next()?.parse::<u8>().ok()? != 0;
        (size.0 > 0 && size.1 > 0).then_some(Self {
            position,
            size,
            maximized,
        })
    }
}

/// Where frames are kept: `$XDG_STATE_HOME/zgui/<executable>/frames` (by
/// default `~/.local/state`), a line per name.
fn frames_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state")))?;
    let app = std::env::current_exe()
        .ok()
        .and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "zgui".into());
    Some(state.join("zgui").join(app).join("frames"))
}
/// A name as a line keeps it: one line, the tab its separator.
fn key(name: &str) -> String {
    name.replace(['\t', '\n', '\r'], " ")
}
fn load_from(path: &Path, name: &str) -> Option<Frame> {
    let name = key(name);
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(each, _)| *each == name)
        .find_map(|(_, frame)| Frame::decode(frame))
}
/// Keep `frame` under `name`, the other names' as they were. Written whole
/// and renamed into place: a reader never sees half a file.
fn store_to(path: &Path, name: &str, frame: Frame) -> std::io::Result<()> {
    let name = key(name);
    let mut text: String = std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.split_once('\t').is_some_and(|(each, _)| each != name))
        .flat_map(|line| [line, "\n"])
        .collect();
    text.push_str(&format!("{name}\t{}\n", frame.encode()));
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    let partial = path.with_extension(format!("{}", std::process::id()));
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}

/// Keep a window's frame under `name`: `normal`, its frame when last
/// neither maximized nor fullscreen, and whether it is maximized now.
pub(crate) fn save(name: &str, normal: crate::WindowBounds, maximized: bool) {
    let Some(path) = frames_path() else {
        return;
    };
    let frame = Frame {
        position: normal.position,
        size: normal.size,
        maximized,
    };
    if load_from(&path, name) != Some(frame) {
        let _ = store_to(&path, name, frame);
    }
}

/// Open `window` (X11, not shown yet) maximized. winit asks the window
/// manager with a `_NET_WM_STATE` message, which it heeds for the windows it
/// manages, not one not yet shown; that one's state is its `_NET_WM_STATE`
/// property, which the window manager reads as it maps the window (EWMH).
pub(crate) fn maximize_hidden(window: &Window) {
    let Some(id) = x11_id(window) else {
        return;
    };
    let Ok((connection, _)) = x11rb::connect(None) else {
        return;
    };
    let atom = |name: &str| {
        connection
            .intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()
            .map(|reply| reply.atom)
    };
    let (Some(state), Some(vertical), Some(horizontal)) = (
        atom("_NET_WM_STATE"),
        atom("_NET_WM_STATE_MAXIMIZED_VERT"),
        atom("_NET_WM_STATE_MAXIMIZED_HORZ"),
    ) else {
        return;
    };
    let mut states: Vec<u32> = connection
        .get_property(false, id, state, AtomEnum::ATOM, 0, 64)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| Some(reply.value32()?.collect()))
        .unwrap_or_default();
    for each in [vertical, horizontal] {
        if !states.contains(&each) {
            states.push(each);
        }
    }
    let _ = connection.change_property32(PropMode::REPLACE, id, state, AtomEnum::ATOM, &states);
    // Before winit's connection maps it (see `move_frame`).
    let _ = connection.sync();
}

/// Place `window` (not shown yet), as `super::place` says: with content
/// point `under_pointer` (logical) under the pointer; else at its saved
/// frame; else, unless its options gave it a position (`placed`), a step on
/// from the app's front window.
pub(crate) fn place(
    window: &Window,
    under_pointer: Option<(f64, f64)>,
    placed: bool,
    options: &crate::WindowOptions,
) -> Placed {
    let x11 = x11_id(window).and_then(|id| {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots[screen].root;
        Some((connection, root, id))
    });
    if let Some(point) = under_pointer {
        if let Some((connection, root, id)) = &x11 {
            under_pointer_at(connection, *root, *id, window, point);
        }
        return Placed::default();
    }
    let mut positioned = placed;
    let mut placed = Placed::default();
    if let Some(frame) = options
        .frame_autosave
        .as_deref()
        .zip(frames_path())
        .and_then(|(name, path)| load_from(&path, name))
    {
        let _ = window.request_inner_size(PhysicalSize::new(frame.size.0, frame.size.1));
        placed = Placed {
            restored: true,
            maximized: frame.maximized,
        };
        // Where it was, if a monitor still shows its title bar (not one
        // since unplugged).
        if let (Some((connection, _, id)), Some(at)) = (&x11, frame.position)
            && shows_title(window, at, frame.size.0)
        {
            move_frame(connection, *id, at, WmSizeHintsSpecification::UserSpecified);
            positioned = true;
        }
    }
    if let Some((connection, root, id)) = &x11 {
        if !positioned {
            cascade(connection, *root, *id, window);
        }
        LAST.set(Some(*id));
    }
    placed
}

thread_local! {
    /// The last window placed: the front window, until the window manager
    /// says which is active.
    static LAST: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

fn x11_id(window: &Window) -> Option<u32> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(handle) => Some(handle.window as u32),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
        _ => None,
    }
}

/// Whether a monitor shows the title bar of a frame at `(x, y)`, `width`
/// wide.
fn shows_title(window: &Window, (x, y): (i32, i32), width: u32) -> bool {
    let title = (
        x,
        y,
        width as i32,
        (TITLE * window.scale_factor()).round() as i32,
    );
    window.available_monitors().any(|monitor| {
        let (position, size) = (monitor.position(), monitor.size());
        let screen = (
            position.x,
            position.y,
            size.width as i32,
            size.height as i32,
        );
        crate::x11_work_area::intersect(title, screen).is_some()
    })
}

/// Put `id`'s frame's top-left at `(left, top)` (root pixels), as `how`
/// says it was chosen: the user's (`USPosition`), which window managers
/// keep, or the program's (`PPosition`).
fn move_frame(
    connection: &RustConnection,
    id: u32,
    (left, top): (i32, i32),
    how: WmSizeHintsSpecification,
) {
    let mut hints = WmSizeHints::get_normal_hints(connection, id)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .flatten()
        .unwrap_or_default();
    hints.position = Some((how, left, top));
    let _ = hints.set_normal_hints(connection, id);
    let _ = connection.configure_window(id, &ConfigureWindowAux::new().x(left).y(top));
    // A round trip, not a flush: winit's connection rewrites the hints (a
    // size request reads them back first) and maps the window, and the
    // server takes two connections' requests in no set order. Once this
    // returns, the hints and position are the server's, and the window
    // manager sees them as it maps the window.
    let _ = connection.sync();
}

/// Put content point `(x, y)` (logical) of `id` (not shown yet) under the
/// pointer.
fn under_pointer_at(
    connection: &RustConnection,
    root: u32,
    id: u32,
    window: &Window,
    (x, y): (f64, f64),
) {
    let Some(pointer) = connection
        .query_pointer(root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return;
    };
    let frame = crate::x11_work_area::frame_extents(connection, root, id).unwrap_or_default();
    let scale = window.scale_factor();
    // The frame's top-left: the content's, less the frame's left and top.
    let left = pointer.root_x as i32 - (x * scale).round() as i32 - frame.0 as i32;
    let top = pointer.root_y as i32 - (y * scale).round() as i32 - frame.2 as i32;
    move_frame(
        connection,
        id,
        (left, top),
        WmSizeHintsSpecification::UserSpecified,
    );
}

/// Put `id` (not shown yet) a step on from the app's front window, down
/// and right by its title bar's height, as AppKit and Windows cascade a
/// document window, rather than over it; back at its work area's top-left
/// when that would take it past the work area. None in front, the window
/// manager places it.
fn cascade(connection: &RustConnection, root: u32, id: u32, window: &Window) {
    let Some(front) = front(connection, root, id) else {
        return;
    };
    let Some(origin) = connection
        .translate_coordinates(front, root, 0, 0)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return;
    };
    let frame = crate::x11_work_area::frame_extents(connection, root, front).unwrap_or_default();
    let step = if frame.2 > 0 {
        frame.2 as i32
    } else {
        (TITLE * window.scale_factor()).round() as i32
    };
    let corner = (
        i32::from(origin.dst_x) - frame.0 as i32,
        i32::from(origin.dst_y) - frame.2 as i32,
    );
    let mut at = (corner.0 + step, corner.1 + step);
    // Its frame as large as the front's: the same window manager draws it.
    let size = window.inner_size();
    let outer = (
        size.width as i32 + (frame.0 + frame.1) as i32,
        size.height as i32 + (frame.2 + frame.3) as i32,
    );
    if let Some(work) = work_area_at(connection, root, window, corner)
        && (at.0 + outer.0 > work.0 + work.2 || at.1 + outer.1 > work.1 + work.3)
    {
        at = (work.0, work.1);
    }
    move_frame(
        connection,
        id,
        at,
        WmSizeHintsSpecification::ProgramSpecified,
    );
}

/// The app's front window, other than `id`: the active window if it is
/// one of this process's, else the last placed, if still shown.
fn front(connection: &RustConnection, root: u32, id: u32) -> Option<u32> {
    let atom = |name: &str| {
        connection
            .intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()
            .map(|reply| reply.atom)
    };
    let first = |window: u32, property: u32, kind: AtomEnum| {
        connection
            .get_property(false, window, property, kind, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()
    };
    let pid = atom("_NET_WM_PID")?;
    let ours = |window: u32| {
        window != 0
            && window != id
            && first(window, pid, AtomEnum::CARDINAL) == Some(std::process::id())
            && connection
                .get_window_attributes(window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .is_some_and(|attributes| attributes.map_state == MapState::VIEWABLE)
    };
    atom("_NET_ACTIVE_WINDOW")
        .and_then(|active| first(root, active, AtomEnum::WINDOW))
        .filter(|window| ours(*window))
        .or_else(|| LAST.get().filter(|window| ours(*window)))
}

/// The work area of the monitor showing `point`.
fn work_area_at(
    connection: &RustConnection,
    root: u32,
    window: &Window,
    (x, y): (i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let work = crate::x11_work_area::work_area(connection, root)?;
    let monitor = window.available_monitors().find(|monitor| {
        let (position, size) = (monitor.position(), monitor.size());
        (position.x..position.x + size.width as i32).contains(&x)
            && (position.y..position.y + size.height as i32).contains(&y)
    })?;
    let (position, size) = (monitor.position(), monitor.size());
    crate::x11_work_area::intersect(
        work,
        (
            position.x,
            position.y,
            size.width as i32,
            size.height as i32,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_frame_reads_back_as_it_was_written() {
        let frame = Frame {
            position: Some((-1200, 40)),
            size: (900, 600),
            maximized: true,
        };
        assert_eq!(Frame::decode(&frame.encode()), Some(frame));
        let wayland = Frame {
            position: None,
            ..frame
        };
        assert_eq!(Frame::decode(&wayland.encode()), Some(wayland));
        assert_eq!(Frame::decode("10 20 0 30 0"), None, "an empty frame");
        assert_eq!(Frame::decode("10 20"), None);
        assert_eq!(Frame::decode("garbage"), None);
    }

    #[test]
    fn frames_are_kept_by_name_and_replaced() {
        let directory = std::env::temp_dir().join(format!("zgui-frames-{}", std::process::id()));
        let path = directory.join("app").join("frames");
        let one = Frame {
            position: Some((10, 20)),
            size: (400, 300),
            maximized: false,
        };
        let two = Frame {
            position: None,
            size: (800, 600),
            maximized: true,
        };
        assert_eq!(load_from(&path, "main"), None, "nothing saved yet");
        store_to(&path, "main", one).unwrap();
        store_to(&path, "tab\tbed", two).unwrap();
        assert_eq!(load_from(&path, "main"), Some(one));
        assert_eq!(load_from(&path, "tab\tbed"), Some(two));
        store_to(&path, "main", two).unwrap();
        assert_eq!(load_from(&path, "main"), Some(two));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2, "one line per name: {text}");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
