//! A window's zoom, as a browser's page zoom: Command-plus and Command-minus
//! (Control elsewhere) step through Chrome's levels, Command-0 goes back to
//! 100%. The host folds it into the window's scale (its content scale is
//! the display's times the zoom), so layout reflows in a larger or smaller
//! logical viewport and text and paths draw crisply at the scale they show
//! at; at 100% the scale is the display's, untouched.
//!
//! A step costs what a change of display does (one reflow and one
//! re-rasterization); nothing runs while the zoom stays. Scroll views keep
//! what was at their top in place across it. An app's keymap or menu taking
//! the keys keeps them; `Ui::set_zoomable(false)` turns zoom off.
//!
//! On Windows and Linux Control with the wheel zooms too, a level a notch,
//! as browsers there do: the wheel goes to the nodes first, and zooms if
//! none took it (scroll views leave it). macOS browsers zoom with neither
//! wheel nor pinch, and neither does zgx there.
//!
//! An element can be zoomable too (`View::zoomable`): its zoom bound to a
//! state the user changes with Control and the wheel or a pinch over it,
//! the window zooming where none is. The window's `zoomable` (whether the
//! user may zoom it) and an element's are the same idea at two levels.
use crate::{
    input::{InputEvent, Key, Modifiers},
    scene::NodeId,
    widgets::Ui,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

thread_local! {
    /// A Control-wheel being dispatched (`wheel`): its notch, in points.
    /// Scroll views leave it.
    static WHEEL: Cell<Option<f32>> = const { Cell::new(None) };
}

/// Run `dispatch`, a Control-wheel's scroll going to the nodes, with the
/// window's scroll views leaving it to the zoom (`Ui::zoom_wheel`).
/// `notch`: the points a notch scrolls, which a zoomable element counts
/// its notches in (`View::zoomable`).
#[doc(hidden)]
pub fn wheel_notched<R>(notch: f32, dispatch: impl FnOnce() -> R) -> R {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            WHEEL.with(|wheel| wheel.set(None));
        }
    }
    WHEEL.with(|wheel| wheel.set(Some(notch)));
    let _reset = Reset;
    dispatch()
}
/// `wheel_notched` with no notch known, as zgx-desktop 0.1.3 calls it: scroll
/// views leave the Control-wheel to the window's zoom, and zoomable
/// elements, which count notches, take none.
#[doc(hidden)]
pub fn wheel<R>(dispatch: impl FnOnce() -> R) -> R {
    wheel_notched(f32::NAN, dispatch)
}
/// Whether the scroll being dispatched is a Control-wheel's zoom: a scroll
/// view leaves it.
pub(crate) fn wheel_zooms() -> bool {
    WHEEL.with(Cell::get).is_some()
}
/// The points a notch of the Control-wheel being dispatched scrolls.
fn wheel_notch() -> Option<f32> {
    WHEEL
        .with(Cell::get)
        .filter(|notch| notch.is_finite() && *notch > 0.)
}
/// The next of Chrome's levels above `zoom` (`up`) or below it, the last
/// one at either end.
fn next_level(zoom: f64, up: bool) -> f64 {
    match up {
        true => LEVELS
            .iter()
            .copied()
            .find(|level| *level > zoom + 1e-6)
            .unwrap_or(LEVELS[LEVELS.len() - 1]),
        false => LEVELS
            .iter()
            .rev()
            .copied()
            .find(|level| *level < zoom - 1e-6)
            .unwrap_or(LEVELS[0]),
    }
}

/// Chrome's zoom levels.
pub const LEVELS: [f64; 17] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1., 1.1, 1.25, 1.5, 1.75, 2., 2.5, 3., 4., 5.,
];

/// What scroll views had at their top before a zoom: each viewport, the
/// node there and how far below the viewport's top it was.
#[doc(hidden)]
pub struct Anchors(Vec<(NodeId, NodeId, f32)>);

