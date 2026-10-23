//! Popups past the window on Wayland: each an `xdg_popup` of the window's
//! `xdg_surface` (the vendored winit's `WindowExtWayland::xdg_surface`), which
//! the compositor places and keeps on its outputs.
//!
//! zgui's own objects live on an event queue of its own on winit's display,
//! dispatched where the event loop turns (`dispatch`): winit reads the socket
//! for both, so the queue costs no wakeup of its own. A popup is a
//! `wl_surface` with an `xdg_surface` and its `xdg_popup` role, made as it
//! shows and destroyed as it hides, drawn by a renderer of its own on the
//! window's device. Its positioner anchors it where the scene placed it (the
//! window geometry's coordinates, which start above the content by any title
//! bar winit draws) and lets the compositor slide it onto the output; the
//! compositor's word on where it went is final, and input goes by where the
//! scene drew it, so what is under the pointer is what shows there.
//!
//! The popup takes no grab: keys stay the window's, which drives menus as
//! before, and a press on another app takes the window's focus, which
//! closes them. Pointer input over a popup comes from zgui's own
//! `wl_pointer`, in the window's coordinates, to its host.
//! todo(wayland): the grab (`xdg_popup.grab`), with keys through a
//! `wl_keyboard` of zgui's own; flips at the output's edge from the
//! placement's side (`xdg_positioner` anchored on the trigger).
use super::PopupInput;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
    backend::ObjectId,
    delegate_noop,
    protocol::{
        wl_compositor::WlCompositor,
        wl_pointer::{self, WlPointer},
        wl_registry::{self, WlRegistry},
        wl_seat::{self, Capability, WlSeat},
        wl_surface::WlSurface,
    },
};
use wayland_protocols::{
    wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
    xdg::shell::client::{
        xdg_popup::{self, XdgPopup},
        xdg_positioner::{Anchor, ConstraintAdjustment, Gravity, XdgPositioner},
        xdg_surface::{self, XdgSurface},
        xdg_wm_base::{self, XdgWmBase},
    },
};
use winit::{
    dpi::LogicalPosition,
    event::{MouseButton, MouseScrollDelta, TouchPhase},
    platform::wayland::WindowExtWayland,
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle, RawWindowHandle, WaylandWindowHandle},
    window::Window,
};
use zgui::scene::{NodeId, Rect, Scene};
use zgui_gpu::{GpuContext, GpuError, GpuRenderer};

type Sink = Rc<dyn Fn(PopupInput)>;

/// Where a popup's pointer input goes: its box's origin in its parent
/// window (the scene's), and the parent's host.
struct Target {
    origin: (f32, f32),
    scale: f64,
    send: Sink,
}

/// What zgui's queue hears.
#[derive(Default)]
struct State {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    viewporter: Option<WpViewporter>,
    seats: Vec<(WlSeat, Option<WlPointer>)>,
    /// Each popup's surface's target.
    targets: HashMap<ObjectId, Target>,
    /// The popup the pointer is over, and where (its parent's points).
    over: Option<(ObjectId, (f32, f32))>,
    /// `xdg_surface`s configured before they first showed, by the serial
    /// `show` acks.
    configured: HashMap<ObjectId, u32>,
    /// `xdg_surface`s shown: configured again, acked at once.
    live: std::collections::HashSet<ObjectId>,
    /// Popups the compositor dismissed (`popup_done`).
    done: Vec<ObjectId>,
}

/// zgui's queue on winit's display.
struct Client {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    display: RawDisplayHandle,
}

thread_local! {
    /// Never dropped: winit closes the display before the thread's locals
    /// go, and a queue dropped after its display would touch freed memory.
    static CLIENT: RefCell<Option<std::mem::ManuallyDrop<Client>>> = const { RefCell::new(None) };
    /// Tried and found wanting (no `xdg_wm_base`, or not Wayland).
    static FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// zgui's queue on `window`'s display, made as first wanted.
fn with_client<T>(window: &Window, then: impl FnOnce(&mut Client) -> Option<T>) -> Option<T> {
    if FAILED.with(std::cell::Cell::get) {
        return None;
    }
    CLIENT.with(|client| {
        let mut client = client.borrow_mut();
        if client.is_none() {
            *client = connect(window).map(std::mem::ManuallyDrop::new);
            if client.is_none() {
                FAILED.with(|failed| failed.set(true));
            }
        }
        then(client.as_deref_mut()?)
    })
}

fn connect(window: &Window) -> Option<Client> {
    window.xdg_surface()?;
    let display = window.display_handle().ok()?.as_raw();
    let RawDisplayHandle::Wayland(handle) = display else {
        return None;
    };
    // SAFETY: winit's display, alive as long as the event loop (and with it
    // the windows this module draws popups of).
    let backend = unsafe {
        wayland_backend::sys::client::Backend::from_foreign_display(handle.display.as_ptr().cast())
    };
    let connection = Connection::from_backend(backend);
    let mut queue = connection.new_event_queue::<State>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state).ok()?;
    // The seats' capabilities.
    queue.roundtrip(&mut state).ok()?;
    if state.compositor.is_none() || state.wm_base.is_none() {
        return None;
    }
    Some(Client {
        connection,
        queue,
        state,
        display,
    })
}

