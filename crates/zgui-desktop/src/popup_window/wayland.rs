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
//! bar winit draws), against its trigger where it has one
//! (`Scene::popup_anchor`): the compositor flips it to the trigger's other
//! side at the output's edge, as GTK's menus, or slides it onto the output.
//! The compositor's word on where it went is final, and input goes by where
//! the scene drew it, so what is under the pointer is what shows there.
//!
//! The popup takes no grab: keys stay the window's, which drives menus as
//! before, and a press on another app takes the window's focus, which
//! closes them. Pointer input over a popup comes from zgui's own
//! `wl_pointer`, in the window's coordinates, to its host.
//! todo(wayland): the grab (`xdg_popup.grab`), with keys through a
//! `wl_keyboard` of zgui's own.
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
    /// The popup it is a popup of (a submenu's menu), if not the window's.
    parent: Option<NodeId>,
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
            // What goes: popups no longer wanted, or moved or resized (to
            // show anew), and the popups of those (a parent goes after its
            // children, as the protocol has it).
            let mut kept = shown;
            let mut going: Vec<NodeId> = kept
                .iter()
                .filter(|(root, popup)| {
                    !wanted
                        .iter()
                        .any(|(wanted, bounds)| wanted == root && rects_match(*bounds, popup.rect))
                })
                .map(|(root, _)| *root)
                .collect();
            loop {
                let children: Vec<NodeId> = kept
                    .iter()
                    .filter(|(root, popup)| {
                        !going.contains(root) && popup.parent.is_some_and(|p| going.contains(&p))
                    })
                    .map(|(root, _)| *root)
                    .collect();
                if children.is_empty() {
                    break;
                }
                going.extend(children);
            }
            // The newest first: children before their parents.
            for index in (0..kept.len()).rev() {
                if going.contains(&kept[index].0) {
                    let (root, popup) = kept.remove(index);
                    zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
                    popup.destroy(client);
                }
            }
            for (root, bounds) in &wanted {
                let index = match kept.iter().position(|(shown, _)| shown == root) {
                    Some(index) => index,
                    None => {
                        let anchor = scene.popup_anchor(*root);
                        let trigger = anchor.map(|anchor| scene.bounds(anchor));
                        // A submenu is a popup of the menu holding its item,
                        // placed against that menu's surface, wherever the
                        // compositor put it.
                        let holder = anchor.and_then(|anchor| {
                            kept.iter().find(|(shown, _)| {
                                scene.ancestors(anchor).any(|node| node == *shown)
                            })
                        });
                        let of =
                            holder.map(|(holder, popup)| (*holder, popup.xdg.clone(), popup.rect));
                        let Some(popup) =
                            show(client, parent, of, *root, *bounds, trigger, scale, context)
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
            // The newest first: children before their parents.
            for (_, popup) in shown.into_iter().rev() {
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

/// Where an `xdg_positioner` puts a popup the scene placed at `rect` (in the
/// window geometry's coordinates, as `trigger`): against its trigger, on the
/// side the scene put it, the compositor free to flip it to the other side
/// at the output's edge; with no trigger, or over it (`open_over`), at its
/// place, slid onto the output. Anchor rectangle, anchor, gravity, offset
/// and adjustments.
fn position(
    rect: Rect,
    trigger: Option<Rect>,
) -> (Rect, Anchor, Gravity, (i32, i32), ConstraintAdjustment) {
    let slide = ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY;
    let point = (
        Rect::new(rect.x.round(), rect.y.round(), 1., 1.),
        Anchor::TopLeft,
        Gravity::BottomRight,
        (0, 0),
        slide,
    );
    let Some(t) = trigger.filter(|t| t.width >= 1. && t.height >= 1.) else {
        return point;
    };
    let t = Rect::new(t.x.round(), t.y.round(), t.width.round(), t.height.round());
    let (right, bottom) = (t.x + t.width, t.y + t.height);
    let offset = |x: f32, y: f32| (x.round() as i32, y.round() as i32);
    let (flip_x, flip_y) = (
        ConstraintAdjustment::FlipX | slide,
        ConstraintAdjustment::FlipY | slide,
    );
    // The gap from the trigger widens the anchor rectangle on both sides,
    // rather than going in the offset, which a flip keeps as it is
    // (wlroots): flipped, it opens as far from the other side.
    let tall = |gap: f32| Rect::new(t.x, t.y - gap, t.width, t.height + 2. * gap);
    let wide = |gap: f32| Rect::new(t.x - gap, t.y, t.width + 2. * gap, t.height);
    if rect.y >= bottom - 0.5 {
        let anchor = tall((rect.y - bottom).round());
        let at = offset(rect.x - t.x, 0.);
        (anchor, Anchor::BottomLeft, Gravity::BottomRight, at, flip_y)
    } else if rect.y + rect.height <= t.y + 0.5 {
        let anchor = tall((t.y - rect.y - rect.height).round());
        let at = offset(rect.x - t.x, 0.);
        (anchor, Anchor::TopLeft, Gravity::TopRight, at, flip_y)
    } else if rect.x >= right - 0.5 {
        let anchor = wide((rect.x - right).round());
        let at = offset(0., rect.y - t.y);
        (anchor, Anchor::TopRight, Gravity::BottomRight, at, flip_x)
    } else if rect.x + rect.width <= t.x + 0.5 {
        let anchor = wide((t.x - rect.x - rect.width).round());
        let at = offset(0., rect.y - t.y);
        (anchor, Anchor::TopLeft, Gravity::BottomLeft, at, flip_x)
    } else {
        point
    }
}

/// Show popup `root` at `rect` (`parent`'s points), opened against
/// `trigger` if it has one, in an `xdg_popup` of `parent`'s, or of popup `of`
/// (its root, `xdg_surface` and box) where it opens from one, configured and
/// ready to draw.
#[allow(clippy::too_many_arguments)]
fn show(
    client: &mut Client,
    parent: &Window,
    of: Option<(NodeId, XdgSurface, Rect)>,
    root: NodeId,
    rect: Rect,
    trigger: Option<Rect>,
    scale: f32,
    context: &GpuContext,
) -> Option<Shown> {
    let qh = client.queue.handle();
    let (compositor, wm_base) = (
        client.state.compositor.clone()?,
        client.state.wm_base.clone()?,
    );
    // Positions relative to the parent's window geometry: a popup's is its
    // surface, which shows its box from the box's top-left.
    let (parent_xdg, (gx, gy)) = match &of {
        Some((_, xdg, rect)) => (xdg.clone(), (rect.x.round() as i32, rect.y.round() as i32)),
        None => {
            // SAFETY: winit's live xdg_surface on this display: `parent`
            // holds it.
            let xdg = unsafe {
                ObjectId::from_ptr(
                    XdgSurface::interface(),
                    parent.xdg_surface()?.as_ptr().cast(),
                )
            }
            .ok()
            .and_then(|id| XdgSurface::from_id(&client.connection, id).ok())?;
            (xdg, parent.xdg_window_geometry_origin().unwrap_or((0, 0)))
        }
    };
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
    // In the window geometry's coordinates.
    let geometry = |r: Rect| Rect::new(r.x - gx as f32, r.y - gy as f32, r.width, r.height);
    let (anchor, edge, gravity, (dx, dy), adjust) = position(geometry(rect), trigger.map(geometry));
    positioner.set_anchor_rect(
        anchor.x as i32,
        anchor.y as i32,
        anchor.width as i32,
        anchor.height as i32,
    );
    positioner.set_anchor(edge);
    positioner.set_gravity(gravity);
    positioner.set_offset(dx, dy);
    positioner.set_constraint_adjustment(adjust);
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
        parent: of.map(|(holder, ..)| holder),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_popup_opens_against_its_trigger_on_the_side_the_scene_chose() {
        let trigger = Rect::new(16., 396., 120., 28.);
        // Below, 4 points under it: the gap is the anchor rectangle's, on
        // both sides, so a flip opens it as far above.
        let grown = Rect::new(16., 392., 120., 36.);
        let below = position(Rect::new(16., 428., 200., 202.), Some(trigger));
        assert_eq!(
            (below.0, below.1, below.2, below.3),
            (grown, Anchor::BottomLeft, Gravity::BottomRight, (0, 0))
        );
        assert!(below.4.contains(ConstraintAdjustment::FlipY));
        let above = position(Rect::new(16., 190., 200., 202.), Some(trigger));
        assert_eq!(
            (above.0, above.1, above.2, above.3),
            (grown, Anchor::TopLeft, Gravity::TopRight, (0, 0))
        );
        // A submenu beside its item, 2 points off it, its top 4 above the
        // item's: free to flip left.
        let item = Rect::new(20., 100., 180., 24.);
        let beside = position(Rect::new(202., 96., 160., 80.), Some(item));
        assert_eq!(
            (beside.0, beside.1, beside.2, beside.3),
            (
                Rect::new(18., 100., 184., 24.),
                Anchor::TopRight,
                Gravity::BottomRight,
                (0, -4)
            )
        );
        assert!(beside.4.contains(ConstraintAdjustment::FlipX));
        // Over its trigger (`open_over`), or with none: at its place.
        let over = position(Rect::new(10., 390., 200., 202.), Some(trigger));
        assert_eq!((over.0, over.3), (Rect::new(10., 390., 1., 1.), (0, 0)));
        assert_eq!(
            position(Rect::new(5., 6., 7., 8.), None).0,
            Rect::new(5., 6., 1., 1.)
        );
    }
}
