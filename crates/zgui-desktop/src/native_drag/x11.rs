//! Drags handed to X11 as they leave the app's windows: the XDND protocol
//! (version 5), the app the source.
//!
//! The pointer stays winit's: the button pressed in the window grabs it
//! until it is released (X's implicit grab), so the window goes on hearing
//! every move, outside it too. While the drag is X11's, the manager hands
//! those moves here (`moved`, `released`, `cancel`) rather than to the
//! window, and the host hears them back as the platform's (`NativeDrag`),
//! as from AppKit's session. The preview stays the window of its own it was
//! (`popup_window`), moved as the pointer moves, never drawn again.
//!
//! A session owns `XdndSelection` with a window of its own (input only,
//! never shown), on a connection of its own. Over another app's window
//! that says it takes drops (`XdndAware`, through any `XdndProxy`), it
//! sends that window `XdndEnter` with what it carries and `XdndPosition` as
//! the pointer moves, one at a time: the next waits for the target's
//! `XdndStatus`, which a thread on the session's connection hears. Released
//! where the last status accepted the drag, it sends `XdndDrop`; the thread
//! hands the target the data it asks for (`SelectionRequest`) and, once the
//! target says it finished (`XdndFinished`), ends the drag, accepted as the
//! target says. Released elsewhere, or cancelled with Escape, it sends
//! `XdndLeave` and ends there, not accepted. The app's own windows are not
//! targets: winit's say they take drops (for files), and a drag over them
//! is zgui's.
use super::{DragItem, NativeDrag};
use std::{
    cell::RefCell,
    os::fd::AsRawFd,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::{
    connection::{Connection, RequestConnection},
    protocol::{
        Event,
        xproto::{
            AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, PropMode,
            SelectionNotifyEvent, SelectionRequestEvent, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// The protocol's version zgui speaks.
const VERSION: u32 = 5;
/// The oldest version a target may speak.
const OLDEST: u32 = 3;
/// How long a target has to say it finished with a drop, or to answer the
/// position a drag was released at.
const FINISH: Duration = Duration::from_secs(3);

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        XdndAware,
        XdndSelection,
        XdndEnter,
        XdndPosition,
        XdndStatus,
        XdndLeave,
        XdndDrop,
        XdndFinished,
        XdndActionCopy,
        XdndTypeList,
        XdndProxy,
        TARGETS,
        UTF8_STRING,
        TEXT_PLAIN_UTF8: b"text/plain;charset=utf-8",
        TEXT_PLAIN: b"text/plain",
        URI_LIST: b"text/uri-list",
        PNG: b"image/png",
        JPEG: b"image/jpeg",
        TIFF: b"image/tiff",
    }
}

/// What a drag carries, by format, as other apps ask for it: files and URLs
/// as `text/uri-list`; text as UTF-8 under its usual names (or, with no
/// text, the URLs and paths as lines); images by their encoding.
pub(super) fn offers(items: &[DragItem]) -> Vec<(&'static str, Vec<u8>)> {
    let mut uris = Vec::new();
    let mut lines = Vec::new();
    let mut text = None;
    let mut images = Vec::new();
    for item in items {
        match item {
            DragItem::Text(value) => {
                text.get_or_insert_with(|| value.clone());
            }
            DragItem::Url(url) => {
                uris.push(url.clone());
                lines.push(url.clone());
            }
            DragItem::File(path) => {
                uris.push(file_uri(path));
                lines.push(path.to_string_lossy().into_owned());
            }
            DragItem::Image(bytes) => {
                if let Some(kind) = image_kind(bytes) {
                    images.push((kind, bytes.to_vec()));
                }
            }
        }
    }
    let mut offers = Vec::new();
    if !uris.is_empty() {
        let list: String = uris.iter().flat_map(|uri| [uri.as_str(), "\r\n"]).collect();
        offers.push(("text/uri-list", list.into_bytes()));
    }
    if let Some(text) = text.or_else(|| (!lines.is_empty()).then(|| lines.join("\n"))) {
        for name in ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain"] {
            offers.push((name, text.clone().into_bytes()));
        }
    }
    for (kind, bytes) in images {
        if !offers.iter().any(|(name, _)| *name == kind) {
            offers.push((kind, bytes));
        }
    }
    offers
}

/// An encoded image's type, by its signature.
fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Some("image/tiff")
    } else {
        None
    }
}