/// Whether popups show past `window` (Wayland's): its compositor has
/// `xdg_wm_base`, and winit gives its `xdg_surface`.
pub(super) fn available(window: &Window) -> bool {
    with_client(window, |_| Some(())).is_some()
}

/// Dispatch what zgui's queue heard (where the event loop turns: winit read
/// the socket for both).
pub(crate) fn dispatch() {
    CLIENT.with(|client| {
        if let Ok(mut client) = client.try_borrow_mut()
            && let Some(client) = client.as_deref_mut()
        {
            let _ = client.queue.dispatch_pending(&mut client.state);
            let _ = client.connection.flush();
        }
    });
}

/// A popup shown.
struct Shown {
    // Dropped first: its surface goes before the wl_surface it draws to.
    renderer: GpuRenderer,
    popup: XdgPopup,
    xdg: XdgSurface,
    viewport: Option<WpViewport>,
    surface: WlSurface,
    rect: Rect,
    revision: Option<u64>,
}
impl Shown {
    fn destroy(self, client: &mut Client) {
        let Shown {
            renderer,
            popup,
            xdg,
            viewport,
            surface,
            ..
        } = self;
        drop(renderer);
        client.state.targets.remove(&surface.id());
        if client
            .state
            .over
            .as_ref()
            .is_some_and(|(over, _)| *over == surface.id())
        {
            client.state.over = None;
        }
        client.state.live.remove(&xdg.id());
        popup.destroy();
        xdg.destroy();
        if let Some(viewport) = viewport {
            viewport.destroy();
        }
        surface.destroy();
        let _ = client.connection.flush();
    }
}

/// A window's popups on Wayland.
pub(super) struct Popups {
    shown: Vec<(NodeId, Shown)>,
    send: Sink,
}
impl Popups {
    pub fn new(send: Sink) -> Self {
        Self {
            shown: Vec::new(),
            send,
        }
    }
    pub fn showing(&self) -> bool {
        !self.shown.is_empty()
    }
    /// After the scene's flush: show each visible popup root in an
    /// `xdg_popup` of `parent`, draw it again where it changed (always when
    /// `full`), and hide the rest.
    pub fn update(
        &mut self,
        parent: &Window,
        scene: &Scene,
        context: &GpuContext,
        scale: f32,
        full: bool,
    ) -> Result<(), GpuError> {
        if self.shown.is_empty() && scene.popups().is_empty() {
            return Ok(());
        }
        let wanted: Vec<(NodeId, Rect)> = scene
            .popups()
            .iter()
            .filter_map(|root| scene.popup_bounds(*root).map(|bounds| (*root, bounds)))
            .collect();
        let shown = std::mem::take(&mut self.shown);
        let send = self.send.clone();
        with_client(parent, |client| {
            // Dismissed by the compositor: its host closes it.
            for done in client.state.done.drain(..) {
                if shown.iter().any(|(_, popup)| popup.popup.id() == done) {
                    (send)(PopupInput::Dismiss);
                }
            }
            let mut kept = Vec::new();
            for (root, popup) in shown {
                // A popup that moved or resized shows anew.
                let still = wanted
                    .iter()
                    .any(|(wanted, bounds)| *wanted == root && rects_match(*bounds, popup.rect));
                if still {
                    kept.push((root, popup));
                } else {
                    zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
                    popup.destroy(client);
                }
            }
            for (root, bounds) in &wanted {
                let index = match kept.iter().position(|(shown, _)| shown == root) {
                    Some(index) => index,
                    None => {
                        let Some(popup) = show(client, parent, *root, *bounds, scale, context)
                        else {
                            continue;
                        };
                        kept.push((*root, popup));
                        kept.len() - 1
                    }
                };
                let popup = &mut kept[index].1;
                let revision = scene.layer_revision(*root);
                if full || popup.revision != Some(revision) {
                    popup.revision = Some(revision);
                    zgui_gpu::present_log::log(|| format!("popup draw {root:?}"));
                    popup.renderer.set_popup(*root, *bounds);
                    popup.renderer.repaint_all();
                    if popup.renderer.render(scene, &[]).is_ok() {
                        let _ = popup.renderer.present();
                    }
                }
                client.state.targets.insert(
                    popup.surface.id(),
                    Target {
                        origin: (popup.rect.x, popup.rect.y),
                        scale: scale as f64,
                        send: send.clone(),
                    },
                );
            }
            let _ = client.connection.flush();
            self.shown = kept;
            Some(())
        });
        Ok(())
    }
    pub fn hide_all(&mut self, parent: &Window) {
        let shown = std::mem::take(&mut self.shown);
        with_client(parent, |client| {
            for (_, popup) in shown {
                popup.destroy(client);
            }
            Some(())
        });
    }
}