impl Ui {
    /// The window's zoom: 1 at 100%.
    pub fn zoom(&self) -> f64 {
        self.storage.zoom.get()
    }
    /// Zoom the window to `zoom` (1 is 100%), kept within Chrome's 25% to
    /// 500%. The host reflows the window at it, as a browser's page zoom.
    pub fn set_zoom(&self, zoom: f64) {
        if !zoom.is_finite() {
            return;
        }
        let zoom = zoom.clamp(LEVELS[0], LEVELS[LEVELS.len() - 1]);
        if self.storage.zoom.replace(zoom) == zoom {
            return;
        }
        // Fixed nodes undo it (`set_zoom_fixed`).
        if !self.storage.fixed_zooms.borrow().is_empty()
            && let Ok(mut scene) = self.scene.try_borrow_mut()
        {
            refresh_fixed(self, &mut scene);
        }
        let told = self.storage.zoom_host.borrow().clone();
        for tell in told {
            tell();
        }
    }
    /// One of Chrome's levels in: the next above the zoom.
    pub fn zoom_in(&self) {
        self.set_zoom(next_level(self.zoom(), true));
    }
    /// One of Chrome's levels out: the next below the zoom.
    pub fn zoom_out(&self) {
        self.set_zoom(next_level(self.zoom(), false));
    }
    /// Back to 100%.
    pub fn reset_zoom(&self) {
        self.set_zoom(1.);
    }
    /// Whether Command-plus, minus and 0 (Control elsewhere) zoom this
    /// window: on unless the app turns it off, or binds the keys itself.
    pub fn set_zoomable(&self, zoomable: bool) {
        self.storage.zoom_off.set(!zoomable);
    }
    /// Whether the zoom keys zoom this window (`set_zoomable`).
    pub fn zoomable(&self) -> bool {
        !self.storage.zoom_off.get()
    }
    /// Whether a Control-wheel zooms this window (the host asks before
    /// dispatching one: `wheel`, then `zoom_wheel`).
    #[doc(hidden)]
    pub fn wheel_zooms(&self) -> bool {
        self.zoomable() && !self.storage.zoom_host.borrow().is_empty()
    }
    /// A Control-wheel nothing took turned `notches` (up, away from the
    /// user, is positive): a level in or out for each whole notch, a
    /// touchpad's fractions adding up.
    #[doc(hidden)]
    pub fn zoom_wheel(&self, notches: f32) {
        if !notches.is_finite() {
            return;
        }
        let mut turned = self.storage.zoom_wheel.get() + notches;
        while turned >= 1. {
            self.zoom_in();
            turned -= 1.;
        }
        while turned <= -1. {
            self.zoom_out();
            turned += 1.;
        }
        self.storage.zoom_wheel.set(turned);
    }
    /// `told` each time the zoom changes: the host, to apply it; a script,
    /// to show it.
    #[doc(hidden)]
    pub fn on_zoom(&self, told: impl Fn() + 'static) {
        self.storage.zoom_host.borrow_mut().push(Rc::new(told));
    }
    /// What each scrolled view shows at its top, to keep there through a
    /// zoom (`restore_zoom_anchors`).
    #[doc(hidden)]
    pub fn zoom_anchors(&self) -> Anchors {
        let scene = self.scene.borrow();
        let mut anchors = Vec::new();
        let mut pending = vec![scene.root()];
        while let Some(id) = pending.pop() {
            if scene.is_scroll_content(id)
                && scene.transform(id).y < 0.
                && let Some(viewport) = scene.parent(id)
            {
                let top = scene.bounds(viewport).y;
                // The innermost node reaching past the viewport's top.
                let mut anchor = None;
                let mut within = id;
                while let Some(child) = scene.children(within).iter().copied().find(|child| {
                    let b = scene.bounds(*child);
                    b.y + b.height > top
                }) {
                    anchor = Some(child);
                    within = child;
                }
                if let Some(anchor) = anchor {
                    anchors.push((viewport, anchor, scene.bounds(anchor).y - top));
                }
            }
            pending.extend(scene.children(id).iter().rev().copied());
        }
        Anchors(anchors)
    }
    /// Scroll each view so what was at its top (`zoom_anchors`) is there
    /// again, once the zoom's layout is done.
    #[doc(hidden)]
    pub fn restore_zoom_anchors(&mut self, anchors: Anchors) {
        for (viewport, anchor, was) in anchors.0 {
            let delta = {
                let scene = self.scene.borrow();
                if !scene.contains(anchor) || !scene.contains(viewport) {
                    continue;
                }
                scene.bounds(anchor).y - scene.bounds(viewport).y - was
            };
            if delta.abs() < 0.5 {
                continue;
            }
            self.input.dispatch_to(
                &self.scene,
                anchor,
                InputEvent::Scroll {
                    x: 0.,
                    y: 0.,
                    delta_x: 0.,
                    delta_y: delta,
                },
            );
            self.prepare_frame();
        }
    }
}

/// Whether `key` with `modifiers` zooms a window: Command (Control
/// elsewhere) with plus or equals, minus, or 0 (not with Shift), never with
/// Option or the other of Command and Control. An app's binding of one
/// keeps it from zoom (a menu's, too: the zoom items stay out); the chords
/// with Option or both (Command-Option-=, Command-Control-=) are the app's.
pub fn is_zoom_key(key: &Key, modifiers: Modifiers) -> bool {
    let Key::Character(key) = key else {
        return false;
    };
    let both = modifiers.meta && modifiers.control;
    modifiers.primary_shortcut()
        && !modifiers.alt
        && !both
        && match key.as_str() {
            "=" | "+" | "-" | "_" => true,
            "0" => !modifiers.shift,
            _ => false,
        }
}