/// `path` as a `file://` URI: its bytes, those other than unreserved
/// characters and `/` percent-encoded (RFC 8089).
fn file_uri(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(*byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// The window a drag is over that takes drops: messages go to `deliver`
/// (the window, or its `XdndProxy`), in the protocol's `version`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    window: u32,
    deliver: u32,
    version: u32,
}

/// What the session and its thread share.
struct Shared {
    target: Option<Target>,
    /// The last status accepted the drag.
    accepted: bool,
    /// A position went out; its status has not come back.
    awaiting: bool,
    /// The latest position, sent once the awaited status comes back.
    pending: Option<[u32; 5]>,
    /// Released while a status was awaited: decided once it comes.
    releasing: Option<Instant>,
    /// `XdndDrop` went out at.
    dropped: Option<Instant>,
    /// The target asked for the data since the drop.
    served: bool,
    /// Where the pointer was let go, in points.
    point: (f64, f64),
    /// Ended (here or by the thread): nothing more is sent or heard.
    ended: bool,
    send: Box<dyn Fn(NativeDrag) + Send>,
}

/// A drag that is X11's.
struct Session {
    window: winit::window::WindowId,
    connection: Arc<RustConnection>,
    atoms: Atoms,
    source: u32,
    types: Vec<u32>,
    shared: Arc<Mutex<Shared>>,
    point: (f64, f64),
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    /// The main thread's connection for asking where windows are.
    static QUERY: Option<(RustConnection, u32)> = x11rb::connect(None).ok().map(|(connection, screen)| {
        let root = connection.setup().roots[screen].root;
        (connection, root)
    });
}

/// Hand the drag under way in `window` (an X11 window, its button down) to
/// X11, carrying `items`. False when it cannot begin (not X11, nothing
/// other apps take).
pub(crate) fn begin(
    window: &winit::window::Window,
    items: &[DragItem],
    _: Option<super::DragImage>,
    send: impl Fn(NativeDrag) + Send + 'static,
) -> bool {
    let x11 = window.window_handle().is_ok_and(|handle| {
        matches!(
            handle.as_raw(),
            RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)
        )
    });
    let offers = offers(items);
    if !x11 || offers.is_empty() {
        return false;
    }
    let Some(session) = Session::new(window.id(), offers, Box::new(send)) else {
        return false;
    };
    SESSION.with(|current| *current.borrow_mut() = Some(session));
    true
}