fn rects_match(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < 0.5
        && (a.y - b.y).abs() < 0.5
        && (a.width - b.width).abs() < 0.5
        && (a.height - b.height).abs() < 0.5
}

/// Show popup `root` at `rect` (`parent`'s points) in an `xdg_popup` of
/// `parent`, configured and ready to draw.
fn show(
    client: &mut Client,
    parent: &Window,
    root: NodeId,
    rect: Rect,
    scale: f32,
    context: &GpuContext,
) -> Option<Shown> {
    let qh = client.queue.handle();
    let (compositor, wm_base) = (
        client.state.compositor.clone()?,
        client.state.wm_base.clone()?,
    );
    // SAFETY: winit's live xdg_surface on this display: `parent` holds it.
    let parent_xdg = unsafe {
        ObjectId::from_ptr(
            XdgSurface::interface(),
            parent.xdg_surface()?.as_ptr().cast(),
        )
    }
    .ok()
    .and_then(|id| XdgSurface::from_id(&client.connection, id).ok())?;
    let (gx, gy) = parent.xdg_window_geometry_origin().unwrap_or((0, 0));
    let width = rect.width.ceil().max(1.) as i32;
    let height = rect.height.ceil().max(1.) as i32;
    let surface = compositor.create_surface(&qh, ());
    let viewport = client
        .state
        .viewporter
        .as_ref()
        .map(|viewporter| viewporter.get_viewport(&surface, &qh, ()));
    let xdg = wm_base.get_xdg_surface(&surface, &qh, ());
    let positioner = wm_base.create_positioner(&qh, ());
    positioner.set_size(width, height);
    // Where the scene put it, in the window geometry's coordinates.
    positioner.set_anchor_rect(rect.x.round() as i32 - gx, rect.y.round() as i32 - gy, 1, 1);
    positioner.set_anchor(Anchor::TopLeft);
    positioner.set_gravity(Gravity::BottomRight);
    positioner
        .set_constraint_adjustment(ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY);
    let popup = xdg.get_popup(Some(&parent_xdg), &positioner, &qh, ());
    positioner.destroy();
    xdg.set_window_geometry(0, 0, width, height);
    let physical = (
        (rect.width * scale).round().max(1.) as u32,
        (rect.height * scale).round().max(1.) as u32,
    );
    match &viewport {
        Some(viewport) => viewport.set_destination(width, height),
        None => surface.set_buffer_scale(scale.round().max(1.) as i32),
    }
    surface.commit();
    // Configured before its first frame, as the protocol asks.
    let xdg_id = xdg.id();
    for _ in 0..4 {
        if client.state.configured.contains_key(&xdg_id) {
            break;
        }
        client.queue.roundtrip(&mut client.state).ok()?;
    }
    let serial = client.state.configured.remove(&xdg_id)?;
    xdg.ack_configure(serial);
    client.state.live.insert(xdg_id);
    let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(std::ptr::NonNull::new(
        surface.id().as_ptr().cast(),
    )?));
    zgui_gpu::present_log::log(|| format!("popup show {root:?} at {rect:?}"));
    // SAFETY: the display outlives the popup, and its surface is destroyed
    // only after the renderer (`Shown::destroy`).
    let renderer = unsafe {
        GpuRenderer::for_raw_surface(
            client.display,
            window,
            physical.0,
            physical.1,
            scale,
            context,
        )
    };
    let renderer = match renderer {
        Ok(renderer) => renderer,
        Err(_) => {
            popup.destroy();
            xdg.destroy();
            if let Some(viewport) = viewport {
                viewport.destroy();
            }
            surface.destroy();
            return None;
        }
    };
    Some(Shown {
        renderer,
        popup,
        xdg,
        viewport,
        surface,
        rect,
        revision: None,
    })
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
                "wp_viewporter" => state.viewporter = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => state
                    .seats
                    .push((registry.bind(name, version.min(5), qh, ()), None)),
                _ => {}
            }
        }
    }
}
impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        else {
            return;
        };
        let Some((_, pointer)) = state.seats.iter_mut().find(|(each, _)| each == seat) else {
            return;
        };
        match (capabilities.contains(Capability::Pointer), pointer.take()) {
            (true, None) => *pointer = Some(seat.get_pointer(qh, ())),
            (true, kept) => *pointer = kept,
            (false, Some(old)) if old.version() >= 3 => old.release(),
            (false, _) => {}
        }
    }
}
impl Dispatch<WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let at = |target: &Target, x: f64, y: f64| {
            (target.origin.0 + x as f32, target.origin.1 + y as f32)
        };
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                let Some(target) = state.targets.get(&surface.id()) else {
                    return;
                };
                let point = at(target, surface_x, surface_y);
                (target.send)(PopupInput::Moved {
                    x: point.0,
                    y: point.1,
                });
                state.over = Some((surface.id(), point));
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                let Some((id, point)) = &mut state.over else {
                    return;
                };
                let Some(target) = state.targets.get(id) else {
                    return;
                };
                *point = at(target, surface_x, surface_y);
                (target.send)(PopupInput::Moved {
                    x: point.0,
                    y: point.1,
                });
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if let Some(target) = state.targets.get(&surface.id()) {
                    (target.send)(PopupInput::Left);
                }
                state.over = None;
            }
            wl_pointer::Event::Button {
                button,
                state: pressed,
                ..
            } => {
                let Some((id, (x, y))) = &state.over else {
                    return;
                };
                let Some(target) = state.targets.get(id) else {
                    return;
                };
                // Linux's input codes (BTN_LEFT and on).
                let button = match button {
                    0x110 => MouseButton::Left,
                    0x111 => MouseButton::Right,
                    0x112 => MouseButton::Middle,
                    0x116 => MouseButton::Back,
                    0x115 => MouseButton::Forward,
                    other => MouseButton::Other(other as u16),
                };
                (target.send)(PopupInput::Button {
                    x: *x,
                    y: *y,
                    button,
                    pressed: matches!(pressed, WEnum::Value(wl_pointer::ButtonState::Pressed)),
                });
            }
            wl_pointer::Event::Axis {
                axis: WEnum::Value(axis),
                value,
                ..
            } => {
                let Some((id, (x, y))) = &state.over else {
                    return;
                };
                let Some(target) = state.targets.get(id) else {
                    return;
                };
                // Wayland's sign is the inverse of winit's.
                let (dx, dy) = match axis {
                    wl_pointer::Axis::HorizontalScroll => (-value, 0.),
                    _ => (0., -value),
                };
                (target.send)(PopupInput::Wheel {
                    x: *x,
                    y: *y,
                    delta: MouseScrollDelta::PixelDelta(
                        LogicalPosition::new(dx, dy).to_physical(target.scale),
                    ),
                    phase: TouchPhase::Moved,
                });
            }
            _ => {}
        }
    }
}
impl Dispatch<XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}
impl Dispatch<XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg: &XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            if state.live.contains(&xdg.id()) {
                xdg.ack_configure(serial);
            } else {
                state.configured.insert(xdg.id(), serial);
            }
        }
    }
}
impl Dispatch<XdgPopup, ()> for State {
    fn event(
        state: &mut Self,
        popup: &XdgPopup,
        event: xdg_popup::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => zgui_gpu::present_log::log(|| {
                format!("popup configured at {x},{y} {width}x{height}")
            }),
            xdg_popup::Event::PopupDone => state.done.push(popup.id()),
            _ => {}
        }
    }
}
delegate_noop!(State: ignore WlCompositor);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ignore XdgPositioner);
delegate_noop!(State: ignore WpViewporter);
delegate_noop!(State: ignore WpViewport);
