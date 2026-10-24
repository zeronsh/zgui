//! Owned Wayland data devices: the clipboard, the primary selection and
//! external file drags, through one device of each kind per seat; and
//! raising a window with another launch's token (`xdg_activation_v1`, which
//! winit uses only for a new window).
//!
//! One device, not one per feature: Weston serves a selection only through
//! the newest offer of it, so a second device in this process would leave
//! the other reading nothing, and would cancel this process's own clipboard
//! by releasing that offer.
use std::{
    collections::HashMap,
    fs::File,
    io::{ErrorKind, Read, Write},
    os::fd::{AsFd, OwnedFd},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    protocol::{
        wl_buffer, wl_compositor, wl_data_device,
        wl_data_device_manager::{self, DndAction},
        wl_data_offer, wl_data_source, wl_keyboard, wl_pointer, wl_registry,
        wl_seat::{self, Capability},
        wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1 as PrimaryManager,
    zwp_primary_selection_device_v1::{self, ZwpPrimarySelectionDeviceV1 as PrimaryDevice},
    zwp_primary_selection_offer_v1::{self, ZwpPrimarySelectionOfferV1 as PrimaryOffer},
    zwp_primary_selection_source_v1::{self, ZwpPrimarySelectionSourceV1 as PrimarySource},
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1 as XdgActivation;
use winit::{
    event_loop::OwnedDisplayHandle,
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
};
pub(crate) type Routes = Arc<Mutex<HashMap<u32, u64>>>;
#[derive(Debug)]
pub(crate) enum FileEvent {
    Hover {
        x: f32,
        y: f32,
        path: PathBuf,
    },
    Leave,
    Rejected {
        x: f32,
        y: f32,
        reason: zgui::input::FileDropError,
    },
    Drop {
        x: f32,
        y: f32,
        paths: Arc<[PathBuf]>,
        accepted: std::sync::mpsc::Sender<bool>,
    },
}
type Handler = Arc<dyn Fn(u64, FileEvent) + Send + Sync>;
/// Text types offered, in order of preference when reading.
const TEXT_TYPES: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// How long a paste waits for the selection's owner, and how much it takes.
const LOAD_TIMEOUT: Duration = Duration::from_secs(2);
const LOAD_LIMIT: usize = 64 * 1024 * 1024;
enum Command {
    Exit,
    Store {
        primary: bool,
        text: String,
        /// The same as HTML (the clipboard only), offered as `text/html`.
        html: Option<String>,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Load {
        primary: bool,
        reply: mpsc::Sender<Result<Loaded, String>>,
    },
    /// Raise `window` with an activation token; held here until done, so
    /// its surface lives as long.
    Activate {
        token: String,
        window: Arc<winit::window::Window>,
    },
    /// Hand a drag under way in `window` to the compositor
    /// (`native_drag::begin`), carrying `data` by format, its icon `icon`.
    Drag {
        window: Arc<winit::window::Window>,
        data: Vec<(&'static str, Vec<u8>)>,
        icon: Option<crate::native_drag::DragImage>,
        /// Where the pointer was as it left (the window's points).
        point: (f64, f64),
        send: Box<dyn Fn(crate::native_drag::NativeDrag) + Send>,
    },
}
/// The data worker's commands, for a drag handed off (`start_drag`).
static COMMANDS: Mutex<Option<calloop::channel::Sender<Command>>> = Mutex::new(None);

/// Hand the drag under way in `window` (a Wayland window, its button down)
/// to the compositor as a drag of the app's own (`wl_data_device.start_drag`),
/// carrying `data` by format. False when there is no data device to.
pub(crate) fn start_drag(
    window: Arc<winit::window::Window>,
    data: Vec<(&'static str, Vec<u8>)>,
    icon: Option<crate::native_drag::DragImage>,
    point: (f64, f64),
    send: Box<dyn Fn(crate::native_drag::NativeDrag) + Send>,
) -> bool {
    COMMANDS.lock().unwrap().as_ref().is_some_and(|commands| {
        commands
            .send(Command::Drag {
                window,
                data,
                icon,
                point,
                send,
            })
            .is_ok()
    })
}
/// The selection owner writes into the stream; `bool` asks for CRLF
/// normalization.
struct Loaded(UnixStream, bool);
pub(crate) struct NativeData {
    commands: calloop::channel::Sender<Command>,
    worker: Option<thread::JoinHandle<()>>,
    // Worker joins before this owned foreign display is released.
    _display: OwnedDisplayHandle,
}
/// The clipboard and primary selection, served by the data worker.
#[derive(Clone)]
pub(crate) struct WaylandClipboard(calloop::channel::Sender<Command>);
impl WaylandClipboard {
    /// Publish `text`; fails before the seat has delivered any input, as a
    /// selection needs an input serial.
    pub fn store(&self, primary: bool, text: String) -> Result<(), String> {
        self.publish(primary, text, None)
    }
    /// Publish the clipboard as plain text and as HTML: rich editors and
    /// browsers paste the HTML, everything else the text.
    pub fn store_rich(&self, text: String, html: String) -> Result<(), String> {
        self.publish(false, text, Some(html))
    }
    fn publish(&self, primary: bool, text: String, html: Option<String>) -> Result<(), String> {
        let (reply, answer) = mpsc::channel();
        self.0
            .send(Command::Store {
                primary,
                text,
                html,
                reply,
            })
            .map_err(|_| "Wayland data worker stopped".to_string())?;
        answer
            .recv_timeout(LOAD_TIMEOUT)
            .map_err(|_| "Wayland data worker did not answer".to_string())?
    }
    pub fn load(&self, primary: bool) -> Result<String, String> {
        let (reply, answer) = mpsc::channel();
        self.0
            .send(Command::Load { primary, reply })
            .map_err(|_| "Wayland data worker stopped".to_string())?;
        let started = Instant::now();
        let loaded = answer
            .recv_timeout(LOAD_TIMEOUT)
            .map_err(|_| "Wayland data worker did not answer".to_string())??;
        let Loaded(mut stream, normalize) = loaded;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 65536];
        loop {
            let left = LOAD_TIMEOUT
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero())
                .ok_or("selection owner did not finish writing")?;
            stream
                .set_read_timeout(Some(left))
                .map_err(|e| e.to_string())?;
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) if bytes.len() + n > LOAD_LIMIT => return Err("selection too large".into()),
                Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(if normalize {
            text.replace("\r\n", "\n")
        } else {
            text.into_owned()
        })
    }
}
impl NativeData {
    /// Raise `window` with another launch's activation `token`.
    pub fn activate(&self, token: String, window: Arc<winit::window::Window>) {
        let _ = self.commands.send(Command::Activate { token, window });
    }
    pub fn new(
        display: OwnedDisplayHandle,
        routes: Routes,
        handler: Handler,
    ) -> Result<Option<Self>, String> {
        let RawDisplayHandle::Wayland(raw) = display
            .display_handle()
            .map_err(|e| e.to_string())?
            .as_raw()
        else {
            return Ok(None);
        };
        // SAFETY: `display` (winit's connection, kept in `_display`) keeps
        // the wl_display connected until `NativeData` has dropped, which
        // joins the worker first. The backend and every clone of it stay on
        // the worker and go before it ends (or with its closure, if it never
        // starts).
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(raw.display.as_ptr().cast())
        };
        let connection = Connection::from_backend(backend);
        let (sender, commands) = calloop::channel::channel();
        let worker = thread::Builder::new()
            .name("zgui-wayland-data".into())
            .spawn(move || {
                let queue = connection.new_event_queue::<State>();
                let qh = queue.handle();
                let registry = connection.display().get_registry(&qh, ());
                let mut state = State {
                    routes,
                    handler,
                    connection: connection.clone(),
                    qh: qh.clone(),
                    manager: None,
                    primary_manager: None,
                    seats: Vec::new(),
                    offers: HashMap::new(),
                    selection: None,
                    primary_offers: HashMap::new(),
                    primary_selection: None,
                    source: None,
                    primary_source: None,
                    serial: None,
                    press: None,
                    compositor: None,
                    shm: None,
                    outgoing: None,
                    drag: None,
                    completion: None,
                    activation: None,
                    exit: false,
                };
                let Ok(mut event_loop) = calloop::EventLoop::<State>::try_new() else {
                    return;
                };
                let handle = event_loop.handle();
                if handle
                    .insert_source(commands, |event, _, state: &mut State| {
                        let calloop::channel::Event::Msg(command) = event else {
                            state.exit = true;
                            return;
                        };
                        match command {
                            Command::Exit => state.exit = true,
                            Command::Store {
                                primary,
                                text,
                                html,
                                reply,
                            } => {
                                let _ = reply.send(state.store(primary, text, html));
                            }
                            Command::Load { primary, reply } => {
                                let _ = reply.send(state.load(primary));
                            }
                            Command::Activate { token, window } => state.activate(&token, &window),
                            Command::Drag {
                                window,
                                data,
                                icon,
                                point,
                                send,
                            } => state.start_drag(window, data, icon, point, send),
                        }
                        let _ = state.connection.flush();
                    })
                    .is_err()
                {
                    return;
                }
                if calloop_wayland_source::WaylandSource::new(connection.clone(), queue)
                    .insert(handle)
                    .is_err()
                {
                    return;
                }
                while !state.exit {
                    let timeout = (state.drag.as_ref().is_some_and(|d| d.read.is_some())
                        || state.completion.is_some())
                    .then_some(Duration::from_millis(20));
                    if event_loop.dispatch(timeout, &mut state).is_err() {
                        break;
                    }
                    state.read_transfer();
                }
                state.cancel();
                for (_, offer) in state
                    .offers
                    .drain()
                    .chain(state.selection.take().map(|o| (0, o)))
                {
                    offer.proxy.destroy();
                }
                for (_, (offer, _)) in state
                    .primary_offers
                    .drain()
                    .chain(state.primary_selection.take().map(|o| (0, o)))
                {
                    offer.destroy();
                }
                if let Some((source, ..)) = state.source.take() {
                    source.destroy();
                }
                if let Some((source, _)) = state.primary_source.take() {
                    source.destroy();
                }
                for seat in state.seats.drain(..) {
                    seat.release();
                }
                if let Some(manager) = state.primary_manager.take() {
                    manager.destroy();
                }
                if let Some(activation) = state.activation.take() {
                    activation.destroy();
                }
                if let Some(manager) = state.manager.take() {
                    drop(manager);
                }
                drop(registry);
                let _ = connection.flush();
            })
            .map_err(|e| e.to_string())?;
        *COMMANDS.lock().unwrap() = Some(sender.clone());
        Ok(Some(Self {
            commands: sender,
            worker: Some(worker),
            _display: display,
        }))
    }
    pub fn clipboard(&self) -> WaylandClipboard {
        WaylandClipboard(self.commands.clone())
    }
}
impl Drop for NativeData {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Exit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Offer {
    proxy: wl_data_offer::WlDataOffer,
    uris: bool,
    /// The preferred text type offered, an index into `TEXT_TYPES`.
    text: Option<usize>,
}
struct Seat {
    name: u32,
    seat: wl_seat::WlSeat,
    device: Option<wl_data_device::WlDataDevice>,
    primary: Option<PrimaryDevice>,
    // Their enter, key and button serials authorize setting a selection.
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
}
impl Seat {
    fn release(self) {
        if let Some(device) = self.device
            && device.version() >= 2
        {
            device.release();
        }
        if let Some(primary) = self.primary {
            primary.destroy();
        }
        if let Some(keyboard) = self.keyboard
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        if let Some(pointer) = self.pointer
            && pointer.version() >= 3
        {
            pointer.release();
        }
        if self.seat.version() >= 5 {
            self.seat.release();
        }
    }
}
struct Transfer {
    stream: UnixStream,
    bytes: Vec<u8>,
    deadline: Instant,
}
struct Drag {
    offer: wl_data_offer::WlDataOffer,
    token: u64,
    x: f32,
    y: f32,
    read: Option<Transfer>,
    paths: Option<Arc<[PathBuf]>>,
    dropped: bool,
    copy: bool,
    left: bool,
}
/// The app's own drag, the compositor's now.
struct Outgoing {
    source: wl_data_source::WlDataSource,
    data: Vec<(&'static str, Arc<[u8]>)>,
    send: Box<dyn Fn(crate::native_drag::NativeDrag) + Send>,
    /// The window it came from (held while the drag lasts), and its
    /// surface's id: back over it, the drag is zgui's.
    _window: Arc<winit::window::Window>,
    origin: u32,
    icon: Option<(wl_surface::WlSurface, wl_buffer::WlBuffer)>,
    /// Where the pointer was last seen, in the window's points.
    point: (f64, f64),
    /// Over its own window.
    home: bool,
    /// Over another of the app's windows (its host's token), at a point in
    /// its coordinates.
    over: Option<(u64, (f64, f64))>,
    /// The offer of it to one of the app's windows, refused, held until
    /// the drag leaves (destroyed sooner, the compositor cancels the drag).
    offer: Option<wl_data_offer::WlDataOffer>,
    /// The target took the drop (`dnd_drop_performed`).
    performed: bool,
}
struct Completion {
    offer: wl_data_offer::WlDataOffer,
    accepted: std::sync::mpsc::Receiver<bool>,
    deadline: Instant,
}
struct State {
    routes: Routes,
    handler: Handler,
    connection: Connection,
    qh: QueueHandle<Self>,
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    primary_manager: Option<PrimaryManager>,
    seats: Vec<Seat>,
    offers: HashMap<u32, Offer>,
    /// The current clipboard, held until superseded: Weston cancels a
    /// selection's source when its newest offer is destroyed.
    selection: Option<Offer>,
    primary_offers: HashMap<u32, (PrimaryOffer, Option<usize>)>,
    primary_selection: Option<(PrimaryOffer, Option<usize>)>,
    /// What this process published, until another client replaces it.
    source: Option<(wl_data_source::WlDataSource, Arc<str>, Option<Arc<str>>)>,
    primary_source: Option<(PrimarySource, Arc<str>)>,
    /// The latest input serial and the seat it came from.
    serial: Option<(u32, u32)>,
    /// The latest button press's serial and seat: a drag starts from it.
    press: Option<(u32, u32)>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    /// The app's own drag, handed to the compositor (`start_drag`).
    outgoing: Option<Outgoing>,
    drag: Option<Drag>,
    completion: Option<Completion>,
    activation: Option<XdgActivation>,
    exit: bool,
}
impl State {
    /// Activate `window`'s surface with another launch's `token`.
    fn activate(&self, token: &str, window: &winit::window::Window) {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let (Some(activation), Ok(handle)) = (&self.activation, window.window_handle()) else {
            return;
        };
        let RawWindowHandle::Wayland(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: winit's live surface on this display: `window` holds it
        // until this returns, and the id and proxy made of it go by then.
        // Read from this thread, it is only its fixed class, id, listener
        // and user data.
        let Ok(id) = (unsafe {
            wayland_backend::client::ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                handle.surface.as_ptr().cast(),
            )
        }) else {
            return;
        };
        if let Ok(surface) = wl_surface::WlSurface::from_id(&self.connection, id) {
            activation.activate(token.to_owned(), &surface);
        }
    }
    fn devices(&mut self, qh: &QueueHandle<Self>) {
        for seat in &mut self.seats {
            if let Some(manager) = &self.manager
                && seat.device.is_none()
            {
                seat.device = Some(manager.get_data_device(&seat.seat, qh, ()));
            }
            if let Some(manager) = &self.primary_manager
                && seat.primary.is_none()
            {
                seat.primary = Some(manager.get_device(&seat.seat, qh, ()));
            }
        }
    }
    /// Hand the drag under way in `window` to the compositor, from the
    /// press that began it.
    fn start_drag(
        &mut self,
        window: Arc<winit::window::Window>,
        data: Vec<(&'static str, Vec<u8>)>,
        icon: Option<crate::native_drag::DragImage>,
        point: (f64, f64),
        send: Box<dyn Fn(crate::native_drag::NativeDrag) + Send>,
    ) {
        use crate::native_drag::NativeDrag;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        self.end_outgoing(false);
        let ended = |send: &dyn Fn(NativeDrag)| {
            send(NativeDrag::Ended {
                x: point.0,
                y: point.1,
                accepted: false,
            })
        };
        let device = self.press.and_then(|(seat, serial)| {
            let seat = self.seats.iter().find(|each| each.name == seat)?;
            Some((seat.device.clone()?, serial))
        });
        let (Some(manager), Some((device, serial)), Ok(handle)) =
            (&self.manager, device, window.window_handle())
        else {
            return ended(&*send);
        };
        let RawWindowHandle::Wayland(handle) = handle.as_raw() else {
            return ended(&*send);
        };
        // SAFETY: winit's live surface on this display: `window` holds it
        // while the drag lasts (`Outgoing::_window`), and the proxy made of
        // it goes when this returns (only its protocol id is kept).
        let Some(origin) = (unsafe {
            wayland_backend::client::ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                handle.surface.as_ptr().cast(),
            )
        })
        .ok()
        .and_then(|id| wl_surface::WlSurface::from_id(&self.connection, id).ok()) else {
            return ended(&*send);
        };
        let source = manager.create_data_source(&self.qh, ());
        for (kind, _) in &data {
            source.offer((*kind).into());
        }
        if source.version() >= 3 {
            source.set_actions(DndAction::Copy);
        }
        let icon = icon.and_then(|icon| self.icon(icon));
        device.start_drag(
            Some(&source),
            &origin,
            icon.as_ref().map(|(surface, ..)| surface),
            serial,
        );
        // Its content once it is the drag's icon: the grab point under the
        // pointer.
        let icon = icon.map(|(surface, buffer, (x, y))| {
            if surface.version() >= 5 {
                surface.attach(Some(&buffer), 0, 0);
                surface.offset(-x, -y);
            } else {
                surface.attach(Some(&buffer), -x, -y);
            }
            surface.commit();
            (surface, buffer)
        });
        self.outgoing = Some(Outgoing {
            source,
            data: data
                .into_iter()
                .map(|(kind, bytes)| (kind, bytes.into()))
                .collect(),
            send,
            origin: origin.id().protocol_id(),
            _window: window,
            icon,
            point,
            home: false,
            over: None,
            offer: None,
            performed: false,
        });
    }
    /// A drag icon of `image`'s pixels in shared memory, and its grab point
    /// in surface coordinates.
    fn icon(
        &self,
        image: crate::native_drag::DragImage,
    ) -> Option<(wl_surface::WlSurface, wl_buffer::WlBuffer, (i32, i32))> {
        let (compositor, shm) = (self.compositor.as_ref()?, self.shm.as_ref()?);
        let size = image.bgra.len();
        if size != (image.width * image.height * 4) as usize || size == 0 {
            return None;
        }
        // SAFETY: a NUL-terminated name, valid for the call; a new
        // descriptor or -1.
        let fd = unsafe { libc::memfd_create(c"zgui-drag-icon".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        // SAFETY: the descriptor just made, owned by nothing else: the file
        // closes it.
        let mut file = File::from(unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) });
        // ARGB8888 is premultiplied BGRA in memory on little-endian machines,
        // as the image is.
        file.write_all(&image.bgra).ok()?;
        let pool = shm.create_pool(file.as_fd(), size as i32, &self.qh, ());
        let buffer = pool.create_buffer(
            0,
            image.width as i32,
            image.height as i32,
            image.width as i32 * 4,
            wl_shm::Format::Argb8888,
            &self.qh,
            (),
        );
        pool.destroy();
        let surface = compositor.create_surface(&self.qh, ());
        // Drawn at the image's scale: as many surface points as it has.
        let scale = image.scale.round().max(1.) as i32;
        surface.set_buffer_scale(scale);
        let grab = (
            (image.grab.0 * image.scale / scale as f32).round() as i32,
            (image.grab.1 * image.scale / scale as f32).round() as i32,
        );
        Some((surface, buffer, grab))
    }
    /// The app's own drag ends: `accepted` by another app, or not. Its host
    /// hears it unless it already has.
    fn end_outgoing(&mut self, accepted: bool) {
        let Some((x, y)) = self.outgoing.as_ref().map(|outgoing| outgoing.point) else {
            return;
        };
        self.end_outgoing_with(crate::native_drag::NativeDrag::Ended { x, y, accepted });
    }
    /// The app's own drag was dropped on one of its windows: done with the
    /// offer, as a target is (the drop is zgui's, not taken by another app:
    /// the drag ends before the source hears it performed).
    fn finish_own_drop(&mut self) {
        if let Some(offer) = self
            .outgoing
            .as_mut()
            .and_then(|outgoing| outgoing.offer.take())
        {
            if offer.version() >= 3 {
                offer.finish();
            }
            offer.destroy();
        }
    }
    /// The app's own drag ends, its host hearing `how`.
    fn end_outgoing_with(&mut self, how: crate::native_drag::NativeDrag) {
        let Some(outgoing) = self.outgoing.take() else {
            return;
        };
        (outgoing.send)(how);
        if let Some(offer) = outgoing.offer {
            offer.destroy();
        }
        outgoing.source.destroy();
        if let Some((surface, buffer)) = outgoing.icon {
            surface.destroy();
            buffer.destroy();
        }
    }
    fn store(&mut self, primary: bool, text: String, html: Option<String>) -> Result<(), String> {
        let (seat, serial) = self.serial.ok_or("no input has reached this client yet")?;
        let seat = self
            .seats
            .iter()
            .find(|candidate| candidate.name == seat)
            .ok_or("the input seat is gone")?;
        let text: Arc<str> = text.into();
        if primary {
            let (Some(manager), Some(device)) = (&self.primary_manager, &seat.primary) else {
                return Err("the compositor has no primary selection".into());
            };
            let source = manager.create_source(&self.qh, ());
            for kind in TEXT_TYPES {
                source.offer(kind.into());
            }
            device.set_selection(Some(&source), serial);
            if let Some((previous, _)) = self.primary_source.replace((source, text)) {
                previous.destroy();
            }
        } else {
            let (Some(manager), Some(device)) = (&self.manager, &seat.device) else {
                return Err("the compositor has no data device".into());
            };
            let source = manager.create_data_source(&self.qh, ());
            for kind in TEXT_TYPES.iter().chain(html.as_ref().map(|_| &"text/html")) {
                source.offer((*kind).into());
            }
            device.set_selection(Some(&source), serial);
            if let Some((previous, ..)) = self.source.replace((source, text, html.map(Into::into)))
            {
                previous.destroy();
            }
        }
        Ok(())
    }
    fn load(&mut self, primary: bool) -> Result<Loaded, String> {
        // Even this process's own selection goes through the compositor: its
        // writer is another thread, so the caller's wait cannot deadlock.
        let (read, write) = UnixStream::pair().map_err(|e| e.to_string())?;
        let kind = if primary {
            let (offer, kind) = self
                .primary_selection
                .as_ref()
                .ok_or("the primary selection is empty")?;
            let kind = kind.ok_or("the primary selection holds no text")?;
            offer.receive(TEXT_TYPES[kind].into(), write.as_fd());
            kind
        } else {
            let offer = self.selection.as_ref().ok_or("the clipboard is empty")?;
            let kind = offer.text.ok_or("the clipboard holds no text")?;
            offer.proxy.receive(TEXT_TYPES[kind].into(), write.as_fd());
            kind
        };
        drop(write);
        Ok(Loaded(read, TEXT_TYPES[kind].starts_with("text/")))
    }
    fn cancel(&mut self) {
        if let Some(done) = self.completion.take() {
            done.offer.destroy();
        }
        if let Some(drag) = self.drag.take() {
            if !drag.left {
                (self.handler)(drag.token, FileEvent::Leave);
            }
            drag.offer.destroy();
        }
    }
    fn reject(&mut self, reason: zgui::input::FileDropError) {
        if let Some(drag) = &self.drag {
            (self.handler)(
                drag.token,
                FileEvent::Rejected {
                    x: drag.x,
                    y: drag.y,
                    reason,
                },
            );
        }
        self.cancel();
    }
    fn read_transfer(&mut self) {
        if let Some(done) = &self.completion {
            let accepted = match done.accepted.try_recv() {
                Ok(value) => Some(value),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(false),
                Err(_) => (Instant::now() > done.deadline).then_some(false),
            };
            if let Some(accepted) = accepted {
                let done = self.completion.take().unwrap();
                if accepted && done.offer.version() >= 3 {
                    done.offer.finish();
                }
                done.offer.destroy();
            }
        }
        let Some(drag) = &mut self.drag else { return };
        if !self
            .routes
            .lock()
            .unwrap()
            .values()
            .any(|token| *token == drag.token)
        {
            self.cancel();
            return;
        }
        let Some(read) = &mut drag.read else { return };
        match read.poll() {
            Ok(Some(bytes)) => {
                drag.read = None;
                match decode_uris(&bytes) {
                    Ok(paths) => {
                        if !drag.left {
                            for path in &paths {
                                (self.handler)(
                                    drag.token,
                                    FileEvent::Hover {
                                        x: drag.x,
                                        y: drag.y,
                                        path: path.clone(),
                                    },
                                );
                            }
                        }
                        drag.paths = Some(paths.into());
                        self.finish();
                    }
                    Err(reason) => self.reject(reason),
                }
            }
            Err(reason) => self.reject(reason),
            Ok(None) => {}
        }
    }
    fn finish(&mut self) {
        if !self
            .drag
            .as_ref()
            .is_some_and(|d| d.dropped && d.paths.is_some())
        {
            return;
        }
        let drag = self.drag.take().unwrap();
        if drag.copy {
            let (accepted, reply) = std::sync::mpsc::channel();
            (self.handler)(
                drag.token,
                FileEvent::Drop {
                    x: drag.x,
                    y: drag.y,
                    paths: drag.paths.unwrap(),
                    accepted,
                },
            );
            // Only the UI's accepted drop completes the protocol transaction.
            // Closing the owner, rejecting the region, or losing the callback
            // destroys the offer without falsely reporting successful receipt.
            self.completion = Some(Completion {
                offer: drag.offer,
                accepted: reply,
                deadline: Instant::now() + Duration::from_secs(3),
            });
        } else {
            drag.offer.destroy();
        }
    }
}
impl Dispatch<XdgActivation, ()> for State {
    fn event(
        _: &mut Self,
        _: &XdgActivation,
        _: <XdgActivation as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                if interface == "wl_data_device_manager" {
                    state.manager = Some(registry.bind(name, version.min(3), qh, ()));
                }
                if interface == "wl_compositor" {
                    state.compositor = Some(registry.bind(name, version.min(5), qh, ()));
                }
                if interface == "wl_shm" {
                    state.shm = Some(registry.bind(name, 1, qh, ()));
                }
                if interface == "xdg_activation_v1" {
                    state.activation = Some(registry.bind(name, 1, qh, ()));
                }
                if interface == "zwp_primary_selection_device_manager_v1" {
                    state.primary_manager = Some(registry.bind(name, 1, qh, ()));
                }
                if interface == "wl_seat" {
                    state.seats.push(Seat {
                        name,
                        seat: registry.bind(name, version.min(7), qh, name),
                        device: None,
                        primary: None,
                        keyboard: None,
                        pointer: None,
                    });
                }
                state.devices(qh);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(index) = state.seats.iter().position(|seat| seat.name == name) {
                    state.cancel();
                    state.seats.swap_remove(index).release();
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_seat::WlSeat, u32> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_seat::WlSeat,
        event: wl_seat::Event,
        name: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        else {
            return;
        };
        let Some(seat) = state.seats.iter_mut().find(|seat| seat.name == *name) else {
            return;
        };
        match (
            capabilities.contains(Capability::Keyboard),
            seat.keyboard.take(),
        ) {
            (true, None) => seat.keyboard = Some(proxy.get_keyboard(qh, *name)),
            (true, kept) => seat.keyboard = kept,
            (false, Some(keyboard)) if keyboard.version() >= 3 => keyboard.release(),
            (false, _) => {}
        }
        match (
            capabilities.contains(Capability::Pointer),
            seat.pointer.take(),
        ) {
            (true, None) => seat.pointer = Some(proxy.get_pointer(qh, *name)),
            (true, kept) => seat.pointer = kept,
            (false, Some(pointer)) if pointer.version() >= 3 => pointer.release(),
            (false, _) => {}
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        seat: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } =
            event
        {
            state.serial = Some((*seat, serial));
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        seat: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter { serial, .. } | wl_pointer::Event::Button { serial, .. } =
            event
        {
            state.serial = Some((*seat, serial));
        }
        if let wl_pointer::Event::Button {
            serial,
            state: WEnum::Value(wl_pointer::ButtonState::Pressed),
            ..
        } = event
        {
            state.press = Some((*seat, serial));
        }
    }
}
impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // The app's own drag.
            wl_data_source::Event::Send { mime_type, fd }
                if state
                    .outgoing
                    .as_ref()
                    .is_some_and(|outgoing| outgoing.source == *source) =>
            {
                let outgoing = state.outgoing.as_ref().expect("checked above");
                if let Some((_, bytes)) = outgoing.data.iter().find(|(kind, _)| *kind == mime_type)
                {
                    write_bytes(fd, bytes.clone());
                }
            }
            wl_data_source::Event::DndDropPerformed => {
                if let Some(outgoing) = &mut state.outgoing
                    && outgoing.source == *source
                {
                    outgoing.performed = true;
                }
            }
            wl_data_source::Event::DndFinished => {
                if state
                    .outgoing
                    .as_ref()
                    .is_some_and(|outgoing| outgoing.source == *source)
                {
                    state.end_outgoing(true);
                }
            }
            wl_data_source::Event::Cancelled
                if state
                    .outgoing
                    .as_ref()
                    .is_some_and(|outgoing| outgoing.source == *source) =>
            {
                // A version 1 or 2 source hears no finish: a drop
                // performed was taken.
                let taken = state
                    .outgoing
                    .as_ref()
                    .is_some_and(|outgoing| outgoing.performed && outgoing.source.version() < 3);
                if taken {
                    state.end_outgoing(true);
                } else if let Some((x, y)) = state.outgoing.as_ref().map(|outgoing| outgoing.point)
                {
                    // Let go past every window of the app's, taken by none.
                    state.end_outgoing_with(crate::native_drag::NativeDrag::Left { x, y });
                }
            }
            wl_data_source::Event::Send { mime_type, fd } => {
                if let Some((current, text, html)) = &state.source
                    && current == source
                {
                    let html = html.as_ref().filter(|_| mime_type == "text/html");
                    write_selection(fd, html.unwrap_or(text).clone());
                }
            }
            wl_data_source::Event::Cancelled => {
                if state
                    .source
                    .as_ref()
                    .is_some_and(|(current, ..)| current == source)
                {
                    state.source = None;
                }
                source.destroy();
            }
            _ => {}
        }
    }
}
impl Dispatch<PrimaryManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &PrimaryManager,
        _: <PrimaryManager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<PrimaryDevice, ()> for State {
    wayland_client::event_created_child!(State, PrimaryDevice, [0 => (PrimaryOffer, ())]);
    fn event(
        state: &mut Self,
        _: &PrimaryDevice,
        event: zwp_primary_selection_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_device_v1::Event::DataOffer { offer } => {
                if state.primary_offers.len() >= 64 {
                    for (_, (offer, _)) in state.primary_offers.drain() {
                        offer.destroy();
                    }
                }
                state
                    .primary_offers
                    .insert(offer.id().protocol_id(), (offer, None));
            }
            zwp_primary_selection_device_v1::Event::Selection { id } => {
                let offer = id.and_then(|id| state.primary_offers.remove(&id.id().protocol_id()));
                if let Some((previous, _)) = std::mem::replace(&mut state.primary_selection, offer)
                {
                    previous.destroy();
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<PrimaryOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &PrimaryOffer,
        event: zwp_primary_selection_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_offer_v1::Event::Offer { mime_type } = event
            && let Some((_, text)) = state.primary_offers.get_mut(&offer.id().protocol_id())
        {
            *text = preferred(*text, &mime_type);
        }
    }
}
impl Dispatch<PrimarySource, ()> for State {
    fn event(
        state: &mut Self,
        source: &PrimarySource,
        event: zwp_primary_selection_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_source_v1::Event::Send { fd, .. } => {
                if let Some((current, text)) = &state.primary_source
                    && current == source
                {
                    write_selection(fd, text.clone());
                }
            }
            zwp_primary_selection_source_v1::Event::Cancelled => {
                if state
                    .primary_source
                    .as_ref()
                    .is_some_and(|(current, _)| current == source)
                {
                    state.primary_source = None;
                }
                source.destroy();
            }
            _ => {}
        }
    }
}
/// The better of `current` and `kind` among `TEXT_TYPES`.
fn preferred(current: Option<usize>, kind: &str) -> Option<usize> {
    let index = TEXT_TYPES.iter().position(|candidate| *candidate == kind);
    match (current, index) {
        (Some(current), Some(index)) => Some(current.min(index)),
        (current, index) => current.or(index),
    }
}
/// Write a selection this process owns to a reader. On its own thread: a
/// reader that stops reading blocks only that thread, never the worker.
/// Write `bytes` into a drop target's pipe, off this thread.
fn write_bytes(fd: OwnedFd, bytes: Arc<[u8]>) {
    let _ = thread::Builder::new()
        .name("zgui-drag-write".into())
        .spawn(move || {
            let _ = File::from(fd).write_all(&bytes);
        });
}
wayland_client::delegate_noop!(State: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
fn write_selection(fd: OwnedFd, text: Arc<str>) {
    let _ = thread::Builder::new()
        .name("zgui-selection-write".into())
        .spawn(move || {
            let _ = File::from(fd).write_all(text.as_bytes());
        });
}
impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_offer::Event::Offer { mime_type } => {
                if let Some(saved) = state.offers.get_mut(&offer.id().protocol_id()) {
                    saved.uris |= mime_type == "text/uri-list";
                    saved.text = preferred(saved.text, &mime_type);
                }
            }
            wl_data_offer::Event::Action { dnd_action } => {
                if let Some(drag) = &mut state.drag
                    && drag.offer == *offer
                {
                    drag.copy = dnd_action.into_result().is_ok_and(|a| a == DndAction::Copy);
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    wayland_client::event_created_child!(State,wl_data_device::WlDataDevice,[0=>(wl_data_offer::WlDataOffer,())]);
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        connection: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                if state.offers.len() >= 64 {
                    for (_, offer) in state.offers.drain() {
                        offer.proxy.destroy();
                    }
                }
                state.offers.insert(
                    id.id().protocol_id(),
                    Offer {
                        proxy: id,
                        uris: false,
                        text: None,
                    },
                );
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                state.cancel();
                let Some(offer) = id.and_then(|id| state.offers.remove(&id.id().protocol_id()))
                else {
                    return;
                };
                // The app's own drag over one of its windows: zgui's, its
                // window's destinations hearing where. It is accepted there
                // (one of its own types, copied), so a release drops it rather
                // than cancels, and zgui drops it on them.
                if let Some(outgoing) = &mut state.outgoing {
                    let id = surface.id().protocol_id();
                    outgoing.home = id == outgoing.origin;
                    outgoing.over = None;
                    let window = state.routes.lock().unwrap().get(&id).copied();
                    if outgoing.home || window.is_some() {
                        offer.proxy.accept(
                            serial,
                            outgoing.data.first().map(|(kind, _)| (*kind).into()),
                        );
                        if offer.proxy.version() >= 3 {
                            offer.proxy.set_actions(DndAction::Copy, DndAction::Copy);
                        }
                    } else {
                        offer.proxy.accept(serial, None);
                    }
                    if let Some(previous) = outgoing.offer.replace(offer.proxy) {
                        previous.destroy();
                    }
                    if outgoing.home {
                        outgoing.point = (x, y);
                        (outgoing.send)(crate::native_drag::NativeDrag::Moved { x, y });
                    } else if let Some(window) = window {
                        // Over another of the app's windows.
                        outgoing.over = Some((window, (x, y)));
                        (outgoing.send)(crate::native_drag::NativeDrag::MovedIn { window, x, y });
                    }
                    return;
                }
                let token = state
                    .routes
                    .lock()
                    .unwrap()
                    .get(&surface.id().protocol_id())
                    .copied();
                let Some(token) = token.filter(|_| offer.uris) else {
                    offer.proxy.accept(serial, None);
                    offer.proxy.destroy();
                    return;
                };
                let Ok((read, write)) = UnixStream::pair() else {
                    offer.proxy.destroy();
                    return;
                };
                if read.set_nonblocking(true).is_err() {
                    offer.proxy.destroy();
                    return;
                }
                offer.proxy.accept(serial, Some("text/uri-list".into()));
                if offer.proxy.version() >= 3 {
                    offer.proxy.set_actions(DndAction::Copy, DndAction::Copy);
                }
                offer.proxy.receive("text/uri-list".into(), write.as_fd());
                drop(write);
                let copy = offer.proxy.version() < 3;
                state.drag = Some(Drag {
                    offer: offer.proxy,
                    token,
                    x: x as f32,
                    y: y as f32,
                    read: Some(Transfer {
                        stream: read,
                        bytes: Vec::new(),
                        deadline: Instant::now() + Duration::from_secs(3),
                    }),
                    paths: None,
                    dropped: false,
                    copy,
                    left: false,
                });
                let _ = connection.flush();
            }
            wl_data_device::Event::Motion { x, y, .. } if state.outgoing.is_some() => {
                let outgoing = state.outgoing.as_mut().expect("checked above");
                if outgoing.home {
                    outgoing.point = (x, y);
                    (outgoing.send)(crate::native_drag::NativeDrag::Moved { x, y });
                } else if let Some((window, point)) = &mut outgoing.over {
                    *point = (x, y);
                    let window = *window;
                    (outgoing.send)(crate::native_drag::NativeDrag::MovedIn { window, x, y });
                }
            }
            wl_data_device::Event::Drop
                if state
                    .outgoing
                    .as_ref()
                    .is_some_and(|outgoing| outgoing.home) =>
            {
                // Released over its own window: zgui drops it there.
                state.finish_own_drop();
                state.end_outgoing(false);
            }
            wl_data_device::Event::Drop
                if state
                    .outgoing
                    .as_ref()
                    .is_some_and(|outgoing| outgoing.over.is_some()) =>
            {
                // Released over another of the app's windows: zgui drops it
                // there.
                let over = state.outgoing.as_ref().and_then(|outgoing| outgoing.over);
                state.finish_own_drop();
                if let Some((window, (x, y))) = over {
                    state.end_outgoing_with(crate::native_drag::NativeDrag::DroppedIn {
                        window,
                        x,
                        y,
                    });
                }
            }
            wl_data_device::Event::Leave if state.outgoing.is_some() => {
                if let Some(outgoing) = &mut state.outgoing {
                    outgoing.home = false;
                    outgoing.over = None;
                    if let Some(offer) = outgoing.offer.take() {
                        offer.destroy();
                    }
                }
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                if let Some(drag) = &mut state.drag {
                    drag.x = x as f32;
                    drag.y = y as f32;
                    if let Some(paths) = &drag.paths
                        && let Some(path) = paths.first()
                    {
                        (state.handler)(
                            drag.token,
                            FileEvent::Hover {
                                x: drag.x,
                                y: drag.y,
                                path: path.clone(),
                            },
                        );
                    }
                }
            }
            wl_data_device::Event::Drop => {
                if let Some(drag) = &mut state.drag {
                    drag.dropped = true;
                }
                state.finish();
            }
            wl_data_device::Event::Leave => {
                if let Some(drag) = &mut state.drag {
                    (state.handler)(drag.token, FileEvent::Leave);
                    drag.left = true;
                    if !drag.dropped {
                        state.cancel();
                    }
                }
            }
            wl_data_device::Event::Selection { id } => {
                let offer = id.and_then(|id| state.offers.remove(&id.id().protocol_id()));
                if let Some(previous) = std::mem::replace(&mut state.selection, offer) {
                    previous.proxy.destroy();
                }
            }
            _ => {}
        }
    }
}
impl Transfer {
    fn poll(&mut self) -> Result<Option<Vec<u8>>, zgui::input::FileDropError> {
        use zgui::input::FileDropError;
        if Instant::now() > self.deadline {
            return Err(FileDropError::TimedOut);
        }
        let mut buffer = [0u8; 8192];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => return Ok(Some(std::mem::take(&mut self.bytes))),
                Ok(n) => {
                    if self.bytes.len() + n > 1024 * 1024 {
                        return Err(FileDropError::TooLarge);
                    }
                    self.bytes.extend_from_slice(&buffer[..n]);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Err(FileDropError::InvalidData),
            }
        }
    }
}
fn decode_uris(bytes: &[u8]) -> Result<Vec<PathBuf>, zgui::input::FileDropError> {
    use zgui::input::FileDropError;
    let text = std::str::from_utf8(bytes).map_err(|_| FileDropError::InvalidData)?;
    let mut paths = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if paths.len() == 1024 {
            return Err(FileDropError::TooManyFiles);
        }
        paths.push(crate::portal_dialog::file_uri(line).ok_or(FileDropError::InvalidData)?);
    }
    if paths.is_empty() {
        Err(FileDropError::InvalidData)
    } else {
        Ok(paths)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zgui::input::FileDropError;
    #[test]
    fn uri_list_is_bounded_and_all_or_nothing() {
        assert_eq!(
            decode_uris(b"# files\r\nfile:///tmp/a\r\nfile:///tmp/second%20file.txt\r\n").unwrap(),
            [
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/second file.txt")
            ]
        );
        for bad in [
            &b"file:///good\nhttps://remote/file"[..],
            b"file:///bad%00path",
            b"# comment only",
            b"file:///bad%",
        ] {
            assert_eq!(decode_uris(bad), Err(FileDropError::InvalidData));
        }
        assert_eq!(
            decode_uris("file:///tmp/a\n".repeat(1025).as_bytes()),
            Err(FileDropError::TooManyFiles)
        );
    }
    #[test]
    fn stalled_oversized_and_disconnected_streams_do_not_block() {
        use std::io::Write;
        let (read, mut write) = UnixStream::pair().unwrap();
        read.set_nonblocking(true).unwrap();
        let mut transfer = Transfer {
            stream: read,
            bytes: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(3),
        };
        assert!(transfer.poll().unwrap().is_none());
        transfer.deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(transfer.poll(), Err(FileDropError::TimedOut));
        transfer.deadline = Instant::now() + Duration::from_secs(3);
        transfer.bytes.resize(1024 * 1024, b'a');
        write.write_all(b"x").unwrap();
        assert_eq!(transfer.poll(), Err(FileDropError::TooLarge));
        transfer.bytes.clear();
        write.write_all(b"file:///truncated%").unwrap();
        drop(write);
        let bytes = transfer.poll().unwrap().unwrap();
        assert_eq!(decode_uris(&bytes), Err(FileDropError::InvalidData));
    }
}