impl Session {
    fn new(
        window: winit::window::WindowId,
        offers: Vec<(&'static str, Vec<u8>)>,
        send: Box<dyn Fn(NativeDrag) + Send>,
    ) -> Option<Self> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let connection = Arc::new(connection);
        let root = connection.setup().roots[screen].root;
        let atoms = Atoms::new(&*connection).ok()?.reply().ok()?;
        let names: Vec<&str> = offers.iter().map(|(name, _)| *name).collect();
        let types: Vec<u32> = names
            .iter()
            .map(|name| atom(&*connection, name))
            .collect::<Option<_>>()?;
        let source = connection.generate_id().ok()?;
        connection
            .create_window(
                0,
                source,
                root,
                -1,
                -1,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .ok()?;
        if types.len() > 3 {
            connection
                .change_property32(
                    PropMode::REPLACE,
                    source,
                    atoms.XdndTypeList,
                    AtomEnum::ATOM,
                    &types,
                )
                .ok()?;
        }
        connection
            .set_selection_owner(source, atoms.XdndSelection, x11rb::CURRENT_TIME)
            .ok()?;
        connection.sync().ok()?;
        let shared = Arc::new(Mutex::new(Shared {
            target: None,
            accepted: false,
            awaiting: false,
            pending: None,
            releasing: None,
            dropped: None,
            served: false,
            point: (0., 0.),
            ended: false,
            send,
        }));
        let data: Vec<(u32, Vec<u8>)> = types
            .iter()
            .copied()
            .zip(offers.into_iter().map(|(_, data)| data))
            .collect();
        let worker = Worker {
            connection: connection.clone(),
            atoms,
            source,
            data,
            shared: shared.clone(),
        };
        std::thread::Builder::new()
            .name("zgui-xdnd".into())
            .spawn(move || worker.run())
            .ok()?;
        Some(Self {
            window,
            connection,
            atoms,
            source,
            types,
            shared,
            point: (0., 0.),
        })
    }

    /// The window at root point `(x, y)` that takes drops, other than the
    /// app's own: down from the root through the window manager's frame,
    /// the first that says so.
    fn target_at(&self, x: i16, y: i16) -> Option<Target> {
        QUERY.with(|query| {
            let (connection, root) = query.as_ref()?;
            let mut window = *root;
            for _ in 0..8 {
                let child = connection
                    .translate_coordinates(*root, window, x, y)
                    .ok()?
                    .reply()
                    .ok()?
                    .child;
                if child == x11rb::NONE {
                    return None;
                }
                if let Some(version) =
                    first(connection, child, self.atoms.XdndAware, AtomEnum::ATOM)
                {
                    if version < OLDEST {
                        return None;
                    }
                    // A proxy says so on itself too.
                    let deliver = first(connection, child, self.atoms.XdndProxy, AtomEnum::WINDOW)
                        .filter(|proxy| {
                            first(connection, *proxy, self.atoms.XdndProxy, AtomEnum::WINDOW)
                                == Some(*proxy)
                        })
                        .unwrap_or(child);
                    return Some(Target {
                        window: child,
                        deliver,
                        version: version.min(VERSION),
                    });
                }
                window = child;
            }
            None
        })
    }

    fn message(&self, target: Target, kind: u32, data: [u32; 5]) {
        message(&self.connection, target, kind, data);
    }

    /// The pointer at root pixel `root` (`point` in points), over one of
    /// the app's windows (`own`) or not.
    fn moved(&mut self, root: (i32, i32), point: (f64, f64), own: bool) {
        self.point = point;
        let (x, y) = (
            root.0.clamp(0, i16::MAX as i32) as i16,
            root.1.clamp(0, i16::MAX as i32) as i16,
        );
        let target = if own { None } else { self.target_at(x, y) };
        let mut shared = self.shared.lock().unwrap();
        if shared.ended || shared.releasing.is_some() || shared.dropped.is_some() {
            return;
        }
        if target.map(|target| target.window) != shared.target.map(|target| target.window) {
            if let Some(old) = shared.target.take() {
                self.message(old, self.atoms.XdndLeave, [self.source, 0, 0, 0, 0]);
            }
            shared.accepted = false;
            shared.awaiting = false;
            shared.pending = None;
            if let Some(new) = target {
                let more = u32::from(self.types.len() > 3);
                let mut data = [self.source, (new.version << 24) | more, 0, 0, 0];
                for (slot, kind) in data[2..].iter_mut().zip(&self.types) {
                    *slot = *kind;
                }
                self.message(new, self.atoms.XdndEnter, data);
            }
            shared.target = target;
        }
        let Some(target) = shared.target else {
            return;
        };
        let position = [
            self.source,
            0,
            ((x as u32) << 16) | (y as u16 as u32),
            x11rb::CURRENT_TIME,
            self.atoms.XdndActionCopy,
        ];
        if shared.awaiting {
            shared.pending = Some(position);
        } else {
            shared.awaiting = true;
            self.message(target, self.atoms.XdndPosition, position);
        }
    }

    /// The button was released: `Some(accepted)` if the drag ends here
    /// (nothing takes it), else it ends once its target says (`send`).
    fn released(&self) -> Option<bool> {
        let mut shared = self.shared.lock().unwrap();
        shared.point = self.point;
        let Some(target) = shared.target else {
            shared.ended = true;
            return Some(false);
        };
        if shared.awaiting {
            // Decided by the status on its way.
            shared.releasing = Some(Instant::now());
            return None;
        }
        if shared.accepted {
            shared.dropped = Some(Instant::now());
            self.message(
                target,
                self.atoms.XdndDrop,
                [self.source, 0, x11rb::CURRENT_TIME, 0, 0],
            );
            return None;
        }
        self.message(target, self.atoms.XdndLeave, [self.source, 0, 0, 0, 0]);
        shared.target = None;
        shared.ended = true;
        Some(false)
    }

    /// Escape: the target hears the drag leave.
    fn cancel(&self) {
        let mut shared = self.shared.lock().unwrap();
        if let Some(target) = shared.target.take()
            && shared.dropped.is_none()
        {
            self.message(target, self.atoms.XdndLeave, [self.source, 0, 0, 0, 0]);
        }
        shared.ended = true;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.lock().unwrap().ended = true;
        // Its selection goes with it, and the thread ends (DestroyNotify).
        let _ = self.connection.destroy_window(self.source);
        let _ = self.connection.flush();
    }
}

fn message(connection: &RustConnection, target: Target, kind: u32, data: [u32; 5]) {
    let event = ClientMessageEvent::new(32, target.window, kind, data);
    let _ = connection.send_event(false, target.deliver, EventMask::NO_EVENT, event);
    let _ = connection.flush();
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

/// The first value of `window`'s 32-bit `property`.
fn first(connection: &impl Connection, window: u32, property: u32, kind: AtomEnum) -> Option<u32> {
    connection
        .get_property(false, window, property, kind, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}

/// The thread on a session's connection: it hears targets' answers, and
/// hands them the data.
struct Worker {
    connection: Arc<RustConnection>,
    atoms: Atoms,
    source: u32,
    data: Vec<(u32, Vec<u8>)>,
    shared: Arc<Mutex<Shared>>,
}
impl Worker {
    fn run(self) {
        let fd = self.connection.stream().as_raw_fd();
        loop {
            while let Ok(Some(event)) = self.connection.poll_for_event() {
                match event {
                    Event::ClientMessage(event) if event.type_ == self.atoms.XdndStatus => {
                        self.status(event.data.as_data32());
                    }
                    Event::ClientMessage(event) if event.type_ == self.atoms.XdndFinished => {
                        self.finished(event.data.as_data32());
                    }
                    Event::SelectionRequest(request)
                        if request.selection == self.atoms.XdndSelection =>
                    {
                        self.serve(&request);
                    }
                    Event::DestroyNotify(event) if event.window == self.source => return,
                    _ => {}
                }
            }
            self.expire();
            // Woken by the server, or soon: the main thread may have read
            // its events while waiting on a reply.
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one pollfd, ours, for the call.
            unsafe { libc::poll(&mut poll, 1, 50) };
        }
    }

    fn status(&self, data: [u32; 5]) {
        let mut shared = self.shared.lock().unwrap();
        let Some(target) = shared.target.filter(|target| target.window == data[0]) else {
            return;
        };
        if shared.ended {
            return;
        }
        shared.awaiting = false;
        shared.accepted = data[1] & 1 != 0 && data[4] != x11rb::NONE;
        if shared.releasing.take().is_some() {
            if shared.accepted {
                shared.dropped = Some(Instant::now());
                message(
                    &self.connection,
                    target,
                    self.atoms.XdndDrop,
                    [self.source, 0, x11rb::CURRENT_TIME, 0, 0],
                );
            } else {
                message(
                    &self.connection,
                    target,
                    self.atoms.XdndLeave,
                    [self.source, 0, 0, 0, 0],
                );
                end(&mut shared, false);
            }
        } else if let Some(position) = shared.pending.take() {
            shared.awaiting = true;
            message(&self.connection, target, self.atoms.XdndPosition, position);
        }
    }

    fn finished(&self, data: [u32; 5]) {
        let mut shared = self.shared.lock().unwrap();
        let Some(target) = shared.target.filter(|target| target.window == data[0]) else {
            return;
        };
        if shared.dropped.is_none() || shared.ended {
            return;
        }
        // Before version 5 a target finishing took the drop.
        let accepted = target.version < 5 || data[1] & 1 != 0;
        end(&mut shared, accepted);
    }

    /// A target that never answers: a drop it asked the data for was taken.
    fn expire(&self) {
        let mut shared = self.shared.lock().unwrap();
        if shared.ended {
            return;
        }
        if shared.dropped.is_some_and(|at| at.elapsed() > FINISH) {
            let served = shared.served;
            end(&mut shared, served);
        } else if shared.releasing.is_some_and(|at| at.elapsed() > FINISH) {
            end(&mut shared, false);
        }
    }

    /// Hand `request`'s requestor the data in the format it asks for (or the
    /// formats there are, `TARGETS`), refusing one there is not.
    fn serve(&self, request: &SelectionRequestEvent) {
        let property = if request.property == x11rb::NONE {
            request.target
        } else {
            request.property
        };
        let room = self.connection.maximum_request_bytes().saturating_sub(64);
        let given = if request.target == self.atoms.TARGETS {
            let mut targets = vec![self.atoms.TARGETS];
            targets.extend(self.data.iter().map(|(kind, _)| *kind));
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &targets,
                )
                .is_ok()
        } else if let Some((kind, data)) = self
            .data
            .iter()
            .find(|(kind, data)| *kind == request.target && data.len() <= room)
        {
            self.connection
                .change_property8(PropMode::REPLACE, request.requestor, property, *kind, data)
                .is_ok()
        } else {
            false
        };
        let notify = SelectionNotifyEvent {
            response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: if given { property } else { x11rb::NONE },
        };
        let _ = self
            .connection
            .send_event(false, request.requestor, EventMask::NO_EVENT, notify);
        let _ = self.connection.flush();
        let mut shared = self.shared.lock().unwrap();
        shared.served |= given && shared.dropped.is_some();
    }
}

/// The drag ends, `accepted` or not: the host hears it.
fn end(shared: &mut Shared, accepted: bool) {
    shared.ended = true;
    let (x, y) = shared.point;
    (shared.send)(NativeDrag::Ended { x, y, accepted });
}

/// Whether `window`'s drag is X11's.
pub(crate) fn active(window: winit::window::WindowId) -> bool {
    SESSION.with(|session| {
        session
            .borrow()
            .as_ref()
            .is_some_and(|session| session.window == window)
    })
}

/// The pointer of the drag that is X11's moved: to root pixel `root`, at
/// `point` (points), over one of the app's windows (`own`) or not.
pub(crate) fn moved(root: (i32, i32), point: (f64, f64), own: bool) {
    SESSION.with(|session| {
        if let Some(session) = session.borrow_mut().as_mut() {
            session.moved(root, point, own);
        }
    });
}

/// Its button was released: `Some((x, y, accepted))` if it ends at once,
/// else it ends as its target says, through the session's `send`.
pub(crate) fn released() -> Option<(f64, f64, bool)> {
    SESSION.with(|session| {
        let session = session.borrow();
        let session = session.as_ref()?;
        let (x, y) = session.point;
        session.released().map(|accepted| (x, y, accepted))
    })
}

/// Escape: it ends where the pointer is, not accepted.
pub(crate) fn cancel() -> Option<(f64, f64)> {
    SESSION.with(|session| {
        let session = session.borrow();
        let session = session.as_ref()?;
        session.cancel();
        Some(session.point)
    })
}

/// The drag ended (heard by the host): its session goes.
pub(crate) fn end_session() {
    SESSION.with(|session| session.borrow_mut().take());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_urls_go_as_a_uri_list_and_text_as_utf8() {
        let offers = offers(&[
            DragItem::File("/tmp/a b/ü.txt".into()),
            DragItem::Url("https://example.com/x".into()),
        ]);
        let get = |name: &str| {
            offers
                .iter()
                .find(|(each, _)| *each == name)
                .map(|(_, data)| String::from_utf8(data.clone()).unwrap())
        };
        assert_eq!(
            get("text/uri-list").as_deref(),
            Some("file:///tmp/a%20b/%C3%BC.txt\r\nhttps://example.com/x\r\n")
        );
        // No text: the paths and URLs, a line each.
        assert_eq!(
            get("UTF8_STRING").as_deref(),
            Some("/tmp/a b/ü.txt\nhttps://example.com/x")
        );

        let offers = super::offers(&[DragItem::Text("héllo".into())]);
        let names: Vec<_> = offers.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain"]
        );
        assert!(offers.iter().all(|(_, data)| data == "héllo".as_bytes()));
    }

    #[test]
    fn images_go_by_their_encoding() {
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let offers = offers(&[
            DragItem::Image(png.clone().into()),
            DragItem::Image(b"not an image".to_vec().into()),
        ]);
        assert_eq!(offers, [("image/png", png)]);
    }
}