impl Ui {
    /// Keep `node` at its own size whatever zooms its ancestors and the
    /// window have (a toolbar that stays put, a canvas that zooms itself):
    /// its zoom (`Scene::set_zoom`) undoes theirs, kept so as they change.
    /// `false` leaves it at 1.
    pub fn set_zoom_fixed(&self, node: NodeId, fixed: bool) {
        let mut scene = self.scene.borrow_mut();
        set_fixed(self, &mut scene, node, fixed);
    }
}

/// `Ui::set_zoom_fixed`, with the scene already borrowed.
pub(crate) fn set_fixed(ui: &Ui, scene: &mut crate::scene::Scene, node: NodeId, fixed: bool) {
    let mut list = ui.storage.fixed_zooms.borrow_mut();
    match (list.contains(&node), fixed) {
        (false, true) => list.push(node),
        (true, false) => {
            list.retain(|other| *other != node);
            drop(list);
            scene.set_zoom(node, 1.);
            return;
        }
        _ => return,
    }
    drop(list);
    refresh_fixed(ui, scene);
}

/// Each fixed node's zoom, undoing the window's and its ancestors': after
/// either changes. Gone nodes leave the list.
// Inlined: the style path (`apply_resolved`) took it inline before a
// zoomable's binding called it too; out of line, it cost every style apply.
#[inline(always)]
pub(crate) fn refresh_fixed(ui: &Ui, scene: &mut crate::scene::Scene) {
    let mut list = ui.storage.fixed_zooms.borrow_mut();
    if list.is_empty() {
        return;
    }
    list.retain(|id| scene.contains(*id));
    let window = ui.zoom() as f32;
    for id in list.iter() {
        let around = scene
            .parent(*id)
            .map_or(1., |parent| scene.zoom_around(parent));
        scene.set_zoom(*id, (window * around).recip());
    }
}

/// The window's zoom keys (`is_zoom_key`), for keys nothing else took: in
/// with plus or equals, out with minus, back to 100% with 0. Whether it
/// took the key.
pub(crate) fn after_dispatch(ui: &Ui, event: &InputEvent) -> bool {
    let InputEvent::KeyDown { key, modifiers, .. } = event else {
        return false;
    };
    if !is_zoom_key(key, *modifiers) || !ui.zoomable() || ui.storage.zoom_host.borrow().is_empty() {
        return false;
    }
    match key {
        Key::Character(key) if matches!(key.as_str(), "=" | "+") => ui.zoom_in(),
        Key::Character(key) if matches!(key.as_str(), "-" | "_") => ui.zoom_out(),
        _ => ui.reset_zoom(),
    }
    true
}

impl crate::compose::View {
    /// Let the user zoom it as the window's zoom lets them zoom the page:
    /// its zoom (`Styled::zoom`) bound to `zoom`, which Control with the
    /// wheel over it steps through Chrome's levels a notch at a time
    /// (Windows and Linux, where the window zooms so) and a trackpad pinch
    /// over it changes continuously (every platform), within 25% and 500%.
    /// It takes the gesture: the window zooms only where no zoomable is
    /// under the pointer, and the innermost zoomable under it takes it, at
    /// its limit too; a wheel gesture stays with it as it grows under the
    /// pointer, as one a scroll view takes does. In a scroll view what is
    /// under the pointer stays
    /// there; elsewhere it zooms about its top-left, as CSS's `zoom` does.
    /// Zooms multiply with its ancestors' and the window's; the keyboard's
    /// zoom stays the window's. `Ui::set_zoomable` is the same idea for the
    /// window: whether the user may zoom it at all.
    pub fn zoomable(self, zoom: crate::reactive::Signal<f32>) -> Self {
        let written = zoom.clone();
        let read = zoom.clone();
        self.zoomable_with(
            Box::new(move || zoom.get()),
            Box::new(move || read.with_untracked(|zoom| *zoom)),
            Box::new(move |zoom| {
                written.set(zoom);
            }),
        )
    }
    /// `zoomable`, its state read (`get`, tracked; `peek`, not) and written
    /// (`set`) through the caller's own: a script's number state. Boxed and
    /// out of line: one copy in the crate, whoever calls it (a generic one
    /// moved what the compiler inlined elsewhere in it).
    #[doc(hidden)]
    #[inline(never)]
    pub fn zoomable_with(
        self,
        mut get: Box<dyn FnMut() -> f32>,
        peek: Box<dyn Fn() -> f32>,
        set: Box<dyn Fn(f32)>,
    ) -> Self {
        // TODO(zoom): tell assistive technology the zoom (a value of its own,
        // not the node's), once AccessKit nodes can carry one beside theirs.
        let handle: Rc<RefCell<Option<crate::compose::NodeHandle>>> = Rc::default();
        let mounted = handle.clone();
        // A touchpad's notches short of a whole one.
        let turned = Cell::new(0f32);
        self.on_mount(move |node| {
            if let Some(mut ui) = node.ui_handle().upgrade() {
                let weak = ui.downgrade();
                let id = node.node();
                ui.bind(id, move || {
                    let zoom = within_limits(get());
                    if let Some(ui) = weak.upgrade() {
                        let mut scene = ui.scene.borrow_mut();
                        if scene.contains(id) {
                            scene.set_zoom(id, zoom);
                            refresh_fixed(&ui, &mut scene);
                        }
                    }
                });
            }
            *mounted.borrow_mut() = Some(node);
        })
        .on_event(move |cx| {
            if cx.phase == crate::input::EventPhase::Capture {
                return;
            }
            let was = within_limits(peek());
            let (x, y, next) = match cx.event {
                InputEvent::Scroll { x, y, delta_y, .. } => {
                    let Some(notch) = wheel_notch() else {
                        return;
                    };
                    let mut next = f64::from(was);
                    let mut notches = turned.get() - delta_y / notch;
                    while notches >= 1. {
                        next = next_level(next, true);
                        notches -= 1.;
                    }
                    while notches <= -1. {
                        next = next_level(next, false);
                        notches += 1.;
                    }
                    turned.set(notches);
                    (x, y, next as f32)
                }
                InputEvent::Pinch { x, y, delta } if delta.is_finite() => {
                    (x, y, within_limits(was * (1. + delta)))
                }
                _ => return,
            };
            // Its gesture, at its limit too: an ancestor (the window) never
            // zooms in its place mid-gesture.
            cx.stop_propagation();
            cx.prevent_default();
            if next == was {
                return;
            }
            // Where the pointer is in it before it zooms.
            let anchor = handle
                .borrow()
                .as_ref()
                .and_then(|node| Some((node.clone(), under((x, y), node)?)));
            set(next);
            if let Some((node, at)) = anchor {
                keep_under_pointer(&node, (x, y), at);
            }
        })
    }
}

/// A zoom the user may give: 25% to 500%, nonfinite as 100%.
fn within_limits(zoom: f32) -> f32 {
    match zoom.is_finite() {
        true => zoom.clamp(LEVELS[0] as f32, LEVELS[LEVELS.len() - 1] as f32),
        false => 1.,
    }
}

/// Where `pointer` is in `node`, in its own points.
fn under(pointer: (f32, f32), node: &crate::compose::NodeHandle) -> Option<(f32, f32)> {
    let ui = node.ui_handle().upgrade()?;
    let scene = ui.scene.borrow();
    let id = node.node();
    if !scene.contains(id) {
        return None;
    }
    let shown = scene.transformed_bounds(id);
    let scale = shown.width / scene.bounds(id).width.max(f32::EPSILON);
    Some(((pointer.0 - shown.x) / scale, (pointer.1 - shown.y) / scale))
}

/// Once `node`'s zoom has changed and it is laid out again, scroll what
/// holds it so that its point `at` (in its own points) shows at `pointer`
/// again.
fn keep_under_pointer(node: &crate::compose::NodeHandle, pointer: (f32, f32), at: (f32, f32)) {
    let Some(ui) = node.ui_handle().upgrade() else {
        return;
    };
    let id = node.node();
    let weak = ui.downgrade();
    node.after_layout(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let (delta, parent) = {
            let scene = ui.scene.borrow();
            if !scene.contains(id) {
                return;
            }
            let shown = scene.transformed_bounds(id);
            let scale = shown.width / scene.bounds(id).width.max(f32::EPSILON);
            let point = (shown.x + at.0 * scale, shown.y + at.1 * scale);
            ((point.0 - pointer.0, point.1 - pointer.1), scene.parent(id))
        };
        let Some(parent) = parent else {
            return;
        };
        if delta.0.abs() < 0.5 && delta.1.abs() < 0.5 {
            return;
        }
        // What scrolls it (if anything does) scrolls by that.
        ui.input.dispatch_to(
            &ui.scene,
            parent,
            InputEvent::Scroll {
                x: pointer.0,
                y: pointer.1,
                delta_x: delta.0,
                delta_y: delta.1,
            },
        );
    });
}
